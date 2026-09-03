<script setup lang="ts">
/** Picker shown by the "Add to Playlist" context menu action. */
import { computed, ref } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import BaseModal from "../ui/BaseModal.vue";
import { usePlaylistStore } from "@/stores/playlists";
import { useUiStore } from "@/stores/ui";

const playlists = usePlaylistStore();
const ui = useUiStore();

const creating = ref(false);
const draftName = ref("");

const tracks = computed(() => ui.addToPlaylistFor ?? []);
const label = computed(() =>
  tracks.value.length === 1 ? `"${tracks.value[0].title}"` : `${tracks.value.length} songs`,
);

function close() {
  ui.addToPlaylistFor = null;
  creating.value = false;
  draftName.value = "";
}

async function addTo(playlistId: string, name: string) {
  const ids = tracks.value.map((t) => t.id);
  close();
  const added = await playlists.addTracks(playlistId, ids);
  ui.notify(`Added ${added} ${added === 1 ? "song" : "songs"} to ${name}`);
}

async function createAndAdd() {
  const name = draftName.value.trim();
  if (!name) return;
  const created = await playlists.create(name);
  await addTo(created.id, created.name);
}
</script>

<template>
  <BaseModal
    :open="ui.addToPlaylistFor !== null"
    title="Add to Playlist"
    :subtitle="`Adding ${label}`"
    :width="340"
    @close="close"
  >
    <div class="dialog__list scroll-area">
      <button
        v-for="playlist in playlists.summaries"
        :key="playlist.id"
        class="dialog__item"
        @click="addTo(playlist.id, playlist.name)"
      >
        <PnmIcon name="addToPlaylist" :size="16" />
        <span class="truncate">{{ playlist.name }}</span>
        <span class="dialog__count">{{ playlist.trackCount }}</span>
      </button>
      <p v-if="playlists.summaries.length === 0" class="dialog__empty">
        You have no playlists yet.
      </p>
    </div>

    <div v-if="creating" class="dialog__create">
      <input
        v-model="draftName"
        class="text-field"
        placeholder="Playlist name"
        autofocus
        @keydown.enter="createAndAdd"
      />
      <button class="pill-button" @click="createAndAdd">Create</button>
    </div>
    <button v-else class="dialog__item dialog__item--new" @click="creating = true">
      <PnmIcon name="plus" :size="16" />
      <span>New Playlist…</span>
    </button>
  </BaseModal>
</template>

<style scoped>
.dialog__list {
  margin: 0 -6px;
  padding: 0 6px;
}

.dialog__item {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 8px 9px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  text-align: left;
}

.dialog__item:hover {
  background: var(--bg-hover);
}

.dialog__item span:first-of-type {
  flex: 1;
  min-width: 0;
}

.dialog__item--new {
  margin-top: 6px;
  border-top: 1px solid var(--separator);
  border-radius: 0 0 var(--radius-sm) var(--radius-sm);
  padding-top: 12px;
  color: var(--accent);
}

.dialog__count {
  font-size: 11.5px;
  color: var(--text-tertiary);
}

.dialog__empty {
  padding: 20px 9px;
  font-size: 12.5px;
  color: var(--text-tertiary);
}

.dialog__create {
  display: flex;
  gap: 7px;
  margin-top: 10px;
  padding-top: 12px;
  border-top: 1px solid var(--separator);
}
</style>
