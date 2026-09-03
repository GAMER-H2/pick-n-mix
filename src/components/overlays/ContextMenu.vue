<script setup lang="ts">
/** The right-click menu from the drawings. */
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import MenuSurface from "../ui/MenuSurface.vue";
import type { IconName } from "../icons/paths";
import { useDismiss } from "@/lib/dismiss";
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

  // A mixed playlist goes into the queue whole; see `PlaylistMenuOptions`.
  const asMix = m.playlistOptions?.masterMixEnabled ? m.playlistOptions : null;

  const list: Item[] = [
    {
      label: "Play Next",
      icon: "playNext",
      action: async () => {
        if (asMix) {
          await asMix.onPlayNext();
          await player.refreshQueue();
          ui.notify("Playing the mix next");
          return;
        }
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
        if (asMix) {
          await asMix.onAddToQueue();
          await player.refreshQueue();
          ui.notify("Added the mix to the queue");
          return;
        }
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

/** The flat item list split into groups at each `separated` row. */
const groups = computed(() => {
  const out: {
    items: {
      id: string;
      label: string;
      icon: IconName;
      danger?: boolean;
      warning?: boolean;
      checked?: boolean;
    }[];
  }[] = [];
  for (const item of items.value) {
    if (item.separated || out.length === 0) out.push({ items: [] });
    out[out.length - 1].items.push({
      id: item.label,
      label: item.label,
      icon: item.icon,
      danger: item.danger,
      warning: item.warning,
      checked: item.checked,
    });
  }
  return out;
});

async function run(item: Item) {
  const onSelect = menu.value?.onSelect;
  ui.closeContextMenu();
  try {
    await onSelect?.();
    await item.action();
  } catch (e) {
    ui.notify(`${item.label} failed: ${e}`, "error");
  }
}

function onSelect(id: string) {
  const item = items.value.find((candidate) => candidate.label === id);
  if (item) void run(item);
}

useDismiss(
  () => ui.contextMenu !== null,
  () => ui.closeContextMenu(),
  el,
);
</script>

<template>
  <Transition name="pop">
    <div v-if="menu && items.length" ref="el" class="menu" :style="position">
      <MenuSurface :groups="groups" @select="onSelect" />
    </div>
  </Transition>
</template>

<style scoped>
.menu {
  position: fixed;
  z-index: var(--z-context);
  min-width: 214px;
  transform-origin: top left;
  max-height: calc(100vh - 16px);
  overflow-y: auto;
}
</style>
