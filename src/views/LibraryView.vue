<script setup lang="ts">
/**
 * The library: songs, albums and artists, plus the folder setup shown when
 * nothing has been imported yet.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { open } from "@tauri-apps/plugin-dialog";
import PnmIcon from "@/components/icons/PnmIcon.vue";
import Artwork from "@/components/media/Artwork.vue";
import TrackList, { type TrackListItem } from "@/components/collections/TrackList.vue";
import EmptyState from "@/components/ui/EmptyState.vue";
import IconButton from "@/components/ui/IconButton.vue";
import MediaCard from "@/components/ui/MediaCard.vue";
import SearchField from "@/components/ui/SearchField.vue";
import SelectMenu from "@/components/ui/SelectMenu.vue";
import Tabs from "@/components/ui/Tabs.vue";
import { formatTotal } from "@/lib/format";
import { resolveSort, sortItems, type SortDirection, type SortOption } from "@/lib/sort";
import { useLibraryStore } from "@/stores/library";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";
import { useMenu } from "@/composables/useMenu";
import { useCollectionPlayback } from "@/composables/useCollectionPlayback";
import type { Album, Artist, Track } from "@/lib/types";

const library = useLibraryStore();
const player = usePlayerStore();
const ui = useUiStore();
const router = useRouter();
const route = useRoute();
const { openMenu } = useMenu();

const TABS = ["songs", "albums", "artists"] as const;
type Tab = (typeof TABS)[number];

const TAB_OPTIONS: ReadonlyArray<{ id: string; label: string }> = TABS.map((tab) => ({
  id: tab,
  label: tab[0].toUpperCase() + tab.slice(1),
}));

/**
 * The selected tab lives in the URL rather than in local state, so that
 * switching tabs is a navigation: the sidebar's back and forward buttons then
 * step through them like any other page.
 */
const tab = computed<Tab>(() => {
  const requested = route.query.tab;
  return TABS.includes(requested as Tab) ? (requested as Tab) : "songs";
});

function selectTab(next: string) {
  const nextTab = TABS.find((option) => option === next);
  if (!nextTab || nextTab === tab.value) return;
  // Guard against pushing a duplicate entry, which would make Back appear to
  // do nothing the first time it is pressed.
  router.push({ name: "library", query: { ...route.query, tab: nextTab } });
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
const matches = computed(
  () =>
    (...fields: Array<string | number | null | undefined>) =>
      !normalizedQuery.value ||
      fields.some((field) => String(field ?? "").toLocaleLowerCase().includes(normalizedQuery.value)),
);

// -- sorting -----------------------------------------------------------------

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
      matches.value(track.title, track.artist, track.album, track.albumArtist, track.genre, track.year),
    ),
    resolveSort(SONG_SORTS, sortId.value),
    sortDirection.value,
  ),
);
const filteredAlbums = computed<Album[]>(() =>
  sortItems(
    library.albums.filter((album) => matches.value(album.name, album.artist, album.year)),
    resolveSort(ALBUM_SORTS, sortId.value),
    sortDirection.value,
  ),
);
const filteredArtists = computed<Artist[]>(() =>
  sortItems(
    library.artists.filter((artist) => matches.value(artist.name)),
    resolveSort(ARTIST_SORTS, sortId.value),
    sortDirection.value,
  ),
);

const trackItems = computed<TrackListItem[]>(() =>
  filteredTracks.value.map((track) => ({ track })),
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

const { playFromList } = useCollectionPlayback();

/**
 * Clicking the row that is already playing toggles it, rather than restarting
 * it from the beginning. Anything else starts the list from that song.
 */
function playFrom(index: number) {
  return playFromList(filteredTracks.value, index, {
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
    <EmptyState
      v-if="library.isEmpty && library.folders.length === 0"
      icon="folder"
      title="Add your music"
      message="Choose a folder and Pick n Mix will index everything in it, reading titles, artwork and ReplayGain straight from your files. Nothing is sent anywhere."
    >
      <button class="pill-button" @click="chooseFolder">
        <PnmIcon name="plus" :size="14" />
        <span>Choose Folder</span>
      </button>
      <p class="library__later">Streaming from Navidrome or Jellyfin is planned.</p>
    </EmptyState>

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
          <SearchField v-model="query" :placeholder="searchPlaceholder" />
          <SelectMenu
            :model-value="sortId"
            :options="sortOptions"
            label="Sort"
            @update:model-value="applySort($event, sortDirection)"
          />
          <IconButton
            class="library__direction"
            :icon="sortDirection === 'asc' ? 'chevronUp' : 'chevronDown'"
            :label="sortDirection === 'asc' ? 'Sorted ascending' : 'Sorted descending'"
            :size="15"
            @click="applySort(sortId, sortDirection === 'asc' ? 'desc' : 'asc')"
          />
          <button
            class="pill-button is-plain"
            :disabled="library.scanning"
            title="Rescan watched folders"
            @click="library.scan()"
          >
            {{ library.scanning ? "Scanning…" : "Rescan" }}
          </button>
          <IconButton icon="plus" label="Add folder" :size="17" @click="chooseFolder" />
        </div>
      </header>

      <Tabs :tabs="TAB_OPTIONS" :model-value="tab" @update:model-value="selectTab" />

      <!-- Songs -->
      <TrackList
        v-if="tab === 'songs'"
        :items="trackItems"
        :current-id="player.track?.id ?? null"
        :playing="player.playing"
        show-artwork
        :empty-message="`No songs match “${query}”.`"
        @play="playFrom"
        @menu="(event, index) => { const track = filteredTracks[index]; if (track) openMenu(event, { tracks: [track] }); }"
      />

      <!-- Albums -->
      <div v-else-if="tab === 'albums'" class="grid">
        <MediaCard
          v-for="album in filteredAlbums"
          :key="album.id"
          :title="album.name"
          :subtitle="album.artist"
          @open="router.push({ name: 'album', params: { id: album.id } })"
        >
          <Artwork :artwork-id="album.artworkId" :size="152" :radius="7" shadow />
        </MediaCard>
        <EmptyState
          v-if="filteredAlbums.length === 0"
          compact
          class="library__grid-empty"
          :message="`No albums match “${query}”.`"
        />
      </div>

      <!-- Artists -->
      <div v-else class="grid grid--artists">
        <MediaCard
          v-for="artist in filteredArtists"
          :key="artist.id"
          :title="artist.name"
          :subtitle="`${artist.albumCount} ${artist.albumCount === 1 ? 'album' : 'albums'}`"
          @open="router.push({ name: 'artist', params: { id: artist.id } })"
        >
          <Artwork :artwork-id="artist.artworkId" :size="132" :radius="66" shadow />
        </MediaCard>
        <EmptyState
          v-if="filteredArtists.length === 0"
          compact
          class="library__grid-empty"
          :message="`No artists match “${query}”.`"
        />
      </div>
    </template>
  </div>
</template>

<style scoped>
.library {
  padding: 6px 26px 40px;
}

.library__later {
  margin-top: 6px;
  font-size: 11.5px;
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

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(152px, 1fr));
  gap: 22px 18px;
  padding-top: 8px;
}

.grid--artists {
  grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
}

.library__grid-empty {
  grid-column: 1 / -1;
  margin: 0;
}
</style>
