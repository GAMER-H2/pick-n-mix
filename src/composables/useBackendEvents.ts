/**
 * The engine's event fan-out: every `listen` subscription the shell needs,
 * plus the initial refresh that seeds the stores afterwards.
 *
 * It owns its own store instances so `App.vue` stays pure wiring, and exposes
 * a single `init()` rather than its own `onMounted` so the shell keeps the
 * mount ordering — settings, then shortcuts, then listeners, then fetch.
 */
import { onBeforeUnmount, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { usePlayerStore } from "@/stores/player";
import { useLibraryStore } from "@/stores/library";
import { usePlaylistStore } from "@/stores/playlists";
import { useMixerStore } from "@/stores/mixer";
import { useCrossfadeStore } from "@/stores/crossfade";
import { useHomeStore } from "@/stores/home";
import { useUiStore } from "@/stores/ui";
import { useMasterMixStore } from "@/stores/masterMix";
import { useBounceStore } from "@/stores/bounce";
import type { PlaybackSnapshot, QueueView, ResolvedMixer, Track } from "@/lib/types";

export function useBackendEvents() {
  const player = usePlayerStore();
  const library = useLibraryStore();
  const playlists = usePlaylistStore();
  const mixer = useMixerStore();
  const crossfade = useCrossfadeStore();
  const home = useHomeStore();
  const ui = useUiStore();
  const masterMix = useMasterMixStore();
  const bounce = useBounceStore();

  const unlisteners = ref<UnlistenFn[]>([]);

  async function init() {
    unlisteners.value = await Promise.all([
      listen<PlaybackSnapshot>("playback", (e) => player.applySnapshot(e.payload)),
      listen<Track | null>("track-changed", (e) => {
        player.track = e.payload;
        // The track layer of the cascade changed with it, but the preset list
        // and filter catalogue did not, so avoid the disk-reading variant.
        mixer.refreshLayers();
        // Starting a track is also how a mixed playlist stops being what plays,
        // so the bar has to be told to stop showing one.
        void player.refreshMasterMix();
      }),
      listen<QueueView>("queue-changed", (e) => (player.queue = e.payload)),
      listen<boolean>("playing-changed", () => player.refresh()),
      listen("queue-ended", () => player.refresh()),
      listen("master-mix-ended", () => masterMix.previewEnded()),
      listen("library-changed", () => library.refresh()),
      listen("playlists-changed", () => playlists.refresh()),
      // Only the sidebar's pinned list: rebuilding the whole home page here
      // would fight with whatever the listener is looking at.
      listen("home-changed", () => home.refreshPinned()),
      listen<ResolvedMixer>("mixer-changed", () => mixer.refresh()),
      listen<{ count: number; path: string }>(
        "scan-progress",
        (e) => (library.scanProgress = e.payload),
      ),
      listen<string>("engine-error", (e) => ui.notify(e.payload, "error")),
      listen<{ id: string; fraction: number }>("bounce-progress", (e) =>
        bounce.onProgress(e.payload.id, e.payload.fraction),
      ),
      listen<{ id: string; path: string; error: string | null }>("bounce-finished", (e) => {
        bounce.onFinished(e.payload.id, e.payload.path, e.payload.error);
        if (e.payload.error) ui.notify(e.payload.error, "error");
      }),
    ]);

    // Listeners first, then the initial fetch: an event emitted between the two
    // would otherwise be missed, leaving the UI showing nothing while the engine
    // is already playing.
    await Promise.all([
      library.refresh(),
      playlists.refresh(),
      home.refreshPinned(),
      player.refresh(),
      mixer.refresh(),
      crossfade.refresh(),
    ]);
  }

  onBeforeUnmount(() => {
    unlisteners.value.forEach((un) => un());
  });

  return { init };
}
