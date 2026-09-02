//! Application state shared by every Tauri command.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use crossbeam_channel::Receiver;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::audio::crossfade::CrossfadeSettings;
use crate::audio::{AudioEngine, EngineEvent};
use crate::library::model::TrackFile;
use crate::library::Db;
use crate::player::Player;

/// Where everything lives on disk. All under the app's data directory so the
/// whole app can be reset by deleting one folder.
#[derive(Debug, Clone)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub db: PathBuf,
    pub artwork: PathBuf,
    pub playlists: PathBuf,
    /// User-imported ambience files, kept under app data.
    pub filters: PathBuf,
    /// Packaged ambience files, if they are available in this runtime.
    pub bundled_ambience: Option<PathBuf>,
    pub presets: PathBuf,
}

impl Paths {
    pub fn new(data_dir: &Path, bundled_ambience: Option<PathBuf>) -> Self {
        Paths {
            data_dir: data_dir.to_path_buf(),
            db: data_dir.join("library.db"),
            artwork: data_dir.join("artwork"),
            playlists: data_dir.join("playlists"),
            filters: data_dir.join("filters"),
            bundled_ambience,
            presets: crate::presets::presets_path(data_dir),
        }
    }

    pub fn ensure(&self) -> Result<()> {
        for dir in [
            &self.data_dir,
            &self.artwork,
            &self.playlists,
            &self.filters,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct PreviewSession {
    pub original: Option<PreviewOriginal>,
}

#[derive(Debug)]
pub(crate) struct PreviewOriginal {
    pub path: PathBuf,
    pub gain_db: f32,
    pub position_secs: f64,
    pub playing: bool,
}

pub struct AppState {
    pub db: Db,
    pub engine: AudioEngine,
    pub player: Mutex<Player>,
    pub paths: Paths,
    /// The first preview captures normal playback here; later previews retain it.
    pub(crate) preview: Mutex<Option<PreviewSession>>,
    /// Handed to the background pump in `lib.rs`.
    pub engine_events: Mutex<Option<Receiver<EngineEvent>>>,
    /// Guards MusicBrainz lookups so only one runs at a time.
    pub metadata: Mutex<Option<Arc<crate::library::metadata::MusicBrainz>>>,
    /// System transport controls (MPRIS / macOS Now Playing).
    pub media: crate::media::MediaBridge,
    /// Generates cover-art thumbnails off the request path.
    pub thumbnails: crate::library::thumbnail::Thumbnailer,
    /// Measures what is actually being listened to, for the home page.
    pub plays: crate::history::PlayTracker,
    /// Generated mixes, held for the life of the process. See
    /// [`AppState::mix`].
    pub mixes: Mutex<std::collections::HashMap<String, Vec<String>>>,
}

/// Key under which the global mixer is persisted.
pub const SETTING_GLOBAL_MIXER: &str = "mixer.global";
pub const SETTING_VOLUME: &str = "player.volume";
pub const SETTING_REPEAT: &str = "player.repeat";
pub const SETTING_SHUFFLE: &str = "player.shuffle";
/// Persisted visual, playback, and recommendation preferences.
pub const SETTING_APP_PREFERENCES: &str = "app.preferences";
/// Window geometry as the user left it, restored on the next launch.
pub const SETTING_WINDOW: &str = "window.geometry";
/// Legacy location for crossfade settings. On startup its value is migrated
/// into [`SETTING_GLOBAL_MIXER`] when that mixer has no crossfade section.
pub const SETTING_CROSSFADE: &str = "crossfade.global";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppPreferences {
    pub theme: String,
    pub accent: String,
    pub fade_mode: String,
    /// Let reverb and delay tails ring out after a pause.
    pub keep_reverb_on_pause: bool,
    /// Output device to use, by name. Empty means the system default.
    pub output_device: String,
    pub mix_length: usize,
    pub replay_days: u32,
    pub replay_min_plays: u32,
    pub archive_days: u32,
    pub archive_min_plays: u32,
    pub discover_max_plays: u32,
    pub hidden_built_in_preset_ids: Vec<String>,
    pub hidden_built_in_filter_ids: Vec<String>,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            accent: "#f56300".into(),
            fade_mode: "off".into(),
            keep_reverb_on_pause: false,
            output_device: String::new(),
            mix_length: 50,
            replay_days: 30,
            replay_min_plays: 2,
            archive_days: 60,
            archive_min_plays: 3,
            discover_max_plays: 3,
            hidden_built_in_preset_ids: Vec::new(),
            hidden_built_in_filter_ids: Vec::new(),
        }
    }
}

impl AppPreferences {
    /// Normalise values supplied by the webview before they reach persistence,
    /// SQL limits, or the audio callback.
    pub fn validated(mut self) -> Self {
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            self.theme = "system".into();
        }
        if !is_hex_colour(&self.accent) {
            self.accent = Self::default().accent;
        }
        if !matches!(self.fade_mode.as_str(), "off" | "play" | "pause" | "both") {
            self.fade_mode = Self::default().fade_mode;
        }
        self.mix_length = self.mix_length.clamp(10, 200);
        self.replay_days = self.replay_days.clamp(1, 3_650);
        self.replay_min_plays = self.replay_min_plays.clamp(1, 100);
        self.archive_days = self.archive_days.clamp(1, 3_650);
        self.archive_min_plays = self.archive_min_plays.clamp(1, 100);
        self.discover_max_plays = self.discover_max_plays.clamp(1, 100);
        deduplicate_non_empty(&mut self.hidden_built_in_preset_ids);
        deduplicate_non_empty(&mut self.hidden_built_in_filter_ids);
        self
    }

