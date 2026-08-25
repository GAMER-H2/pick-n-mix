<script setup lang="ts">
/** Picker shown by the "Add to Playlist" context menu action. */
import { computed, ref } from "vue";
import PnmIcon from "./icons/PnmIcon.vue";
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
  <Transition name="fade">
    <div v-if="ui.addToPlaylistFor" class="scrim" @click.self="close">
      <div class="dialog" role="dialog" aria-label="Add to playlist">
        <header class="dialog__head">
          <h2>Add to Playlist</h2>
          <button class="icon-button" aria-label="Close" @click="close">
            <PnmIcon name="close" :size="17" />
          </button>
        </header>
        <p class="dialog__subtitle">Adding {{ label }}</p>

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
            @keydown.esc="creating = false"
          />
          <button class="pill-button" @click="createAndAdd">Create</button>
        </div>
        <button v-else class="dialog__item dialog__item--new" @click="creating = true">
          <PnmIcon name="plus" :size="16" />
          <span>New Playlist…</span>
        </button>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.scrim {
  position: fixed;
  inset: 0;
  z-index: 500;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.28);
  backdrop-filter: blur(3px);
}

.dialog {
  width: 340px;
  max-height: 70vh;
  display: flex;
  flex-direction: column;
  padding: 16px;
  border-radius: var(--radius-lg);
  background: var(--bg-elevated);
  box-shadow: var(--shadow-popover);
}

.dialog__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.dialog__head h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.dialog__subtitle {
  margin: 2px 0 12px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.dialog__list {
  flex: 1;
  min-height: 0;
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
