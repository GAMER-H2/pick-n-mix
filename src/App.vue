<script setup lang="ts">
/**
 * Application shell: sidebar, the routed view, optional right-hand panels,
 * and the player bar along the bottom.
 */
import { onBeforeUnmount, onMounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import Sidebar from "./components/Sidebar.vue";
import NowPlayingBar from "./components/NowPlayingBar.vue";
import QueuePanel from "./components/QueuePanel.vue";
import NowPlayingScreen from "./components/NowPlayingScreen.vue";
import AdvancedMixer from "./components/mixer/AdvancedMixer.vue";
import ContextMenu from "./components/ContextMenu.vue";
import AddToPlaylistDialog from "./components/AddToPlaylistDialog.vue";
import { usePlayerStore } from "./stores/player";
import { useLibraryStore } from "./stores/library";
import { usePlaylistStore } from "./stores/playlists";
import { useMixerStore } from "./stores/mixer";
import { useUiStore } from "./stores/ui";
import { installShortcuts } from "./lib/keyboard";
import type { PlaybackSnapshot, QueueView, ResolvedMixer, Track } from "./lib/types";

const player = usePlayerStore();
const library = useLibraryStore();
const playlists = usePlaylistStore();
const mixer = useMixerStore();
const ui = useUiStore();

const unlisteners = ref<UnlistenFn[]>([]);
let removeShortcuts: (() => void) | null = null;

onMounted(async () => {
  // Follow the system appearance.
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const applyTheme = () =>
    document.documentElement.setAttribute("data-theme", media.matches ? "dark" : "light");
  applyTheme();
  media.addEventListener("change", applyTheme);

  removeShortcuts = installShortcuts(player, ui);

  await Promise.all([library.refresh(), playlists.refresh(), player.refresh(), mixer.refresh()]);

  unlisteners.value = await Promise.all([
    listen<PlaybackSnapshot>("playback", (e) => player.applySnapshot(e.payload)),
    listen<Track | null>("track-changed", (e) => {
      player.track = e.payload;
      // The track layer of the cascade changed with it.
      mixer.refresh();
    }),
    listen<QueueView>("queue-changed", (e) => (player.queue = e.payload)),
    listen<boolean>("playing-changed", () => player.refresh()),
    listen("queue-ended", () => player.refresh()),
    listen("library-changed", () => library.refresh()),
    listen("playlists-changed", () => playlists.refresh()),
    listen<ResolvedMixer>("mixer-changed", () => mixer.refresh()),
    listen<{ count: number; path: string }>(
      "scan-progress",
      (e) => (library.scanProgress = e.payload),
    ),
    listen<string>("engine-error", (e) => ui.notify(e.payload, "error")),
  ]);
});

onBeforeUnmount(() => {
  unlisteners.value.forEach((un) => un());
  removeShortcuts?.();
});
</script>

<template>
  <div class="app">
    <div class="app__body">
      <Sidebar />

      <main v-if="ui.nowPlayingOpen" class="app__screen">
        <NowPlayingScreen />
      </main>

      <main v-else class="app__main scroll-area">
        <!-- Draggable strip beneath the overlay title bar. -->
        <div class="app__titlebar" data-tauri-drag-region />
        <RouterView v-slot="{ Component }">
          <Transition name="fade" mode="out-in">
            <component :is="Component" />
          </Transition>
        </RouterView>
      </main>

      <Transition name="slide-panel">
        <QueuePanel v-if="ui.queueOpen && !ui.nowPlayingOpen" />
      </Transition>

      <Transition name="slide-panel">
        <AdvancedMixer v-if="mixer.panelOpen" />
      </Transition>
    </div>

    <NowPlayingBar />

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
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg);
}

.app__body {
  flex: 1;
  min-height: 0;
  display: flex;
}

.app__main {
  flex: 1;
  min-width: 0;
  position: relative;
}

.app__screen {
  flex: 1;
  min-width: 0;
  min-height: 0;
}

.app__titlebar {
  position: sticky;
  top: 0;
  z-index: 5;
  height: 30px;
  background: linear-gradient(var(--bg) 60%, transparent);
}

.app__scan {
  position: fixed;
  left: 50%;
  bottom: calc(var(--player-height) + 14px);
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
