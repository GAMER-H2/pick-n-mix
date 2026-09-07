<script setup lang="ts">
/**
 * An artist page: a round portrait header, then their albums, then everything
 * of theirs in one list. Designed to sit alongside the album and playlist
 * pages, which the drawings did define.
 */
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import Artwork from "@/components/media/Artwork.vue";
import MediaCard from "@/components/ui/MediaCard.vue";
import SearchField from "@/components/ui/SearchField.vue";
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

/** Rows are played from the filtered list, so indexes line up with what is shown. */
function play(index: number) {
  return playFromList(filteredTracks.value, index, context.value);
}

function shuffle() {
  return shuffleAndPlay(() => play(0));
}

/**
 * The search text lives in the URL, like the library's does: back and forward
 * restore it, and typing `replace`s the current history entry rather than
 * pushing one per keystroke. It filters only the song list below it; the album
 * shelf stays whole.
 */
const query = ref(typeof route.query.q === "string" ? route.query.q : "");
let queryTimer: number | undefined;

watch(query, (value) => {
  window.clearTimeout(queryTimer);
  queryTimer = window.setTimeout(() => {
    const next = { ...route.query };
    if (value.trim()) next.q = value;
    else delete next.q;
    router.replace({ query: next });
  }, 200);
});

// Arriving on a history entry that carried different text, via back or forward.
watch(
  () => route.query.q,
  (value) => {
    const incoming = typeof value === "string" ? value : "";
    if (incoming !== query.value) query.value = incoming;
  },
);

onBeforeUnmount(() => window.clearTimeout(queryTimer));

const normalizedQuery = computed(() => query.value.trim().toLocaleLowerCase());
const matches = computed(
  () =>
    (...fields: Array<string | number | null | undefined>) =>
      !normalizedQuery.value ||
      fields.some((field) => String(field ?? "").toLocaleLowerCase().includes(normalizedQuery.value)),
);

const filteredTracks = computed(() =>
  tracks.value.filter((track) => matches.value(track.title, track.artist, track.album)),
);
const filteredItems = computed(() => filteredTracks.value.map((track) => ({ track })));
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
      <div class="artist__tools">
        <SearchField v-model="query" placeholder="Search songs" />
      </div>
      <h2 class="artist__heading">All Songs</h2>
      <TrackList
        :items="filteredItems"
        :current-id="player.track?.id ?? null"
        :playing="player.playing"
        show-artwork
        :empty-message="`No songs match “${query}”.`"
        @play="play"
        @menu="(event, index) => { const track = filteredTracks[index]; if (track) openMenu(event, { tracks: [track] }); }"
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

.artist__tools {
  display: flex;
  justify-content: flex-end;
  margin-top: 20px;
}

.artist__albums {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 20px 18px;
}
</style>
