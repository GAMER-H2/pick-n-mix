import { defineStore } from "pinia";
import { ref } from "vue";
import type { Track } from "@/lib/types";

export interface ContextMenuState {
  x: number;
  y: number;
  tracks: Track[];
  /** Set when the menu was opened from inside a playlist. */
  playlistId?: string;
  entryIndex?: number;
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
