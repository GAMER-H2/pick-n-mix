<script setup lang="ts">
/**
 * Application shell: sidebar, the routed view, optional right-hand panels,
 * and the player bar along the bottom.
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import Sidebar from "./components/layout/Sidebar.vue";
import NowPlayingBar from "./components/layout/NowPlayingBar.vue";
import QueuePanel from "./components/layout/QueuePanel.vue";
import AdvancedMixer from "./components/mixer/AdvancedMixer.vue";
import ContextMenu from "./components/overlays/ContextMenu.vue";
import AddToPlaylistDialog from "./components/dialogs/AddToPlaylistDialog.vue";
import DuplicateFilesDialog from "./components/dialogs/DuplicateFilesDialog.vue";
import PlaylistDialogs from "./components/dialogs/PlaylistDialogs.vue";
import SettingsModal from "./components/settings/SettingsModal.vue";
import MasterMixModal from "./components/mastermix/MasterMixModal.vue";
import BounceProgress from "./components/layout/BounceProgress.vue";
import PnmIcon from "./components/icons/PnmIcon.vue";
import { usePlayerStore } from "./stores/player";
import { useLibraryStore } from "./stores/library";
import { useMixerStore } from "./stores/mixer";
import { useUiStore } from "./stores/ui";
import { useMasterMixStore } from "./stores/masterMix";
import { useSettingsStore } from "./stores/settings";
import { installShortcuts } from "./lib/keyboard";
import { registerScroller } from "./lib/viewState";
import { useWindowChrome } from "./composables/useWindowChrome";
import { useBackendEvents } from "./composables/useBackendEvents";

const player = usePlayerStore();
const library = useLibraryStore();
const mixer = useMixerStore();
const ui = useUiStore();
const masterMix = useMasterMixStore();
const settings = useSettingsStore();
const route = useRoute();
const router = useRouter();

const isNowPlaying = computed(() => route.name === "nowPlaying");
/** The scrolling element, handed to the back/forward scroll restore. */
const mainEl = ref<HTMLElement | null>(null);

const {
  usesCustomTitlebar,
  isFlush,
  isFocused,
  resizeRegions,
  minimizeWindow,
  toggleMaximizeWindow,
  closeWindow,
  startResizeWindow,
  reportWindowControlError,
} = useWindowChrome();

const { init: initBackendEvents } = useBackendEvents();

let removeShortcuts: (() => void) | null = null;

onMounted(async () => {
  await settings.initialise();

  registerScroller(mainEl.value);

  // The Master Mixer runs its own transport against the same engine, so the
  // global keys stand down for as long as it is open.
  removeShortcuts = installShortcuts(player, ui, router, {
    isSuspended: () => masterMix.open,
    bindings: () => settings.preferences.shortcuts,
    seekStep: () => settings.preferences.seekStepSecs,
  });

  await initBackendEvents();
});

onBeforeUnmount(() => {
  removeShortcuts?.();
  registerScroller(null);
});
</script>

