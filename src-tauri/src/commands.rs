//! Tauri commands: the entire surface the frontend talks to.
//!
//! Every command returns `Result<T, String>` because `anyhow::Error` is not
//! serialisable; `err` turns any error into a message safe to show the user.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context as _};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::audio::ambience::{self, FilterInfo};
use crate::audio::crossfade::CrossfadeSettings;
use crate::audio::decode::StreamInfo;
use crate::audio::params::{MixerSettings, Resolved};
use crate::audio::PlaybackSnapshot;
use crate::library::model::{
    normalise, stable_id, Album, Artist, HomePick, PlayRecord, ScanReport, Track, TrackFile,
};
use crate::library::scan;
use crate::player::{Context, QueueItem, QueueView, Repeat};
use crate::playlist::{self, Playlist};
use crate::presets::{self, Preset, PresetKind};
use crate::state::{
    load_app_preferences, AppPreferences, AppState, SETTING_APP_PREFERENCES, SETTING_GLOBAL_MIXER,
    SETTING_REPEAT, SETTING_SHUFFLE, SETTING_VOLUME,
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
pub fn list_track_files(state: State<'_, AppState>, song_id: String) -> Cmd<Vec<TrackFile>> {
    state.db.files_for_song(&song_id).map_err(err)
}

#[tauri::command]
pub fn set_preferred_track_file(
    app: AppHandle,
    state: State<'_, AppState>,
    song_id: String,
    file_id: Option<String>,
) -> Cmd<Track> {
    state
        .db
        .set_preferred_file(&song_id, file_id.as_deref())
        .map_err(err)?;
    let track = state
        .db
        .get_track(&song_id)
        .map_err(err)?
        .ok_or_else(|| "song not found after setting its preferred file".to_string())?;
    let _ = app.emit("library-changed", ());
    Ok(track)
}

#[tauri::command]
pub fn preview_track_file(state: State<'_, AppState>, song_id: String, file_id: String) -> Cmd<()> {
    let file = state
        .db
        .file_by_id(&file_id)
        .map_err(err)?
        .ok_or_else(|| format!("file not found: {file_id}"))?;
    let song = state
        .db
        .get_track(&song_id)
        .map_err(err)?
        .ok_or_else(|| format!("song not found: {song_id}"))?;
    if file.song_id != song.id {
        return Err(format!(
            "file {file_id} does not belong to song {}",
            song.id
        ));
    }
    if !file.available {
        return Err("that file is missing and cannot be previewed".into());
    }
    state.preview_file(&file).map_err(err)
}

#[tauri::command]
pub fn stop_track_file_preview(state: State<'_, AppState>) -> Cmd<()> {
    state.stop_preview().map(|_| ()).map_err(err)
}

#[tauri::command]
pub fn restore_needs_destination(state: State<'_, AppState>, path: String) -> Cmd<bool> {
    let Ok(path) = Path::new(&path).canonicalize() else {
        return Ok(true);
    };
    let folders = state.db.folders().map_err(err)?;
    Ok(!folders.iter().any(|folder| {
        Path::new(folder)
            .canonicalize()
            .map(|root| path.starts_with(root))
            .unwrap_or(false)
    }))
}

#[tauri::command]
pub fn relink_track_file(
    app: AppHandle,
    state: State<'_, AppState>,
    file_id: String,
    path: String,
    destination_folder: Option<String>,
) -> Cmd<Track> {
    let file = state
        .db
        .file_by_id(&file_id)
        .map_err(err)?
        .ok_or_else(|| format!("file not found: {file_id}"))?;
    let selected = Path::new(&path)
        .canonicalize()
        .with_context(|| format!("reading selected file {path}"))
        .map_err(err)?;
    if !selected.is_file() {
        return Err(format!(
            "selected path is not a file: {}",
            selected.display()
        ));
    }

    // Read and validate before creating a restored copy.
    let mut replacement = scan::read_track(&selected, &state.paths.artwork).map_err(err)?;
    let versions = state.db.files_for_song(&file.song_id).map_err(err)?;
    if !versions
        .iter()
        .all(|existing| relink_identity_matches(&replacement, existing))
    {
        return Err("selected audio does not match this song".into());
    }

    let folders = state.db.folders().map_err(err)?;
    let roots: Vec<(String, PathBuf)> = folders
        .iter()
        .filter_map(|folder| {
            Path::new(folder)
                .canonicalize()
                .ok()
                .map(|root| (folder.clone(), root))
        })
        .collect();
    let target = if roots.iter().any(|(_, root)| selected.starts_with(root)) {
        selected
    } else {
        let destination = destination_folder
            .as_deref()
            .ok_or_else(|| "a configured destination folder is required".to_string())?;
        let canonical_destination = Path::new(destination)
            .canonicalize()
            .with_context(|| format!("reading destination folder {destination}"))
            .map_err(err)?;
        let root = roots
            .iter()
            .find(|(_, root)| *root == canonical_destination)
            .map(|(_, root)| root)
            .ok_or_else(|| "destination folder is not a configured library folder".to_string())?;
        let restored = root.join("Pick n Mix Restored");
        std::fs::create_dir_all(&restored).map_err(err)?;
        let target = unique_restored_path(&restored, &selected).map_err(err)?;
        reject_indexed_target(&state, &file_id, &target).map_err(err)?;
        std::fs::copy(&selected, &target)
            .with_context(|| format!("copying restored file to {}", target.display()))
            .map_err(err)?;
        target
    };

    reject_indexed_target(&state, &file_id, &target).map_err(err)?;
    replacement.location = target.display().to_string();
    replacement.id = stable_id("t", &replacement.location);
    state.db.relink_file(&file_id, &replacement).map_err(err)?;
    let track = state
        .db
        .get_track(&file.song_id)
        .map_err(err)?
        .ok_or_else(|| "song disappeared after relinking its file".to_string())?;
    let _ = app.emit("library-changed", ());
    Ok(track)
}

#[tauri::command]
pub fn trash_track_file(
    app: AppHandle,
    state: State<'_, AppState>,
    file_id: String,
) -> Cmd<Option<Track>> {
    let file = state
        .db
        .file_by_id(&file_id)
        .map_err(err)?
        .ok_or_else(|| format!("file not found: {file_id}"))?;
    if !file.available {
        return Err("missing files can be forgotten, but cannot be moved to Trash".into());
    }
    state.stop_preview().map_err(err)?;
    trash::delete(Path::new(&file.location)).map_err(err)?;
    state.db.forget_file(&file_id).map_err(err)?;
    let track = state.db.get_track(&file.song_id).map_err(err)?;
    let _ = app.emit("library-changed", ());
    Ok(track)
}

#[tauri::command]
pub fn forget_missing_track_file(
    app: AppHandle,
    state: State<'_, AppState>,
    file_id: String,
) -> Cmd<Option<Track>> {
    let file = state
        .db
        .file_by_id(&file_id)
        .map_err(err)?
        .ok_or_else(|| format!("file not found: {file_id}"))?;
    if file.available {
        return Err("available files must be moved to Trash, not forgotten".into());
    }
    state.db.forget_file(&file_id).map_err(err)?;
    let track = state.db.get_track(&file.song_id).map_err(err)?;
    let _ = app.emit("library-changed", ());
    Ok(track)
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

/// Resolve and load whatever logical song the player says is current.
pub(crate) fn start_current(app: &AppHandle, state: &AppState) -> Cmd<()> {
    load_current(app, state, 0.0, true).map(|_| ())
}

fn load_current(app: &AppHandle, state: &AppState, position_secs: f64, playing: bool) -> Cmd<bool> {
    state.cancel_preview();
    let Some(song_id) = state.player.lock().current().map(|track| track.id.clone()) else {
        state.engine.clear();
        state.end_play();
        let _ = app.emit("track-changed", Option::<Track>::None);
        return Ok(false);
    };

    let refreshed = state
        .db
        .get_track(&song_id)
        .map_err(err)?
        .ok_or_else(|| format!("song not found: {song_id}"))?;
    // Restarting the song that is already playing is a replay, so this is
    // deliberately unconditional: the previous listen is banked and a fresh
    // one begins, rather than the two being silently merged into one.
    state.begin_play(&song_id, refreshed.duration_secs);
    state.player.lock().refresh_current_track(refreshed.clone());
    let _ = app.emit("track-changed", Some(&refreshed));
    let _ = app.emit("queue-changed", state.player.lock().view());

    state.sync_mixer();
    let files = state.playback_files(&song_id).map_err(err)?;
    let mut failures = Vec::new();
    for file in files {
        match state.engine.load(
            PathBuf::from(&file.location),
            position_secs.max(0.0),
            file.gain_db.unwrap_or(0.0),
        ) {
            Ok(_) => {
                if playing {
                    state.engine.play();
                } else {
                    state.engine.pause();
                }
                return Ok(true);
            }
            Err(error) => failures.push(format!("{}: {error}", file.location)),
        }
    }

    state.engine.pause();
    let message = if failures.is_empty() {
        format!("no available file for {}", refreshed.title)
    } else {
        format!(
            "none of the available files for {} could be opened: {}",
            refreshed.title,
            failures.join("; ")
        )
    };
    let _ = app.emit("engine-error", &message);
    Err(message)
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
    state.cancel_preview();
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
    state.cancel_preview();
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
    if let Some(preview) = state.cancel_preview() {
        let normal_was_playing = preview
            .original
            .as_ref()
            .map(|original| original.playing)
            .unwrap_or(false);
        let now_playing = !normal_was_playing;
        let loaded = load_current(&app, &state, 0.0, now_playing)?;
        let now_playing = loaded && now_playing;
        let _ = app.emit("playing-changed", now_playing);
        return Ok(now_playing);
    }

    if playing {
        state.engine.pause();
    } else {
        if state.player.lock().current().is_none() {
            return Ok(false);
        }
        if state.engine.snapshot().stream.is_none() {
            load_current(&app, &state, 0.0, true)?;
        } else {
            state.engine.play();
        }
    }
    let now_playing = !playing;
    let _ = app.emit("playing-changed", now_playing);
    Ok(now_playing)
}

#[tauri::command]
pub fn next_track(app: AppHandle, state: State<'_, AppState>) -> Cmd<()> {
    state.cancel_preview();
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
    let cancelled = state.cancel_preview();
    let position = cancelled
        .as_ref()
        .and_then(|preview| preview.original.as_ref())
        .map(|original| original.position_secs)
        .unwrap_or_else(|| state.engine.snapshot().position_secs);
    if position > RESTART_AFTER_SECS {
        if cancelled.is_some() {
            load_current(&app, &state, 0.0, true)?;
        } else {
            state.engine.seek(0.0);
        }
        return Ok(());
    }
    if state.player.lock().previous().is_none() {
        if cancelled.is_some() {
            return load_current(&app, &state, 0.0, true).map(|_| ());
        }
        return Ok(());
    }
    start_current(&app, &state)
}

#[tauri::command]
pub fn seek(app: AppHandle, state: State<'_, AppState>, position_secs: f64) -> Cmd<()> {
    if let Some(preview) = state.cancel_preview() {
        let playing = preview
            .original
            .as_ref()
            .map(|original| original.playing)
            .unwrap_or(false);
        load_current(&app, &state, position_secs, playing)?;
    } else {
        state.engine.seek(position_secs.max(0.0));
    }
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

/// One frame of the output spectrum, ready to draw.
///
/// The axis is described alongside the data rather than duplicated as
/// constants in the UI, so the two cannot disagree about what a bin means.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyserFrame {
    /// Magnitudes in dBFS, log-spaced from `min_hz` to `max_hz`.
    pub bins: Vec<f32>,
    pub min_hz: f32,
    pub max_hz: f32,
    pub floor_db: f32,
}

/// Turn the spectrum on only while something is drawing it.
#[tauri::command]
pub fn set_analyser_enabled(state: State<'_, AppState>, enabled: bool) -> Cmd<()> {
    state.engine.set_analyser_enabled(enabled);
    Ok(())
}

/// Polled by the UI at frame rate while the expanded EQ is open.
///
/// Pulled on demand rather than pushed as an event: at 60 Hz an event stream
/// would be a lot of traffic for data that is only worth anything to a view
/// that may not even be open.
#[tauri::command]
pub fn analyser_frame(state: State<'_, AppState>) -> Cmd<AnalyserFrame> {
    use crate::audio::analyser;
    Ok(AnalyserFrame {
        bins: (*state.engine.analyser_bins()).clone(),
        min_hz: analyser::MIN_HZ,
        max_hz: analyser::MAX_HZ,
        floor_db: analyser::FLOOR_DB,
    })
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
    state.cancel_preview();
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
    state.cancel_preview();
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
    state.cancel_preview();
    state.player.lock().remove_at(index);
    // The removed entry might be exactly what a pending crossfade was
    // prepared into.
    state.engine.cancel_next();
    let _ = app.emit("queue-changed", state.player.lock().view());
    Ok(())
}

#[tauri::command]
pub fn clear_queue(app: AppHandle, state: State<'_, AppState>) -> Cmd<()> {
    state.cancel_preview();
    state.player.lock().clear();
    state.engine.clear();
    let _ = app.emit("queue-changed", state.player.lock().view());
    let _ = app.emit("track-changed", Option::<Track>::None);
    Ok(())
}

#[tauri::command]
pub fn set_shuffle(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Cmd<()> {
    state.cancel_preview();
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
    state.cancel_preview();
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
// App preferences
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn app_preferences(state: State<'_, AppState>) -> Cmd<AppPreferences> {
    Ok(load_app_preferences(&state.db))
}

#[tauri::command]
pub fn set_app_preferences(
    app: AppHandle,
    state: State<'_, AppState>,
    preferences: AppPreferences,
) -> Cmd<AppPreferences> {
    let previous = load_app_preferences(&state.db);
    let preferences = preferences.validated();
    let raw = serde_json::to_string(&preferences).map_err(err)?;
    state
        .db
        .set_setting(SETTING_APP_PREFERENCES, &raw)
        .map_err(err)?;
    state.engine.set_fade_mode(&preferences.fade_mode);
    state.engine.set_keep_tail(preferences.keep_reverb_on_pause);
    if preferences.output_device != previous.output_device {
        apply_output_device(&app, &state, &preferences.output_device)?;
    }
    if preferences.recommendation_parameters_differ(&previous) {
        state.clear_mixes();
        let _ = app.emit("home-changed", ());
    }
    Ok(preferences)
}

/// Output devices available right now, so the picker can offer them.
#[tauri::command]
pub fn output_devices() -> Cmd<Vec<String>> {
    Ok(crate::audio::AudioEngine::output_devices())
}

/// Move playback to another output, keeping the listener's place.
///
/// Switching device changes the sample rate, and a decoder resamples to the
/// rate it was opened with — so the current track has to be reopened. The
/// position and play state are captured first and restored afterwards, which
/// is what makes this feel like a device change rather than a stop.
fn apply_output_device(app: &AppHandle, state: &AppState, name: &str) -> Cmd<()> {
    let snapshot = state.engine.snapshot();
    let resume_at = snapshot.position_secs;
    let was_playing = snapshot.playing;

    state
        .engine
        .set_output_device(Some(name).filter(|n| !n.is_empty()))
        .map_err(err)?;

    // The worker dropped its voices with the old ring, so there is nothing
    // playing to preserve if the queue is empty.
    if state.player.lock().current().is_some() {
        load_current(app, state, resume_at, was_playing)?;
    }
    let _ = app.emit("playback", &state.engine.snapshot());
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
        filters: ambience::catalogue(
            state.paths.bundled_ambience.as_deref(),
            &state.paths.filters,
        ),
    })
}

/// Just the cascade, with no disk access. Used on every track change, where
/// the preset list and the ambience catalogue cannot have changed.
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
    kind: Option<PresetKind>,
) -> Cmd<Vec<Preset>> {
    presets::upsert_with_kind(
        &state.paths.presets,
        &name,
        kind.unwrap_or_default(),
        settings,
    )
    .map_err(err)
}

#[tauri::command]
pub fn update_preset(
    state: State<'_, AppState>,
    id: String,
    name: String,
    settings: MixerSettings,
) -> Cmd<Vec<Preset>> {
    presets::update_user(&state.paths.presets, &id, &name, settings).map_err(err)
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
    Ok(ambience::catalogue(
        state.paths.bundled_ambience.as_deref(),
        &state.paths.filters,
    ))
}

/// Where to drop custom ambience audio files, shown in the mixer.
#[tauri::command]
pub fn filters_directory(state: State<'_, AppState>) -> Cmd<String> {
    Ok(state.paths.filters.display().to_string())
}

#[tauri::command]
pub fn import_filter(state: State<'_, AppState>, source_path: String) -> Cmd<Vec<FilterInfo>> {
    let source = Path::new(&source_path)
        .canonicalize()
        .with_context(|| format!("reading ambience audio {source_path}"))
        .map_err(err)?;
    if !source.is_file() {
        return Err(format!(
            "ambience source is not a file: {}",
            source.display()
        ));
    }
    if !ambience::is_supported_audio(&source) {
        return Err("unsupported ambience audio format".into());
    }
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "ambience source has no safe UTF-8 filename".to_string())?;
    let destination = state.paths.filters.join(file_name);
    if source
        != destination
            .canonicalize()
            .unwrap_or_else(|_| destination.clone())
    {
        std::fs::copy(&source, &destination)
            .with_context(|| format!("importing ambience to {}", destination.display()))
            .map_err(err)?;
    }
    Ok(ambience::catalogue(
        state.paths.bundled_ambience.as_deref(),
        &state.paths.filters,
    ))
}

