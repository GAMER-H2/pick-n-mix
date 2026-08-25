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
import PnmIcon from "@/components/icons/PnmIcon.vue";
import CollectionHeader from "@/components/CollectionHeader.vue";
import TrackRow from "@/components/TrackRow.vue";
import { formatTotal } from "@/lib/format";
import * as api from "@/lib/api";
import { usePlaylistStore } from "@/stores/playlists";
import { usePlayerStore } from "@/stores/player";
import { useMixerStore } from "@/stores/mixer";
import { useUiStore } from "@/stores/ui";
import type { ResolvedEntry } from "@/lib/types";

const route = useRoute();
const playlists = usePlaylistStore();
const player = usePlayerStore();
const mixer = useMixerStore();
const ui = useUiStore();

const editingDescription = ref(false);
const draftDescription = ref("");

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
  return parts.join(" · ");
});

watch(
  () => route.params.id,
  (id) => {
    if (typeof id === "string") playlists.load(id);
  },
  { immediate: true },
);

async function play(startIndex = 0) {
  if (!playlist.value) return;
  await api.playPlaylist(playlist.value.id, startIndex);
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
        })
      "
    >
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

    <div v-else class="playlist__list">
      <TrackRow
        v-for="item in items"
        :key="item.index"
        :track="item.track"
        show-mixer
        :fallback-title="item.entry.title"
        :fallback-subtitle="`${item.entry.album} · ${item.entry.artist}`"
        show-artwork
        :current="!!item.track && player.track?.id === item.track.id"
        :playing="player.playing"
        :has-mixer-override="!!item.entry.mixer"
        @play="play(item.index)"
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
  </div>

  <div v-else-if="playlists.loading" class="playlist__loading">Loading…</div>
  <div v-else class="playlist__loading">This playlist could not be found.</div>
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
