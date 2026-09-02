pub mod audio;
pub mod commands;
pub mod history;
pub mod library;
pub mod media;
pub mod player;
pub mod playlist;
pub mod presets;
pub mod state;

use std::time::Duration;

use tauri::{Emitter, Manager};

use crate::audio::{EngineEvent, PlaybackSnapshot};
use crate::state::AppState;

/// Serves cached cover art as `art://localhost/<id>`.
///
/// A custom scheme is used rather than the asset protocol so artwork can be
/// served straight from the cache without opening the whole filesystem to the
/// webview, and so remote sources can plug into the same URLs later.
fn artwork_response(state: &AppState, uri: &tauri::http::Uri) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::{Response, StatusCode};

    let not_found = || {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .expect("static response")
    };

    // The id is the last path segment, whichever host form the platform uses.
    let Some(id) = uri.path().rsplit('/').next().filter(|s| !s.is_empty()) else {
        return not_found();
    };
    let id = percent_decode(id);

    // Refuse anything that could climb out of the artwork directory.
    if id.contains("..") || id.contains('/') || id.contains('\\') {
        return not_found();
    }

    let original = state.paths.artwork.join(&id);

    // `?w=<pixels>` asks for a downscaled copy sized for where it is actually
    // displayed, rather than decoding the (possibly multi-megapixel) embedded
    // picture for a 38px list row. Generation happens off this request, in
    // `Thumbnailer`'s own worker pool — this call either gets an
    // already-cached answer or queues one and falls back to the original for
    // now, but it never itself decodes or resizes anything.
    let width = uri.query().and_then(|query| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix("w="))
            .and_then(|value| value.parse::<u32>().ok())
    });
    let (path, cacheable) = match width {
        Some(w) => match state.thumbnails.get(&id, &original, w) {
            library::thumbnail::Art::Thumb(p) => (p, true),
            // Permanently the right answer: nothing will ever change it.
            library::thumbnail::Art::Original => (original, true),
            // A background job was just queued (or is already running). The
            // original is correct but bigger than asked for, and must not be
            // cached by the webview or a later request would keep getting
            // this answer back and never see the thumbnail once it exists.
            library::thumbnail::Art::Pending => (original, false),
        },
        None => (original, true),
    };

    let Ok(bytes) = std::fs::read(&path) else {
        return not_found();
    };

    let mime = match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/jpeg",
    };

    let cache_control = if cacheable {
        // Artwork ids are content-addressed, so a resolved answer can be
        // cached hard.
        "public, max-age=31536000, immutable"
    } else {
        "no-store"
    };

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", mime)
        .header("Cache-Control", cache_control)
        .header("Access-Control-Allow-Origin", "*")
        .body(bytes)
        .unwrap_or_else(|_| not_found())
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The window geometry carried between runs.
///
/// Position is deliberately not stored: a window restored onto a monitor that
/// is no longer attached is worse than one the compositor places itself.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowGeometry {
    width: u32,
    height: u32,
    maximized: bool,
}

fn saved_geometry(state: &AppState) -> Option<WindowGeometry> {
    let raw = state.db.get_setting(crate::state::SETTING_WINDOW).ok()??;
    serde_json::from_str(&raw).ok()
}

/// Put the window back the size it was closed at.
fn restore_geometry(state: &AppState, window: &tauri::WebviewWindow) {
    let Some(saved) = saved_geometry(state) else {
        return;
    };
    // A degenerate size would leave an unusable window; fall back to the
    // configured default by simply not applying it.
    if saved.width >= 400 && saved.height >= 300 {
        let _ = window.set_size(tauri::PhysicalSize::new(saved.width, saved.height));
    }
    if saved.maximized {
        let _ = window.maximize();
    }
}

