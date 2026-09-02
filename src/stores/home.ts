import { defineStore } from "pinia";
import { computed, ref } from "vue";
import * as api from "@/lib/api";
import type { HomeShelves, MixKind, MixSummary } from "@/lib/types";

/**
 * The home page's shelves, and the mixes pinned into the sidebar.
 *
 * The mixes themselves are generated and held by the backend for the life of
 * the process, so this store is only a view onto them — refreshing it will not
 * reshuffle a mix that is being listened to.
 */
export const useHomeStore = defineStore("home", () => {
  const shelves = ref<HomeShelves | null>(null);
  const pinned = ref<MixSummary[]>([]);
  const loading = ref(false);

  /** A mix needs a few songs before it is worth offering. */
  const MIN_MIX_LENGTH = 5;

  const mixes = computed(() => shelves.value?.mixes ?? []);
  const picks = computed(() => shelves.value?.picks ?? []);
  const recentPlaylists = computed(() => shelves.value?.recentPlaylists ?? []);
  /** Nothing has been listened to yet, so every shelf is empty for one reason. */
  const isEmpty = computed(() => (shelves.value?.playTotal ?? 0) === 0);

  function isReady(mix: MixSummary) {
    return mix.trackCount >= MIN_MIX_LENGTH;
  }

  async function refresh() {
    loading.value = true;
    try {
      const [next, pins] = await Promise.all([api.homeShelves(), api.listPinnedMixes()]);
      shelves.value = next;
      pinned.value = pins;
    } finally {
      loading.value = false;
    }
  }

  /** Only the pinned list, for the sidebar, without rebuilding the shelves. */
  async function refreshPinned() {
    pinned.value = await api.listPinnedMixes();
  }

  async function setPinned(kind: MixKind, next: boolean) {
    await api.setMixPinned(kind, next);
    await refresh();
  }

  /** Build the mixes again from current history. */
  async function regenerate() {
    await api.refreshMixes();
    await refresh();
  }

  function mix(kind: string): MixSummary | null {
    return (
      mixes.value.find((m) => m.kind === kind) ??
      pinned.value.find((m) => m.kind === kind) ??
      null
    );
  }

  return {
    shelves,
    pinned,
    loading,
    mixes,
    picks,
    recentPlaylists,
    isEmpty,
    isReady,
    refresh,
    refreshPinned,
    setPinned,
    regenerate,
    mix,
  };
});
