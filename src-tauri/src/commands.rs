//! Tauri commands: the entire surface the frontend talks to.
//!
//! Every command returns `Result<T, String>` because `anyhow::Error` is not
//! serialisable; `err` turns any error into a message safe to show the user.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::audio::ambience::{self, FilterInfo};
use crate::audio::crossfade::CrossfadeSettings;
use crate::audio::decode::StreamInfo;
use crate::audio::params::{MixerSettings, Resolved};
use crate::audio::PlaybackSnapshot;
use crate::library::model::{Album, Artist, ScanReport, Track};
use crate::library::scan;
use crate::player::{Context, QueueItem, QueueView, Repeat};
use crate::playlist::{self, Playlist};
use crate::presets::{self, Preset};
use crate::state::{
    AppState, SETTING_GLOBAL_MIXER, SETTING_REPEAT, SETTING_SHUFFLE, SETTING_VOLUME,
};

type Cmd<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ---------------------------------------------------------------------------
// Library
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_folders(state: State<'_, AppState>) -> Cmd<Vec<String>> {
    state.db.folders().map_err(err)
}

#[tauri::command]
pub fn add_folder(state: State<'_, AppState>, path: String) -> Cmd<Vec<String>> {
    state.db.add_folder(&path).map_err(err)?;
    state.db.folders().map_err(err)
}

#[tauri::command]
pub fn remove_folder(state: State<'_, AppState>, path: String) -> Cmd<Vec<String>> {
    state.db.remove_folder(&path).map_err(err)?;
    state.db.folders().map_err(err)
}

/// Walk every watched folder and refresh the index, reporting progress as it
/// goes so a large library does not look frozen.
#[tauri::command]
pub fn scan_library(app: AppHandle, state: State<'_, AppState>) -> Cmd<ScanReport> {
    let folders = state.db.folders().map_err(err)?;
    if folders.is_empty() {
        return Ok(ScanReport::default());
    }

    let mut last_emit = std::time::Instant::now();
    let report = scan::scan_folders(&state.db, &state.paths.artwork, &folders, |count, path| {
        // Throttle so the event stream cannot outrun the webview.
        if last_emit.elapsed() > std::time::Duration::from_millis(100) {
            last_emit = std::time::Instant::now();
            let _ = app.emit(
                "scan-progress",
                serde_json::json!({ "count": count, "path": path }),
            );
        }
    })
    .map_err(err)?;

    let _ = app.emit("library-changed", ());
    Ok(report)
}

#[tauri::command]
pub fn list_tracks(state: State<'_, AppState>) -> Cmd<Vec<Track>> {
    state.db.all_tracks().map_err(err)
}

#[tauri::command]
pub fn list_albums(state: State<'_, AppState>) -> Cmd<Vec<Album>> {
    state.db.albums().map_err(err)
}

#[tauri::command]
pub fn list_artists(state: State<'_, AppState>) -> Cmd<Vec<Artist>> {
    state.db.artists().map_err(err)
}

#[tauri::command]
pub fn album_tracks(state: State<'_, AppState>, album_id: String) -> Cmd<Vec<Track>> {
    state.db.tracks_by_album(&album_id).map_err(err)
}

#[tauri::command]
pub fn artist_tracks(state: State<'_, AppState>, artist_id: String) -> Cmd<Vec<Track>> {
    state.db.tracks_by_artist(&artist_id).map_err(err)
}

#[tauri::command]
pub fn get_track(state: State<'_, AppState>, id: String) -> Cmd<Option<Track>> {
    state.db.get_track(&id).map_err(err)
}

#[tauri::command]
pub fn search(state: State<'_, AppState>, query: String) -> Cmd<Vec<Track>> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    state.db.search(&query, 200).map_err(err)
}