/// Record the geometry on the way out.
///
/// While maximised the outer size is the whole screen, which would be a poor
/// size to restore to once un-maximised, so the previously stored size is kept
/// and only the maximised flag is updated.
fn store_geometry(state: &AppState, window: &tauri::WebviewWindow) {
    let maximized = window.is_maximized().unwrap_or(false);
    let previous = saved_geometry(state);

    let size = match (maximized, previous) {
        (true, Some(previous)) => (previous.width, previous.height),
        _ => match window.outer_size() {
            Ok(size) => (size.width, size.height),
            Err(_) => return,
        },
    };

    let geometry = WindowGeometry {
        width: size.0,
        height: size.1,
        maximized,
    };
    if let Ok(raw) = serde_json::to_string(&geometry) {
        let _ = state.db.set_setting(crate::state::SETTING_WINDOW, &raw);
    }
}

/// Forwards engine events to the frontend and advances the queue when a track
/// ends. Runs off the audio path so nothing here can glitch playback.
fn spawn_event_pump(app: tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(events) = state.engine_events.lock().take() else {
        return;
    };

    std::thread::Builder::new()
        .name("pnm-events".into())
        .spawn(move || {
            for event in events.iter() {
                let Some(state) = app.try_state::<AppState>() else {
                    return;
                };
                match event {
                    EngineEvent::TrackFinished => {
                        if state.is_previewing() {
                            continue;
                        }
                        let has_next = state.player.lock().advance(true).is_some();
                        if has_next {
                            let _ = commands::start_current(&app, &state);
                        } else {
                            state.engine.pause();
                            let _ = app.emit("playing-changed", false);
                            let _ = app.emit("queue-ended", ());
                        }
                    }
                    EngineEvent::Error { message } => {
                        let _ = app.emit("engine-error", message);
                    }
                    EngineEvent::NeedNext { token } => {
                        if state.is_previewing() {
                            state.engine.decline_next(token);
                            continue;
                        }
                        // Resolved under the lock (cheap: no I/O), then acted
                        // on after dropping it — opening a decoder can take
                        // tens to hundreds of milliseconds, and holding the
                        // player lock across that would stall every command
                        // the UI sends in the meantime.
                        let prepared = {
                            let player = state.player.lock();
                            player.peek_next().map(|(order_index, item)| {
                                (
                                    order_index,
                                    item.track.id.clone(),
                                    player.effective_mixer_for(item),
                                )
                            })
                        };
                        match prepared {
                            Some((order_index, track_id, settings)) => {
                                let rate = state.engine.device_sample_rate();
                                let files = match state.playback_files(&track_id) {
                                    Ok(files) => files,
                                    Err(error) => {
                                        let _ = app.emit("engine-error", error.to_string());
                                        state.engine.decline_next(token);
                                        continue;
                                    }
                                };
                                let mut opened = None;
                                for file in files {
                                    let path = std::path::PathBuf::from(&file.location);
                                    if let Ok(decoder) =
                                        crate::audio::decode::TrackDecoder::open(&path, rate)
                                    {
                                        opened = Some((decoder, file.gain_db.unwrap_or(0.0)));
                                        break;
                                    }
                                }
                                match opened {
                                    Some((decoder, gain_db)) => {
                                        if let Ok(Some(track)) = state.db.get_track(&track_id) {
                                            state
                                                .player
                                                .lock()
                                                .refresh_track_at(order_index, track);
                                        }
                                        state.engine.prepare_next(
                                            decoder,
                                            settings,
                                            gain_db,
                                            token,
                                            order_index,
                                            track_id,
                                        );
                                    }
                                    None => state.engine.decline_next(token),
                                }
                            }
                            // End of the queue, or a shuffled repeat-all wrap
                            // that cannot be predicted: this transition simply
                            // does not crossfade, exactly as if the feature
                            // were off. Declined, not cancelled, so the worker
                            // stops asking for the rest of this track.
                            None => state.engine.decline_next(token),
                        }
                    }
                    EngineEvent::TrackAdvanced {
                        order_index,
                        track_id,
                    } => {
                        if state.is_previewing() {
                            continue;
                        }
                        {
                            let mut player = state.player.lock();
                            match player.jump_to(order_index) {
                                Some(track) if track.id != track_id => {
                                    // Should not happen: every queue mutation
                                    // also cancels a pending crossfade. Play on
                                    // regardless — the audio has already
                                    // switched — but this is worth knowing
                                    // about if it ever fires.
                                    eprintln!(
                                        "audio: crossfade landed on queue index {order_index} \
                                         but found a different track there than expected"
                                    );
                                }
                                Some(_) => {}
                                None => {
                                    player.advance(true);
                                }
                            }
                        }
                        state.sync_mixer_settings();
                        let current = state.player.lock().current().cloned();
                        // A crossfade switches songs on the worker's own
                        // schedule without going through `load_current`, so
                        // the history has to be rolled over here too.
                        if let Some(track) = current.as_ref() {
                            state.begin_play(&track.id, track.duration_secs);
                        }
                        let _ = app.emit("track-changed", &current);
                        let _ = app.emit("queue-changed", state.player.lock().view());
                    }
                }
            }
        })
        .ok();
}