#[tauri::command]
pub fn delete_filter(state: State<'_, AppState>, id: String) -> Cmd<Vec<FilterInfo>> {
    let path = ambience::catalogue(
        state.paths.bundled_ambience.as_deref(),
        &state.paths.filters,
    )
    .into_iter()
    .find(|filter| filter.id == id)
    .and_then(|filter| filter.path)
    .map(PathBuf::from)
    .ok_or_else(|| format!("ambience not found: {id}"))?;
    let filters_dir = state
        .paths
        .filters
        .canonicalize()
        .context("resolving custom ambience directory")
        .map_err(err)?;
    let canonical = path
        .canonicalize()
        .with_context(|| format!("resolving ambience {}", path.display()))
        .map_err(err)?;
    if !canonical.starts_with(&filters_dir) || !canonical.is_file() {
        return Err("refusing to delete ambience outside the custom ambience directory".into());
    }
    std::fs::remove_file(&path)
        .with_context(|| format!("deleting ambience {}", path.display()))
        .map_err(err)?;
    state.engine.remove_bed(&id);
    Ok(ambience::catalogue(
        state.paths.bundled_ambience.as_deref(),
        &state.paths.filters,
    ))
}

// ---------------------------------------------------------------------------
// Playlists
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub track_count: usize,
    pub artwork: Option<String>,
    pub has_mixer: bool,
    /// Whether a timeline has ever been built for this playlist.
    pub has_master_mix: bool,
    /// Whether that timeline is the thing that plays.
    pub master_mix_enabled: bool,
    pub shuffle_only: bool,
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
            has_master_mix: p.master_mix.is_some(),
            master_mix_enabled: p.master_mix.as_ref().is_some_and(|m| m.enabled),
            shuffle_only: p.shuffle_only,
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
        has_master_mix: false,
        master_mix_enabled: false,
        shuffle_only: p.shuffle_only,
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

