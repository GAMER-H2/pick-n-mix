/**
 * The things you can do to a playlist as a whole: rename, delete, share it as
 * a file, import someone else's, and the full context menu shared by the
 * sidebar's playlist rows and the playlist page's header.
 *
 * Shared by the sidebar and the playlist page so both offer the same actions
 * with the same wording and the same failure handling. The dialogs these need
 * are opened through the `ui` store, and rendered once by `PlaylistDialogs`.
 */
import { save, open } from "@tauri-apps/plugin-dialog";
import { useRouter } from "vue-router";
import * as api from "@/lib/api";
import { usePlaylistStore } from "@/stores/playlists";
import { usePlayerStore } from "@/stores/player";
import { useSettingsStore } from "@/stores/settings";
import { useUiStore, type ContextMenuItem } from "@/stores/ui";
import type { Track } from "@/lib/types";

/** The playlist file's extension. Mirrors `EXTENSION` in `src-tauri/src/playlist.rs`. */
const PLAYLIST_EXTENSION = "pnmx";

/**
 * What the shared menu needs to know about a playlist. Both the sidebar's
 * `PlaylistSummary` rows and the playlist page's loaded `ResolvedPlaylist`
 * satisfy this shape.
 */
export interface PlaylistMenuTarget {
  id: string;
  name: string;
  artwork: string | null;
  shuffleOnly: boolean;
}

