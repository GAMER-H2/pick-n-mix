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
const trackIds = computed(() => menu.value?.tracks.map((t) => t.id) ?? []);

/** Keep the menu inside the window. */
const position = computed(() => {
  const m = menu.value;
  if (!m) return { left: "0px", top: "0px" };
  const width = 232;
  const height = 300;
  return {
    left: `${Math.min(m.x, window.innerWidth - width - 8)}px`,
    top: `${Math.min(m.y, window.innerHeight - height - 8)}px`,
  };
});

interface Item {
  label: string;
  icon: IconName;
  /** Return values are ignored; several of these resolve to router results. */
  action: () => unknown;
  separated?: boolean;
  danger?: boolean;
}

const items = computed<Item[]>(() => {
  const m = menu.value;
  if (!m || !track.value) return [];
  const t = track.value;

  const count = m.tracks.length;
  const what = count === 1 ? `"${t.title}"` : `${count} songs`;

  const list: Item[] = [
    {
      label: "Play Next",
      icon: "playNext",
      action: async () => {
        await api.playNext(trackIds.value);
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
        await api.addToQueue(trackIds.value);
        await player.refreshQueue();
        ui.notify(`Added ${what} to the queue`);
      },
    },
    {
      label: "Add to Playlist",
      icon: "addToPlaylist",
      action: () => (ui.addToPlaylistFor = m.tracks),
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

  list.push(
    { label: "Get Info", icon: "info", separated: true, action: () => (ui.infoTrack = t) },
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
          @click="run(item)"
        >
          <PnmIcon :name="item.icon" :size="17" />
          <span>{{ item.label }}</span>
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