/// Opt-in online lookup. Nothing contacts the network until this is called.
#[tauri::command]
pub fn enrich_track(app: AppHandle, state: State<'_, AppState>, id: String) -> Cmd<Option<Track>> {
    use crate::library::metadata::MetadataProvider;

    let Some(mut track) = state.db.get_track(&id).map_err(err)? else {
        return Ok(None);
    };
    let provider = state.metadata_provider().map_err(err)?;
    let Some(enrichment) = provider.lookup(&track).map_err(err)? else {
        return Ok(None);
    };

    crate::library::metadata::apply(&mut track, &enrichment);
    state.db.upsert_track(&track).map_err(err)?;
    let _ = app.emit("library-changed", ());
    Ok(Some(track))
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

/// Load whatever the player says is current and start it.
fn start_current(app: &AppHandle, state: &AppState) -> Cmd<()> {
    let (path, gain) = {
        let player = state.player.lock();
        match player.current() {
            Some(track) => (PathBuf::from(&track.location), track.gain_db.unwrap_or(0.0)),
            None => {
                state.engine.clear();
                let _ = app.emit("track-changed", Option::<Track>::None);
                return Ok(());
            }
        }
    };

    // Announce the new track before loading it. Opening the file and refilling
    // the ring takes a few tens of milliseconds; there is no reason for the
    // title and artwork to wait for that.
    let current = state.player.lock().current().cloned();
    let _ = app.emit("track-changed", &current);
    let _ = app.emit("queue-changed", state.player.lock().view());

    state.sync_mixer();
    match state.engine.load(path, 0.0, gain) {
        Ok(_) => state.engine.play(),
        Err(e) => {
            // The UI has already moved on, so say why the audio did not.
            let _ = app.emit("engine-error", format!("could not play that track: {e}"));
            return Err(err(e));
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayRequest {
    /// Tracks to load into the queue.
    pub track_ids: Vec<String>,
    /// Which of them to start on.
    #[serde(default)]
    pub start_index: usize,
    #[serde(default)]
    pub context: Option<Context>,
    /// Mixer override carried by the playlist this came from.
    #[serde(default)]
    pub context_mixer: Option<MixerSettings>,
}

#[tauri::command]
pub fn play_tracks(app: AppHandle, state: State<'_, AppState>, request: PlayRequest) -> Cmd<()> {
    let tracks = load_tracks(&state, &request.track_ids)?;
    if tracks.is_empty() {
        return Ok(());
    }
    {
        let mut player = state.player.lock();
        player.context = request.context;
        player.context_mixer = request.context_mixer;
        player.set_queue(tracks, request.start_index);
    }
    start_current(&app, &state)
}

/// Jump to a position in the current play order.
#[tauri::command]
pub fn play_queue_index(app: AppHandle, state: State<'_, AppState>, index: usize) -> Cmd<()> {
    {
        let mut player = state.player.lock();
        if player.jump_to(index).is_none() {
            return Ok(());
        }
    }
    start_current(&app, &state)
}

#[tauri::command]
pub fn toggle_play(app: AppHandle, state: State<'_, AppState>) -> Cmd<bool> {
    let playing = state.engine.is_playing();
    if playing {
        state.engine.pause();
    } else {
        // Nothing loaded yet: start at the top of the queue.
        if state.player.lock().current().is_none() {
            return Ok(false);
        }
        state.engine.play();
    }
    let now_playing = !playing;
    let _ = app.emit("playing-changed", now_playing);
    Ok(now_playing)
}

#[tauri::command]
pub fn next_track(app: AppHandle, state: State<'_, AppState>) -> Cmd<()> {
    let has_next = state.player.lock().advance(false).is_some();
    if !has_next {
        state.engine.pause();
        return Ok(());
    }
    start_current(&app, &state)
}

/// Pressing previous within the first few seconds goes back a track; after
/// that it restarts the current one, which is what every player does.
#[tauri::command]
pub fn previous_track(app: AppHandle, state: State<'_, AppState>) -> Cmd<()> {
    const RESTART_AFTER_SECS: f64 = 3.0;
    if state.engine.snapshot().position_secs > RESTART_AFTER_SECS {
        state.engine.seek(0.0);
        return Ok(());
    }
    if state.player.lock().previous().is_none() {
        return Ok(());
    }
    start_current(&app, &state)
}

#[tauri::command]
pub fn seek(state: State<'_, AppState>, position_secs: f64) -> Cmd<()> {
    state.engine.seek(position_secs.max(0.0));
    Ok(())
}

#[tauri::command]
pub fn set_volume(state: State<'_, AppState>, volume: f32) -> Cmd<()> {
    state.engine.set_volume(volume);
    let _ = state.db.set_setting(SETTING_VOLUME, &volume.to_string());
    Ok(())
}

#[tauri::command]
pub fn playback_state(state: State<'_, AppState>) -> Cmd<PlaybackSnapshot> {
    Ok(state.engine.snapshot())
}

#[tauri::command]
pub fn stream_info(state: State<'_, AppState>) -> Cmd<Option<StreamInfo>> {
    Ok(state.engine.snapshot().stream)
}

// ---------------------------------------------------------------------------
// Queue
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn queue_state(state: State<'_, AppState>) -> Cmd<QueueView> {
    Ok(state.player.lock().view())
}

#[tauri::command]
pub fn current_track(state: State<'_, AppState>) -> Cmd<Option<Track>> {
    Ok(state.player.lock().current().cloned())
}

#[tauri::command]
pub fn play_next(app: AppHandle, state: State<'_, AppState>, track_ids: Vec<String>) -> Cmd<()> {
    let tracks = load_tracks(&state, &track_ids)?;
    let was_empty = {
        let mut player = state.player.lock();
        let empty = player.is_empty();
        player.play_next(tracks);
        empty
    };
    if was_empty {
        return start_current(&app, &state);
    }
    // Inserting right after the current track can change what a pending
    // crossfade was prepared into; safest to drop it and let the engine ask
    // again once it is actually needed.
    state.engine.cancel_next();
    let _ = app.emit("queue-changed", state.player.lock().view());
    Ok(())
}

#[tauri::command]
pub fn add_to_queue(app: AppHandle, state: State<'_, AppState>, track_ids: Vec<String>) -> Cmd<()> {
    let tracks = load_tracks(&state, &track_ids)?;
    let was_empty = {
        let mut player = state.player.lock();
        let empty = player.is_empty();
        player.add_to_queue(tracks);
        empty
    };
    if was_empty {
        return start_current(&app, &state);
    }
    state.engine.cancel_next();
    let _ = app.emit("queue-changed", state.player.lock().view());
    Ok(())
}

#[tauri::command]
pub fn remove_from_queue(app: AppHandle, state: State<'_, AppState>, index: usize) -> Cmd<()> {
    state.player.lock().remove_at(index);
    // The removed entry might be exactly what a pending crossfade was
    // prepared into.
    state.engine.cancel_next();
    let _ = app.emit("queue-changed", state.player.lock().view());
    Ok(())
}

#[tauri::command]
pub fn clear_queue(app: AppHandle, state: State<'_, AppState>) -> Cmd<()> {
    state.player.lock().clear();
    state.engine.clear();
    let _ = app.emit("queue-changed", state.player.lock().view());
    let _ = app.emit("track-changed", Option::<Track>::None);
    Ok(())
}

#[tauri::command]
pub fn set_shuffle(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Cmd<()> {
    state.player.lock().set_shuffle(enabled);
    let _ = state
        .db
        .set_setting(SETTING_SHUFFLE, if enabled { "true" } else { "false" });
    // What comes after the current track can change completely.
    state.engine.cancel_next();
    let _ = app.emit("queue-changed", state.player.lock().view());
    Ok(())
}

#[tauri::command]
pub fn set_repeat(app: AppHandle, state: State<'_, AppState>, mode: Repeat) -> Cmd<()> {
    state.player.lock().set_repeat(mode);
    if let Ok(raw) = serde_json::to_string(&mode) {
        let _ = state.db.set_setting(SETTING_REPEAT, &raw);
    }
    // `peek_next` under the old mode may no longer be what comes next.
    state.engine.cancel_next();
    let _ = app.emit("queue-changed", state.player.lock().view());
    Ok(())
}

// ---------------------------------------------------------------------------
// Mixer
// ---------------------------------------------------------------------------

/// Everything the mixer panels need in one call.
/// The cascade on its own, cheap enough to fetch on every track change.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerLayers {
    pub global: MixerSettings,
    pub context: Option<MixerSettings>,
    pub track: Option<MixerSettings>,
    pub effective: Resolved,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerState {
    pub global: MixerSettings,
    /// The playlist layer for whatever is playing, if any.
    pub context: Option<MixerSettings>,
    /// The playing queue entry's own layer, if any. Only ever set when the
    /// queue came from a playlist.
    pub track: Option<MixerSettings>,
    /// The three layers collapsed, which is what is actually being heard.
    pub effective: Resolved,
    pub presets: Vec<Preset>,
    pub filters: Vec<FilterInfo>,
}

#[tauri::command]
pub fn mixer_state(state: State<'_, AppState>) -> Cmd<MixerState> {
    let layers = mixer_layers(state.clone())?;
    // Read from disk only after the player lock has been released: holding it
    // across file I/O stalls every other command, which made pressing next
    // feel laggy while the previous track change was still settling.
    Ok(MixerState {
        global: layers.global,
        context: layers.context,
        track: layers.track,
        effective: layers.effective,
        presets: presets::load_all(&state.paths.presets),
        filters: ambience::catalogue(&state.paths.filters),
    })
}

/// Just the cascade, with no disk access. Used on every track change, where
/// the preset list and the filter catalogue cannot have changed.
#[tauri::command]
pub fn mixer_layers(state: State<'_, AppState>) -> Cmd<MixerLayers> {
    let player = state.player.lock();
    Ok(MixerLayers {
        global: player.global_mixer.clone(),
        context: player.context_mixer.clone(),
        track: player.current_item().and_then(|i| i.mixer.clone()),
        effective: player.effective_mixer(),
    })
}

#[tauri::command]
pub fn set_global_mixer(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: MixerSettings,
) -> Cmd<Resolved> {
    {
        let mut player = state.player.lock();
        player.global_mixer = settings;
    }
    persist_global_mixer(&state);
    state.sync_mixer();
    request_missing_beds(&state);

    let effective = state.player.lock().effective_mixer();
    let _ = app.emit("mixer-changed", &effective);
    Ok(effective)
}

/// Set the playlist layer, both live and in the playlist file.
#[tauri::command]
pub fn set_playlist_mixer(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
    settings: Option<MixerSettings>,
) -> Cmd<()> {
    if let Some((path, mut playlist)) = find_playlist(&state, &playlist_id) {
        playlist.mixer = settings.clone();
        playlist.save(&path).map_err(err)?;
    }

    // Only touch live playback if this playlist is the one playing.
    let applies_now = state
        .player
        .lock()
        .context
        .as_ref()
        .map(|c| c.id == playlist_id)
        .unwrap_or(false);
    if applies_now {
        state.player.lock().context_mixer = settings;
        state.sync_mixer();
        request_missing_beds(&state);
        let _ = app.emit("mixer-changed", state.player.lock().effective_mixer());
    }
    let _ = app.emit("playlists-changed", ());
    Ok(())
}

#[tauri::command]
pub fn list_presets(state: State<'_, AppState>) -> Cmd<Vec<Preset>> {
    Ok(presets::load_all(&state.paths.presets))
}

#[tauri::command]
pub fn save_preset(
    state: State<'_, AppState>,
    name: String,
    settings: MixerSettings,
) -> Cmd<Vec<Preset>> {
    presets::upsert(&state.paths.presets, &name, settings).map_err(err)
}

#[tauri::command]
pub fn delete_preset(state: State<'_, AppState>, id: String) -> Cmd<Vec<Preset>> {
    presets::delete(&state.paths.presets, &id).map_err(err)
}

// ---------------------------------------------------------------------------
// Crossfade
//
// A global mixer section, with an optional playlist override. These commands
// edit the global layer; `crossfade_settings` returns the effective value.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn crossfade_settings(state: State<'_, AppState>) -> Cmd<CrossfadeSettings> {
    Ok(state.player.lock().effective_mixer().crossfade)
}

/// Set the crossfade length from the simple slider. Keeps the curve's shape
/// (symmetric, or whatever the advanced graph left it as) and rescales it to
/// the new length.
#[tauri::command]
pub fn set_crossfade_length(
    state: State<'_, AppState>,
    length_secs: f32,
) -> Cmd<CrossfadeSettings> {
    let next = {
        let mut player = state.player.lock();
        let next = player
            .global_mixer
            .crossfade
            .clone()
            .unwrap_or_default()
            .with_length(length_secs);
        player.global_mixer.crossfade = Some(next.clone());
        next
    };
    persist_global_mixer(&state);
    state.sync_mixer();
    Ok(next)
}

/// Set the crossfade curve directly, from the advanced graph. `length_secs`
/// is derived from the curve's own extent so the two stay in step regardless
/// of which control was used last.
#[tauri::command]
pub fn set_crossfade_curve(
    state: State<'_, AppState>,
    curve: crate::audio::crossfade::CrossfadeCurve,
) -> Cmd<CrossfadeSettings> {
    let next = {
        let mut player = state.player.lock();
        let length = player
            .global_mixer
            .crossfade
            .as_ref()
            .map(|crossfade| crossfade.length_secs)
            .unwrap_or_default();
        let next = CrossfadeSettings {
            length_secs: length,
            curve: curve.clamp(length),
        };
        player.global_mixer.crossfade = Some(next.clone());
        next
    };
    persist_global_mixer(&state);
    state.sync_mixer();
    Ok(next)
}

fn persist_global_mixer(state: &AppState) {
    if let Ok(raw) = serde_json::to_string(&state.player.lock().global_mixer) {
        let _ = state.db.set_setting(SETTING_GLOBAL_MIXER, &raw);
    }
}

#[tauri::command]
pub fn list_filters(state: State<'_, AppState>) -> Cmd<Vec<FilterInfo>> {
    Ok(ambience::catalogue(&state.paths.filters))
}

/// Where to drop ambience audio files, shown in the mixer's filter section.
#[tauri::command]
pub fn filters_directory(state: State<'_, AppState>) -> Cmd<String> {
    Ok(state.paths.filters.display().to_string())
}

// ---------------------------------------------------------------------------
// Playlists
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub track_count: usize,
    pub artwork: Option<String>,
    pub has_mixer: bool,
    pub path: String,
}

#[tauri::command]
pub fn list_playlists(state: State<'_, AppState>) -> Cmd<Vec<PlaylistSummary>> {
    Ok(playlist::list(&state.paths.playlists)
        .into_iter()
        .map(|(path, p)| PlaylistSummary {
            id: p.id,
            name: p.name,
            description: p.description,
            track_count: p.tracks.len(),
            artwork: p.artwork,
            has_mixer: p.mixer.is_some(),
            path: path.display().to_string(),
        })
        .collect())
}

#[tauri::command]
pub fn get_playlist(state: State<'_, AppState>, id: String) -> Cmd<Option<playlist::Resolved>> {
    let Some((_, p)) = find_playlist(&state, &id) else {
        return Ok(None);
    };
    p.resolve(&state.db).map(Some).map_err(err)
}

#[tauri::command]
pub fn create_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
) -> Cmd<PlaylistSummary> {
    let mut p = Playlist {
        name: name.clone(),
        ..Default::default()
    };
    p.description = description.unwrap_or_default();
    let path = state
        .paths
        .playlists
        .join(playlist::file_name_for(&p.name, &p.id));
    p.save(&path).map_err(err)?;

    let _ = app.emit("playlists-changed", ());
    Ok(PlaylistSummary {
        id: p.id,
        name: p.name,
        description: p.description,
        track_count: 0,
        artwork: p.artwork,
        has_mixer: false,
        path: path.display().to_string(),
    })
}

#[tauri::command]
pub fn update_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    description: Option<String>,
) -> Cmd<()> {
    let Some((path, mut p)) = find_playlist(&state, &id) else {
        return Err("playlist not found".into());
    };
    if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
        p.name = name;
    }
    if let Some(description) = description {
        p.description = description;
    }
    p.save(&path).map_err(err)?;
    let _ = app.emit("playlists-changed", ());
    Ok(())
}