    pub fn recommendation_parameters_differ(&self, other: &Self) -> bool {
        self.mix_length != other.mix_length
            || self.replay_days != other.replay_days
            || self.replay_min_plays != other.replay_min_plays
            || self.archive_days != other.archive_days
            || self.archive_min_plays != other.archive_min_plays
            || self.discover_max_plays != other.discover_max_plays
    }
}

fn deduplicate_non_empty(ids: &mut Vec<String>) {
    let mut seen = HashSet::new();
    ids.retain(|id| !id.trim().is_empty() && seen.insert(id.clone()));
}

fn is_hex_colour(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

pub fn load_app_preferences(db: &Db) -> AppPreferences {
    db.get_setting(SETTING_APP_PREFERENCES)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<AppPreferences>(&raw).ok())
        .unwrap_or_default()
        .validated()
}

impl AppState {
    pub fn new(data_dir: &Path, bundled_ambience: Option<PathBuf>) -> Result<Self> {
        let paths = Paths::new(data_dir, bundled_ambience);
        paths.ensure()?;

        let db = Db::open(&paths.db)?;
        let (tx, rx) = crossbeam_channel::unbounded();
        let engine = AudioEngine::new(tx)?;

        let mut player = Player::new();

        // Restore what the user left set last time.
        if let Ok(Some(raw)) = db.get_setting(SETTING_GLOBAL_MIXER) {
            match serde_json::from_str(&raw) {
                Ok(mixer) => player.global_mixer = mixer,
                Err(e) => eprintln!("state: ignoring unreadable saved mixer: {e}"),
            }
        }
        // Migrate the former standalone crossfade setting into the global
        // mixer once. A saved mixer section always takes precedence.
        if player.global_mixer.crossfade.is_none() {
            if let Ok(Some(raw)) = db.get_setting(SETTING_CROSSFADE) {
                match serde_json::from_str::<CrossfadeSettings>(&raw) {
                    Ok(crossfade) => {
                        player.global_mixer.crossfade = Some(crossfade);
                        if let Ok(raw) = serde_json::to_string(&player.global_mixer) {
                            let _ = db.set_setting(SETTING_GLOBAL_MIXER, &raw);
                        }
                    }
                    Err(e) => {
                        eprintln!("state: ignoring unreadable legacy crossfade settings: {e}")
                    }
                }
            }
        }

        if let Ok(Some(raw)) = db.get_setting(SETTING_VOLUME) {
            if let Ok(v) = raw.parse::<f32>() {
                engine.set_volume(v);
            }
        }
        if let Ok(Some(raw)) = db.get_setting(SETTING_SHUFFLE) {
            player.set_shuffle(raw == "true");
        }
        if let Ok(Some(raw)) = db.get_setting(SETTING_REPEAT) {
            if let Ok(mode) = serde_json::from_str(&raw) {
                player.set_repeat(mode);
            }
        }
        let preferences = load_app_preferences(&db);
        engine.set_fade_mode(&preferences.fade_mode);
        engine.set_keep_tail(preferences.keep_reverb_on_pause);
        // A device that has since been unplugged simply falls back to the
        // system default rather than leaving the app with no audio.
        if !preferences.output_device.is_empty() {
            if let Err(error) = engine.set_output_device(Some(&preferences.output_device)) {
                eprintln!("audio: saved output device unavailable, using the default: {error}");
            }
        }

        let effective_mixer = player.effective_mixer();
        engine.set_settings(effective_mixer.clone());
        engine.set_crossfade(effective_mixer.crossfade);

        let thumbnails = crate::library::thumbnail::Thumbnailer::new(paths.artwork.clone());

        Ok(AppState {
            db,
            engine,
            player: Mutex::new(player),
            paths,
            preview: Mutex::new(None),
            engine_events: Mutex::new(Some(rx)),
            metadata: Mutex::new(None),
            media: crate::media::MediaBridge::new(),
            thumbnails,
            plays: crate::history::PlayTracker::new(),
            mixes: Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Start following `song_id`, writing out whatever was playing before it.
    ///
    /// Every route by which one song replaces another funnels through here, so
    /// a play is recorded exactly once however the change came about: the
    /// track ending, the listener skipping, or a crossfade promoting the next
    /// voice on the audio thread's own schedule.
    pub fn begin_play(&self, song_id: &str, duration_secs: f64) {
        let (kind, id) = {
            let player = self.player.lock();
            match player.context.as_ref() {
                Some(context) => (Some(context.kind.clone()), Some(context.id.clone())),
                None => (None, None),
            }
        };
        let finished =
            self.plays
                .begin(song_id, duration_secs, kind, id, crate::library::db::now());
        self.store_play(finished);
    }

    /// Stop following the current song, writing out its play.
    pub fn end_play(&self) {
        let finished = self.plays.finish(crate::library::db::now());
        self.store_play(finished);
    }

    /// The songs of a generated mix, built on first use and then held.
    ///
    /// Mixes are deliberately *not* recomputed per request. Two of the three
    /// are partly random, and all three are derived from listening history
    /// that playing the mix immediately changes — so a live query would
    /// reshuffle a mix while it was being listened to, and could drop the
    /// playing song out of the queue behind the listener. Holding them for the
    /// session means a mix is a fixed thing you can play, pin and save;
    /// [`Self::clear_mixes`] is the deliberate way to get a new one.
    pub fn mix(&self, kind: &str) -> Result<Vec<crate::library::Track>> {
        let cached = self.mixes.lock().get(kind).cloned();
        let ids = match cached {
            Some(ids) => ids,
            None => {
                let ids: Vec<String> = self
                    .generate_mix(kind)?
                    .into_iter()
                    .map(|track| track.id)
                    .collect();
                self.mixes.lock().insert(kind.to_string(), ids.clone());
                ids
            }
        };

        // Resolved fresh each time rather than cached as whole tracks, so a
        // rescan's new metadata and artwork show up without rebuilding the mix.
        let mut tracks = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(track) = self.db.get_track(&id)? {
                tracks.push(track);
            }
        }
        Ok(tracks)
    }

    fn generate_mix(&self, kind: &str) -> Result<Vec<crate::library::Track>> {
        const DAY: i64 = 86_400;
        let preferences = load_app_preferences(&self.db);
        let now = crate::library::db::now();
        match kind {
            "replay" => self.db.replay_mix(
                preferences.mix_length,
                now - i64::from(preferences.replay_days) * DAY,
                preferences.replay_min_plays,
            ),
            "archive" => self.db.archive_mix(
                preferences.mix_length,
                now - i64::from(preferences.archive_days) * DAY,
                preferences.archive_min_plays,
            ),
            "discover" => self
                .db
                .discover_mix(preferences.mix_length, preferences.discover_max_plays),
            other => Err(anyhow::anyhow!("unknown mix: {other}")),
        }
    }

    /// Forget the generated mixes so the next request builds them again.
    pub fn clear_mixes(&self) {
        self.mixes.lock().clear();
    }

    fn store_play(&self, play: Option<crate::library::model::Play>) {
        let Some(play) = play else { return };
        // History is a nicety; failing to write it must never interrupt
        // playback, so this reports and moves on.
        if let Err(error) = self.db.record_play(&play) {
            eprintln!("history: could not record a play: {error}");
        }
    }

    /// Recompute the cascade and hand it to the audio engine. Called after any
    /// change to the global, playlist or track layers, and on track change.
    pub fn sync_mixer(&self) {
        self.sync_mixer_settings();
        self.engine
            .set_track_gain_db(self.player.lock().current_gain_db());
    }

    /// Sync only effects and crossfade. A promoted crossfade voice already owns
    /// the gain of the physical file that was actually opened.
    pub(crate) fn sync_mixer_settings(&self) {
        let effective_mixer = self.player.lock().effective_mixer();
        self.engine.set_settings(effective_mixer.clone());
        self.engine.set_crossfade(effective_mixer.crossfade);
    }

    /// Available files with the effective (preferred, when available) version
    /// first, followed by the remaining automatic quality ranking.
    pub(crate) fn playback_files(&self, song_id: &str) -> Result<Vec<TrackFile>> {
        let effective = self.db.effective_file_for_song(song_id)?;
        let mut files = self.db.ranked_available_files(song_id)?;
        if let Some(effective) = effective {
            files.retain(|file| file.id != effective.id);
            files.insert(0, effective);
        }
        Ok(files)
    }

    pub(crate) fn is_previewing(&self) -> bool {
        self.preview.lock().is_some()
    }

    /// Load one exact physical version while preserving the first normal
    /// playback snapshot for the whole preview session.
    pub(crate) fn preview_file(&self, file: &TrackFile) -> Result<()> {
        let mut preview = self.preview.lock();
        let first = preview.is_none();
        if first {
            let snapshot = self.engine.snapshot();
            let original = if snapshot.stream.is_some() {
                self.player
                    .lock()
                    .current()
                    .map(|track| track.id.clone())
                    .and_then(|song_id| self.db.effective_file_for_song(&song_id).ok().flatten())
                    .map(|file| PreviewOriginal {
                        path: PathBuf::from(file.location),
                        gain_db: file.gain_db.unwrap_or(0.0),
                        position_secs: snapshot.position_secs,
                        playing: snapshot.playing,
                    })
            } else {
                None
            };
            *preview = Some(PreviewSession { original });
        }
        drop(preview);

        self.engine.cancel_next();
        if let Err(error) = self.engine.load(
            PathBuf::from(&file.location),
            0.0,
            file.gain_db.unwrap_or(0.0),
        ) {
            if first {
                let _ = self.stop_preview();
            }
            return Err(error);
        }
        self.engine.play();
        Ok(())
    }

    /// End preview and restore the exact normal file, position and play state
    /// captured by the first preview. With no prior stream, leave the engine empty.
    pub(crate) fn stop_preview(&self) -> Result<bool> {
        let Some(preview) = self.preview.lock().take() else {
            return Ok(false);
        };
        self.engine.cancel_next();
        match preview.original {
            Some(original) => {
                if let Err(error) =
                    self.engine
                        .load(original.path, original.position_secs, original.gain_db)
                {
                    self.engine.clear();
                    return Err(error);
                }
                if original.playing {
                    self.engine.play();
                } else {
                    self.engine.pause();
                }
            }
            None => self.engine.clear(),
        }
        Ok(true)
    }

    /// End preview without restoring it. Normal transport commands call this
    /// before loading or manipulating their intended logical queue item.
    pub(crate) fn cancel_preview(&self) -> Option<PreviewSession> {
        let preview = self.preview.lock().take();
        if preview.is_some() {
            self.engine.cancel_next();
            self.engine.clear();
        }
        preview
    }

    /// A MusicBrainz client, created on first use so the app starts up without
    /// building an HTTP stack it may never need.
    pub fn metadata_provider(&self) -> Result<Arc<crate::library::metadata::MusicBrainz>> {
        let mut slot = self.metadata.lock();
        if let Some(existing) = slot.as_ref() {
            return Ok(Arc::clone(existing));
        }
        let provider = Arc::new(crate::library::metadata::MusicBrainz::new(
            &self.paths.artwork,
        )?);
        *slot = Some(Arc::clone(&provider));
        Ok(provider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_use_frontend_defaults_and_camel_case() {
        let preferences = AppPreferences::default();
        let json = serde_json::to_value(&preferences).unwrap();
        assert_eq!(json["theme"], "system");
        assert_eq!(json["accent"], "#f56300");
        assert_eq!(json["fadeMode"], "off");
        assert!(json.get("fadePausePlay").is_none());
        assert_eq!(json["mixLength"], 50);
        assert_eq!(json["discoverMaxPlays"], 3);
        assert_eq!(json["hiddenBuiltInPresetIds"], serde_json::json!([]));
        assert_eq!(json["hiddenBuiltInFilterIds"], serde_json::json!([]));
    }

    #[test]
    fn invalid_preferences_are_safely_normalised() {
        let preferences = AppPreferences {
            theme: "sepia".into(),
            accent: "#not-a-colour".into(),
            fade_mode: "sometimes".into(),
            mix_length: usize::MAX,
            replay_days: 0,
            replay_min_plays: 0,
            archive_days: u32::MAX,
            archive_min_plays: u32::MAX,
            discover_max_plays: 0,
            hidden_built_in_preset_ids: vec![
                "flat".into(),
                String::new(),
                "flat".into(),
                "lofi-study".into(),
                "   ".into(),
            ],
            hidden_built_in_filter_ids: vec!["rain".into(), "rain".into(), "ocean".into()],
            ..AppPreferences::default()
        }
        .validated();

        assert_eq!(preferences.theme, "system");
        assert_eq!(preferences.accent, "#f56300");
        assert_eq!(preferences.fade_mode, "off");
        assert_eq!(preferences.mix_length, 200);
        assert_eq!(preferences.replay_days, 1);
        assert_eq!(preferences.replay_min_plays, 1);
        assert_eq!(preferences.archive_days, 3_650);
        assert_eq!(preferences.archive_min_plays, 100);
        assert_eq!(preferences.discover_max_plays, 1);
        assert_eq!(
            preferences.hidden_built_in_preset_ids,
            ["flat", "lofi-study"]
        );
        assert_eq!(preferences.hidden_built_in_filter_ids, ["rain", "ocean"]);
    }

    #[test]
    fn saved_preferences_fill_new_fields_from_defaults() {
        let preferences: AppPreferences =
            serde_json::from_str(r#"{"theme":"dark","fadePausePlay":true}"#).unwrap();
        assert_eq!(preferences.theme, "dark");
        assert_eq!(preferences.accent, "#f56300");
        assert_eq!(preferences.fade_mode, "off");
        assert_eq!(preferences.mix_length, 50);
        assert!(preferences.hidden_built_in_preset_ids.is_empty());
        assert!(preferences.hidden_built_in_filter_ids.is_empty());
    }

    #[test]
    fn every_supported_fade_mode_survives_validation() {
        for mode in ["off", "play", "pause", "both"] {
            let preferences = AppPreferences {
                fade_mode: mode.into(),
                ..AppPreferences::default()
            }
            .validated();
            assert_eq!(preferences.fade_mode, mode);
        }
    }
}