/// Toggle "shuffle-only": the stored order is ignored every time this playlist
/// is played.
#[tauri::command]
pub fn set_playlist_shuffle_only(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Cmd<()> {
    let Some((path, mut p)) = find_playlist(&state, &id) else {
        return Err("playlist not found".into());
    };
    p.shuffle_only = enabled;
    p.save(&path).map_err(err)?;
    let _ = app.emit("playlists-changed", ());
    Ok(())
}

/// Replace a playlist's image with a copy of a file from this machine.
///
/// The image is copied into the artwork cache, so the playlist keeps its
/// picture even if the original is later moved or deleted.
#[tauri::command]
pub fn set_playlist_artwork(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    source_path: String,
) -> Cmd<String> {
    let Some((path, mut p)) = find_playlist(&state, &id) else {
        return Err("playlist not found".into());
    };
    let artwork_id =
        scan::store_artwork_file(&state.paths.artwork, Path::new(&source_path)).map_err(err)?;
    p.artwork = Some(artwork_id.clone());
    p.save(&path).map_err(err)?;
    let _ = app.emit("playlists-changed", ());
    Ok(artwork_id)
}

/// Drop a custom image, falling back to the cover of the first track again.
#[tauri::command]
pub fn clear_playlist_artwork(app: AppHandle, state: State<'_, AppState>, id: String) -> Cmd<()> {
    let Some((path, mut p)) = find_playlist(&state, &id) else {
        return Err("playlist not found".into());
    };
    // The file itself is left in the cache: it is content-addressed and may be
    // shared with a track's embedded cover.
    p.artwork = None;
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
    if p.remove_entry(index) {
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
    if p.move_entry(from, to) {
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
                // A per-song override can turn on an atmosphere the global and
                // playlist layers never asked for, so its audio has to be
                // fetched here too — the same as the other two layers do.
                request_missing_beds(&state);
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
    state.cancel_preview();
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
    state.cancel_preview();
    let Some((_, p)) = find_playlist(&state, &id) else {
        return Err("playlist not found".into());
    };
    let context = Context {
        kind: "playlist".into(),
        id: p.id.clone(),
        name: p.name.clone(),
    };
    let context_mixer = p.mixer.clone();
    let shuffle_only = p.shuffle_only;
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

    // Shuffle-only playlists ignore their stored order. A track the user
    // actually clicked still plays first; only what follows is shuffled.
    if shuffle_only {
        let chosen = items.remove(adjusted_start);
        crate::player::shuffle_in_place(&mut items);
        items.insert(0, chosen);
        adjusted_start = 0;
    }

    {
        let mut player = state.player.lock();
        player.context = Some(context);
        player.context_mixer = context_mixer;
        player.set_queue_items(items, adjusted_start);
    }
    start_current(&app, &state)
}

/// Add one playlist entry to the queue without playing the playlist.
///
/// The entry brings that playlist's mixer with it, collapsed playlist-then-track
/// into the queue entry's own layer, so it applies to this one play of this one
/// song and to nothing else. Because the override lives on the queue entry,
/// playback reverts to whatever was in force as soon as the song is over, and
/// nothing is written back to the playlist file.
///
/// Crossfade is deliberately dropped: it spans two playlist entries, and only
/// one of them is being queued.
#[tauri::command]
pub fn queue_playlist_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
    index: usize,
    next: bool,
) -> Cmd<()> {
    state.cancel_preview();
    let Some((_, p)) = find_playlist(&state, &playlist_id) else {
        return Err("playlist not found".into());
    };
    let Some(entry) = p.tracks.get(index) else {
        return Err("that playlist entry no longer exists".into());
    };

    let track = state
        .db
        .resolve(
            entry.musicbrainz_recording_id.as_deref(),
            &entry.artist,
            &entry.title,
            &entry.album,
        )
        .map_err(err)?
        .ok_or_else(|| format!("\"{}\" is not in your library", entry.title))?;

    // When this playlist is already the queue's context its mixer is applied as
    // the context layer, so folding it in again would pin sections that should
    // still fall through.
    let already_playing_it = state
        .player
        .lock()
        .context
        .as_ref()
        .map(|c| c.id == playlist_id)
        .unwrap_or(false);

    let empty = MixerSettings::default();
    let mut mixer = if already_playing_it {
        entry.mixer.clone().unwrap_or_default()
    } else {
        p.mixer
            .as_ref()
            .unwrap_or(&empty)
            .overlay(entry.mixer.as_ref().unwrap_or(&empty))
    };
    mixer.crossfade = None;

    let item = QueueItem {
        track,
        mixer: (mixer != MixerSettings::default()).then_some(mixer),
    };

    let was_empty = {
        let mut player = state.player.lock();
        let empty_queue = player.is_empty();
        if next {
            player.play_next_items(vec![item]);
        } else {
            player.add_to_queue_items(vec![item]);
        }
        empty_queue
    };
    if was_empty {
        return start_current(&app, &state);
    }
    state.engine.cancel_next();
    let _ = app.emit("queue-changed", state.player.lock().view());
    Ok(())
}

// ---------------------------------------------------------------------------
// Home
// ---------------------------------------------------------------------------

/// The three generated mixes, in the order the home page shows them.
pub const MIX_KINDS: [&str; 3] = ["replay", "archive", "discover"];

/// Setting holding the ids of mixes pinned into the sidebar.
const SETTING_PINNED_MIXES: &str = "home.pinnedMixes";

/// A mix as the home page needs to draw it, without its full track list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixSummary {
    pub kind: String,
    pub name: String,
    pub description: String,
    pub track_count: usize,
    /// Covers of the first few songs, for the card's artwork.
    pub artwork_ids: Vec<String>,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeShelves {
    pub mixes: Vec<MixSummary>,
    pub picks: Vec<HomePick>,
    pub recent_playlists: Vec<PlaylistSummary>,
    /// Total counted plays, so the UI can distinguish "no history yet" from
    /// "history exists but this shelf came up empty".
    pub play_total: u32,
}

fn mix_name(kind: &str) -> &'static str {
    match kind {
        "replay" => "Replay Mix",
        "archive" => "Archive Mix",
        _ => "Discover Mix",
    }
}

fn mix_description(kind: &str) -> &'static str {
    match kind {
        "replay" => "Songs you keep coming back to lately",
        "archive" => "You played these a lot once",
        _ => "Corners of your library you have barely touched",
    }
}