#[tauri::command]
pub fn delete_playlist(app: AppHandle, state: State<'_, AppState>, id: String) -> Cmd<()> {
    let Some((path, _)) = find_playlist(&state, &id) else {
        return Ok(());
    };
    std::fs::remove_file(&path).map_err(err)?;
    let _ = app.emit("playlists-changed", ());
    Ok(())
}

#[tauri::command]
pub fn add_to_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
    track_ids: Vec<String>,
) -> Cmd<usize> {
    let Some((path, mut p)) = find_playlist(&state, &playlist_id) else {
        return Err("playlist not found".into());
    };
    let tracks = load_tracks(&state, &track_ids)?;
    for track in &tracks {
        p.add_track(track);
    }
    p.save(&path).map_err(err)?;
    let _ = app.emit("playlists-changed", ());
    Ok(tracks.len())
}

#[tauri::command]
pub fn remove_from_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
    index: usize,
) -> Cmd<()> {
    let Some((path, mut p)) = find_playlist(&state, &playlist_id) else {
        return Err("playlist not found".into());
    };
    if index < p.tracks.len() {
        p.tracks.remove(index);
        p.save(&path).map_err(err)?;
        let _ = app.emit("playlists-changed", ());
    }
    Ok(())
}

/// Reorder by moving one entry to a new index.
#[tauri::command]
pub fn move_in_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
    from: usize,
    to: usize,
) -> Cmd<()> {
    let Some((path, mut p)) = find_playlist(&state, &playlist_id) else {
        return Err("playlist not found".into());
    };
    if from < p.tracks.len() && to < p.tracks.len() {
        let entry = p.tracks.remove(from);
        p.tracks.insert(to, entry);
        p.save(&path).map_err(err)?;
        let _ = app.emit("playlists-changed", ());
    }
    Ok(())
}

