<script setup lang="ts">
/**
 * The playlist-wide dialogs — rename, delete, and saving the queue as a new
 * playlist — mounted once for the whole app.
 *
 * They are opened from several places (the sidebar row menu, the playlist
 * page's menu, the queue panel), so their state lives in the `ui` store and
 * the dialogs themselves live here rather than being repeated at every call
 * site.
 */
import { computed } from "vue";
import { useRouter } from "vue-router";
import ConfirmDialog from "../ui/ConfirmDialog.vue";
import NameDialog from "../ui/NameDialog.vue";
import * as api from "@/lib/api";
import { usePlayerStore } from "@/stores/player";
import { usePlaylistStore } from "@/stores/playlists";
import { useUiStore } from "@/stores/ui";
import { usePlaylistActions } from "@/composables/usePlaylistActions";

const ui = useUiStore();
const player = usePlayerStore();
const playlists = usePlaylistStore();
const router = useRouter();
const { rename, remove } = usePlaylistActions();

/** Songs in the queue. A mix sits there as one indivisible block, not as songs. */
const queueTracks = computed(() =>
  player.queue.items.flatMap((item) => (item.kind === "track" ? [item.track] : [])),
);

/** What the new playlist is called before the user types anything. */
const suggestedName = computed(() => player.queue.context?.name ?? "Queue");

async function submitRename(name: string) {
  const target = ui.playlistRename;
  ui.playlistRename = null;
  if (target) await rename(target.id, name);
}

async function confirmDelete() {
  const target = ui.playlistDelete;
  ui.playlistDelete = null;
  if (target) await remove(target.id, target.name);
}

/**
 * The queue as it stands, as a playlist: the songs in their current order,
 * including the ones already played, since the queue is the list you built.
 */
async function saveQueue(name: string) {
  ui.saveQueueOpen = false;
  const ids = queueTracks.value.map((track) => track.id);
  try {
    const created = await playlists.create(name);
    if (ids.length > 0) await api.addToPlaylist(created.id, ids);
    await playlists.refresh();
    await router.push({ name: "playlist", params: { id: created.id } });
    const skipped = player.queue.items.length - ids.length;
    ui.notify(
      skipped > 0
        ? `Saved ${ids.length} songs to "${name}" — a queued mix cannot be saved as songs`
        : `Saved ${ids.length} songs to "${name}"`,
    );
  } catch (error) {
    ui.notify(
      `Could not save the queue: ${error instanceof Error ? error.message : String(error)}`,
      "error",
    );
  }
}
</script>

<template>
  <NameDialog
    v-if="ui.playlistRename"
    title="Rename playlist"
    label="Playlist name"
    confirm-label="Rename"
    :initial="ui.playlistRename.name"
    @submit="submitRename"
    @close="ui.playlistRename = null"
  />

  <ConfirmDialog
    v-if="ui.playlistDelete"
    title="Delete playlist?"
    :message="`&quot;${ui.playlistDelete.name}&quot; will be deleted. The songs in it stay in your library.`"
    confirm-label="Delete"
    danger
    @confirm="confirmDelete"
    @close="ui.playlistDelete = null"
  />

  <NameDialog
    v-if="ui.saveQueueOpen"
    title="Save queue as playlist"
    :subtitle="`${queueTracks.length} songs`"
    label="Playlist name"
    confirm-label="Save"
    :initial="suggestedName"
    @submit="saveQueue"
    @close="ui.saveQueueOpen = false"
  />
</template>
