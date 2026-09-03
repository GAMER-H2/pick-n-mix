<script setup lang="ts">
/**
 * An album, laid out like the second drawing: large artwork on the left with
 * the track list beside it.
 */
import { computed, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import CollectionHeader from "@/components/collections/CollectionHeader.vue";
import TrackList from "@/components/collections/TrackList.vue";
import { subtitleFor } from "@/lib/format";
import * as api from "@/lib/api";
import { stableArtistId } from "@/lib/ids";
import { useCollectionMeta } from "@/composables/useCollectionMeta";
import { useCollectionPlayback } from "@/composables/useCollectionPlayback";
import { useMenu } from "@/composables/useMenu";
import { useRouteParamLoader } from "@/composables/useRouteParamLoader";
import type { PlayContext, Track } from "@/lib/types";

const route = useRoute();
const router = useRouter();
const { openMenu } = useMenu();

const tracks = ref<Track[]>([]);
useRouteParamLoader("id", async (id) => {
  tracks.value = await api.albumTracks(id);
});

const { meta } = useCollectionMeta(tracks);
const first = computed(() => tracks.value[0] ?? null);
const items = computed(() => tracks.value.map((track) => ({ track })));
const context = computed<PlayContext>(() => ({
  kind: "album",
  id: String(route.params.id),
  name: first.value?.album ?? "",
}));

const { player, playFromList, shuffleAndPlay } = useCollectionPlayback();

function play(index: number) {
  return playFromList(tracks.value, index, context.value);
}

function shuffle() {
  return shuffleAndPlay(() => play(0));
}
</script>

<template>
  <div class="album">
    <CollectionHeader
      v-if="first"
      :title="first.album"
      :subtitle="first.albumArtist || first.artist"
      :meta="subtitleFor([first.year, meta])"
      :artwork-id="first.artworkId"
      :show-mixer="false"
      @play="play(0)"
      @shuffle="shuffle"
      @menu="openMenu($event, { tracks })"
    />

    <button
      v-if="first"
      class="album__artist"
      @click="router.push({ name: 'artist', params: { id: stableArtistId(first) } })"
    >
      Go to {{ first.albumArtist || first.artist }}
    </button>

    <TrackList
      :items="items"
      :current-id="player.track?.id ?? null"
      :playing="player.playing"
      numbered
      empty-message="This album is no longer in your library."
      @play="play"
      @menu="(event, index) => { const track = tracks[index]; if (track) openMenu(event, { tracks: [track] }); }"
    />
  </div>
</template>

<style scoped>
.album {
  padding: 6px 26px 40px;
}

.album__artist {
  margin-bottom: 12px;
  font-size: 12.5px;
  font-weight: 500;
  color: var(--accent);
}
</style>