/// Pushes a playback snapshot to the UI on a timer, and decodes any ambience
/// bed the mixer has requested.
fn spawn_ticker(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("pnm-ticker".into())
        .spawn(move || {
            // While paused, the snapshot is identical tick to tick; re-emitting
            // it anyway would re-render the UI's playback bar 5 times a second
            // for no reason, including its blurred backdrop.
            let mut last_emitted: Option<PlaybackSnapshot> = None;
            loop {
                std::thread::sleep(Duration::from_millis(200));
                let Some(state) = app.try_state::<AppState>() else {
                    return;
                };

                let snapshot = state.engine.snapshot();
                // Follows the engine's own flag, so listening time tracks real
                // audio rather than what the UI last asked for.
                state.plays.tick(snapshot.playing);
                if snapshot.playing || last_emitted.as_ref() != Some(&snapshot) {
                    let _ = app.emit("playback", &snapshot);
                    last_emitted = Some(snapshot.clone());
                }

                // Keep the OS transport in step. `publish` skips the call when
                // nothing has changed, so this is cheap at tick rate.
                let current = state.player.lock().current().cloned();
                media::publish(
                    &app,
                    current.as_ref(),
                    snapshot.playing,
                    snapshot.position_secs,
                );

            }
        })
        .ok();
}

