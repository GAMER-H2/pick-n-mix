<script setup lang="ts">
/** The right-click menu from the drawings. */
import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import { useRouter } from "vue-router";
import PnmIcon from "./icons/PnmIcon.vue";
import type { IconName } from "./icons/paths";
import { useUiStore } from "@/stores/ui";
import { usePlaylistStore } from "@/stores/playlists";
import { usePlayerStore } from "@/stores/player";
import { useMixerStore } from "@/stores/mixer";
import * as api from "@/lib/api";
import { stableAlbumId, stableArtistId } from "@/lib/ids";

const ui = useUiStore();
const playlists = usePlaylistStore();
const player = usePlayerStore();
const mixer = useMixerStore();
const router = useRouter();

const el = ref<HTMLElement | null>(null);
const menu = computed(() => ui.contextMenu);
const track = computed(() => menu.value?.tracks[0] ?? null);
/** Keep the complete menu inside the window, including conditional rows. */
const position = computed(() => {
  const m = menu.value;
  const t = track.value;
  if (!m || !t) return { left: "0px", top: "0px" };

  const playlistRows = m.playlistId !== undefined && m.entryIndex !== undefined ? 2 : 0;
  const duplicateRows = m.tracks.length === 1 && t.fileCount > 1 ? 1 : 0;
  const optionRows = m.playlistOptions ? (m.playlistOptions.hasArtwork ? 3 : 2) : 0;
  const buttonCount =
    6 + (t.album.trim() === "" ? 0 : 1) + playlistRows + duplicateRows + optionRows;
  const separatorCount = 2 + (playlistRows > 0 ? 1 : 0) + (optionRows > 0 ? 1 : 0);
  const width = 232;
  const height = Math.min(buttonCount * 31 + separatorCount * 11 + 10, window.innerHeight - 16);

  return {
    left: `${Math.max(8, Math.min(m.x, window.innerWidth - width - 8))}px`,
    top: `${Math.max(8, Math.min(m.y, window.innerHeight - height - 8))}px`,
  };
});

interface Item {
  label: string;
  icon: IconName;
  /** Return values are ignored; several of these resolve to router results. */
  action: () => unknown;
  separated?: boolean;
  danger?: boolean;
  warning?: boolean;
  /** Draws a tick on the right, for options that are on or off. */
  checked?: boolean;
}

const items = computed<Item[]>(() => {
  const m = menu.value;
  if (!m || !track.value) return [];
  const t = track.value;

  const count = m.tracks.length;
  const what = count === 1 ? `"${t.title}"` : `${count} songs`;
  // Captured now, not read from `menu` later: `run` closes the menu before it
  // awaits the action, which would leave these reading an empty list.
  const ids = m.tracks.map((track) => track.id);

  const tracks = [...m.tracks];

  /**
   * Queuing a song out of a playlist takes that playlist's mixer with it, for
   * that one play only. The dedicated command exists so the playlist and entry
   * layers are collapsed onto the queue entry rather than being lost, which is
   * what queuing the bare track id would do.
   */
  const fromPlaylistEntry =
    m.playlistId !== undefined && m.entryIndex !== undefined && count === 1
      ? { playlistId: m.playlistId, index: m.entryIndex }
      : null;

  const list: Item[] = [
    {
      label: "Play Next",
      icon: "playNext",
      action: async () => {
        if (fromPlaylistEntry) {
          await api.queuePlaylistEntry(
            fromPlaylistEntry.playlistId,
            fromPlaylistEntry.index,
            true,
          );
        } else {
          await api.playNext(ids);
        }
        // Refresh explicitly as well as listening for the event, so the panel
        // is right even if the event is missed, and say so either way.
        await player.refreshQueue();
        ui.notify(`Playing ${what} next`);
      },
    },
    {
      label: "Add to Queue",
      icon: "addToQueue",
      action: async () => {
        if (fromPlaylistEntry) {
          await api.queuePlaylistEntry(
            fromPlaylistEntry.playlistId,
            fromPlaylistEntry.index,
            false,
          );
        } else {
          await api.addToQueue(ids);
        }
        await player.refreshQueue();
        ui.notify(`Added ${what} to the queue`);
      },
    },
    {
      label: "Add to Playlist",
      icon: "addToPlaylist",
      action: () => (ui.addToPlaylistFor = tracks),
    },
    {
      label: "Go to Artist",
      icon: "artist",
      separated: true,
      action: () => router.push({ name: "artist", params: { id: stableArtistId(t) } }),
    },
  ];

  // A track with no album tag belongs to no album, so there is nowhere to go.
  if (t.album.trim() !== "") {
    list.push({
      label: "Go to Album",
      icon: "album",
      action: () => router.push({ name: "album", params: { id: stableAlbumId(t) } }),
    });
  }

  const showDuplicates = count === 1 && t.fileCount > 1;
  if (showDuplicates) {
    const hasMissingFiles = t.missingFileCount > 0;
    list.push({
      label: "Show duplicate files",
      icon: hasMissingFiles ? "warningCircle" : "duplicateFiles",
      warning: hasMissingFiles,
      separated: true,
      action: () => (ui.duplicateTrack = t),
    });
  }

  list.push(
    {
      label: "Get Info",
      icon: "info",
      separated: !showDuplicates,
      action: () => (ui.infoTrack = t),
    },
    {
      label: "Look Up Online",
      icon: "sparkle",
      action: async () => {
        ui.notify(`Looking up "${t.title}" on MusicBrainz...`);
        try {
          const updated = await api.enrichTrack(t.id);
          ui.notify(updated ? `Updated "${updated.title}"` : "No match found");
        } catch (e) {
          ui.notify(`Look-up failed: ${e}`, "error");
        }
      },
    },
  );

  // Mixer overrides and removal only exist in a playlist context.
  if (m.playlistId !== undefined && m.entryIndex !== undefined) {
    const playlistId = m.playlistId;
    const entryIndex = m.entryIndex;
    list.push({
      label: "Mixer Settings",
      icon: "mixer",
      separated: true,
      action: async () => {
        await mixer.editPlaylistEntry(playlistId, entryIndex, t.title, null);
        mixer.panelOpen = true;
      },
    });
    list.push({
      label: "Remove from Playlist",
      icon: "trash",
      danger: true,
      action: async () => {
        await api.removeFromPlaylist(playlistId, entryIndex);
        await playlists.refresh();
      },
    });
  }

  // Options for the playlist as a whole, from its own "more" button.
  const options = m.playlistOptions;
  if (options) {
    list.push({
      label: "Shuffle-Only",
      icon: "shuffle",
      separated: true,
      checked: options.shuffleOnly,
      action: options.onToggleShuffleOnly,
    });
    list.push({
      label: "Change Image…",
      icon: "image",
      action: options.onChooseArtwork,
    });
    if (options.hasArtwork) {
      list.push({
        label: "Reset Image",
        icon: "trash",
        action: options.onClearArtwork,
      });
    }
  }

  return list;
});

