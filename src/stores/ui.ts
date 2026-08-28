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
}

export const useUiStore = defineStore("ui", () => {
  const contextMenu = ref<ContextMenuState | null>(null);
  const infoTrack = ref<Track | null>(null);
  const duplicateTrack = ref<Track | null>(null);
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
    duplicateTrack,
    queueOpen,
    addToPlaylistFor,
    toast,
    openContextMenu,
    closeContextMenu,
    notify,
  };
});
