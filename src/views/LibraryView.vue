<script setup lang="ts">
/**
 * The library: songs, albums and artists, plus the folder setup shown when
 * nothing has been imported yet.
 */
import { computed, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { open } from "@tauri-apps/plugin-dialog";
import PnmIcon from "@/components/icons/PnmIcon.vue";
import Artwork from "@/components/Artwork.vue";
import TrackRow from "@/components/TrackRow.vue";
import { formatTotal } from "@/lib/format";
import { useLibraryStore } from "@/stores/library";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";
import type { Track } from "@/lib/types";

const library = useLibraryStore();
const player = usePlayerStore();
const ui = useUiStore();
const router = useRouter();
const route = useRoute();

const TABS = ["songs", "albums", "artists"] as const;
type Tab = (typeof TABS)[number];

/**
 * The selected tab lives in the URL rather than in local state, so that
 * switching tabs is a navigation: the sidebar's back and forward buttons then
 * step through them like any other page.
 */
const tab = computed<Tab>(() => {
  const requested = route.query.tab;
  return TABS.includes(requested as Tab) ? (requested as Tab) : "songs";
});

function selectTab(next: Tab) {
  // Guard against pushing a duplicate entry, which would make Back appear to
  // do nothing the first time it is pressed.
  if (next === tab.value) return;
  router.push({ name: "library", query: { ...route.query, tab: next } });
}
const query = ref("");

const filtered = computed<Track[]>(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return library.tracks;
  return library.tracks.filter(
    (t) =>
      t.title.toLowerCase().includes(q) ||
      t.artist.toLowerCase().includes(q) ||
      t.album.toLowerCase().includes(q),
  );
});

const totalDuration = computed(() =>
  library.tracks.reduce((sum, t) => sum + t.durationSecs, 0),
);

async function chooseFolder() {
  const selected = await open({ directory: true, multiple: false, title: "Choose a music folder" });
  if (typeof selected === "string") {
    await library.addFolder(selected);
    ui.notify(
      library.lastReport
        ? `Added ${library.lastReport.added} tracks, updated ${library.lastReport.updated}`
        : "Folder added",
    );
  }
}

async function playFrom(index: number) {
  await player.playTracks(filtered.value, index, {
    kind: "library",
    id: "library",
    name: "Library",
  });
}


onMounted(() => {
  if (library.tracks.length === 0) library.refresh();
});
</script>

