import { defineStore } from "pinia";
import { ref } from "vue";
import type { Track } from "@/lib/types";

/**
 * Playlist-wide options, shown when the menu was opened from a playlist's own
 * "more" button rather than from one of its rows.
 *
 * The handlers are supplied by the playlist view because they need its loaded
 * playlist and its refresh; the menu only has to draw them.
 */
export interface PlaylistMenuOptions {
  id: string;
  shuffleOnly: boolean;
  hasArtwork: boolean;
  /**
   * Whether this playlist plays as a master mix.
   *
   * When it does, queueing it queues the arrangement as one block rather than
   * its songs one by one — an arrangement cannot be spread across a queue —
   * so the menu's queue actions are the playlist's own.
   */
  masterMixEnabled: boolean;
  onPlayNext: () => Promise<void>;
  onAddToQueue: () => Promise<void>;
  onToggleShuffleOnly: () => Promise<void>;
  onChooseArtwork: () => Promise<void>;
  onClearArtwork: () => Promise<void>;
}

export interface ContextMenuState {
  x: number;
  y: number;
  tracks: Track[];
  /** Set when the menu was opened from inside a playlist. */
  playlistId?: string;
  entryIndex?: number;
  /** Set when the menu was opened from a playlist header. */
  playlistOptions?: PlaylistMenuOptions;
  /** Called after a menu action is chosen, before that action runs. */
  onSelect?: () => unknown;
}

export const useUiStore = defineStore("ui", () => {
  const contextMenu = ref<ContextMenuState | null>(null);
  const infoTrack = ref<Track | null>(null);
  /**
   * Whether the information bubble is showing the mix that is playing.
   *
   * A flag rather than a copy of the mix: what it describes is live — the
   * arrangement, the stream the engine built from it, and where the output is
   * going — so the popover reads all three from the player and closes itself
   * if the mix stops.
   */
  const infoMixOpen = ref(false);
  const duplicateTrack = ref<Track | null>(null);
  const settingsOpen = ref(false);
  const queueOpen = ref(false);
  const addToPlaylistFor = ref<Track[] | null>(null);
  const toast = ref<{ message: string; kind: "info" | "error" } | null>(null);
  let toastTimer: number | undefined;

  function openContextMenu(state: ContextMenuState) {
    contextMenu.value = state;
  }

  function closeContextMenu() {
    contextMenu.value = null;
  }

  function notify(message: string, kind: "info" | "error" = "info") {
    toast.value = { message, kind };
    window.clearTimeout(toastTimer);
    toastTimer = window.setTimeout(() => (toast.value = null), 4000);
  }

  return {
    contextMenu,
    infoTrack,
    infoMixOpen,
    duplicateTrack,
    settingsOpen,
    queueOpen,
    addToPlaylistFor,
    toast,
    openContextMenu,
    closeContextMenu,
    notify,
  };
});
