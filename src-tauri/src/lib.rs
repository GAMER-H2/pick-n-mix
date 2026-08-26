pub mod audio;
pub mod commands;
pub mod library;
pub mod media;
pub mod player;
pub mod playlist;
pub mod presets;
pub mod state;

use std::time::Duration;

use tauri::{Emitter, Manager};

use crate::audio::EngineEvent;
use crate::state::AppState;

/// Serves cached cover art as `art://localhost/<id>`.
///
/// A custom scheme is used rather than the asset protocol so artwork can be
/// served straight from the cache without opening the whole filesystem to the
/// webview, and so remote sources can plug into the same URLs later.
fn artwork_response(state: &AppState, uri: &str) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::{Response, StatusCode};

    let not_found = || {
        Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()).expect("static response")
    };

    // The id is the last path segment, whichever host form the platform uses.
    let Some(id) = uri.rsplit('/').next().filter(|s| !s.is_empty()) else {
        return not_found();
    };
    let id = percent_decode(id);

    // Refuse anything that could climb out of the artwork directory.
    if id.contains("..") || id.contains('/') || id.contains('\\') {
        return not_found();
    }

    let path = state.paths.artwork.join(&id);
    let Ok(bytes) = std::fs::read(&path) else {
        return not_found();
    };

    let mime = match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/jpeg",
    };

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", mime)
        // Artwork ids are content-addressed, so they can be cached hard.
        .header("Cache-Control", "public, max-age=31536000, immutable")
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
                        let next = {
                            let mut player = state.player.lock();
                            player.advance(true).cloned()
                        };
                        match next {
                            Some(track) => {
                                state.sync_mixer();
                                match state.engine.load(
                                    std::path::PathBuf::from(&track.location),
                                    0.0,
                                    track.gain_db.unwrap_or(0.0),
                                ) {
                                    Ok(_) => {
                                        state.engine.play();
                                        let _ = app.emit("track-changed", Some(&track));
                                        let _ =
                                            app.emit("queue-changed", state.player.lock().view());
                                    }
                                    Err(e) => {
                                        let _ = app.emit(
                                            "engine-error",
                                            format!("could not play {}: {e}", track.title),
                                        );
                                    }
                                }
                            }
                            None => {
                                state.engine.pause();
                                let _ = app.emit("playing-changed", false);
                                let _ = app.emit("queue-ended", ());
                            }
                        }
                    }
                    EngineEvent::Error { message } => {
                        let _ = app.emit("engine-error", message);
                    }
                }
            }
        })
        .ok();
}

/// Pushes a playback snapshot to the UI on a timer, and decodes any ambience
/// bed the mixer has asked for.
fn spawn_ticker(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("pnm-ticker".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(200));
            let Some(state) = app.try_state::<AppState>() else {
                return;
            };

            let snapshot = state.engine.snapshot();
            let _ = app.emit("playback", &snapshot);

            // Keep the OS transport in step. `publish` skips the call when
            // nothing has changed, so this is cheap at tick rate.
            let current = state.player.lock().current().cloned();
            media::publish(&app, current.as_ref(), snapshot.playing, snapshot.position_secs);

            for id in state.engine.take_bed_requests() {
                if state.engine.has_bed(&id) {
                    continue;
                }
                let catalogue = audio::ambience::catalogue(&state.paths.filters);
                let Some(info) = catalogue.into_iter().find(|f| f.id == id && f.available) else {
                    // No file supplied for this bed yet; the UI shows it greyed.
                    continue;
                };
                let Some(path) = info.path else { continue };
                let rate = state.engine.device_sample_rate();
                match audio::ambience::load_bed(std::path::Path::new(&path), rate) {
                    Ok(samples) => state.engine.install_bed(id, samples),
                    Err(e) => {
                        let _ = app.emit("engine-error", format!("could not load filter {id}: {e}"));
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
        .register_uri_scheme_protocol("art", |ctx, request| {
            let app = ctx.app_handle();
            match app.try_state::<AppState>() {
                Some(state) => artwork_response(&state, request.uri().path()),
                None => tauri::http::Response::builder()
                    .status(tauri::http::StatusCode::SERVICE_UNAVAILABLE)
                    .body(Vec::new())
                    .expect("static response"),
            }
        })
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let state = AppState::new(&data_dir)?;
            app.manage(state);

            // Needs the managed state, so it goes after `manage`.
            media::init(app.handle());

            spawn_event_pump(app.handle().clone());
            spawn_ticker(app.handle().clone());
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
            // mixer
            commands::mixer_state,
            commands::mixer_layers,
            commands::set_global_mixer,
            commands::set_playlist_mixer,
            commands::list_presets,
            commands::save_preset,
            commands::delete_preset,
            commands::list_filters,
            commands::filters_directory,
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
