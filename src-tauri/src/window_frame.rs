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
//!
//! ## What the compositor will and will not say
//!
//! Maximised, fullscreen and quick-tiled windows are reported outright. KDE's
//! *custom* tile layouts are not: KWin derives the `xdg_toplevel` tiled states
//! from `Tile::anchors()`, which names only the edges where the tile touches
//! the screen — and returns nothing at all for a layout with any padding,
//! which its tile editor adds by default. A window put in a custom tile can
//! therefore go on believing it is floating.
//!
//! So the state is backed up by a guess at the shape: a tile is a partition of
//! the work area and fills it exactly along at least one axis, which an
//! ordinary window very rarely does. That catches full-height columns and
//! full-width rows. It cannot catch a tile that is short of the work area on
//! both axes — a quarter tile — because on Wayland a client is not told where
//! it is, and at that point the window is the same size and shape as one the
//! user sized by hand. `windowCorners` in the app preferences is the way out
//! of that, in either direction: `square` squares the window off whatever the
//! compositor says, and `rounded` drops the shape guess and takes only the
//! compositor's word. Both move the shadow margin with the corner radius,
//! which have to agree about where the window's edge is.
//!
//! Nothing here picks a WebKit rendering path. An AppImage carries its own
//! WebKitGTK, and against a much newer host Mesa its DMA-BUF renderer can fail
//! to agree a format and composite nothing — which, behind a `transparent`
//! window, reads as an invisible window rather than a blank one. The obvious
//! guard is to force `WEBKIT_DISABLE_DMABUF_RENDERER=1` for AppImage runs, and
//! it was done here once: it takes the webview off the GPU entirely and makes
//! the whole app sluggish on every host, including the great majority where
//! the accelerated path works. The env var is left to the user instead; see
//! the AppImage notes in `README.md`.
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use tauri::WebviewWindow;

/// The transparent margin reserved for the shadow, in logical pixels.
///
/// Must match `--frame-inset` in `App.vue`: the CSS decides where the shadow
/// is actually drawn, and this is only the compositor's copy of it.
#[cfg(target_os = "linux")]
const FRAME_INSET: i32 = 24;

/// How much of the work area the window must span along one axis before it is
/// taken to have been placed there by the compositor rather than by the user.
///
/// Short of 1.0 by the padding a tile layout can leave around its tiles, and
/// no further: the closer this gets to the size a window is left at by hand,
/// the more ordinary windows lose their shadow.
#[cfg(target_os = "linux")]
const TILE_FILL: f64 = 0.97;

/// What the user has asked the window's corners to do, as the discriminant of
/// [`Corners`]. Read from a GTK callback and a command thread alike.
static CORNERS: AtomicU8 = AtomicU8::new(Corners::Auto as u8);

/// The last state pushed to the frontend, so a listener that starts after the
/// window is already tiled can ask for it. Written only from the main thread,
/// read from a command thread.
static FLUSH: AtomicBool = AtomicBool::new(false);

/// The window's corner treatment, as the `windowCorners` preference sets it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corners {
    /// Follow the compositor, and the shape of the window where it is silent.
    Auto = 0,
    /// Always square, with no shadow margin. The way out when a tile layout
    /// leaves the window a shape nothing here can recognise.
    Square = 1,
    /// Take the compositor's word and nothing else — still square when
    /// maximised or tiled, which is never in doubt, but never squared off by
    /// the shape guess. The way out when that guess is wrong about a large
    /// floating window.
    Rounded = 2,
}

impl Corners {
    /// Anything unrecognised — an older or hand-edited settings row — falls
    /// back to following the compositor.
    pub fn from_id(id: &str) -> Self {
        match id {
            "square" => Corners::Square,
            "rounded" => Corners::Rounded,
            _ => Corners::Auto,
        }
    }

    /// Only the Linux frame reads this back; elsewhere the preference is
    /// stored and nothing consults it.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn from_stored(raw: u8) -> Self {
        match raw {
            1 => Corners::Square,
            2 => Corners::Rounded,
            _ => Corners::Auto,
        }
    }
}

/// Whether the window is currently maximised, fullscreen or tiled, and so is
/// drawing no shadow. Always false where there is no client-side frame.
pub fn is_flush() -> bool {
    FLUSH.load(Ordering::Relaxed)
}

#[cfg(not(target_os = "linux"))]
pub fn install(_window: &WebviewWindow) {}

/// Apply the `windowCorners` preference. Takes effect immediately: the margin
/// the compositor is holding changes with the radius the CSS draws, or the two
/// disagree about where the window's edge is.
#[cfg(not(target_os = "linux"))]
pub fn set_corners(_window: &WebviewWindow, corners: Corners) {
    CORNERS.store(corners as u8, Ordering::Relaxed);
}

#[cfg(target_os = "linux")]
pub fn set_corners(window: &WebviewWindow, corners: Corners) {
    use tauri::Manager;

    CORNERS.store(corners as u8, Ordering::Relaxed);
    // GTK objects belong to the main thread and a command does not run there.
    let handle = window.app_handle().clone();
    let window = window.clone();
    if let Err(error) = handle.run_on_main_thread(move || refresh(&window, false)) {
        eprintln!("Unable to reach the main thread to reshape the window: {error}");
    }
}

