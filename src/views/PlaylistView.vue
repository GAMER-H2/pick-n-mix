<script setup lang="ts">
/**
 * A playlist, matching the first drawing: artwork, name, description, the
 * play/shuffle/mixer row, then the track list.
 *
 * Entries that could not be matched to anything in this library stay visible
 * and greyed out, so a shared playlist tells you what you are missing rather
 * than silently shrinking.
 */
import { computed, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { open } from "@tauri-apps/plugin-dialog";
import PnmIcon from "@/components/icons/PnmIcon.vue";
import CollectionHeader from "@/components/CollectionHeader.vue";
import TrackRow from "@/components/TrackRow.vue";
import BounceDialog from "@/components/mastermix/BounceDialog.vue";
import { formatTotal } from "@/lib/format";
import * as api from "@/lib/api";
import { usePlaylistStore } from "@/stores/playlists";
import { usePlayerStore } from "@/stores/player";
import { useMixerStore } from "@/stores/mixer";
import { useMasterMixStore } from "@/stores/masterMix";
import { useUiStore } from "@/stores/ui";
import { useDragReorder } from "@/lib/dragReorder";
import type { ResolvedEntry } from "@/lib/types";

const route = useRoute();
const playlists = usePlaylistStore();
const player = usePlayerStore();
const mixer = useMixerStore();
const masterMix = useMasterMixStore();
const ui = useUiStore();

const editingDescription = ref(false);
const draftDescription = ref("");
const bounceOpen = ref(false);

const playlist = computed(() => playlists.open);
const items = computed<ResolvedEntry[]>(() => playlist.value?.items ?? []);
const available = computed(() => items.value.filter((i) => i.track !== null));
const totalDuration = computed(() =>
  items.value.reduce((sum, i) => sum + (i.track?.durationSecs ?? i.entry.durationSecs), 0),
);
const artworkId = computed(
  () => playlist.value?.artwork ?? available.value.find((i) => i.track?.artworkId)?.track?.artworkId,
);

const meta = computed(() => {
  const p = playlist.value;
  if (!p) return "";
  const parts = [`${p.items.length} songs`, formatTotal(totalDuration.value)];
  if (p.missingCount > 0) parts.push(`${p.missingCount} not in your library`);
  if (p.masterMix?.enabled) parts.push("mixed");
  return parts.join(" · ");
});

watch(
  () => route.params.id,
  (id) => {
    if (typeof id === "string") playlists.load(id);
  },
  { immediate: true },
);

/**
 * A song in a playlist is its own thing: it carries this playlist's mixer and
 * this playlist's position. So a row only counts as playing when the queue is
 * actually running *this* playlist — the same song playing from the library or
 * another playlist leaves these rows alone.
 */
const playingThisPlaylist = computed(
  () => !!playlist.value && player.queue.context?.id === playlist.value.id,
);

function isCurrent(item: ResolvedEntry) {
  return (
    playingThisPlaylist.value && !!item.track && player.track?.id === item.track.id
  );
}

async function play(startIndex = 0) {
  if (!playlist.value) return;
  await api.playPlaylist(playlist.value.id, startIndex);
}

/** Clicking the row that is already playing toggles it instead of restarting. */
async function playEntry(item: ResolvedEntry) {
  if (isCurrent(item)) {
    await player.toggle();
    return;
  }
  await play(item.index);
}

const listEl = ref<HTMLElement | null>(null);
const { dragFrom, dropAt, isDragging, onHandleDown, onHandleMove, onHandleUp, onHandleCancel } =
  useDragReorder(listEl, async (from, to) => {
    const p = playlist.value;
    if (p) await playlists.move(p.id, from, to);
  });

async function toggleShuffleOnly() {
  const p = playlist.value;
  if (!p) return;
  await api.setPlaylistShuffleOnly(p.id, !p.shuffleOnly);
  await playlists.refresh();
}

/**
 * Replace the playlist image. The backend copies the file into the artwork
 * cache, so the picture survives the original being moved or deleted.
 */
async function chooseArtwork() {
  const p = playlist.value;
  if (!p) return;
  const selected = await open({
    multiple: false,
    title: "Choose a playlist image",
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp"] }],
  });
  if (typeof selected !== "string") return;
  try {
    await api.setPlaylistArtwork(p.id, selected);
    await playlists.refresh();
    ui.notify("Playlist image updated");
  } catch (e) {
    ui.notify(`Could not use that image: ${e}`, "error");
  }
}

async function clearArtwork() {
  const p = playlist.value;
  if (!p) return;
  await api.clearPlaylistArtwork(p.id);
  await playlists.refresh();
}

async function shuffle() {
  if (!playlist.value) return;
  await player.setShuffle(true);
  await play(0);
}

async function openPlaylistMixer() {
  const p = playlist.value;
  if (!p) return;
  await mixer.editPlaylist(p.id, p.name, p.mixer);
  mixer.panelOpen = true;
}

/**
 * The master mixer: this playlist as a timeline rather than a list.
 *
 * Opened on the playlist rather than on a song because an arrangement is a
 * property of the whole playlist — a crossfade lives in the join between two
 * songs, not in either one.
 */
async function openMasterMix() {
  const p = playlist.value;
  if (!p) return;
  await masterMix.openFor(p.id);
}

function onBounced(path: string) {
  bounceOpen.value = false;
  ui.notify(`Bounced mix to ${path}`);
}

/**
 * Per-entry override. It is written into this playlist's file, so the same
 * song in another playlist, or played from the library, is unaffected.
 */
async function openEntryMixer(item: ResolvedEntry) {
  const p = playlist.value;
  if (!p || !item.track) return;
  await mixer.editPlaylistEntry(p.id, item.index, item.entry.title, item.entry.mixer, p.mixer);
  mixer.panelOpen = true;
}

function startEditingDescription() {
  draftDescription.value = playlist.value?.description ?? "";
  editingDescription.value = true;
}

async function saveDescription() {
  const p = playlist.value;
  if (!p) return;
  editingDescription.value = false;
  await api.updatePlaylist(p.id, undefined, draftDescription.value);
  await playlists.refresh();
}
</script>

<template>
  <div v-if="playlist" class="playlist">
    <CollectionHeader
      :title="playlist.name"
      :meta="meta"
      :artwork-id="artworkId"
      :mixer-active="!!playlist.mixer"
      :disabled="available.length === 0"
      @play="play(0)"
      @shuffle="shuffle"
      @mixer="openPlaylistMixer"
      @menu="
        ui.openContextMenu({
          x: $event.clientX,
          y: $event.clientY,
          tracks: available.map((i) => i.track!),
          playlistOptions: {
            id: playlist!.id,
            shuffleOnly: playlist!.shuffleOnly,
            hasArtwork: !!playlist!.artwork,
            onToggleShuffleOnly: toggleShuffleOnly,
            onChooseArtwork: chooseArtwork,
            onClearArtwork: clearArtwork,
          },
        })
      "
    >
      <template #actions>
        <button
          class="icon-button"
          :class="{ 'is-active': playlist!.masterMix?.enabled }"
          title="Master mixer: arrange this playlist on a timeline"
          aria-label="Open the master mixer"
          :disabled="available.length === 0"
          @click="openMasterMix"
        >
          <PnmIcon name="timeline" :size="18" />
        </button>
        <button
          class="icon-button"
          title="Bounce this playlist to an audio file"
          aria-label="Bounce mix"
          :disabled="available.length === 0"
          @click="bounceOpen = true"
        >
          <PnmIcon name="bounce" :size="18" />
        </button>
      </template>
    </CollectionHeader>

    <div class="playlist__description">
      <input
        v-if="editingDescription"
        v-model="draftDescription"
        class="text-field"
        placeholder="Add a description"
        autofocus
        @keydown.enter="saveDescription"
        @blur="saveDescription"
      />
      <button v-else class="playlist__description-button" @click="startEditingDescription">
        {{ playlist.description || "Add a description" }}
      </button>
    </div>

    <p v-if="playlist.missingCount > 0" class="playlist__notice">
      <PnmIcon name="info" :size="14" />
      <span>
        {{ playlist.missingCount }}
        {{ playlist.missingCount === 1 ? "song is" : "songs are" }} not in your library. They stay
        in the file, so they will appear once the music is added.
      </span>
    </p>

    <div v-if="items.length === 0" class="playlist__empty">
      <p>This playlist is empty.</p>
      <p class="playlist__hint">
        Right-click any song in your library and choose <strong>Add to Playlist</strong>.
      </p>
    </div>

    <div v-else ref="listEl" class="playlist__list" :class="{ 'is-dragging': isDragging }">
      <template v-for="item in items" :key="item.index">
        <div v-if="dropAt === item.index && isDragging" class="playlist__drop" />

        <div
          data-row
          class="playlist__row"
          :class="{ 'is-lifted': dragFrom === item.index }"
        >
          <button
            class="playlist__grip"
            title="Drag to reorder"
            aria-label="Drag to reorder"
            @pointerdown="onHandleDown($event, item.index)"
            @pointermove="onHandleMove"
            @pointerup="onHandleUp"
            @pointercancel="onHandleCancel"
          >
            <PnmIcon name="grip" :size="15" />
          </button>

          <TrackRow
            class="playlist__row-track"
            :track="item.track"
            show-mixer
            :fallback-title="item.entry.title"
            :fallback-subtitle="`${item.entry.album} · ${item.entry.artist}`"
            show-artwork
            :current="isCurrent(item)"
            :playing="player.playing"
            :has-mixer-override="!!item.entry.mixer"
            @play="playEntry(item)"
            @mixer="openEntryMixer(item)"
            @menu="
              item.track &&
                ui.openContextMenu({
                  x: $event.clientX,
                  y: $event.clientY,
                  tracks: [item.track],
                  playlistId: playlist!.id,
                  entryIndex: item.index,
                })
            "
          />
        </div>
      </template>

      <div v-if="dropAt === items.length && isDragging" class="playlist__drop" />
    </div>
  </div>

  <div v-else-if="playlists.loading" class="playlist__loading">Loading…</div>
  <div v-else class="playlist__loading">This playlist could not be found.</div>

  <BounceDialog
    v-if="bounceOpen && playlist"
    :playlist-id="playlist.id"
    :playlist-name="playlist.name"
    @close="bounceOpen = false"
    @bounced="onBounced"
  />
</template>

<style scoped>
.playlist {
  padding: 6px 26px 40px;
}

.playlist__description {
  margin: -14px 0 18px;
  max-width: 640px;
}

.playlist__description-button {
  font-size: 13px;
  color: var(--text-secondary);
  text-align: left;
}

.playlist__description-button:hover {
  color: var(--text);
}

.playlist__notice {
  display: flex;
  align-items: flex-start;
  gap: 7px;
  margin: 0 0 14px;
  padding: 9px 11px;
  border-radius: var(--radius-sm);
  background: var(--bg-sunken);
  font-size: 11.5px;
  line-height: 1.45;
  color: var(--text-secondary);
}

.playlist__empty {
  padding: 50px 0;
  text-align: center;
  color: var(--text-tertiary);
}

.playlist__empty p {
  margin: 0 0 5px;
  font-size: 13px;
}

.playlist__hint {
  font-size: 12px !important;
}

.playlist__loading {
  padding: 60px 26px;
  text-align: center;
  font-size: 13px;
  color: var(--text-tertiary);
}

.playlist__list.is-dragging {
  cursor: grabbing;
}

/* Insertion marker, rather than animating every row out of the way. */
.playlist__drop {
  height: 2px;
  margin: 1px 8px;
  border-radius: 2px;
  background: var(--accent);
}

.playlist__row {
  display: flex;
  align-items: center;
  gap: 2px;
}

.playlist__row.is-lifted {
  opacity: 0.4;
}

.playlist__row-track {
  flex: 1;
  min-width: 0;
}

.playlist__grip {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  flex: none;
  color: var(--text-tertiary);
  cursor: grab;
  opacity: 0;
  touch-action: none;
  transition: opacity 0.12s var(--ease);
}

.playlist__row:hover .playlist__grip,
.playlist__grip:focus-visible {
  opacity: 1;
}

.playlist__grip:active {
  cursor: grabbing;
}
</style>
