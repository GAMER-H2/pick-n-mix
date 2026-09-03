<script setup lang="ts">
/**
 * A generated mix, shown as a playlist.
 *
 * The list is held by the backend for the session, so this page is a stable
 * thing that can be played, pinned and saved rather than a query that changes
 * every time it is opened. Saving takes a copy into a real playlist, which is
 * the only way to keep a mix past the next regeneration.
 */
import { computed, ref } from "vue";
import { useRoute } from "vue-router";
import PnmIcon from "@/components/icons/PnmIcon.vue";
import CollectionHeader from "@/components/collections/CollectionHeader.vue";
import TrackList from "@/components/collections/TrackList.vue";
import EmptyState from "@/components/ui/EmptyState.vue";
import * as api from "@/lib/api";
import { useHomeStore } from "@/stores/home";
import { usePlayerStore } from "@/stores/player";
import { usePlaylistStore } from "@/stores/playlists";
import { useUiStore } from "@/stores/ui";
import { useCollectionMeta } from "@/composables/useCollectionMeta";
import { useCollectionPlayback } from "@/composables/useCollectionPlayback";
import { useMenu } from "@/composables/useMenu";
import { useRouteParamLoader } from "@/composables/useRouteParamLoader";
import type { MixKind, Track } from "@/lib/types";

const route = useRoute();
const home = useHomeStore();
const player = usePlayerStore();
const playlists = usePlaylistStore();
const ui = useUiStore();
const { openMenu } = useMenu();

const tracks = ref<Track[]>([]);
const saving = ref(false);
const savePickerOpen = ref(false);

const kind = computed(() => String(route.params.kind) as MixKind);
const summary = computed(() => home.mix(kind.value));
const artworkId = computed(() => tracks.value.find((t) => t.artworkId)?.artworkId ?? null);
const items = computed(() => tracks.value.map((track) => ({ track })));
const { meta } = useCollectionMeta(tracks);

const { loading } = useRouteParamLoader("kind", async (next) => {
  if (!home.shelves) await home.refresh();
  // The route only ever carries one of the three known mix kinds.
  tracks.value = await api.mixTracks(next as MixKind);
});

/** A mix is only "playing" when the queue is actually running this mix. */
const playingThisMix = computed(() => player.queue.context?.id === kind.value);
const currentId = computed(() =>
  playingThisMix.value ? (player.track?.id ?? null) : null,
);

function isCurrent(track: Track) {
  return playingThisMix.value && player.track?.id === track.id;
}

const { playOrToggle, shuffleAndPlay } = useCollectionPlayback();

async function play(startIndex = 0) {
  await api.playMix(kind.value, startIndex);
}

/** Clicking the row already playing toggles it rather than restarting it. */
function playTrack(index: number) {
  const track = tracks.value[index];
  return playOrToggle(isCurrent(track), () => play(index));
}

function shuffle() {
  return shuffleAndPlay(() => play(0));
}

async function save(playlistId?: string) {
  saving.value = true;
  try {
    const created = await api.saveMixToPlaylist(kind.value, playlistId);
    savePickerOpen.value = false;
    await playlists.refresh();
    ui.notify(`Saved to ${created.name}`);
  } catch (error) {
    ui.notify(`Could not save that mix: ${error}`, "error");
  } finally {
    saving.value = false;
  }
}

async function togglePinned() {
  if (!summary.value) return;
  await home.setPinned(kind.value, !summary.value.pinned);
}

/** Build this mix again from current listening history. */
async function regenerate() {
  await home.regenerate();
  tracks.value = await api.mixTracks(kind.value);
}
</script>

<template>
  <div class="mix">
    <CollectionHeader
      :title="summary?.name ?? 'Mix'"
      :subtitle="summary?.description"
      :meta="meta"
      :artwork-id="artworkId"
      :show-mixer="false"
      :disabled="tracks.length === 0"
      @play="play(0)"
      @shuffle="shuffle"
      @menu="openMenu($event, { tracks })"
    />

    <div class="mix__actions">
      <button class="mix__action" :disabled="saving" @click="savePickerOpen = !savePickerOpen">
        <PnmIcon name="addToPlaylist" :size="14" />
        <span>Save as playlist</span>
      </button>
      <button
        class="mix__action"
        :class="{ 'is-on': summary?.pinned }"
        @click="togglePinned"
      >
        <PnmIcon name="queue" :size="14" />
        <span>{{ summary?.pinned ? "Pinned to sidebar" : "Pin to sidebar" }}</span>
      </button>
      <button class="mix__action" title="Build this mix again" @click="regenerate">
        <PnmIcon name="shuffle" :size="14" />
        <span>Regenerate</span>
      </button>
    </div>

    <!-- Saving into an existing playlist appends; saving with nothing chosen
         creates a new one named after the mix. -->
    <div v-if="savePickerOpen" class="save">
      <button class="save__option" :disabled="saving" @click="save()">
        <PnmIcon name="plus" :size="13" />
        <span>New playlist</span>
      </button>
      <button
        v-for="playlist in playlists.summaries"
        :key="playlist.id"
        class="save__option"
        :disabled="saving"
        @click="save(playlist.id)"
      >
        <span class="truncate">{{ playlist.name }}</span>
      </button>
      <p v-if="playlists.summaries.length === 0" class="save__empty">
        You have no playlists to add to yet.
      </p>
    </div>

    <EmptyState
      v-if="!loading && tracks.length === 0"
      compact
      message="There is not enough listening history to build this mix yet. Play a few things and come back."
    />

    <TrackList
      v-else
      :items="items"
      :current-id="currentId"
      :playing="player.playing"
      show-artwork
      @play="playTrack"
      @menu="(event, index) => { const track = tracks[index]; if (track) openMenu(event, { tracks: [track] }); }"
    />
  </div>
</template>

<style scoped>
.mix {
  padding: 6px 26px 40px;
}

.mix__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: -8px 0 14px;
}

.mix__action {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 11px;
  border-radius: 999px;
  border: 1px solid var(--separator-strong);
  font-size: 12px;
  color: var(--text-secondary);
}

.mix__action:hover:not(:disabled) {
  color: var(--text);
  border-color: var(--text-tertiary);
}

.mix__action.is-on {
  color: var(--accent);
  border-color: var(--accent);
}

.save {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 16px;
  padding: 10px;
  border-radius: var(--radius);
  background: var(--bg-sunken);
  border: 0.5px solid var(--separator);
}

.save__option {
  display: flex;
  align-items: center;
  gap: 6px;
  max-width: 220px;
  padding: 5px 10px;
  border-radius: var(--radius-sm);
  background: var(--bg-elevated);
  border: 0.5px solid var(--separator);
  font-size: 12px;
}

.save__option:hover:not(:disabled) {
  border-color: var(--accent);
  color: var(--accent);
}

.save__empty {
  margin: 2px 4px;
  font-size: 11.5px;
  color: var(--text-tertiary);
}
</style>
