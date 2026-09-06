//! The window's client-side frame on Linux.
//!
//! With `decorations: false` the app draws its own shadow, which it does by
//! leaving a transparent margin inside the window and casting the shadow into
//! it (see `--frame-inset` in `App.vue`). Nothing about that margin is visible
//! to the compositor, though, so KWin snaps, tiles and maximises against the
//! outside of the shadow and the app ends up inset from the edge it was put
//! against, with the gap coming out of the space it was given.
//!
//! The fix is the same one GTK's own CSD windows use: tell the compositor how
//! much of the window is shadow, via `xdg_surface.set_window_geometry` on
//! Wayland or `_GTK_FRAME_EXTENTS` on X11. GDK writes whichever applies from a
//! single `set_shadow_width` call. The compositor then positions the *visible*
//! frame flush against the edge, and the shadow is simply outside the tile.
//!
//! A window sitting flush against something has no room for a shadow at all,
//! so once it is maximised, fullscreen or tiled the margin is dropped on both
//! sides: zeroed here, and flattened in the CSS by the `window-flush` event
//! this emits.
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::WebviewWindow;

/// The transparent margin reserved for the shadow, in logical pixels.
///
/// Must match `--frame-inset` in `App.vue`: the CSS decides where the shadow
/// is actually drawn, and this is only the compositor's copy of it.
#[cfg(target_os = "linux")]
const FRAME_INSET: i32 = 24;

/// The last state pushed to the frontend, so a listener that starts after the
/// window is already tiled can ask for it. Written only from the main thread,
/// read from a command thread.
static FLUSH: AtomicBool = AtomicBool::new(false);

/// Whether the window is currently maximised, fullscreen or tiled, and so is
/// drawing no shadow. Always false where there is no client-side frame.
pub fn is_flush() -> bool {
    FLUSH.load(Ordering::Relaxed)
}

#[cfg(not(target_os = "linux"))]
pub fn install(_window: &WebviewWindow) {}

/// Publishes the shadow margin to the compositor and keeps it in step with the
/// window state. Errors are logged rather than fatal: a missing shadow margin
/// costs some snapped-window space, which is not worth failing to start over.
#[cfg(target_os = "linux")]
pub fn install(window: &WebviewWindow) {
    use gtk::gdk::WindowState;
    use gtk::glib::Propagation;
    use gtk::prelude::*;
    use tauri::Emitter;

    /// Every state in which the window is put flush against an edge. KWin
    /// reports its own tile layouts through the standard tiled states, so
    /// custom layouts need nothing extra here.
    fn flush_state(state: WindowState) -> bool {
        state.intersects(
            WindowState::MAXIMIZED
                | WindowState::FULLSCREEN
                | WindowState::TILED
                | WindowState::TOP_TILED
                | WindowState::RIGHT_TILED
                | WindowState::BOTTOM_TILED
                | WindowState::LEFT_TILED,
        )
    }

    /// A window that is not yet realised has no GDK window to set this on; the
    /// `realize` handler below covers that case.
    fn apply(gtk_window: &gtk::ApplicationWindow, flush: bool) {
        let Some(gdk_window) = gtk_window.window() else {
            return;
        };
        let inset = if flush { 0 } else { FRAME_INSET };
        gdk_window.set_shadow_width(inset, inset, inset, inset);
    }

    let gtk_window = match window.gtk_window() {
        Ok(gtk_window) => gtk_window,
        Err(error) => {
            eprintln!("Unable to reach the GTK window, so no shadow margin: {error}");
            return;
        }
    };

    let flush = gtk_window.window().is_some_and(|w| flush_state(w.state()));
    FLUSH.store(flush, Ordering::Relaxed);
    apply(&gtk_window, flush);

    gtk_window.connect_realize(|gtk_window| apply(gtk_window, is_flush()));

    let emitter = window.clone();
    gtk_window.connect_window_state_event(move |gtk_window, event| {
        let flush = flush_state(event.new_window_state());
        if flush != FLUSH.swap(flush, Ordering::Relaxed) {
            apply(gtk_window, flush);
            let _ = emitter.emit("window-flush", flush);
        }
        Propagation::Proceed
    });
}
