/**
 * The things you can do to a playlist as a whole: rename, delete, share it as
 * a file, and import someone else's.
 *
 * Shared by the sidebar and the playlist page so both offer the same actions
 * with the same wording and the same failure handling. The dialogs these need
 * are opened through the `ui` store, and rendered once by `PlaylistDialogs`.
 */
import { save, open } from "@tauri-apps/plugin-dialog";
import { useRouter } from "vue-router";
import * as api from "@/lib/api";
import { usePlaylistStore } from "@/stores/playlists";
import { useSettingsStore } from "@/stores/settings";
import { useUiStore } from "@/stores/ui";

/** The playlist file's extension. Mirrors `EXTENSION` in `src-tauri/src/playlist.rs`. */
const PLAYLIST_EXTENSION = "pnmx";

export function usePlaylistActions() {
  const playlists = usePlaylistStore();
  const settings = useSettingsStore();
  const ui = useUiStore();
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

  return { askRename, rename, askRemove, remove, share, importFile, reorder };
}
