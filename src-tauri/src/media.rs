//! System media controls.
//!
//! Publishes what is playing to the OS and accepts the hardware and desktop
//! transport keys back: MPRIS on Linux (so KDE's panel and media keys work)
//! and `MPNowPlayingInfoCenter` on macOS (Control Centre, the Touch Bar and
//! the F7/F8/F9 keys).
//!
//! Events are handled here rather than being forwarded to the webview so the
//! keys keep working regardless of what the UI is doing, and so they run
//! exactly the same code paths as the on-screen buttons.

use std::time::Duration;

use parking_lot::Mutex;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};
use tauri::{AppHandle, Manager};

use crate::library::model::Track;
use crate::state::AppState;

/// How far the position must drift before it is worth republishing.
const POSITION_EPSILON: f64 = 1.0;
/// Matches the seek amount of the on-screen skip buttons.
const SEEK_SECONDS: f64 = 10.0;

#[derive(Default, PartialEq)]
struct Published {
    track_id: Option<String>,
    playing: bool,
    position_secs: f64,
}

pub struct MediaBridge {
    controls: Mutex<Option<MediaControls>>,
    last: Mutex<Published>,
}

impl MediaBridge {
    pub fn new() -> Self {
        MediaBridge {
            controls: Mutex::new(None),
            last: Mutex::new(Published::default()),
        }
    }
}

impl Default for MediaBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// Register with the OS. A failure here is not fatal: the app simply runs
/// without system transport controls.
pub fn init(app: &AppHandle) {
    let config = PlatformConfig {
        // Reverse-DNS, as MPRIS expects.
        dbus_name: "picknmix",
        display_name: "Pick n Mix",
        // Windows needs a window handle here; the platforms we target do not.
        hwnd: None,
    };

    let mut controls = match MediaControls::new(config) {
        Ok(controls) => controls,
        Err(e) => {
            eprintln!("media: system controls unavailable: {e:?}");
            return;
        }
    };

    let handle = app.clone();
    if let Err(e) = controls.attach(move |event| handle_event(&handle, event)) {
        eprintln!("media: could not attach to system controls: {e:?}");
        return;
    }

    if let Some(state) = app.try_state::<AppState>() {
        *state.media.controls.lock() = Some(controls);
    }
}

fn handle_event(app: &AppHandle, event: MediaControlEvent) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    // Deliberately the same functions the UI buttons call.
    let result = match event {
        MediaControlEvent::Play => {
            state.engine.play();
            let _ = app.emit_playing(true);
            Ok(())
        }
        MediaControlEvent::Pause => {
            state.engine.pause();
            let _ = app.emit_playing(false);
            Ok(())
        }
        MediaControlEvent::Toggle => {
            crate::commands::toggle_play(app.clone(), state.clone()).map(|_| ())
        }
        MediaControlEvent::Next => crate::commands::next_track(app.clone(), state.clone()),
        MediaControlEvent::Previous => crate::commands::previous_track(app.clone(), state.clone()),
        MediaControlEvent::Stop => {
            state.engine.pause();
            let _ = app.emit_playing(false);
            Ok(())
        }
        MediaControlEvent::Seek(direction) => {
            seek_by(&state, direction, SEEK_SECONDS);
            Ok(())
        }
        MediaControlEvent::SeekBy(direction, amount) => {
            seek_by(&state, direction, amount.as_secs_f64());
            Ok(())
        }
        MediaControlEvent::SetPosition(position) => {
            state.engine.seek(position.0.as_secs_f64());
            Ok(())
        }
        // Volume and the rest are not offered to the OS.
        _ => Ok(()),
    };

    if let Err(e) = result {
        eprintln!("media: handling a system control event failed: {e}");
    }
}

fn seek_by(state: &AppState, direction: SeekDirection, seconds: f64) {
    let snapshot = state.engine.snapshot();
    let delta = match direction {
        SeekDirection::Forward => seconds,
        SeekDirection::Backward => -seconds,
    };
    let target = (snapshot.position_secs + delta).clamp(0.0, snapshot.duration_secs.max(0.0));
    state.engine.seek(target);
}

/// Owned copy of what the OS needs, so it can cross a thread boundary.
struct Payload {
    title: String,
    album: Option<String>,
    artist: String,
    cover_url: Option<String>,
    duration: Duration,
}

/// Push the current track and transport state to the OS, skipping the call
/// when nothing meaningful has changed.
///
/// Called from the ticker thread, but the OS calls themselves are marshalled
/// to the main thread: on macOS they reach AppKit, which is not safe to touch
/// from anywhere else.
pub fn publish(app: &AppHandle, track: Option<&Track>, playing: bool, position_secs: f64) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    let next = Published {
        track_id: track.map(|t| t.id.clone()),
        playing,
        position_secs,
    };

    // Change detection stays on this thread: it is cheap and stops us queueing
    // work onto the main thread several times a second for nothing.
    let track_changed = {
        let mut last = state.media.last.lock();
        let track_changed = last.track_id != next.track_id;
        let transport_changed = last.playing != next.playing;
        let drifted = (last.position_secs - next.position_secs).abs() > POSITION_EPSILON;
        if !track_changed && !transport_changed && !drifted {
            return;
        }
        *last = next;
        track_changed
    };

    let payload = track.map(|track| Payload {
        title: track.title.clone(),
        // An empty album would show as a blank line, so omit it.
        album: Some(track.album.clone()).filter(|a| !a.trim().is_empty()),
        artist: track.artist.clone(),
        cover_url: track
            .artwork_id
            .as_ref()
            .map(|id| format!("file://{}", state.paths.artwork.join(id).display())),
        duration: Duration::from_secs_f64(track.duration_secs.max(0.0)),
    });

    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(state) = handle.try_state::<AppState>() else {
            return;
        };
        let mut controls = state.media.controls.lock();
        let Some(controls) = controls.as_mut() else {
            return;
        };

        if track_changed {
            match payload.as_ref() {
                Some(p) => {
                    let _ = controls.set_metadata(MediaMetadata {
                        title: Some(&p.title),
                        album: p.album.as_deref(),
                        artist: Some(&p.artist),
                        cover_url: p.cover_url.as_deref(),
                        duration: Some(p.duration),
                    });
                }
                None => {
                    let _ = controls.set_metadata(MediaMetadata::default());
                }
            }
        }

        let progress = Some(MediaPosition(Duration::from_secs_f64(
            position_secs.max(0.0),
        )));
        let playback = match (payload.is_some(), playing) {
            (false, _) => MediaPlayback::Stopped,
            (true, true) => MediaPlayback::Playing { progress },
            (true, false) => MediaPlayback::Paused { progress },
        };
        let _ = controls.set_playback(playback);
    });
}

/// Small helper so the event handler can keep the UI in step.
trait EmitPlaying {
    fn emit_playing(&self, playing: bool) -> tauri::Result<()>;
}

impl EmitPlaying for AppHandle {
    fn emit_playing(&self, playing: bool) -> tauri::Result<()> {
        use tauri::Emitter;
        self.emit("playing-changed", playing)
    }
}