<template>
  <div
    class="app"
    :class="{
      'app--framed': usesCustomTitlebar,
      'is-flush': isFlush,
      'is-unfocused': !isFocused,
    }"
  >
    <div class="app__body">
      <Sidebar />

      <main ref="mainEl" class="app__main scroll-area" :class="{ 'is-screen': isNowPlaying }">
        <div v-if="!isNowPlaying" class="app__titlebar">
          <div class="app__drag-region" data-tauri-drag-region />
        </div>
        <!-- The full-screen player owns its lightweight curtain fade. Keeping
             the blurred view outside a route-level opacity transition avoids
             re-compositing its filters on every animation frame. -->
        <RouterView />
      </main>

      <Transition name="slide-panel">
        <QueuePanel v-if="ui.queueOpen && !isNowPlaying" />
      </Transition>

      <Transition name="slide-panel">
        <AdvancedMixer v-if="mixer.panelOpen && !masterMix.open" customisable />
      </Transition>
    </div>

    <BounceProgress />

    <NowPlayingBar />

    <!-- A direct child of `.app` rather than of `.app__main`, so the buttons
         stay pinned to the window's own corner: unmoved by the sidebar or any
         side panel's width, and never crossed by `.app__main`'s scrollbar. -->
    <div v-if="usesCustomTitlebar" class="app__window-controls">
      <button
        class="icon-button app__window-control"
        type="button"
        title="Minimize"
        aria-label="Minimize window"
        @click="minimizeWindow().catch(reportWindowControlError)"
      >
        <PnmIcon name="minimize" :size="16" />
      </button>
      <button
        class="icon-button app__window-control"
        type="button"
        title="Maximize or restore"
        aria-label="Maximize or restore window"
        @click="toggleMaximizeWindow().catch(reportWindowControlError)"
      >
        <PnmIcon name="maximize" :size="15" />
      </button>
      <button
        class="icon-button app__window-control app__window-control--close"
        type="button"
        title="Close"
        aria-label="Close window"
        @click="closeWindow().catch(reportWindowControlError)"
      >
        <PnmIcon name="close" :size="16" />
      </button>
    </div>

    <div
      v-if="library.scanning"
      class="app__scan"
      :title="library.scanProgress?.path ?? 'Scanning'"
    >
      <span class="app__scan-dot" />
      <span class="truncate">
        Scanning your library… {{ library.scanProgress?.count ?? 0 }} files
      </span>
    </div>

    <ContextMenu />
    <AddToPlaylistDialog />
    <DuplicateFilesDialog />
    <PlaylistDialogs />
    <Transition name="fade">
      <SettingsModal v-if="ui.settingsOpen" />
    </Transition>
    <Transition name="fade">
      <MasterMixModal v-if="masterMix.open" />
    </Transition>

    <!-- Only meaningful without server-side decorations; on every other
         platform these would be invisible strips swallowing clicks along the
         window edges, including on the scrollbar. -->
    <template v-if="usesCustomTitlebar && !isFlush">
      <div
        v-for="region in resizeRegions"
        :key="region.direction"
        class="app__resize-region"
        :class="region.className"
        @pointerdown.prevent="startResizeWindow(region.direction).catch(reportWindowControlError)"
      />
    </template>
  </div>
</template>
<style scoped>
:global(html.is-custom-titlebar),
:global(html.is-custom-titlebar body),
:global(html.is-custom-titlebar #app) {
  background: transparent;
}

/* `--frame-inset` — how much of the window is given over to the shadow on each
   side, and so the ceiling on how far a blur can spread — is a root token in
   `theme.css`: teleported scrims sit outside this component and need it too.

   The grab band for a resize straddles the visible edge instead of filling the
   whole margin: the soft falloff reaches 24px out from the window, and pointing
   at the faintest part of it should not read as a window border. The band is
   `--resize-band` wide, starting `--resize-outer` into the shadow, so the
   remaining 3px fall just inside the window. */
.app {
  --resize-outer: 9px;
  --resize-band: 12px;
  --resize-corner: 16px;
}

.app {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg);
}

/*
 * Client-side decorations have to draw their own shadow: KWin only shadows
 * windows it decorates itself. The window is therefore made slightly larger
 * than the visible app and the difference left transparent, which is the same
 * trick GTK uses for its CSD windows — a shadow drawn on an element flush with
 * the window edge would simply be clipped away.
 */
.app--framed {
  overflow: hidden;
  height: calc(100% - var(--frame-inset) * 2);
  margin: var(--frame-inset);
  border-radius: var(--frame-radius);
  /* Shaped after KWin's own: a hairline edge, then a broad, soft, mostly
     downward falloff rather than a tight dark ring. The hairline follows the
     theme because a black one disappears into a dark desktop — which is most
     of the reason an undecorated window looks flat.

     Every layer stays inside `--frame-inset`, since anything spreading further
     is simply clipped by the window and wasted. */
  box-shadow:
    0 0 0 1px var(--window-edge),
    0 1px 2px rgba(0, 0, 0, 0.22),
    0 4px 10px rgba(0, 0, 0, 0.24),
    0 10px 28px rgba(0, 0, 0, 0.3);
  transition: box-shadow 0.16s var(--ease);
}

/* An inactive window keeps its outline but loses the shadow, so the focused
   window is the one that stands off the desktop. */
.app--framed.is-unfocused {
  box-shadow: 0 0 0 1px var(--window-edge);
}

/* Maximised and tiled windows sit flush against their edges, so the inset and
   the shadow would only show as a gap. The margin and radius go on their own —
   `is-window-flush` zeroes the tokens for the whole document — and the backend
   drops the compositor's copy of the margin at the same time. See
   `window_frame.rs`. */
.app--framed.is-flush {
  box-shadow: none;
}

.app__body {
  flex: 1;
  min-height: 0;
  display: flex;
}