/// Decodes ambience beds the audio worker has asked for.
///
/// On its own thread rather than the ticker's, because decoding a bed takes
/// seconds — and far longer for one whose sample rate does not match the
/// output device, since that goes through the resampler. Doing it on the
/// ticker stalled the `playback` event that drives the progress bar, and left
/// every bed behind the first one waiting on the one in front.
fn spawn_ambience_loader(app: tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let requests = state.engine.bed_request_stream();

    std::thread::Builder::new()
        .name("pnm-ambience".into())
        .spawn(move || {
            // Blocks until the worker asks for something, so an idle app does
            // no work here at all.
            for id in requests.iter() {
                let Some(state) = app.try_state::<AppState>() else {
                    return;
                };
                // The worker repeats a request until the bed lands, so by the
                // time a duplicate arrives it is usually already loaded.
                if state.engine.has_bed(&id) {
                    continue;
                }
                let found = audio::ambience::catalogue(
                    state.paths.bundled_ambience.as_deref(),
                    &state.paths.filters,
                )
                .into_iter()
                .find(|item| item.id == id && item.available)
                .and_then(|item| item.path);
                // No packaged or custom audio for this bed yet. The worker
                // will ask again if it is still wanted.
                let Some(path) = found else { continue };

                let rate = state.engine.device_sample_rate();
                match audio::ambience::load_bed(std::path::Path::new(&path), rate) {
                    Ok(samples) => state.engine.install_bed(id, samples),
                    Err(e) => {
                        let _ =
                            app.emit("engine-error", format!("could not load ambience {id}: {e}"));
                    }
                }
            }
        })
        .ok();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Asynchronous, not the plain synchronous variant: the webview
        // delivers every scheme request on its own UI thread and blocks on
        // the response, so anything slower than a memory read done here would
        // freeze the whole window rather than just this one image.
        .register_asynchronous_uri_scheme_protocol("art", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let response = match app.try_state::<AppState>() {
                    Some(state) => artwork_response(&state, request.uri()),
                    None => tauri::http::Response::builder()
                        .status(tauri::http::StatusCode::SERVICE_UNAVAILABLE)
                        .body(Vec::new())
                        .expect("static response"),
                };
                responder.respond(response);
            });
        })
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let bundled_ambience = app
                .path()
                .resolve("audio_assets", tauri::path::BaseDirectory::Resource)
                .ok()
                .filter(|path| path.is_dir());
            let state = AppState::new(&data_dir, bundled_ambience)?;
            app.manage(state);

            // Needs the managed state, so it goes after `manage`.
            media::init(app.handle());

            // Size the window before it is shown, then keep the setting in step
            // with however the user leaves it.
            if let Some(window) = app.get_webview_window("main") {
                if let Some(state) = app.try_state::<AppState>() {
                    restore_geometry(&state, &window);
                }
                let handle = app.handle().clone();
                let saved = window.clone();
                window.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                        if let Some(state) = handle.try_state::<AppState>() {
                            store_geometry(&state, &saved);
                            // Bank the song being listened to on the way out,
                            // rather than losing the last play of every session.
                            state.end_play();
                        }
                    }
                });
            }

            spawn_event_pump(app.handle().clone());
            spawn_ticker(app.handle().clone());
            spawn_ambience_loader(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // library
            commands::list_folders,
            commands::add_folder,
            commands::remove_folder,
            commands::scan_library,
            commands::list_tracks,
            commands::list_albums,
            commands::list_artists,
            commands::album_tracks,
            commands::artist_tracks,
            commands::get_track,
            commands::list_track_files,
            commands::set_preferred_track_file,
            commands::preview_track_file,
            commands::stop_track_file_preview,
            commands::restore_needs_destination,
            commands::relink_track_file,
            commands::trash_track_file,
            commands::forget_missing_track_file,
            commands::search,
            commands::enrich_track,
            // playback
            commands::play_tracks,
            commands::play_queue_index,
            commands::toggle_play,
            commands::next_track,
            commands::previous_track,
            commands::seek,
            commands::set_volume,
            commands::playback_state,
            commands::set_analyser_enabled,
            commands::analyser_frame,
            // home
            commands::home_shelves,
            commands::mix_tracks,
            commands::play_mix,
            commands::refresh_mixes,
            commands::set_mix_pinned,
            commands::list_pinned_mixes,
            commands::save_mix_to_playlist,
            commands::listening_history,
            commands::clear_listening_history,
            commands::clear_listening_history_for_song,
            commands::stream_info,
            // queue
            commands::queue_state,
            commands::current_track,
            commands::play_next,
            commands::add_to_queue,
            commands::remove_from_queue,
            commands::move_in_queue,
            commands::clear_queue,
            commands::set_shuffle,
            commands::set_repeat,
            // settings and mixer
            commands::app_preferences,
            commands::set_app_preferences,
            commands::output_devices,
            commands::mixer_state,
            commands::mixer_layers,
            commands::set_global_mixer,
            commands::set_playlist_mixer,
            commands::list_presets,
            commands::save_preset,
            commands::update_preset,
            commands::delete_preset,
            commands::list_filters,
            commands::filters_directory,
            commands::import_filter,
            commands::delete_filter,
            commands::crossfade_settings,
            commands::set_crossfade_length,
            commands::set_crossfade_curve,
            // playlists
            commands::list_playlists,
            commands::get_playlist,
            commands::create_playlist,
            commands::update_playlist,
            commands::delete_playlist,
            commands::add_to_playlist,
            commands::remove_from_playlist,
            commands::move_in_playlist,
            commands::set_playlist_entry_mixer,
            commands::set_playlist_shuffle_only,
            commands::set_playlist_artwork,
            commands::clear_playlist_artwork,
            commands::queue_playlist_entry,
            commands::import_playlist,
            commands::export_playlist,
            commands::play_playlist,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Pick n Mix");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_escapes_are_decoded() {
        assert_eq!(percent_decode("art_1234.jpg"), "art_1234.jpg");
        assert_eq!(percent_decode("a%20b.jpg"), "a b.jpg");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