fn pinned_mixes(state: &AppState) -> Vec<String> {
    state
        .db
        .get_setting(SETTING_PINNED_MIXES)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn home_shelves(state: State<'_, AppState>) -> Cmd<HomeShelves> {
    let pinned = pinned_mixes(&state);
    let mixes = MIX_KINDS
        .iter()
        .map(|kind| {
            let tracks = state.mix(kind).map_err(err)?;
            Ok(MixSummary {
                kind: (*kind).to_string(),
                name: mix_name(kind).to_string(),
                description: mix_description(kind).to_string(),
                track_count: tracks.len(),
                artwork_ids: tracks
                    .iter()
                    .filter_map(|t| t.artwork_id.clone())
                    .take(4)
                    .collect(),
                pinned: pinned.iter().any(|k| k == kind),
            })
        })
        .collect::<Cmd<Vec<_>>>()?;

    let picks = state
        .db
        .top_picks(6, crate::library::db::now())
        .map_err(err)?;

    // Ordered by when each was last played from, then topped up with the rest
    // so a fresh install still has something on the shelf.
    let all = list_playlists(state.clone())?;
    let recent_ids = state.db.recent_playlist_ids(8).map_err(err)?;
    let mut recent_playlists: Vec<PlaylistSummary> = recent_ids
        .iter()
        .filter_map(|id| all.iter().find(|p| &p.id == id).cloned())
        .collect();
    for playlist in &all {
        if recent_playlists.len() >= 8 {
            break;
        }
        if !recent_playlists.iter().any(|p| p.id == playlist.id) {
            recent_playlists.push(playlist.clone());
        }
    }

    Ok(HomeShelves {
        mixes,
        picks,
        recent_playlists,
        play_total: state.db.counted_play_total().map_err(err)?,
    })
}

/// The songs of one mix, for the mix view and for playing it.
#[tauri::command]
pub fn mix_tracks(state: State<'_, AppState>, kind: String) -> Cmd<Vec<Track>> {
    state.mix(&kind).map_err(err)
}

#[tauri::command]
pub fn play_mix(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: String,
    start_index: Option<usize>,
) -> Cmd<()> {
    let tracks = state.mix(&kind).map_err(err)?;
    if tracks.is_empty() {
        return Ok(());
    }
    state.cancel_preview();
    {
        let mut player = state.player.lock();
        player.context = Some(Context {
            kind: "mix".into(),
            id: kind.clone(),
            name: mix_name(&kind).to_string(),
        });
        player.context_mixer = None;
        player.set_queue(tracks, start_index.unwrap_or(0));
    }
    start_current(&app, &state)
}

/// Build a mix again from current history, discarding the held one.
#[tauri::command]
pub fn refresh_mixes(app: AppHandle, state: State<'_, AppState>) -> Cmd<()> {
    state.clear_mixes();
    let _ = app.emit("home-changed", ());
    Ok(())
}

#[tauri::command]
pub fn set_mix_pinned(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: String,
    pinned: bool,
) -> Cmd<Vec<String>> {
    if !MIX_KINDS.contains(&kind.as_str()) {
        return Err(format!("unknown mix: {kind}"));
    }
    let mut current = pinned_mixes(&state);
    current.retain(|k| k != &kind);
    if pinned {
        current.push(kind);
    }
    // Kept in the page's own order rather than the order they were pinned, so
    // the sidebar does not depend on the sequence the listener clicked in.
    current.sort_by_key(|k| MIX_KINDS.iter().position(|m| m == k).unwrap_or(usize::MAX));

    let raw = serde_json::to_string(&current).map_err(err)?;
    state
        .db
        .set_setting(SETTING_PINNED_MIXES, &raw)
        .map_err(err)?;
    let _ = app.emit("home-changed", ());
    Ok(current)
}

#[tauri::command]
pub fn list_pinned_mixes(state: State<'_, AppState>) -> Cmd<Vec<MixSummary>> {
    let pinned = pinned_mixes(&state);
    pinned
        .iter()
        .map(|kind| {
            let tracks = state.mix(kind).map_err(err)?;
            Ok(MixSummary {
                kind: kind.clone(),
                name: mix_name(kind).to_string(),
                description: mix_description(kind).to_string(),
                track_count: tracks.len(),
                artwork_ids: tracks
                    .iter()
                    .filter_map(|t| t.artwork_id.clone())
                    .take(4)
                    .collect(),
                pinned: true,
            })
        })
        .collect()
}

/// Freeze a generated mix into a real playlist.
///
/// Saving takes a copy: the playlist keeps these songs even after the mix that
/// produced them has moved on, which is the entire reason to save one.
#[tauri::command]
pub fn save_mix_to_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: String,
    playlist_id: Option<String>,
    name: Option<String>,
) -> Cmd<PlaylistSummary> {
    let tracks = state.mix(&kind).map_err(err)?;
    if tracks.is_empty() {
        return Err("this mix has no songs to save".into());
    }

    let (path, mut playlist) = match playlist_id {
        Some(id) => find_playlist(&state, &id).ok_or_else(|| "playlist not found".to_string())?,
        None => {
            let mut created = Playlist {
                name: name.unwrap_or_else(|| mix_name(&kind).to_string()),
                ..Default::default()
            };
            created.description = format!("Saved from your {}", mix_name(&kind));
            let path = state
                .paths
                .playlists
                .join(playlist::file_name_for(&created.name, &created.id));
            (path, created)
        }
    };

    for track in &tracks {
        playlist.add_track(track);
    }
    playlist.save(&path).map_err(err)?;
    let _ = app.emit("playlists-changed", ());

    Ok(PlaylistSummary {
        id: playlist.id,
        name: playlist.name,
        description: playlist.description,
        track_count: playlist.tracks.len(),
        artwork: playlist.artwork,
        has_mixer: playlist.mixer.is_some(),
        has_master_mix: playlist.master_mix.is_some(),
        master_mix_enabled: playlist.master_mix.as_ref().is_some_and(|m| m.enabled),
        shuffle_only: playlist.shuffle_only,
        path: path.display().to_string(),
    })
}