.app__main {
  flex: 1;
  min-width: 0;
  min-height: 0;
  position: relative;
  overflow-y: auto;
  overscroll-behavior: contain;
  /* Lets a routed view's layout react to the space it actually has — which
     shrinks when a side panel opens — rather than only to the window's own
     width. Sizing containment costs nothing here: this element's own size was
     already fully determined by `flex: 1`, never by its content. */
  container-type: inline-size;
  container-name: app-main;
}

/* The full-screen player manages its own layout and must not scroll. */
.app__main.is-screen {
  overflow: hidden;
}

.app__titlebar {
  position: sticky;
  top: 0;
  z-index: 5;
  display: flex;
  height: 30px;
  background: linear-gradient(var(--bg) 60%, transparent);
}

.app__drag-region {
  flex: 1;
}

/* Pinned to `.app`'s own corner (not `.app__main`'s), so it is unaffected by
   the sidebar, an open side panel, or `.app__main`'s scrollbar. `.app--framed`
   already clips to `--frame-radius` and squares off when flush, so the close
   button's hover fill lands flush with the corner for free.

   Above every overlay: these are the window's controls, not the app's, and a
   dialog covering the close button would leave the window only closable from
   outside it. */
.app__window-controls {
  position: absolute;
  top: 0;
  right: 0;
  z-index: var(--z-window-controls);
  display: flex;
}

.app__window-control {
  width: 30px;
  height: 30px;
  border-radius: 0;
}

.app__window-control--close:hover {
  background: #d83b01;
  color: #fff;
}

/* Part of the window frame rather than of the app, so — like the window
   controls — they stay live over a modal: a dialog should not make the window
   unresizable. */
.app__resize-region {
  position: fixed;
  z-index: var(--z-window-controls);
}

.app__resize-region--north,
.app__resize-region--south {
  /* Starting where the corner boxes end, so the two never overlap. */
  right: calc(var(--frame-inset) - var(--resize-outer) + var(--resize-corner));
  left: calc(var(--frame-inset) - var(--resize-outer) + var(--resize-corner));
  height: var(--resize-band);
  cursor: ns-resize;
}

.app__resize-region--north {
  top: calc(var(--frame-inset) - var(--resize-outer));
}

.app__resize-region--south {
  bottom: calc(var(--frame-inset) - var(--resize-outer));
}

.app__resize-region--east,
.app__resize-region--west {
  top: calc(var(--frame-inset) - var(--resize-outer) + var(--resize-corner));
  bottom: calc(var(--frame-inset) - var(--resize-outer) + var(--resize-corner));
  width: var(--resize-band);
  cursor: ew-resize;
}

.app__resize-region--east {
  right: calc(var(--frame-inset) - var(--resize-outer));
}

.app__resize-region--west {
  left: calc(var(--frame-inset) - var(--resize-outer));
}

/* The diagonals are a little larger than the edges, as they are aimed at
   rather than swept into. */
.app__resize-region--north-east,
.app__resize-region--north-west,
.app__resize-region--south-east,
.app__resize-region--south-west {
  width: var(--resize-corner);
  height: var(--resize-corner);
}

.app__resize-region--north-east {
  top: calc(var(--frame-inset) - var(--resize-outer));
  right: calc(var(--frame-inset) - var(--resize-outer));
  cursor: nesw-resize;
}

.app__resize-region--north-west {
  top: calc(var(--frame-inset) - var(--resize-outer));
  left: calc(var(--frame-inset) - var(--resize-outer));
  cursor: nwse-resize;
}

.app__resize-region--south-east {
  right: calc(var(--frame-inset) - var(--resize-outer));
  bottom: calc(var(--frame-inset) - var(--resize-outer));
  cursor: nwse-resize;
}

.app__resize-region--south-west {
  bottom: calc(var(--frame-inset) - var(--resize-outer));
  left: calc(var(--frame-inset) - var(--resize-outer));
  cursor: nesw-resize;
}

.app__scan {
  position: fixed;
  left: 50%;
  /* Fixed, so it measures from the surface: the shadow margin has to be added
     back or it sits that much lower than the player bar it rides above. */
  bottom: calc(var(--frame-inset) + var(--player-height) + 14px);
  z-index: 200;
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: 340px;
  padding: 8px 14px;
  border-radius: 999px;
  background: var(--bg-elevated);
  border: 0.5px solid var(--separator);
  box-shadow: var(--shadow-popover);
  font-size: 12px;
  transform: translateX(-50%);
}

.app__scan-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--accent);
  animation: pulse 1.1s ease-in-out infinite;
  flex: none;
}

@keyframes pulse {
  0%,
  100% {
    opacity: 0.35;
  }
  50% {
    opacity: 1;
  }
}
</style>
