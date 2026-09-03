<script setup lang="ts">
/**
 * The landing page: three generated mixes across the top, then a grid of
 * explainable recommendations, then the playlists most recently played from.
 *
 * Every shelf here is derived from listening history, so on a fresh library
 * there is nothing honest to show. Rather than filling the space with
 * placeholder cards, the page says so and points at the library.
 */
import { computed, onMounted } from "vue";
import { useRouter } from "vue-router";
import PnmIcon from "@/components/icons/PnmIcon.vue";
import Artwork from "@/components/Artwork.vue";
import PlaylistArtwork from "@/components/PlaylistArtwork.vue";
import MixCard from "@/components/home/MixCard.vue";
import * as api from "@/lib/api";
import { useHomeStore } from "@/stores/home";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";
import type { HomePick, MixSummary, PlaylistSummary, Track } from "@/lib/types";

const router = useRouter();
const home = useHomeStore();
const player = usePlayerStore();
const ui = useUiStore();

onMounted(() => home.refresh());

/** Nothing to show at all: no history, and no playlists to fall back on. */
const isBare = computed(
  () => home.isEmpty && home.recentPlaylists.length === 0 && home.picks.length === 0,
);

function openMix(mix: MixSummary) {
  router.push({ name: "mix", params: { kind: mix.kind } });
}

async function playMix(mix: MixSummary) {
  await api.playMix(mix.kind);
}

async function playPick(pick: HomePick) {
  if (pick.trackIds.length === 0) return;
  await player.playTracks(pick.trackIds, 0, {
    kind: pick.kind === "album" ? "album" : "library",
    id: pick.id,
    name: pick.title,
  });
}

function openPick(pick: HomePick) {
  if (pick.kind === "album") {
    router.push({ name: "album", params: { id: pick.id } });
    return;
  }
  void playPick(pick);
}

function showMenu(event: MouseEvent, tracks: Track[]) {
  if (tracks.length === 0) {
    ui.notify("No available songs in this item", "error");
    return;
  }
  ui.openContextMenu({ x: event.clientX, y: event.clientY, tracks });
}

async function openPickMenu(pick: HomePick, event: MouseEvent) {
  try {
    const resolved = await Promise.all(pick.trackIds.map((id) => api.getTrack(id)));
    showMenu(event, resolved.filter((track): track is Track => track !== null));
  } catch (error) {
    ui.notify(`Could not open that menu: ${error}`, "error");
  }
}

async function openMixMenu(mix: MixSummary, event: MouseEvent) {
  try {
    showMenu(event, await api.mixTracks(mix.kind));
  } catch (error) {
    ui.notify(`Could not open that menu: ${error}`, "error");
  }
}

async function openPlaylistMenu(playlist: PlaylistSummary, event: MouseEvent) {
  try {
    const resolved = await api.getPlaylist(playlist.id);
    const tracks = resolved?.items
      .map((item) => item.track)
      .filter((track): track is Track => track !== null) ?? [];
    showMenu(event, tracks);
  } catch (error) {
    ui.notify(`Could not open that menu: ${error}`, "error");
  }
}

</script>

