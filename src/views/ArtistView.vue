<script setup lang="ts">
/**
 * An artist page: a round portrait header, then their albums, then everything
 * of theirs in one list. Designed to sit alongside the album and playlist
 * pages, which the drawings did define.
 */
import { computed, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import Artwork from "@/components/media/Artwork.vue";
import MediaCard from "@/components/ui/MediaCard.vue";
import CollectionHeader from "@/components/collections/CollectionHeader.vue";
import TrackList from "@/components/collections/TrackList.vue";
import EmptyState from "@/components/ui/EmptyState.vue";
import { formatTotal } from "@/lib/format";
import * as api from "@/lib/api";
import { stableAlbumId } from "@/lib/ids";
import { useCollectionMeta } from "@/composables/useCollectionMeta";
import { useCollectionPlayback } from "@/composables/useCollectionPlayback";
import { useMenu } from "@/composables/useMenu";
import { useRouteParamLoader } from "@/composables/useRouteParamLoader";
import type { PlayContext, Track } from "@/lib/types";

const route = useRoute();
const router = useRouter();
const { openMenu } = useMenu();

const tracks = ref<Track[]>([]);
const { loading } = useRouteParamLoader("id", async (id) => {
  tracks.value = await api.artistTracks(id);
});

const name = computed(() => tracks.value[0]?.albumArtist || tracks.value[0]?.artist || "Artist");
const { count, totalDuration } = useCollectionMeta(tracks);
const items = computed(() => tracks.value.map((track) => ({ track })));
const context = computed<PlayContext>(() => ({
  kind: "artist",
  id: String(route.params.id),
  name: name.value,
}));

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
    `${count.value} songs · ${formatTotal(totalDuration.value)}`,
);

const { player, playFromList, shuffleAndPlay } = useCollectionPlayback();

function play(index: number) {
  return playFromList(tracks.value, index, context.value);
}

function shuffle() {
  return shuffleAndPlay(() => play(0));
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
      @menu="openMenu($event, { tracks })"
    />

    <template v-if="albums.length">
      <h2 class="artist__heading">Albums</h2>
      <div class="artist__albums">
        <MediaCard
          v-for="album in albums"
          :key="album.id"
          :title="album.name"
          :subtitle="album.year !== null ? String(album.year) : undefined"
          @open="router.push({ name: 'album', params: { id: album.id } })"
        >
          <Artwork :artwork-id="album.artworkId" :size="140" :radius="7" shadow />
        </MediaCard>
      </div>
    </template>

    <template v-if="tracks.length">
      <h2 class="artist__heading">All Songs</h2>
      <TrackList
        :items="items"
        :current-id="player.track?.id ?? null"
        :playing="player.playing"
        show-artwork
        @play="play"
        @menu="(event, index) => { const track = tracks[index]; if (track) openMenu(event, { tracks: [track] }); }"
      />
    </template>

    <EmptyState
      v-if="!loading && tracks.length === 0"
      compact
      message="This artist is no longer in your library."
    />
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
</style>
