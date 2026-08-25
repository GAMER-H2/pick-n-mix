import { defineStore } from "pinia";
import { computed, ref } from "vue";
import * as api from "@/lib/api";
import type { Album, Artist, ScanReport, Track } from "@/lib/types";

export const useLibraryStore = defineStore("library", () => {
  const tracks = ref<Track[]>([]);
  const albums = ref<Album[]>([]);
  const artists = ref<Artist[]>([]);
  const folders = ref<string[]>([]);
  const loading = ref(false);
  const scanning = ref(false);
  const scanProgress = ref<{ count: number; path: string } | null>(null);
  const lastReport = ref<ScanReport | null>(null);

  const isEmpty = computed(() => !loading.value && tracks.value.length === 0);
  const byId = computed(() => new Map(tracks.value.map((t) => [t.id, t])));

  async function refresh() {
    loading.value = true;
    try {
      const [t, al, ar, f] = await Promise.all([
        api.listTracks(),
        api.listAlbums(),
        api.listArtists(),
        api.listFolders(),
      ]);
      tracks.value = t;
      albums.value = al;
      artists.value = ar;
      folders.value = f;
    } finally {
      loading.value = false;
    }
  }

  async function addFolder(path: string) {
    folders.value = await api.addFolder(path);
    await scan();
  }

  async function removeFolder(path: string) {
    folders.value = await api.removeFolder(path);
    await refresh();
  }

  async function scan() {
    if (scanning.value) return;
    scanning.value = true;
    scanProgress.value = null;
    try {
      lastReport.value = await api.scanLibrary();
      await refresh();
    } finally {
      scanning.value = false;
      scanProgress.value = null;
    }
  }

  return {
    tracks,
    albums,
    artists,
    folders,
    loading,
    scanning,
    scanProgress,
    lastReport,
    isEmpty,
    byId,
    refresh,
    addFolder,
    removeFolder,
    scan,
  };
});
