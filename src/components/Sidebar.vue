<script setup lang="ts">
/** Navigation and playlists, matching the drawings' left column. */
import { ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import PnmIcon from "./icons/PnmIcon.vue";
import { usePlaylistStore } from "@/stores/playlists";
import { useUiStore } from "@/stores/ui";
import { canGoBack, canGoForward } from "@/lib/navigation";

const route = useRoute();
const router = useRouter();
const playlists = usePlaylistStore();
const ui = useUiStore();

const creating = ref(false);
const draftName = ref("");

async function create() {
  const name = draftName.value.trim();
  if (!name) {
    creating.value = false;
    return;
  }
  const created = await playlists.create(name);
  creating.value = false;
  draftName.value = "";
  router.push({ name: "playlist", params: { id: created.id } });
}

function isPlaylistOpen(id: string) {
  return route.name === "playlist" && route.params.id === id;
}
</script>

<template>
  <nav class="sidebar">
    <!-- Leaves room for the traffic lights under the overlay title bar. -->
    <div class="sidebar__drag" data-tauri-drag-region>
      <div class="sidebar__nav">
        <button
          class="icon-button sidebar__nav-button"
          :disabled="!canGoBack"
          title="Back"
          aria-label="Go back"
          @click="router.back()"
        >
          <PnmIcon name="chevronLeft" :size="17" />
        </button>
        <button
          class="icon-button sidebar__nav-button"
          :disabled="!canGoForward"
          title="Forward"
          aria-label="Go forward"
          @click="router.forward()"
        >
          <PnmIcon name="chevronRight" :size="17" />
        </button>
      </div>
    </div>

    <div class="sidebar__primary">
      <RouterLink to="/" class="sidebar__link" :class="{ 'is-active': route.name === 'home' }">
        <PnmIcon name="home" :size="19" />
        <span>Home</span>
      </RouterLink>
      <RouterLink
        to="/library"
        class="sidebar__link"
        :class="{ 'is-active': ['library', 'album', 'artist'].includes(String(route.name)) }"
      >
        <PnmIcon name="library" :size="19" />
        <span>Library</span>
      </RouterLink>
    </div>

    <div class="sidebar__divider" />

    <div class="sidebar__playlists scroll-area">
      <div v-if="playlists.summaries.length === 0 && !creating" class="sidebar__empty">
        No playlists yet
      </div>

      <RouterLink
        v-for="playlist in playlists.summaries"
        :key="playlist.id"
        :to="{ name: 'playlist', params: { id: playlist.id } }"
        class="sidebar__playlist"
        :class="{ 'is-active': isPlaylistOpen(playlist.id) }"
      >
        <span class="truncate">{{ playlist.name }}</span>
        <PnmIcon
          v-if="playlist.hasMixer"
          name="mixer"
          :size="13"
          class="sidebar__badge"
          title="This playlist has its own mixer settings"
        />
      </RouterLink>

      <div v-if="creating" class="sidebar__create">
        <input
          v-model="draftName"
          class="text-field"
          placeholder="Playlist name"
          autofocus
          @keydown.enter="create"
          @keydown.esc="creating = false"
          @blur="create"
        />
      </div>
    </div>

    <button class="sidebar__new" @click="creating = true">
      <PnmIcon name="plus" :size="15" />
      <span>Create New Playlist</span>
    </button>

    <div v-if="ui.toast" class="sidebar__toast" :class="{ 'is-error': ui.toast.kind === 'error' }">
      {{ ui.toast.message }}
    </div>
  </nav>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  width: var(--sidebar-width);
  flex: none;
  padding: 0 10px 10px;
  background: var(--bg-sidebar);
  border-right: 1px solid var(--separator);
}

.sidebar__drag {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  height: 38px;
  flex: none;
}

.sidebar__nav {
  display: flex;
  gap: 1px;
  /* Not part of the drag region, or the buttons could not be clicked. */
  -webkit-app-region: no-drag;
}

.sidebar__nav-button {
  width: 26px;
  height: 26px;
}

.sidebar__primary {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.sidebar__link {
  display: flex;
  align-items: center;
  gap: 11px;
  height: 34px;
  padding: 0 10px;
  border-radius: var(--radius-sm);
  font-size: 13.5px;
  font-weight: 500;
  color: var(--text);
  text-decoration: none;
}

.sidebar__link:hover {
  background: var(--bg-hover);
}

.sidebar__link.is-active {
  color: var(--accent);
  background: var(--accent-tint);
}

.sidebar__divider {
  height: 1px;
  margin: 10px 10px;
  background: var(--separator);
}

.sidebar__playlists {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.sidebar__empty {
  padding: 6px 10px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.sidebar__playlist {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  border-radius: var(--radius-sm);
  font-size: 12.5px;
  color: var(--text);
  text-decoration: none;
}

.sidebar__playlist:hover {
  background: var(--bg-hover);
}

.sidebar__playlist.is-active {
  background: var(--bg-active);
  font-weight: 500;
}

.sidebar__badge {
  color: var(--accent);
  flex: none;
}

.sidebar__create {
  padding: 4px 2px;
}

.sidebar__new {
  display: flex;
  align-items: center;
  gap: 7px;
  height: 32px;
  padding: 0 10px;
  margin-top: 8px;
  border-radius: var(--radius-sm);
  font-size: 12.5px;
  color: var(--text-secondary);
}

.sidebar__new:hover {
  background: var(--bg-hover);
  color: var(--text);
}

.sidebar__toast {
  margin-top: 8px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  background: var(--bg-active);
  font-size: 11.5px;
  line-height: 1.4;
}

.sidebar__toast.is-error {
  background: rgba(215, 55, 63, 0.14);
  color: #d7373f;
}
</style>