/// Set a per-entry mixer override inside a playlist file.
/// Set the mixer override for one entry of one playlist.
///
/// This is the only place a per-song override lives: the same song in another
/// playlist, or played straight from the library, is unaffected.
#[tauri::command]
pub fn set_playlist_entry_mixer(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
    index: usize,
    mut settings: Option<MixerSettings>,
) -> Cmd<()> {
    // Crossfade is scoped only to global and playlist mixer layers.
    if let Some(settings) = settings.as_mut() {
        settings.crossfade = None;
    }

    let Some((path, mut p)) = find_playlist(&state, &playlist_id) else {
        return Err("playlist not found".into());
    };
    let Some(entry) = p.tracks.get_mut(index) else {
        return Ok(());
    };
    entry.mixer = settings.clone();
    let entry_title = entry.title.clone();
    let entry_artist = entry.artist.clone();
    let entry_album = entry.album.clone();
    let entry_mbid = entry.musicbrainz_recording_id.clone();
    p.save(&path).map_err(err)?;

    // If this playlist is the one playing, apply the change without waiting
    // for the track to come round again.
    let playing_this = state
        .player
        .lock()
        .context
        .as_ref()
        .map(|c| c.id == playlist_id)
        .unwrap_or(false);
    if playing_this {
        if let Some(track) = state
            .db
            .resolve(
                entry_mbid.as_deref(),
                &entry_artist,
                &entry_title,
                &entry_album,
            )
            .map_err(err)?
        {
            let changed = state.player.lock().set_entry_mixer(&track.id, settings);
            if changed {
                state.sync_mixer();
                let _ = app.emit("mixer-changed", state.player.lock().effective_mixer());
            }
        }
    }

    let _ = app.emit("playlists-changed", ());
    Ok(())
}