export function usePlaylistActions() {
  const playlists = usePlaylistStore();
  const settings = useSettingsStore();
  const ui = useUiStore();
  const player = usePlayerStore();
  const router = useRouter();

  function message(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  /** Opens the rename dialog; the write happens when it is submitted. */
  function askRename(id: string, name: string) {
    ui.playlistRename = { id, name };
  }

  async function rename(id: string, name: string) {
    try {
      await api.updatePlaylist(id, name);
      await playlists.refresh();
      ui.notify(`Renamed to "${name}"`);
    } catch (error) {
      ui.notify(`Could not rename that playlist: ${message(error)}`, "error");
    }
  }

  /** Opens the delete confirmation; deleting a playlist cannot be undone. */
  function askRemove(id: string, name: string) {
    ui.playlistDelete = { id, name };
  }

  async function remove(id: string, name: string) {
    try {
      const wasOpen = playlists.open?.id === id;
      await playlists.remove(id);
      // Its page is now a playlist that does not exist.
      if (wasOpen) await router.push({ name: "home" });
      ui.notify(`Deleted "${name}"`);
    } catch (error) {
      ui.notify(`Could not delete that playlist: ${message(error)}`, "error");
    }
  }

  /**
   * Write the playlist out as a file to share. It is portable by design —
   * entries identify songs by what they are, so the other end matches them
   * against its own library.
   */
  async function share(id: string, name: string) {
    const destination = await save({
      title: "Share playlist",
      defaultPath: `${name}.${PLAYLIST_EXTENSION}`,
      filters: [{ name: "Pick n Mix playlist", extensions: [PLAYLIST_EXTENSION] }],
    });
    if (typeof destination !== "string") return;
    try {
      await api.exportPlaylist(id, destination);
      ui.notify(`Saved "${name}" to ${destination.split(/[/\\]/).pop() ?? destination}`);
    } catch (error) {
      ui.notify(`Could not save that playlist: ${message(error)}`, "error");
    }
  }

  /** Take in a shared playlist file and open what it became. */
  async function importFile() {
    const selected = await open({
      multiple: false,
      title: "Import a playlist",
      filters: [{ name: "Pick n Mix playlist", extensions: [PLAYLIST_EXTENSION, "json"] }],
    });
    if (typeof selected !== "string") return;
    try {
      const id = await api.importPlaylist(selected);
      await playlists.refresh();
      await router.push({ name: "playlist", params: { id } });
      ui.notify("Playlist imported");
    } catch (error) {
      ui.notify(`Could not import that playlist: ${message(error)}`, "error");
    }
  }

  /**
   * Persist the sidebar's order.
   *
   * Stored as a preference rather than in the playlist files: it is one
   * person's arrangement of their own sidebar, and has no business travelling
   * with a shared playlist.
   */
  async function reorder(from: number, to: number) {
    const ordered = [...playlists.summaries];
    const [moved] = ordered.splice(from, 1);
    if (!moved) return;
    ordered.splice(to, 0, moved);
    // Applied locally first so the row lands where it was dropped, then
    // confirmed by the refresh once the backend has the new order.
    playlists.summaries = ordered;
    try {
      await settings.update({ playlistOrder: ordered.map((playlist) => playlist.id) });
      await playlists.refresh();
    } catch (error) {
      ui.notify(`Could not save the playlist order: ${message(error)}`, "error");
      await playlists.refresh();
    }
  }

  /**
   * Replace the playlist image. Mirrors the playlist page's own action: the
   * backend copies the file into the artwork cache, so the picture survives
   * the original being moved or deleted.
   */
  async function chooseArtwork(playlist: PlaylistMenuTarget) {
    const selected = await open({
      multiple: false,
      title: "Choose a playlist image",
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp"] }],
    });
    if (typeof selected !== "string") return;
    try {
      await api.setPlaylistArtwork(playlist.id, selected);
      await playlists.refresh();
      ui.notify("Playlist image updated");
    } catch (error) {
      ui.notify(`Could not use that image: ${error}`, "error");
    }
  }

  async function clearArtwork(playlist: PlaylistMenuTarget) {
    try {
      await api.clearPlaylistArtwork(playlist.id);
      await playlists.refresh();
    } catch (error) {
      ui.notify(`Could not reset that image: ${error}`, "error");
    }
  }

  async function toggleShuffleOnly(playlist: PlaylistMenuTarget) {
    try {
      await api.setPlaylistShuffleOnly(playlist.id, !playlist.shuffleOnly);
      await playlists.refresh();
    } catch (error) {
      ui.notify(`Could not change shuffle-only: ${error}`, "error");
    }
  }

  /**
   * The playlist menu, shared by the sidebar's rows and the playlist page's
   * header: playback, then how the playlist plays and looks, then
   * organisation, with the destructive one last and marked dangerous.
   *
   * The page omits Play — it already has a Play button beside the menu.
   */
  function playlistMenuItems(
    playlist: PlaylistMenuTarget,
    { includePlay = true }: { includePlay?: boolean } = {},
  ): ContextMenuItem[] {
    const items: ContextMenuItem[] = [];
    if (includePlay) {
      items.push({ label: "Play", icon: "play", action: () => api.playPlaylist(playlist.id, 0) });
    }
    items.push(
      {
        label: "Play Next",
        icon: "playNext",
        action: async () => {
          await api.queuePlaylist(playlist.id, true);
          await player.refreshQueue();
        },
      },
      {
        label: "Add to Queue",
        icon: "addToQueue",
        action: async () => {
          await api.queuePlaylist(playlist.id, false);
          await player.refreshQueue();
        },
      },
      {
        label: "Add to Playlist",
        icon: "addToPlaylist",
        // The shared dialog takes whole tracks (it names them and maps to ids
        // itself), so the playlist is resolved here; the menu has closed by
        // the time this runs, exactly like the song menu's own item.
        action: async () => {
          const resolved = await api.getPlaylist(playlist.id);
          const tracks = (resolved?.items ?? [])
            .map((item) => item.track)
            .filter((track): track is Track => track !== null);
          if (tracks.length === 0) {
            ui.notify("No available songs in this playlist", "error");
            return;
          }
          ui.addToPlaylistFor = tracks;
        },
      },
      {
        label: "Shuffle-Only",
        icon: "shuffle",
        separated: true,
        checked: playlist.shuffleOnly,
        action: () => toggleShuffleOnly(playlist),
      },
      { label: "Change Image…", icon: "image", action: () => chooseArtwork(playlist) },
      {
        label: "Rename…",
        icon: "edit",
        separated: true,
        action: () => askRename(playlist.id, playlist.name),
      },
      { label: "Share…", icon: "share", action: () => share(playlist.id, playlist.name) },
      {
        label: "Delete Playlist",
        icon: "trash",
        separated: true,
        danger: true,
        action: () => askRemove(playlist.id, playlist.name),
      },
    );
    // Only offered when there is a picture to reset, like the page's own menu.
    if (playlist.artwork) {
      const renameAt = items.findIndex((item) => item.label === "Rename…");
      items.splice(renameAt, 0, {
        label: "Reset Image",
        icon: "trash",
        action: () => clearArtwork(playlist),
      });
    }
    return items;
  }

  return {
    askRename,
    rename,
    askRemove,
    remove,
    share,
    importFile,
    reorder,
    chooseArtwork,
    clearArtwork,
    toggleShuffleOnly,
    playlistMenuItems,
  };
}
