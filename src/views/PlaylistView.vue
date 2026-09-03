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
import CollectionHeader from "@/components/collections/CollectionHeader.vue";
import TrackList, { type TrackListItem } from "@/components/collections/TrackList.vue";
import BounceDialog from "@/components/dialogs/BounceDialog.vue";
import { formatTotal } from "@/lib/format";
import * as api from "@/lib/api";
import { usePlaylistStore } from "@/stores/playlists";
import { usePlayerStore } from "@/stores/player";
import { useMixerStore } from "@/stores/mixer";
import { useMasterMixStore } from "@/stores/masterMix";
import { useUiStore } from "@/stores/ui";
import { useCollectionPlayback } from "@/composables/useCollectionPlayback";
import { useMenu } from "@/composables/useMenu";
import type { ResolvedEntry } from "@/lib/types";

const route = useRoute();
const playlists = usePlaylistStore();
const player = usePlayerStore();
const mixer = useMixerStore();
const masterMix = useMasterMixStore();
const ui = useUiStore();
const { openMenu } = useMenu();
const { playOrToggle } = useCollectionPlayback();

const editingDescription = ref(false);
const draftDescription = ref("");
const bounceOpen = ref(false);

const playlist = computed(() => playlists.open);
const items = computed<ResolvedEntry[]>(() => playlist.value?.items ?? []);
const available = computed(() => items.value.filter((i) => i.track !== null));
const totalDuration = computed(() =>
  items.value.reduce((sum, i) => sum + (i.track?.durationSecs ?? i.entry.durationSecs), 0),
);
const artworkId = computed(() => playlist.value?.artwork ?? null);

const listItems = computed<TrackListItem[]>(() =>
  items.value.map((item) => ({
    key: String(item.index),
    track: item.track,
    fallbackTitle: item.entry.title,
    fallbackSubtitle: `${item.entry.album} · ${item.entry.artist}`,
    mixerOverride: !!item.entry.mixer,
  })),
);

/**
 * Covers of the first four different songs, which is what a playlist with no
 * picture of its own is drawn as.
 */
const artworkIds = computed(() => {
  const seen: string[] = [];
  for (const item of available.value) {
    const id = item.track?.artworkId;
    if (id && !seen.includes(id)) seen.push(id);
    if (seen.length === 4) break;
  }
  return seen;
});

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

const currentId = computed(() =>
  playingThisPlaylist.value ? (player.track?.id ?? null) : null,
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
function playEntry(item: ResolvedEntry) {
  return playOrToggle(isCurrent(item), () => play(item.index));
}

function playIndex(index: number) {
  const item = items.value[index];
  if (item) return playEntry(item);
}

function reorder(from: number, to: number) {
  const p = playlist.value;
  if (p) return playlists.move(p.id, from, to);
}

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

async function shuffleAndPlay() {
  if (!playlist.value) return;
  await player.setShuffle(true);
  await play(0);
}

async function openPlaylistMixer() {
  const p = playlist.value;
  if (!p) return;
  // A playlist that plays as a mix ignores the global mixer, so the panel is
  // told not to show it underneath what is being edited here.
  await mixer.editPlaylist(p.id, p.name, p.mixer, !!p.masterMix?.enabled);
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

/** The render has been started, not finished: it reports its own progress. */
function onBounced(path: string) {
  bounceOpen.value = false;
  ui.notify(`Bouncing to ${path.split(/[/\\]/).pop() ?? path}…`);
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

function openEntryMixerAt(index: number) {
  const item = items.value[index];
  if (item) return openEntryMixer(item);
}

function startEditingDescription() {
  draftDescription.value = playlist.value?.description ?? "";
  editingDescription.value = true;
}

/**
 * Queue this playlist as a whole.
 *
 * Used only for a playlist that plays as a mix: the arrangement goes in as one
 * block, since its songs overlap and cannot be spread across a queue.
 */
async function queueWholePlaylist(next: boolean) {
  const p = playlist.value;
  if (!p) return;
  await api.queuePlaylist(p.id, next);
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
      :artwork-ids="artworkIds"
      :mixer-active="!!playlist.mixer"
      :disabled="available.length === 0"
      @play="play(0)"
      @shuffle="shuffleAndPlay"
      @mixer="openPlaylistMixer"
      @menu="
        openMenu($event, {
          tracks: available.map((i) => i.track!),
          playlistOptions: {
            id: playlist!.id,
            shuffleOnly: playlist!.shuffleOnly,
            hasArtwork: !!playlist!.artwork,
            masterMixEnabled: !!playlist!.masterMix?.enabled,
            onPlayNext: () => queueWholePlaylist(true),
            onAddToQueue: () => queueWholePlaylist(false),
            onToggleShuffleOnly: toggleShuffleOnly,
            onChooseArtwork: chooseArtwork,
            onClearArtwork: clearArtwork,
          },
        })
      "
    >
      <template #actions>
        <!-- Named, like Play and Shuffle beside it: the master mixer is one
             of the things you do to a playlist, not a setting hidden behind a
             glyph. -->
        <button
          class="pill-button"
          :class="{ 'is-secondary': !playlist!.masterMix?.enabled }"
          :title="
            playlist!.masterMix?.enabled
              ? 'Master mixer: this playlist plays as a mix'
              : 'Master mixer: arrange this playlist on a timeline'
          "
          :disabled="available.length === 0"
          @click="openMasterMix"
        >
          <PnmIcon name="timeline" :size="14" />
          <span>Master Mix</span>
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

    <TrackList
      v-else
      :items="listItems"
      :current-id="currentId"
      :playing="player.playing"
      show-artwork
      show-mixer
      :reorderable="!playlist?.masterMix?.enabled"
      @play="playIndex"
      @mixer="(_, index) => openEntryMixerAt(index)"
      @reorder="reorder"
      @menu="
        (event, index) => {
          const item = items[index];
          if (item?.track) openMenu(event, { tracks: [item.track], playlistId: playlist!.id, entryIndex: item.index });
        }
      "
    />
  </div>

  <div v-else-if="playlists.loading" class="playlist__loading">Loading…</div>
  <div v-else class="playlist__loading">This playlist could not be found.</div>

  <BounceDialog
    v-if="bounceOpen && playlist"
    :playlist-id="playlist.id"
    :playlist-name="playlist.name"
    :has-artwork="!!playlist.artwork"
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
</style>