/// Reorder the play queue.
#[tauri::command]
pub fn move_in_queue(
    app: AppHandle,
    state: State<'_, AppState>,
    from: usize,
    to: usize,
) -> Cmd<()> {
    if state.player.lock().move_item(from, to) {
        // Reordering can change what immediately follows the current track.
        state.engine.cancel_next();
        let _ = app.emit("queue-changed", state.player.lock().view());
    }
    Ok(())
}

/// Copy a playlist file someone shared into the library.
#[tauri::command]
pub fn import_playlist(app: AppHandle, state: State<'_, AppState>, path: String) -> Cmd<String> {
    let source = PathBuf::from(&path);
    let mut p = Playlist::load(&source).map_err(err)?;
    // Give the import a fresh id so it cannot collide with an existing file.
    p.id =
        crate::library::model::stable_id("pl", &format!("{}{}", p.name, crate::library::db::now()));
    let destination = state
        .paths
        .playlists
        .join(playlist::file_name_for(&p.name, &p.id));
    p.save(&destination).map_err(err)?;
    let _ = app.emit("playlists-changed", ());
    Ok(p.id)
}

#[tauri::command]
pub fn export_playlist(state: State<'_, AppState>, id: String, destination: String) -> Cmd<()> {
    let Some((path, _)) = find_playlist(&state, &id) else {
        return Err("playlist not found".into());
    };
    std::fs::copy(&path, &destination).map_err(err)?;
    Ok(())
}

