//! Application state shared by every Tauri command.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use crossbeam_channel::Receiver;
use parking_lot::Mutex;

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
}

/// Key under which the global mixer is persisted.
pub const SETTING_GLOBAL_MIXER: &str = "mixer.global";
pub const SETTING_VOLUME: &str = "player.volume";
pub const SETTING_REPEAT: &str = "player.repeat";
pub const SETTING_SHUFFLE: &str = "player.shuffle";
/// Window geometry as the user left it, restored on the next launch.
pub const SETTING_WINDOW: &str = "window.geometry";
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
            preview: Mutex::new(None),
            engine_events: Mutex::new(Some(rx)),
            metadata: Mutex::new(None),
            media: crate::media::MediaBridge::new(),
        })
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
