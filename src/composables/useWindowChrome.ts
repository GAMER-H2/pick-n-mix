/**
 * Custom window chrome: whether this window draws its own title bar, window
 * controls and resize regions, and the maximised/focused state their styling
 * reflects.
 *
 * Both facts are asked of the window rather than inferred from the user agent,
 * so they stay true to whatever `decorations` the platform config actually
 * applied instead of being a second, silently divergent source of truth.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

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
  const isTiled = ref(false);
  const isFocused = ref(true);

  /**
   * Whether the window sits flush against an edge, and so has no room for its
   * shadow. Tiling is reported by the backend, the only side that can see it:
   * neither the window API nor the DOM tells a tiled window from a small one.
   */
  const isFlush = computed(() => isMaximized.value || isTiled.value);

  // On the document rather than in the shell, because the shadow margin is a
  // root token: everything teleported to `<body>` — every scrim — has to know
  // where the window's real edge is too.
  watch(isFlush, (flush) => {
    document.documentElement.classList.toggle("is-window-flush", flush);
  });

  let unlistenResize: UnlistenFn | null = null;
  let unlistenFocus: UnlistenFn | null = null;
  let unlistenFlush: UnlistenFn | null = null;

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

      // Tiling, on the platforms that have it, arrives from the backend: it is
      // also where the compositor's copy of the shadow margin is dropped, so
      // the two stay in step.
      unlistenFlush = await listen<boolean>("window-flush", ({ payload }) => {
        isTiled.value = payload;
      });
      // The state the window started in, which no event will repeat.
      try {
        isTiled.value = await invoke<boolean>("window_is_flush");
      } catch (error) {
        reportWindowControlError(error);
      }
    }
  });

  onBeforeUnmount(() => {
    unlistenResize?.();
    unlistenFocus?.();
    unlistenFlush?.();
    document.documentElement.classList.remove("is-custom-titlebar");
    document.documentElement.classList.remove("is-window-flush");
    document.documentElement.classList.remove("is-mac-overlay");
  });

  return {
    usesCustomTitlebar,
    isFlush,
    isFocused,
    resizeRegions,
    minimizeWindow,
    toggleMaximizeWindow,
    closeWindow,
    startResizeWindow,
    reportWindowControlError,
  };
}
