import { defineStore } from "pinia";
import { ref } from "vue";
import * as api from "@/lib/api";
import type { PlaylistSummary, ResolvedPlaylist } from "@/lib/types";

export const usePlaylistStore = defineStore("playlists", () => {
  const summaries = ref<PlaylistSummary[]>([]);
  const open = ref<ResolvedPlaylist | null>(null);
  const loading = ref(false);

  async function refresh() {
    summaries.value = await api.listPlaylists();
    // Keep an open playlist in sync after an edit elsewhere.
    if (open.value) {
      open.value = await api.getPlaylist(open.value.id);
    }
  }

  async function load(id: string) {
    loading.value = true;
    try {
      open.value = await api.getPlaylist(id);
    } finally {
      loading.value = false;
    }
  }

  async function create(name: string, description?: string) {
    const created = await api.createPlaylist(name, description);
    await refresh();
    return created;
  }

  async function remove(id: string) {
    await api.deletePlaylist(id);
    if (open.value?.id === id) open.value = null;
    await refresh();
  }

  async function addTracks(playlistId: string, trackIds: string[]) {
    const added = await api.addToPlaylist(playlistId, trackIds);
    await refresh();
    return added;
  }

  async function move(playlistId: string, from: number, to: number) {
    await api.moveInPlaylist(playlistId, from, to);
    await refresh();
  }

  return { summaries, open, loading, refresh, load, create, remove, addTracks, move };
});