/// Play a whole playlist, applying its mixer override for the session.
#[tauri::command]
pub fn play_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    start_index: Option<usize>,
) -> Cmd<()> {
    let Some((_, p)) = find_playlist(&state, &id) else {
        return Err("playlist not found".into());
    };
    let context = Context {
        kind: "playlist".into(),
        id: p.id.clone(),
        name: p.name.clone(),
    };
    let context_mixer = p.mixer.clone();
    let resolved = p.resolve(&state.db).map_err(err)?;

    // Entries with nothing matching locally are skipped, and the start index
    // is adjusted so playback still begins on the track that was clicked.
    let requested = start_index.unwrap_or(0);
    let mut items = Vec::new();
    let mut adjusted_start = 0;
    for item in resolved.items.iter() {
        if let Some(track) = item.track.as_ref() {
            if item.index <= requested {
                adjusted_start = items.len();
            }
            // The entry's own override rides along with the queue entry.
            items.push(QueueItem {
                track: track.clone(),
                mixer: item.entry.mixer.clone(),
            });
        }
    }
    if items.is_empty() {
        return Err("none of this playlist's tracks are in your library".into());
    }

    {
        let mut player = state.player.lock();
        player.context = Some(context);
        player.context_mixer = context_mixer;
        player.set_queue_items(items, adjusted_start);
    }
    start_current(&app, &state)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_tracks(state: &AppState, ids: &[String]) -> Cmd<Vec<Track>> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(track) = state.db.get_track(id).map_err(err)? {
            out.push(track);
        }
    }
    Ok(out)
}

fn find_playlist(state: &AppState, id: &str) -> Option<(PathBuf, Playlist)> {
    playlist::list(&state.paths.playlists)
        .into_iter()
        .find(|(_, p)| p.id == id)
}

/// Kick off decoding for any ambience bed the mixer now wants but has not got.
fn request_missing_beds(state: &AppState) {
    let wanted = state.player.lock().effective_mixer().filters;
    for filter in wanted.iter().filter(|f| f.enabled) {
        if !state.engine.has_bed(&filter.id) {
            state.engine.request_bed(&filter.id);
        }
    }
}