/// Publishes the shadow margin to the compositor and keeps it in step with the
/// window state. Errors are logged rather than fatal: a missing shadow margin
/// costs some snapped-window space, which is not worth failing to start over.
#[cfg(target_os = "linux")]
pub fn install(window: &WebviewWindow) {
    use gtk::glib::Propagation;
    use gtk::prelude::*;

    let gtk_window = match window.gtk_window() {
        Ok(gtk_window) => gtk_window,
        Err(error) => {
            eprintln!("Unable to reach the GTK window, so no shadow margin: {error}");
            return;
        }
    };

    // The state the window started in, which no event will repeat.
    STATE_FLUSH.store(
        gtk_window.window().is_some_and(|w| flush_state(w.state())),
        Ordering::Relaxed,
    );
    // Forced: the compositor has to be given the margin once even when the
    // answer is the `false` this started at.
    refresh(window, true);

    // A window that is not realised yet has no GDK window to set the margin
    // on, so the first real chance to do it is here.
    let emitter = window.clone();
    gtk_window.connect_realize(move |_| refresh(&emitter, true));

    // Maximise, fullscreen and quick tiling all arrive here.
    let emitter = window.clone();
    gtk_window.connect_window_state_event(move |_, event| {
        STATE_FLUSH.store(flush_state(event.new_window_state()), Ordering::Relaxed);
        refresh(&emitter, false);
        Propagation::Proceed
    });

    // A custom tile arrives as nothing but a resize, which is all the shape
    // guess has to go on.
    let emitter = window.clone();
    gtk_window.connect_size_allocate(move |_, _| refresh(&emitter, false));
}

/// The flush state GDK last reported, kept separately because
/// `window-state-event` carries the new state before the window has it.
#[cfg(target_os = "linux")]
static STATE_FLUSH: AtomicBool = AtomicBool::new(false);

/// Every state in which the window is put flush against an edge. KWin reports
/// its own *quick* tiling through the standard tiled states; its custom tile
/// layouts report nothing, and are left to `fills_work_area`.
#[cfg(target_os = "linux")]
fn flush_state(state: gtk::gdk::WindowState) -> bool {
    use gtk::gdk::WindowState;

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

/// Whether the window's *visible* frame spans its monitor's work area along
/// one axis and not the other — the shape a tile has and a window left alone
/// almost never does.
///
/// Both axes would mean a window very nearly the size of the screen, which is
/// either already maximised — reported outright, and handled above — or a
/// deliberately huge floating window that should keep its shadow.
#[cfg(target_os = "linux")]
fn fills_work_area(gtk_window: &gtk::ApplicationWindow) -> bool {
    use gtk::prelude::*;

    let Some(gdk_window) = gtk_window.window() else {
        return false;
    };
    let Some(monitor) = gdk_window.display().monitor_at_window(&gdk_window) else {
        return false;
    };
    let work_area = monitor.workarea();
    if work_area.width() <= 0 || work_area.height() <= 0 {
        return false;
    }

    // The window is larger than the app by the margin it is currently holding
    // for its shadow; the compositor sized the visible frame, not the surface.
    let inset = 2 * current_inset();
    let (width, height) = gtk_window.size();
    let fills_width = f64::from(width - inset) / f64::from(work_area.width()) >= TILE_FILL;
    let fills_height = f64::from(height - inset) / f64::from(work_area.height()) >= TILE_FILL;
    fills_width != fills_height
}

/// The shadow margin in force right now, which is what the current window size
/// includes.
#[cfg(target_os = "linux")]
fn current_inset() -> i32 {
    if is_flush() {
        0
    } else {
        FRAME_INSET
    }
}

/// Work out whether the window is flush, and if that has changed, hand the
/// answer to the compositor and the frontend at the same time.
///
/// `force` re-states the margin to the compositor without anything having
/// changed, for the two moments when it has never been said at all: startup,
/// and the window being realised.
#[cfg(target_os = "linux")]
fn refresh(window: &WebviewWindow, force: bool) {
    use gtk::prelude::*;
    use tauri::Emitter;

    let Ok(gtk_window) = window.gtk_window() else {
        return;
    };

    let flush = match Corners::from_stored(CORNERS.load(Ordering::Relaxed)) {
        Corners::Square => true,
        Corners::Rounded => STATE_FLUSH.load(Ordering::Relaxed),
        Corners::Auto => STATE_FLUSH.load(Ordering::Relaxed) || fills_work_area(&gtk_window),
    };
    let changed = flush != FLUSH.swap(flush, Ordering::Relaxed);
    if !changed && !force {
        return;
    }

    // A window that is not yet realised has no GDK window to set this on; the
    // `realize` handler covers that case.
    if let Some(gdk_window) = gtk_window.window() {
        let inset = if flush { 0 } else { FRAME_INSET };
        gdk_window.set_shadow_width(inset, inset, inset, inset);
    }
    if changed {
        let _ = window.emit("window-flush", flush);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_ids_round_trip_and_fall_back() {
        assert_eq!(Corners::from_id("square"), Corners::Square);
        assert_eq!(Corners::from_id("rounded"), Corners::Rounded);
        assert_eq!(Corners::from_id("auto"), Corners::Auto);
        assert_eq!(
            Corners::from_id("what the app wrote in 2027"),
            Corners::Auto
        );
    }

    #[test]
    fn stored_discriminants_survive_the_round_trip() {
        for corners in [Corners::Auto, Corners::Square, Corners::Rounded] {
            assert_eq!(Corners::from_stored(corners as u8), corners);
        }
        assert_eq!(Corners::from_stored(99), Corners::Auto);
    }
}