// ---------------------------------------------------------------------------
// Listening history
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn listening_history(state: State<'_, AppState>, limit: Option<usize>) -> Cmd<Vec<PlayRecord>> {
    state.db.recent_plays(limit.unwrap_or(200)).map_err(err)
}

/// Erase listening history.
///
/// Also drops the held mixes, since every one of them is derived from the
/// history that has just been deleted — leaving them in place would keep
/// serving recommendations built from data the listener asked to be rid of.
#[tauri::command]
pub fn clear_listening_history(app: AppHandle, state: State<'_, AppState>) -> Cmd<()> {
    state.db.clear_history().map_err(err)?;
    state.plays.reset_progress(None);
    state.clear_mixes();
    let _ = app.emit("home-changed", ());
    Ok(())
}

#[tauri::command]
pub fn clear_listening_history_for_song(
    app: AppHandle,
    state: State<'_, AppState>,
    song_id: String,
) -> Cmd<()> {
    state.db.clear_history_for_song(&song_id).map_err(err)?;
    state.plays.reset_progress(Some(&song_id));
    state.clear_mixes();
    let _ = app.emit("home-changed", ());
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn relink_identity_matches(candidate: &Track, existing: &TrackFile) -> bool {
    if matches!(
        (candidate.disc_number, existing.disc_number),
        (Some(a), Some(b)) if a != b
    ) || matches!(
        (candidate.track_number, existing.track_number),
        (Some(a), Some(b)) if a != b
    ) {
        return false;
    }

    let album = normalise(&candidate.album);
    if album != normalise(&existing.album) {
        return false;
    }
    let same_mbid = match (
        candidate.musicbrainz_recording_id.as_deref().map(str::trim),
        existing.musicbrainz_recording_id.as_deref().map(str::trim),
    ) {
        (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => a.eq_ignore_ascii_case(b),
        _ => false,
    };
    if same_mbid {
        return true;
    }
    !album.is_empty()
        && normalise(&candidate.artist) == normalise(&existing.artist)
        && normalise(&candidate.title) == normalise(&existing.title)
        && candidate.duration_secs.is_finite()
        && existing.duration_secs.is_finite()
        && (candidate.duration_secs - existing.duration_secs).abs() <= 2.0
}

fn unique_restored_path(folder: &Path, source: &Path) -> anyhow::Result<PathBuf> {
    let file_name = source
        .file_name()
        .ok_or_else(|| anyhow!("selected path has no file name"))?;
    let first = folder.join(file_name);
    if !first.exists() {
        return Ok(first);
    }

    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("selected file name is not valid Unicode"))?;
    let extension = source.extension().and_then(|value| value.to_str());
    for suffix in 1u32.. {
        let name = match extension {
            Some(extension) => format!("{stem} ({suffix}).{extension}"),
            None => format!("{stem} ({suffix})"),
        };
        let candidate = folder.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    unreachable!("the numeric suffix space is finite only after exhausting u32")
}

fn reject_indexed_target(state: &AppState, file_id: &str, target: &Path) -> anyhow::Result<()> {
    let location = target.display().to_string();
    if let Some(indexed) = state.db.file_by_location(scan::SOURCE_LOCAL, &location)? {
        if indexed.id != file_id {
            bail!("target path is already indexed by file {}", indexed.id);
        }
    }

    if let Ok(target) = target.canonicalize() {
        for location in state.db.locations(scan::SOURCE_LOCAL)? {
            if location == target.display().to_string() {
                continue;
            }
            let path = Path::new(&location);
            if path.canonicalize().ok().as_deref() == Some(target.as_path()) {
                if let Some(indexed) = state.db.file_by_location(scan::SOURCE_LOCAL, &location)? {
                    if indexed.id != file_id {
                        bail!("target path is already indexed by file {}", indexed.id);
                    }
                }
            }
        }
    }
    Ok(())
}

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

// ---------------------------------------------------------------------------
// Master mix
// ---------------------------------------------------------------------------
//
// The timeline behind a playlist's master mixer. Editing is coarse on purpose:
// the webview owns the arrangement while the modal is open and hands the whole
// document back, rather than there being a command per drag. That keeps undo,
// multi-block moves and the blade tool entirely on the side that has the mouse,
// and leaves exactly one place — `MasterMix::normalise` — where anything the
// interface sends is checked before it can reach the audio engine.

/// One playlist entry, as the timeline needs to know it: enough to label a
/// block, bound a trim, and grey out a song that is not in this library.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixEntry {
    pub index: usize,
    pub title: String,
    pub artist: String,
    pub artwork_id: Option<String>,
    /// The song's full length, which is the longest a block of it can be.
    pub duration_secs: f64,
    /// False when nothing in this library matched. Its blocks still draw, so
    /// the arrangement is visible, but they are silent.
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterMixView {
    pub playlist_id: String,
    pub playlist_name: String,
    pub mix: crate::master_mix::MasterMix,
    pub entries: Vec<MixEntry>,
    pub duration_secs: f64,
    /// False when this is the default arrangement offered for a playlist that
    /// has never been mixed — nothing is written to the file until an edit.
    pub saved: bool,
}

/// The mix for a playlist, building the default arrangement if there is none.
#[tauri::command]
pub fn master_mix(state: State<'_, AppState>, playlist_id: String) -> Cmd<MasterMixView> {
    let Some((_, p)) = find_playlist(&state, &playlist_id) else {
        return Err("playlist not found".into());
    };
    view_of(&state, p).map_err(err)
}

fn view_of(state: &AppState, p: Playlist) -> anyhow::Result<MasterMixView> {
    let saved = p.master_mix.is_some();
    let playlist_id = p.id.clone();
    let playlist_name = p.name.clone();
    let mut mix = p.master_mix_or_default(&state.db)?;
    mix.normalise(p.tracks.len());

    let resolved = p.resolve(&state.db)?;
    let entries = resolved
        .items
        .iter()
        .map(|item| MixEntry {
            index: item.index,
            title: item
                .track
                .as_ref()
                .map(|t| t.title.clone())
                .unwrap_or_else(|| item.entry.title.clone()),
            artist: item
                .track
                .as_ref()
                .map(|t| t.artist.clone())
                .unwrap_or_else(|| item.entry.artist.clone()),
            artwork_id: item.track.as_ref().and_then(|t| t.artwork_id.clone()),
            duration_secs: item
                .track
                .as_ref()
                .map(|t| t.duration_secs)
                .filter(|d| *d > 0.0)
                .unwrap_or(item.entry.duration_secs),
            available: item.track.is_some(),
        })
        .collect();

    Ok(MasterMixView {
        playlist_id,
        playlist_name,
        duration_secs: mix.duration_secs(),
        mix,
        entries,
        saved,
    })
}

/// Replace the whole arrangement.
///
/// Validated here and nowhere else: everything downstream — the renderer, the
/// engine — is entitled to assume a mix it is handed is playable.
#[tauri::command]
pub fn set_master_mix(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
    mix: crate::master_mix::MasterMix,
) -> Cmd<MasterMixView> {
    let Some((path, mut p)) = find_playlist(&state, &playlist_id) else {
        return Err("playlist not found".into());
    };
    let mut mix = mix;
    mix.normalise(p.tracks.len());
    mix.touch();
    p.master_mix = Some(mix);
    p.save(&path).map_err(err)?;
    let _ = app.emit("playlists-changed", ());
    view_of(&state, p).map_err(err)
}

/// Turn the master mix on or off without discarding it.
///
/// Off, the playlist plays as an ordinary list again; the arrangement stays in
/// the file so it is still there when it is switched back on.
#[tauri::command]
pub fn set_master_mix_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
    enabled: bool,
) -> Cmd<MasterMixView> {
    let Some((path, mut p)) = find_playlist(&state, &playlist_id) else {
        return Err("playlist not found".into());
    };
    let mut mix = p.master_mix_or_default(&state.db).map_err(err)?;
    mix.normalise(p.tracks.len());
    mix.enabled = enabled;
    mix.touch();
    p.master_mix = Some(mix);
    p.save(&path).map_err(err)?;
    let _ = app.emit("playlists-changed", ());
    view_of(&state, p).map_err(err)
}

/// Throw the arrangement away and start again from the playlist's own order.
#[tauri::command]
pub fn reset_master_mix(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
) -> Cmd<MasterMixView> {
    let Some((path, mut p)) = find_playlist(&state, &playlist_id) else {
        return Err("playlist not found".into());
    };
    p.master_mix = None;
    let mut fresh = p.master_mix_or_default(&state.db).map_err(err)?;
    fresh.normalise(p.tracks.len());
    p.master_mix = Some(fresh);
    p.save(&path).map_err(err)?;
    let _ = app.emit("playlists-changed", ());
    view_of(&state, p).map_err(err)
}

/// Waveform peaks for one playlist entry.
///
/// One waveform per *song*, not per block: splitting a block or dragging its
/// edges only changes which slice of the same waveform is drawn, so there is
/// nothing to recompute. Decoding is slow, so this runs on a blocking thread
/// and its result is cached on disk between runs.
#[tauri::command]
pub async fn entry_waveform(
    state: State<'_, AppState>,
    playlist_id: String,
    index: usize,
) -> Cmd<crate::audio::peaks::Waveform> {
    let Some((_, p)) = find_playlist(&state, &playlist_id) else {
        return Err("playlist not found".into());
    };
    let resolved = p.resolve(&state.db).map_err(err)?;
    let Some(item) = resolved.items.get(index) else {
        return Err("that playlist entry no longer exists".into());
    };
    let Some(track) = item.track.as_ref() else {
        // Not an error: a song this library does not have simply has no shape
        // to draw, and the block is shown as an outline instead.
        return Ok(crate::audio::peaks::Waveform {
            peaks: Vec::new(),
            peaks_per_sec: crate::audio::peaks::PEAKS_PER_SEC,
            duration_secs: item.entry.duration_secs,
        });
    };
    let files = state.playback_files(&track.id).map_err(err)?;
    let Some(file) = files.into_iter().next() else {
        return Err("that song has no readable file".into());
    };
    let cache = state.paths.waveforms.clone();

    tauri::async_runtime::spawn_blocking(move || {
        crate::audio::peaks::waveform(Path::new(&file.location), &cache)
    })
    .await
    .map_err(err)?
    .map_err(err)
}

/// Audition the arrangement, from `position_secs`.
///
/// The mix takes over playback entirely while the editor is open: the engine
/// plays a timeline instead of a queue entry, and the queue is left exactly as
/// it was so closing the editor can put it back.
#[tauri::command]
pub fn play_master_mix(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
    mix: Option<crate::master_mix::MasterMix>,
    position_secs: Option<f64>,
) -> Cmd<f64> {
    let Some((path, p)) = find_playlist(&state, &playlist_id) else {
        return Err("playlist not found".into());
    };
    // Unsaved edits are auditioned as they are: the whole point of a preview
    // is to hear the move you just made before deciding to keep it.
    let mut mix = match mix {
        Some(mix) => mix,
        None => p.master_mix_or_default(&state.db).map_err(err)?,
    };
    mix.normalise(p.tracks.len());

    let plan = build_plan(&state, &p, &mix, &path).map_err(err)?;
    if plan.is_empty() {
        return Err("there is nothing in this mix to play".into());
    }
    let duration = plan.duration_secs;

    // Ends any duplicate-file audition and stops the queue's own playback
    // before the timeline replaces it.
    state.cancel_preview();
    *state.master_mix_preview.lock() = Some(playlist_id);

    let rate = state.engine.device_sample_rate();
    let mut source = crate::audio::timeline::TimelineSource::new(plan, rate);
    if let Some(position) = position_secs.filter(|p| *p > 0.0) {
        source.seek(position).map_err(err)?;
    }
    state.engine.load_timeline(source).map_err(err)?;
    state.engine.play();
    let _ = app.emit("playing-changed", true);
    Ok(duration)
}

/// Stop auditioning and hand the engine back to the queue.
#[tauri::command]
pub fn stop_master_mix(app: AppHandle, state: State<'_, AppState>) -> Cmd<()> {
    if state.master_mix_preview.lock().take().is_none() {
        return Ok(());
    }
    state.engine.clear();
    let _ = app.emit("playing-changed", false);
    Ok(())
}

/// Resolve a mix against this library: every audible block paired with a file
/// on this machine and the mixer cascade that applies to it.
///
/// A block whose song is not here is dropped rather than failing the whole
/// mix — the roadmap's open question, answered the way that keeps a shared
/// playlist usable: you hear what you have.
fn build_plan(
    state: &AppState,
    p: &Playlist,
    mix: &crate::master_mix::MasterMix,
    playlist_path: &Path,
) -> anyhow::Result<crate::audio::timeline::Plan> {
    use crate::master_mix::BlockSource;

    let resolved = p.clone().resolve(&state.db)?;
    let global = state.player.lock().global_mixer.clone();
    let playlist_layer = p.mixer.clone().unwrap_or_default();
    let assets = Playlist::assets_dir(playlist_path);

    let mut blocks = Vec::new();
    for lane in mix.lanes.iter() {
        if !mix.lane_audible(lane) {
            continue;
        }
        let lane_gain = db_to_gain(lane.gain_db);
        for block in lane.blocks.iter() {
            let (path, entry_layer, track_gain_db) = match &block.source {
                BlockSource::Entry { index } => {
                    let Some(item) = resolved.items.get(*index) else {
                        continue;
                    };
                    let Some(track) = item.track.as_ref() else {
                        continue;
                    };
                    let Some(file) = state.playback_files(&track.id)?.into_iter().next() else {
                        continue;
                    };
                    (
                        PathBuf::from(file.location),
                        item.entry.mixer.clone().unwrap_or_default(),
                        file.gain_db.unwrap_or(0.0),
                    )
                }
                BlockSource::Asset { file } => {
                    let path = assets.join(file);
                    if !path.is_file() {
                        continue;
                    }
                    // An imported file is the user's own audio, already at the
                    // level they chose. Nothing to normalise against.
                    (path, MixerSettings::default(), 0.0)
                }
            };

            let block_layer = block.mixer.clone().unwrap_or_default();
            let settings = MixerSettings::resolve(&[
                &global,
                &playlist_layer,
                &entry_layer,
                &block_layer,
            ]);
            blocks.push(crate::audio::timeline::PlanBlock {
                path,
                block: block.clone(),
                lane_gain,
                settings: std::sync::Arc::new(settings),
                track_gain_db,
            });
        }
    }
    Ok(crate::audio::timeline::Plan::new(blocks))
}

fn db_to_gain(db: f32) -> f32 {
    if db <= crate::master_mix::SILENT_DB {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

#[cfg(test)]
mod duplicate_file_tests {
    use super::*;

    fn candidate() -> Track {
        Track {
            title: " Song ".into(),
            artist: "ARTIST".into(),
            album: "Album".into(),
            duration_secs: 181.9,
            track_number: Some(2),
            disc_number: Some(1),
            ..Default::default()
        }
    }

    fn existing() -> TrackFile {
        TrackFile {
            title: "song".into(),
            artist: "Artist".into(),
            album: "album".into(),
            duration_secs: 180.0,
            track_number: Some(2),
            disc_number: Some(1),
            ..Default::default()
        }
    }

    #[test]
    fn relink_identity_requires_album_duration_and_positions() {
        assert!(relink_identity_matches(&candidate(), &existing()));

        let mut other_album = candidate();
        other_album.album = "Other".into();
        assert!(!relink_identity_matches(&other_album, &existing()));

        let mut too_long = candidate();
        too_long.duration_secs = 182.01;
        assert!(!relink_identity_matches(&too_long, &existing()));

        let mut wrong_position = candidate();
        wrong_position.track_number = Some(3);
        assert!(!relink_identity_matches(&wrong_position, &existing()));
    }

    #[test]
    fn equal_nonempty_mbid_allows_two_albumless_versions() {
        let mut candidate = candidate();
        candidate.album.clear();
        candidate.title = "Different".into();
        candidate.musicbrainz_recording_id = Some("MBID".into());
        let mut existing = existing();
        existing.album.clear();
        existing.title = "Other".into();
        existing.musicbrainz_recording_id = Some("mbid".into());
        assert!(relink_identity_matches(&candidate, &existing));

        existing.album = "Album".into();
        assert!(!relink_identity_matches(&candidate, &existing));
    }

    #[test]
    fn restored_collisions_add_a_suffix_before_the_extension() {
        let root = std::env::temp_dir().join(format!(
            "pnm-restore-test-{}",
            stable_id("d", &format!("{:?}", std::time::Instant::now()))
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("song.flac"), b"one").unwrap();
        std::fs::write(root.join("song (1).flac"), b"two").unwrap();

        let target = unique_restored_path(&root, Path::new("/outside/song.flac")).unwrap();
        assert_eq!(target.file_name().unwrap(), "song (2).flac");
        std::fs::remove_dir_all(root).unwrap();
    }
}
