<script setup lang="ts">
/**
 * Navigation and playlists, matching the drawings' left column.
 *
 * Playlist rows are the app's other handle on a playlist: dragged by their
 * grip to reorder the list, right-clicked (or opened from the row's own "more"
 * button) for rename, share and delete — the same actions the playlist page
 * offers, through the same composable.
 */
import { ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import PnmIcon from "../icons/PnmIcon.vue";
import { usePlaylistStore } from "@/stores/playlists";
import { useHomeStore } from "@/stores/home";
import { useMixerStore } from "@/stores/mixer";
import { useUiStore } from "@/stores/ui";
import { canGoBack, canGoForward } from "@/lib/navigation";
import { useDragReorder } from "@/lib/dragReorder";
import { useMenu } from "@/composables/useMenu";
import { usePlaylistActions } from "@/composables/usePlaylistActions";
import type { PlaylistSummary } from "@/lib/types";

const route = useRoute();
const router = useRouter();
const playlists = usePlaylistStore();
const home = useHomeStore();
const mixer = useMixerStore();
const ui = useUiStore();
const { openMenu } = useMenu();
const { askRename, askRemove, share, importFile, reorder } = usePlaylistActions();

const creating = ref(false);
const draftName = ref("");

/** Only the playlist rows carry `data-row`, so pinned mixes are never a drop target. */
const listEl = ref<HTMLElement | null>(null);
const { dragFrom, dropAt, isDragging, onHandleDown, onHandleMove, onHandleUp, onHandleCancel } =
  useDragReorder(listEl, reorder);

function openPlaylistMenu(event: MouseEvent, playlist: PlaylistSummary) {
  openMenu(event, {
    tracks: [],
    items: [
      { label: "Rename…", icon: "edit", action: () => askRename(playlist.id, playlist.name) },
      { label: "Share…", icon: "share", action: () => share(playlist.id, playlist.name) },
      {
        label: "Delete Playlist",
        icon: "trash",
        separated: true,
        danger: true,
        action: () => askRemove(playlist.id, playlist.name),
      },
    ],
  });
}

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

function isMixOpen(kind: string) {
  return route.name === "mix" && route.params.kind === kind;
}

function openSettings() {
  ui.contextMenu = null;
  ui.infoTrack = null;
  mixer.popoverOpen = false;
  ui.settingsOpen = true;
}
</script>

<template>
  <nav class="sidebar">
    <!-- Leaves room for the traffic lights under the overlay title bar. -->
    <div class="sidebar__drag" data-tauri-drag-region>
      <button
        class="icon-button sidebar__nav-button sidebar__settings-button"
        type="button"
        title="Settings"
        aria-label="Open settings"
        @click="openSettings"
      >
        <PnmIcon name="settings" :size="17" />
      </button>
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

    <div ref="listEl" class="sidebar__playlists scroll-area">
      <!-- Pinned mixes sit above real playlists: they are generated, so a
           listener should be able to tell them apart at a glance. -->
      <RouterLink
        v-for="mix in home.pinned"
        :key="mix.kind"
        :to="{ name: 'mix', params: { kind: mix.kind } }"
        class="sidebar__playlist sidebar__playlist--mix"
        :class="{ 'is-active': isMixOpen(mix.kind) }"
      >
        <span class="truncate">{{ mix.name }}</span>
        <PnmIcon name="shuffle" :size="12" class="sidebar__badge" title="A generated mix" />
      </RouterLink>

      <div v-if="home.pinned.length" class="sidebar__divider sidebar__divider--inline" />

      <div
        v-if="playlists.summaries.length === 0 && !creating && home.pinned.length === 0"
        class="sidebar__empty"
      >
        No playlists yet
      </div>

      <template v-for="(playlist, index) in playlists.summaries" :key="playlist.id">
        <div v-if="isDragging && dropAt === index" class="sidebar__drop" />
        <RouterLink
          data-row
          :to="{ name: 'playlist', params: { id: playlist.id } }"
          class="sidebar__playlist"
          :class="{
            'is-active': isPlaylistOpen(playlist.id),
            'is-lifted': dragFrom === index,
          }"
          @contextmenu.prevent="openPlaylistMenu($event, playlist)"
        >
          <button
            class="sidebar__grip"
            title="Drag to reorder"
            :aria-label="`Drag to reorder ${playlist.name}`"
            @click.prevent
            @pointerdown="onHandleDown($event, index)"
            @pointermove="onHandleMove"
            @pointerup="onHandleUp"
            @pointercancel="onHandleCancel"
          >
            <PnmIcon name="grip" :size="13" />
          </button>

          <span class="truncate">{{ playlist.name }}</span>

          <PnmIcon
            v-if="playlist.hasMixer"
            name="mixer"
            :size="13"
            class="sidebar__badge"
            title="This playlist has its own mixer settings"
          />
          <!-- Only once the timeline is what plays: a built-but-disabled
               master mix leaves the playlist playing as a plain list. -->
          <PnmIcon
            v-if="playlist.masterMixEnabled"
            name="timeline"
            :size="13"
            class="sidebar__badge"
            title="This playlist plays as a master mix"
          />
          <button
            class="sidebar__more"
            :title="`More for ${playlist.name}`"
            :aria-label="`More options for ${playlist.name}`"
            @click.prevent.stop="openPlaylistMenu($event, playlist)"
          >
            <PnmIcon name="more" :size="14" />
          </button>
        </RouterLink>
      </template>
      <div v-if="isDragging && dropAt === playlists.summaries.length" class="sidebar__drop" />

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

    <div class="sidebar__actions">
      <button class="sidebar__new" @click="creating = true">
        <PnmIcon name="plus" :size="15" />
        <span>Create New Playlist</span>
      </button>
      <!-- The other half of sharing: a playlist file someone sent you. -->
      <button
        class="icon-button sidebar__import"
        type="button"
        title="Import a playlist file"
        aria-label="Import a playlist file"
        @click="importFile"
      >
        <PnmIcon name="importFile" :size="16" />
      </button>
    </div>

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
}

