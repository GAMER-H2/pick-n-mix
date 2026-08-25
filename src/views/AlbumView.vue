<script setup lang="ts">
/**
 * An album, laid out like the second drawing: large artwork on the left with
 * the track list beside it.
 */
import { computed, ref, watch } from "vue";
import CollectionHeader from "@/components/CollectionHeader.vue";
import TrackRow from "@/components/TrackRow.vue";
import { useRoute, useRouter } from "vue-router";
import { formatTotal, subtitleFor } from "@/lib/format";
import * as api from "@/lib/api";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";
import { stableArtistId } from "@/lib/ids";
import type { Track } from "@/lib/types";

const route = useRoute();
const router = useRouter();
const player = usePlayerStore();
const ui = useUiStore();

const tracks = ref<Track[]>([]);
const loading = ref(false);

const first = computed(() => tracks.value[0] ?? null);
const total = computed(() => tracks.value.reduce((sum, t) => sum + t.durationSecs, 0));
const meta = computed(() =>
  subtitleFor([
    `${tracks.value.length} ${tracks.value.length === 1 ? "song" : "songs"}`,
    formatTotal(total.value),
  ]),
);

watch(
  () => route.params.id,
  async (id) => {
    if (typeof id !== "string") return;
    loading.value = true;
    try {
      tracks.value = await api.albumTracks(id);
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);

async function play(index = 0) {
  if (!first.value) return;
  await player.playTracks(tracks.value, index, {
    kind: "album",
    id: String(route.params.id),
    name: first.value.album,
  });
}

async function shuffle() {
  await player.setShuffle(true);
  await play(0);
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
      @menu="ui.openContextMenu({ x: $event.clientX, y: $event.clientY, tracks })"
    />

    <button
      v-if="first"
      class="album__artist"
      @click="router.push({ name: 'artist', params: { id: stableArtistId(first) } })"
    >
      Go to {{ first.albumArtist || first.artist }}
    </button>

    <div class="album__list">
      <TrackRow
        v-for="(track, index) in tracks"
        :key="track.id"
        :track="track"
        :index="track.trackNumber ?? index + 1"
        :current="player.track?.id === track.id"
        :playing="player.playing"
        @play="play(index)"
        @menu="ui.openContextMenu({ x: $event.clientX, y: $event.clientY, tracks: [track] })"
      />
    </div>

    <p v-if="!loading && tracks.length === 0" class="album__empty">
      This album is no longer in your library.
    </p>
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

.album__empty {
  padding: 60px 0;
  text-align: center;
  font-size: 13px;
  color: var(--text-tertiary);
}
</style>