<template>
  <div class="home">
    <div v-if="isBare && !home.loading" class="home__bare">
      <PnmIcon name="home" :size="44" class="home__bare-icon" />
      <h1>Nothing to go on yet</h1>
      <p>
        Mixes and recommendations are built from what you actually listen to, so this page
        fills in as you play things.
      </p>
      <RouterLink to="/library" class="pill-button">Open Library</RouterLink>
    </div>

    <template v-else>
      <!-- Mixes ------------------------------------------------------------>
      <section class="shelf shelf--mixes">
        <MixCard
          v-for="mix in home.mixes"
          :key="mix.kind"
          :mix="mix"
          :ready="home.isReady(mix)"
          @open="openMix(mix)"
          @play="playMix(mix)"
          @pin="home.setPinned(mix.kind, !mix.pinned)"
          @menu="openMixMenu(mix, $event)"
        />
      </section>

      <!-- Top picks -------------------------------------------------------->
      <section v-if="home.picks.length" class="shelf">
        <header class="shelf__head">
          <h2>Top Picks</h2>
          <button class="shelf__link" title="Build these again" @click="home.regenerate()">
            Refresh
          </button>
        </header>

        <div class="picks">
          <button
            v-for="pick in home.picks"
            :key="`${pick.kind}-${pick.id}`"
            class="pick"
            :title="pick.reason"
            @click="openPick(pick)"
            @contextmenu.prevent="openPickMenu(pick, $event)"
          >
            <div class="pick__art">
              <Artwork :artwork-id="pick.artworkId" :size="44" :radius="5" />
              <span class="pick__play" @click.stop="playPick(pick)">
                <PnmIcon name="play" :size="13" />
              </span>
            </div>
            <div class="pick__text">
              <div class="pick__title truncate">{{ pick.title }}</div>
              <div class="pick__reason truncate">{{ pick.reason }}</div>
            </div>
          </button>
        </div>
      </section>

      <!-- Recent playlists -------------------------------------------------->
      <section v-if="home.recentPlaylists.length" class="shelf">
        <header class="shelf__head">
          <h2>Recent Playlists</h2>
        </header>

        <div class="playlists">
          <button
            v-for="playlist in home.recentPlaylists"
            :key="playlist.id"
            class="card"
            @click="router.push({ name: 'playlist', params: { id: playlist.id } })"
            @contextmenu.prevent="openPlaylistMenu(playlist, $event)"
          >
            <PlaylistArtwork
              :artwork="playlist.artwork"
              :artwork-ids="playlist.artworkIds"
              :size="140"
              :radius="7"
              shadow
            />
            <div class="card__title truncate">{{ playlist.name }}</div>
            <div class="card__subtitle truncate">
              {{ playlist.trackCount }} {{ playlist.trackCount === 1 ? "song" : "songs" }}
            </div>
          </button>
        </div>
      </section>
    </template>
  </div>
</template>

<style scoped>
.home {
  padding: 6px 26px 40px;
}

.home__bare {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  height: 100%;
  min-height: 60vh;
  text-align: center;
  color: var(--text-secondary);
}

.home__bare-icon {
  color: var(--text-tertiary);
}

.home__bare h1 {
  margin: 4px 0 0;
  font-size: 22px;
  font-weight: 600;
  color: var(--text);
}

.home__bare p {
  margin: 0;
  max-width: 400px;
  font-size: 13px;
  line-height: 1.55;
}

.home__bare .pill-button {
  margin-top: 8px;
  text-decoration: none;
}

/* -- shelves --------------------------------------------------------------- */

.shelf {
  padding-top: 18px;
}

/* The three mixes sit above the first rule, as their own banner row. */
.shelf--mixes {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 192px));
  justify-content: center;
  column-gap: clamp(28px, 8vw, 112px);
  padding: 14px 0 24px;
}

.shelf + .shelf {
  border-top: 1px solid var(--separator);
}

.shelf__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}

.shelf__head h2 {
  margin: 0;
  font-size: 17px;
  font-weight: 600;
  letter-spacing: -0.01em;
}

.shelf__link {
  font-size: 12px;
  color: var(--accent);
}

/* -- top picks ------------------------------------------------------------- */

.picks {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 8px 18px;
}

.pick {
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 6px;
  border-radius: var(--radius-sm);
  text-align: left;
  min-width: 0;
}

.pick:hover {
  background: var(--bg-hover);
}

.pick__art {
  position: relative;
  flex: none;
}

/* The play affordance sits over the cover, so a pick can be started without
   first opening it. */
.pick__play {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  border-radius: 5px;
  color: #fff;
  background: rgba(0, 0, 0, 0.45);
  opacity: 0;
  transition: opacity 0.12s var(--ease);
}

.pick:hover .pick__play {
  opacity: 1;
}

.pick__text {
  min-width: 0;
}

.pick__title {
  font-size: 13px;
  font-weight: 500;
}

.pick__reason {
  font-size: 11.5px;
  color: var(--text-secondary);
  margin-top: 1px;
}

/* -- playlists ------------------------------------------------------------- */

.playlists {
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
</style>