.sidebar__settings-button {
  margin-left: 1px;
  order: 2;
}

/* macOS keeps Settings beside the history buttons, clear of the traffic
   lights. Linux uses client-side decorations, so Settings moves to the free
   opposite edge while Back/Forward remain grouped on the right. */
:global(html.is-custom-titlebar) .sidebar__drag {
  justify-content: space-between;
}

:global(html.is-custom-titlebar) .sidebar__settings-button {
  margin-left: 0;
  order: 0;
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

/* Separates pinned mixes from the playlists below them. */
.sidebar__divider--inline {
  margin: 6px 8px;
}

.sidebar__playlist--mix .sidebar__badge {
  color: var(--text-tertiary);
}

/* A generated mix has no grip — nothing about it can be reordered — so it
   takes the space one would occupy as padding instead, and the names in the
   list stay on one left edge. */
.sidebar__playlist--mix {
  padding-left: 26px;
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
  gap: 6px;
  height: 30px;
  padding: 0 6px 0 10px;
  border-radius: var(--radius-sm);
  font-size: 12.5px;
  color: var(--text);
  text-decoration: none;
}

.sidebar__playlist .truncate {
  flex: 1;
  min-width: 0;
}

/* The grip and the "more" button are secondary: they appear on hover, or
   while this row is the one being dragged, so a resting sidebar is just a
   list of names. Both stay visible once focused, for keyboard users. */
.sidebar__grip,
.sidebar__more {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 26px;
  margin: 0 -2px;
  opacity: 0;
  color: var(--text-tertiary);
}

.sidebar__grip {
  cursor: grab;
  margin-left: -8px;
}

.sidebar__playlist:hover .sidebar__grip,
.sidebar__playlist:hover .sidebar__more,
.sidebar__grip:focus-visible,
.sidebar__more:focus-visible,
.sidebar__playlist.is-lifted .sidebar__grip {
  opacity: 1;
}

.sidebar__more:hover {
  color: var(--text);
}

.sidebar__playlist.is-lifted {
  opacity: 0.5;
}

/* Insertion marker, matching the queue's. */
.sidebar__drop {
  height: 2px;
  margin: 1px 8px;
  border-radius: 2px;
  background: var(--accent);
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

.sidebar__actions {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-top: 8px;
}

.sidebar__new {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 7px;
  height: 32px;
  padding: 0 10px;
  border-radius: var(--radius-sm);
  font-size: 12.5px;
  color: var(--text-secondary);
}

.sidebar__import {
  flex: none;
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
