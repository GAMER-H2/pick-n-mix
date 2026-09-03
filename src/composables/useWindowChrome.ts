/**
 * Custom window chrome: whether this window draws its own title bar, window
 * controls and resize regions, and the maximised/focused state their styling
 * reflects.
 *
 * Both facts are asked of the window rather than inferred from the user agent,
 * so they stay true to whatever `decorations` the platform config actually
 * applied instead of being a second, silently divergent source of truth.
 */
import { onBeforeUnmount, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { UnlistenFn } from "@tauri-apps/api/event";

export type ResizeDirection =
  | "East"
  | "North"
  | "NorthEast"
  | "NorthWest"
  | "South"
  | "SouthEast"
  | "SouthWest"
  | "West";

export interface ResizeRegion {
  direction: ResizeDirection;
  className: string;
}

/** The eight edge zones, paired with the classes that place them. */
const resizeRegions: ReadonlyArray<ResizeRegion> = [
  { direction: "North", className: "app__resize-region--north" },
  { direction: "NorthEast", className: "app__resize-region--north-east" },
  { direction: "East", className: "app__resize-region--east" },
  { direction: "SouthEast", className: "app__resize-region--south-east" },
  { direction: "South", className: "app__resize-region--south" },
  { direction: "SouthWest", className: "app__resize-region--south-west" },
  { direction: "West", className: "app__resize-region--west" },
  { direction: "NorthWest", className: "app__resize-region--north-west" },
];

export function useWindowChrome() {
  const usesCustomTitlebar = ref(false);
  const isMaximized = ref(false);
  const isFocused = ref(true);

  let unlistenResize: UnlistenFn | null = null;
  let unlistenFocus: UnlistenFn | null = null;

  /**
   * Whether this window has to draw its own frame.
   *
   * Asked of the window rather than inferred from the user agent, so it stays
   * true to whatever `decorations` the platform config actually applied instead
   * of being a second, silently divergent source of truth.
   */
  async function usesClientSideDecorations() {
    if (!("__TAURI_INTERNALS__" in window)) return false;
    try {
      return !(await getCurrentWindow().isDecorated());
    } catch (error) {
      reportWindowControlError(error);
      return false;
    }
  }

  /**
   * Whether the system's window buttons are floating over our own content.
   *
   * The user agent rather than a plugin: this only decides how much padding to
   * leave, so being wrong costs a little space and nothing else, and the app
   * already avoids adding a dependency for a single boolean.
   */
  function isMacOverlay(): boolean {
    return /Mac(intosh| OS X)/.test(navigator.userAgent);
  }

  /** Kept in step so the frame and its shadow drop away when maximised. */
  async function syncMaximized() {
    try {
      isMaximized.value = await getCurrentWindow().isMaximized();
    } catch (error) {
      reportWindowControlError(error);
    }
  }

  async function minimizeWindow() {
    await getCurrentWindow().minimize();
  }

  async function toggleMaximizeWindow() {
    await getCurrentWindow().toggleMaximize();
    await syncMaximized();
  }

  async function closeWindow() {
    await getCurrentWindow().close();
  }

  async function startResizeWindow(direction: ResizeDirection) {
    await getCurrentWindow().startResizeDragging(direction);
  }

  function reportWindowControlError(error: unknown) {
    console.error("Unable to change the window state:", error);
  }

  onMounted(async () => {
    // macOS keeps its own decorations but floats the traffic lights over the
    // webview's top-left corner, so anything drawn there has to leave room. The
    // two cases are exclusive: a window either draws its own controls or has
    // the system's laid over it.
    if (isMacOverlay()) document.documentElement.classList.add("is-mac-overlay");

    if (await usesClientSideDecorations()) {
      usesCustomTitlebar.value = true;
      document.documentElement.classList.add("is-custom-titlebar");
      await syncMaximized();
      // Maximising, tiling and snapping all arrive as a resize.
      unlistenResize = await getCurrentWindow().onResized(() => {
        void syncMaximized();
      });

      try {
        isFocused.value = await getCurrentWindow().isFocused();
      } catch (error) {
        reportWindowControlError(error);
      }
      unlistenFocus = await getCurrentWindow().onFocusChanged(({ payload }) => {
        isFocused.value = payload;
      });
    }
  });

  onBeforeUnmount(() => {
    unlistenResize?.();
    unlistenFocus?.();
    document.documentElement.classList.remove("is-custom-titlebar");
    document.documentElement.classList.remove("is-mac-overlay");
  });

  return {
    usesCustomTitlebar,
    isMaximized,
    isFocused,
    resizeRegions,
    minimizeWindow,
    toggleMaximizeWindow,
    closeWindow,
    startResizeWindow,
    reportWindowControlError,
  };
}
