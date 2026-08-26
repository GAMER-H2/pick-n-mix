//! Application state shared by every Tauri command.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use crossbeam_channel::Receiver;
use parking_lot::Mutex;

use crate::audio::crossfade::CrossfadeSettings;
use crate::audio::{AudioEngine, EngineEvent};
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
    pub filters: PathBuf,
    pub presets: PathBuf,
}

impl Paths {
    pub fn new(data_dir: &Path) -> Self {
        Paths {
            data_dir: data_dir.to_path_buf(),
            db: data_dir.join("library.db"),
            artwork: data_dir.join("artwork"),
            playlists: data_dir.join("playlists"),
            filters: data_dir.join("filters"),
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

pub struct AppState {
    pub db: Db,
    pub engine: AudioEngine,
    pub player: Mutex<Player>,
    pub paths: Paths,
    /// Handed to the background pump in `lib.rs`.
    pub engine_events: Mutex<Option<Receiver<EngineEvent>>>,
    /// Guards MusicBrainz lookups so only one runs at a time.
    pub metadata: Mutex<Option<Arc<crate::library::metadata::MusicBrainz>>>,
    /// System transport controls (MPRIS / macOS Now Playing).
    pub media: crate::media::MediaBridge,
}

/// Key under which the global mixer is persisted.
pub const SETTING_GLOBAL_MIXER: &str = "mixer.global";
pub const SETTING_VOLUME: &str = "player.volume";
pub const SETTING_REPEAT: &str = "player.repeat";
pub const SETTING_SHUFFLE: &str = "player.shuffle";
/// Legacy location for crossfade settings. On startup its value is migrated
/// into [`SETTING_GLOBAL_MIXER`] when that mixer has no crossfade section.
pub const SETTING_CROSSFADE: &str = "crossfade.global";

impl AppState {
    pub fn new(data_dir: &Path) -> Result<Self> {
        let paths = Paths::new(data_dir);
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

        let effective_mixer = player.effective_mixer();
        engine.set_settings(effective_mixer.clone());
        engine.set_crossfade(effective_mixer.crossfade);

        Ok(AppState {
            db,
            engine,
            player: Mutex::new(player),
            paths,
            engine_events: Mutex::new(Some(rx)),
            metadata: Mutex::new(None),
            media: crate::media::MediaBridge::new(),
        })
    }

    /// Recompute the cascade and hand it to the audio engine. Called after any
    /// change to the global, playlist or track layers, and on track change.
    pub fn sync_mixer(&self) {
        let player = self.player.lock();
        let effective_mixer = player.effective_mixer();
        self.engine.set_settings(effective_mixer.clone());
        self.engine.set_crossfade(effective_mixer.crossfade);
        self.engine.set_track_gain_db(player.current_gain_db());
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
