<script setup lang="ts">
/**
 * The library: songs, albums and artists, plus the folder setup shown when
 * nothing has been imported yet.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { open } from "@tauri-apps/plugin-dialog";
import PnmIcon from "@/components/icons/PnmIcon.vue";
import Artwork from "@/components/Artwork.vue";
import TrackRow from "@/components/TrackRow.vue";
import SelectMenu from "@/components/SelectMenu.vue";
import { formatTotal } from "@/lib/format";
import { resolveSort, sortItems, type SortDirection, type SortOption } from "@/lib/sort";
import { useLibraryStore } from "@/stores/library";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";
import type { Album, Artist, Track } from "@/lib/types";

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
/**
 * The search text also lives in the URL, for the same reason the tab does:
 * back and forward then restore it along with everything else about the page.
 * Keystrokes `replace` rather than `push`, so typing refines the current
 * history entry instead of burying it under one entry per character.
 */
const query = ref(typeof route.query.q === "string" ? route.query.q : "");
let queryTimer: number | undefined;

watch(query, (value) => {
  window.clearTimeout(queryTimer);
  queryTimer = window.setTimeout(() => {
    const next = { ...route.query };
    if (value.trim()) next.q = value;
    else delete next.q;
    router.replace({ name: "library", query: next });
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
const matches = (...fields: Array<string | number | null | undefined>) => {
  const q = normalizedQuery.value;
  return !q || fields.some((field) => String(field ?? "").toLocaleLowerCase().includes(q));
};

// -- sorting ----------------------------------------------------------------

const SONG_SORTS: ReadonlyArray<SortOption<Track>> = [
  { id: "title", label: "Title", value: (t) => t.title },
  { id: "artist", label: "Artist", value: (t) => t.artist },
  { id: "album", label: "Album", value: (t) => t.album },
  { id: "year", label: "Year", value: (t) => t.year },
  { id: "duration", label: "Duration", value: (t) => t.durationSecs },
  { id: "added", label: "Date Added", value: (t) => t.addedAt },
];

const ALBUM_SORTS: ReadonlyArray<SortOption<Album>> = [
  { id: "name", label: "Title", value: (a) => a.name },
  { id: "artist", label: "Artist", value: (a) => a.artist },
  { id: "year", label: "Year", value: (a) => a.year },
  { id: "tracks", label: "Songs", value: (a) => a.trackCount },
  { id: "duration", label: "Duration", value: (a) => a.durationSecs },
];

const ARTIST_SORTS: ReadonlyArray<SortOption<Artist>> = [
  { id: "name", label: "Name", value: (a) => a.name },
  { id: "albums", label: "Albums", value: (a) => a.albumCount },
  { id: "tracks", label: "Songs", value: (a) => a.trackCount },
];

/** Only the labels and ids are read here, so the item type does not matter. */
const sortOptions = computed<ReadonlyArray<{ id: string; label: string }>>(() => {
  switch (tab.value) {
    case "albums":
      return ALBUM_SORTS;
    case "artists":
      return ARTIST_SORTS;
    default:
      return SONG_SORTS;
  }
});

const sortId = computed(() => {
  const requested = route.query.sort;
  const available = sortOptions.value;
  return available.some((o) => o.id === requested) ? (requested as string) : available[0].id;
});

const sortDirection = computed<SortDirection>(() =>
  route.query.dir === "desc" ? "desc" : "asc",
);

function applySort(id: string, direction: SortDirection) {
  router.replace({ name: "library", query: { ...route.query, sort: id, dir: direction } });
}

const filteredTracks = computed<Track[]>(() =>
  sortItems(
    library.tracks.filter((track) =>
      matches(track.title, track.artist, track.album, track.albumArtist, track.genre, track.year),
    ),
    resolveSort(SONG_SORTS, sortId.value),
    sortDirection.value,
  ),
);
const filteredAlbums = computed<Album[]>(() =>
  sortItems(
    library.albums.filter((album) => matches(album.name, album.artist, album.year)),
    resolveSort(ALBUM_SORTS, sortId.value),
    sortDirection.value,
  ),
);
const filteredArtists = computed<Artist[]>(() =>
  sortItems(
    library.artists.filter((artist) => matches(artist.name)),
    resolveSort(ARTIST_SORTS, sortId.value),
    sortDirection.value,
  ),
);

const searchPlaceholder = computed(() => `Search ${tab.value}`);

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

/**
 * Clicking the row that is already playing toggles it, rather than restarting
 * it from the beginning. Anything else starts the list from that song.
 */
async function playFrom(index: number) {
  const track = filteredTracks.value[index];
  if (track && player.track?.id === track.id) {
    await player.toggle();
    return;
  }
  await player.playTracks(filteredTracks.value, index, {
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
            <input
              v-model="query"
              class="library__input"
              :placeholder="searchPlaceholder"
              :aria-label="searchPlaceholder"
              type="search"
            />
          </div>
          <SelectMenu
            :model-value="sortId"
            :options="sortOptions"
            label="Sort"
            @update:model-value="applySort($event, sortDirection)"
          />
          <button
            class="icon-button library__direction"
            :title="sortDirection === 'asc' ? 'Sorted ascending' : 'Sorted descending'"
            :aria-label="sortDirection === 'asc' ? 'Sorted ascending' : 'Sorted descending'"
            @click="applySort(sortId, sortDirection === 'asc' ? 'desc' : 'asc')"
          >
            <PnmIcon :name="sortDirection === 'asc' ? 'chevronUp' : 'chevronDown'" :size="15" />
          </button>
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
          v-for="(track, index) in filteredTracks"
          :key="track.id"
          :track="track"
          show-artwork
          :current="player.track?.id === track.id"
          :playing="player.playing"
          @play="playFrom(index)"
          @menu="ui.openContextMenu({ x: $event.clientX, y: $event.clientY, tracks: [track] })"
        />
        <p v-if="filteredTracks.length === 0" class="list__empty">
          No songs match "{{ query }}".
        </p>
      </div>

      <!-- Albums -->
      <div v-else-if="tab === 'albums'" class="grid">
        <button
          v-for="album in filteredAlbums"
          :key="album.id"
          class="card"
          @click="router.push({ name: 'album', params: { id: album.id } })"
        >
          <Artwork :artwork-id="album.artworkId" :size="152" :radius="7" shadow />
          <div class="card__title truncate">{{ album.name }}</div>
          <div class="card__subtitle truncate">{{ album.artist }}</div>
        </button>
        <p v-if="filteredAlbums.length === 0" class="grid__empty">
          No albums match "{{ query }}".
        </p>
      </div>

      <!-- Artists -->
      <div v-else class="grid grid--artists">
        <button
          v-for="artist in filteredArtists"
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
        <p v-if="filteredArtists.length === 0" class="grid__empty">
          No artists match "{{ query }}".
        </p>
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

.library__direction {
  width: 28px;
  height: 28px;
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
  content-visibility: auto;
  contain-intrinsic-block-size: auto 196px;
}

.grid--artists .card {
  contain-intrinsic-block-size: auto 178px;
}

.grid__empty {
  grid-column: 1 / -1;
  padding: 40px 0;
  margin: 0;
  text-align: center;
  font-size: 13px;
  color: var(--text-tertiary);
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
