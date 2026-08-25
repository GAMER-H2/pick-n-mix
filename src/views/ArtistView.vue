<script setup lang="ts">
/**
 * An artist page: a round portrait header, then their albums, then everything
 * of theirs in one list. Designed to sit alongside the album and playlist
 * pages, which the drawings did define.
 */
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import Artwork from "@/components/Artwork.vue";
import CollectionHeader from "@/components/CollectionHeader.vue";
import TrackRow from "@/components/TrackRow.vue";
import { formatTotal } from "@/lib/format";
import * as api from "@/lib/api";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";
import { stableAlbumId } from "@/lib/ids";
import type { Track } from "@/lib/types";

const route = useRoute();
const router = useRouter();
const player = usePlayerStore();
const ui = useUiStore();

const tracks = ref<Track[]>([]);
const loading = ref(false);

const name = computed(() => tracks.value[0]?.albumArtist || tracks.value[0]?.artist || "Artist");
const total = computed(() => tracks.value.reduce((sum, t) => sum + t.durationSecs, 0));

/** Group into albums, newest first, for the shelf above the song list. */
const albums = computed(() => {
  const map = new Map<string, { id: string; name: string; year: number | null; artworkId: string | null }>();
  for (const track of tracks.value) {
    const id = stableAlbumId(track);
    if (!map.has(id)) {
      map.set(id, { id, name: track.album, year: track.year, artworkId: track.artworkId });
    }
  }
  return [...map.values()].sort((a, b) => (b.year ?? 0) - (a.year ?? 0));
});

const meta = computed(
  () =>
    `${albums.value.length} ${albums.value.length === 1 ? "album" : "albums"} · ` +
    `${tracks.value.length} songs · ${formatTotal(total.value)}`,
);

watch(
  () => route.params.id,
  async (id) => {
    if (typeof id !== "string") return;
    loading.value = true;
    try {
      tracks.value = await api.artistTracks(id);
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);

async function play(index = 0) {
  await player.playTracks(tracks.value, index, {
    kind: "artist",
    id: String(route.params.id),
    name: name.value,
  });
}

async function shuffle() {
  await player.setShuffle(true);
  await play(0);
}

</script>

<template>
  <div class="artist">
    <CollectionHeader
      v-if="tracks.length"
      :title="name"
      :meta="meta"
      :artwork-id="tracks[0].artworkId"
      round
      :show-mixer="false"
      @play="play(0)"
      @shuffle="shuffle"
      @menu="ui.openContextMenu({ x: $event.clientX, y: $event.clientY, tracks })"
    />

    <template v-if="albums.length">
      <h2 class="artist__heading">Albums</h2>
      <div class="artist__albums">
        <button
          v-for="album in albums"
          :key="album.id"
          class="card"
          @click="router.push({ name: 'album', params: { id: album.id } })"
        >
          <Artwork :artwork-id="album.artworkId" :size="140" :radius="7" shadow />
          <div class="card__title truncate">{{ album.name }}</div>
          <div class="card__subtitle">{{ album.year ?? "" }}</div>
        </button>
      </div>
    </template>

    <template v-if="tracks.length">
      <h2 class="artist__heading">All Songs</h2>
      <div class="artist__list">
        <TrackRow
          v-for="(track, index) in tracks"
          :key="track.id"
          :track="track"
          show-artwork
          :current="player.track?.id === track.id"
          :playing="player.playing"
          @play="play(index)"
          @menu="ui.openContextMenu({ x: $event.clientX, y: $event.clientY, tracks: [track] })"
        />
      </div>
    </template>

    <p v-if="!loading && tracks.length === 0" class="artist__empty">
      This artist is no longer in your library.
    </p>
  </div>
</template>

<style scoped>
.artist {
  padding: 6px 26px 40px;
}

.artist__heading {
  margin: 20px 0 12px;
  font-size: 18px;
  font-weight: 600;
  letter-spacing: -0.01em;
}

.artist__albums {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 20px 18px;
}

.card {
  display: flex;
  flex-direction: column;
  gap: 2px;
  text-align: left;
  min-width: 0;
}

.card :deep(.artwork) {
  width: 100% !important;
  height: auto !important;
  aspect-ratio: 1;
  margin-bottom: 8px;
  transition: transform 0.18s var(--ease);
}

.card:hover :deep(.artwork) {
  transform: translateY(-2px);
}

.card__title {
  font-size: 12.5px;
  font-weight: 500;
}

.card__subtitle {
  font-size: 11.5px;
  color: var(--text-secondary);
}

.artist__empty {
  padding: 60px 0;
  text-align: center;
  font-size: 13px;
  color: var(--text-tertiary);
}
</style>