async function run(item: Item) {
  ui.closeContextMenu();
  try {
    await item.action();
  } catch (e) {
    ui.notify(`${item.label} failed: ${e}`, "error");
  }
}

function onPointerDown(event: PointerEvent) {
  if (el.value && !el.value.contains(event.target as Node)) ui.closeContextMenu();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") ui.closeContextMenu();
}

onMounted(() => {
  window.addEventListener("pointerdown", onPointerDown, true);
  window.addEventListener("keydown", onKeydown);
});

onBeforeUnmount(() => {
  window.removeEventListener("pointerdown", onPointerDown, true);
  window.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <Transition name="pop">
    <div v-if="menu && items.length" ref="el" class="menu" :style="position" role="menu">
      <template v-for="item in items" :key="item.label">
        <div v-if="item.separated" class="menu__separator" />
        <button
          class="menu__item"
          :class="{ 'is-danger': item.danger }"
          role="menuitem"
          :aria-checked="item.checked === undefined ? undefined : item.checked"
          @click="run(item)"
        >
          <PnmIcon
            class="menu__icon"
            :class="{ 'is-warning': item.warning }"
            :name="item.icon"
            :size="17"
          />
          <span>{{ item.label }}</span>
          <PnmIcon v-if="item.checked" class="menu__tick" name="check" :size="15" />
        </button>
      </template>
    </div>
  </Transition>
</template>

<style scoped>
.menu {
  position: fixed;
  z-index: 400;
  min-width: 214px;
  padding: 5px;
  border-radius: var(--radius);
  background: var(--bg-elevated);
  box-shadow: var(--shadow-popover);
  border: 0.5px solid var(--separator);
  transform-origin: top left;
  max-height: calc(100vh - 16px);
  overflow-y: auto;
}

.menu__item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 7px 9px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  color: var(--text);
  text-align: left;
}

.menu__item:hover {
  background: var(--accent);
  color: var(--accent-contrast);
}

.menu__tick {
  margin-left: auto;
  flex: none;
}

.menu__icon.is-warning {
  color: #d7373f;
}

.menu__item.is-danger {
  color: #d7373f;
}

.menu__item.is-danger:hover {
  background: #d7373f;
  color: #fff;
}

.menu__separator {
  height: 1px;
  margin: 5px 8px;
  background: var(--separator);
}
</style>