<template>
  <div class="library">
    <!-- Empty state: get music in -->
    <div v-if="library.isEmpty && library.folders.length === 0" class="empty">
      <PnmIcon name="folder" :size="44" class="empty__icon" />
      <h1>Add your music</h1>
      <p>
        Choose a folder and Pick n Mix will index everything in it, reading titles, artwork and
        ReplayGain straight from your files. Nothing is sent anywhere.
      </p>
      <button class="pill-button" @click="chooseFolder">
        <PnmIcon name="plus" :size="14" />
        <span>Choose Folder</span>
      </button>
      <p class="empty__later">Streaming from Navidrome or Jellyfin is planned.</p>
    </div>

    <template v-else>
      <header class="library__head">
        <div class="library__titles">
          <h1>Library</h1>
          <p>
            {{ library.tracks.length }} songs · {{ library.albums.length }} albums ·
            {{ formatTotal(totalDuration) }}
          </p>
        </div>

        <div class="library__tools">
          <div class="library__search">
            <PnmIcon name="search" :size="15" />
            <input v-model="query" class="library__input" placeholder="Search" type="search" />
          </div>
          <button
            class="pill-button is-plain"
            :disabled="library.scanning"
            title="Rescan watched folders"
            @click="library.scan()"
          >
            {{ library.scanning ? "Scanning…" : "Rescan" }}
          </button>
          <button class="icon-button" title="Add folder" @click="chooseFolder">
            <PnmIcon name="plus" :size="17" />
          </button>
        </div>
      </header>

      <nav class="tabs">
        <button
          v-for="option in TABS"
          :key="option"
          class="tabs__tab"
          :class="{ 'is-active': tab === option }"
          @click="selectTab(option)"
        >
          {{ option[0].toUpperCase() + option.slice(1) }}
        </button>
      </nav>

      <!-- Songs -->
      <div v-if="tab === 'songs'" class="list">
        <TrackRow
          v-for="(track, index) in filtered"
          :key="track.id"
          :track="track"
          show-artwork
          :current="player.track?.id === track.id"
          :playing="player.playing"
          @play="playFrom(index)"
          @menu="ui.openContextMenu({ x: $event.clientX, y: $event.clientY, tracks: [track] })"
        />
        <p v-if="filtered.length === 0" class="list__empty">No songs match "{{ query }}".</p>
      </div>

      <!-- Albums -->
      <div v-else-if="tab === 'albums'" class="grid">
        <button
          v-for="album in library.albums"
          :key="album.id"
          class="card"
          @click="router.push({ name: 'album', params: { id: album.id } })"
        >
          <Artwork :artwork-id="album.artworkId" :size="152" :radius="7" shadow />
          <div class="card__title truncate">{{ album.name }}</div>
          <div class="card__subtitle truncate">{{ album.artist }}</div>
        </button>
      </div>

      <!-- Artists -->
      <div v-else class="grid grid--artists">
        <button
          v-for="artist in library.artists"
          :key="artist.id"
          class="card"
          @click="router.push({ name: 'artist', params: { id: artist.id } })"
        >
          <Artwork :artwork-id="artist.artworkId" :size="132" :radius="66" shadow />
          <div class="card__title truncate">{{ artist.name }}</div>
          <div class="card__subtitle truncate">
            {{ artist.albumCount }} {{ artist.albumCount === 1 ? "album" : "albums" }}
          </div>
        </button>
      </div>
    </template>
  </div>
</template>

<style scoped>
.library {
  padding: 6px 26px 40px;
}

.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  min-height: 70vh;
  text-align: center;
  color: var(--text-secondary);
}

.empty__icon {
  color: var(--text-tertiary);
}

.empty h1 {
  margin: 4px 0 0;
  font-size: 22px;
  font-weight: 600;
  color: var(--text);
}

.empty p {
  margin: 0;
  max-width: 420px;
  font-size: 13px;
  line-height: 1.55;
}

.empty__later {
  margin-top: 6px !important;
  font-size: 11.5px !important;
  color: var(--text-tertiary);
}

.library__head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 0 14px;
}

.library__titles h1 {
  margin: 0;
  font-size: 28px;
  font-weight: 700;
  letter-spacing: -0.02em;
}

.library__titles p {
  margin: 4px 0 0;
  font-size: 12px;
  color: var(--text-tertiary);
}

.library__tools {
  display: flex;
  align-items: center;
  gap: 8px;
}

.library__search {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  border-radius: 999px;
  background: var(--bg-sunken);
  color: var(--text-tertiary);
}

.library__input {
  width: 160px;
  border: 0;
  background: none;
  outline: none;
  font-size: 12.5px;
  user-select: text;
}

.tabs {
  display: flex;
  gap: 4px;
  margin-bottom: 10px;
  border-bottom: 1px solid var(--separator);
}

.tabs__tab {
  position: relative;
  padding: 8px 12px;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
}

.tabs__tab.is-active {
  color: var(--text);
}

.tabs__tab.is-active::after {
  content: "";
  position: absolute;
  left: 12px;
  right: 12px;
  bottom: -1px;
  height: 2px;
  border-radius: 2px;
  background: var(--accent);
}

.list__empty {
  padding: 40px 0;
  text-align: center;
  font-size: 13px;
  color: var(--text-tertiary);
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(152px, 1fr));
  gap: 22px 18px;
  padding-top: 8px;
}

.grid--artists {
  grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
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
