import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { usePlaylistActions } from "../usePlaylistActions";
import { usePlaylistStore } from "@/stores/playlists";
import { useSettingsStore } from "@/stores/settings";
import { useUiStore } from "@/stores/ui";
import type { PlaylistSummary } from "@/lib/types";

const updatePlaylist = vi.fn();
const deletePlaylist = vi.fn();
const exportPlaylist = vi.fn();
const importPlaylist = vi.fn();
const listPlaylists = vi.fn();
const setAppPreferences = vi.fn();
const save = vi.fn();
const open = vi.fn();
const push = vi.fn();

vi.mock("@/lib/api", () => ({
  updatePlaylist: (...args: unknown[]) => updatePlaylist(...args),
  deletePlaylist: (...args: unknown[]) => deletePlaylist(...args),
  exportPlaylist: (...args: unknown[]) => exportPlaylist(...args),
  importPlaylist: (...args: unknown[]) => importPlaylist(...args),
  listPlaylists: (...args: unknown[]) => listPlaylists(...args),
  getPlaylist: vi.fn().mockResolvedValue(null),
  setAppPreferences: (...args: unknown[]) => setAppPreferences(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: (...args: unknown[]) => save(...args),
  open: (...args: unknown[]) => open(...args),
}));

vi.mock("vue-router", () => ({ useRouter: () => ({ push }) }));

function summary(id: string, name: string): PlaylistSummary {
  return {
    id,
    name,
    description: "",
    trackCount: 0,
    artwork: null,
    artworkIds: [],
    hasMixer: false,
    hasMasterMix: false,
    masterMixEnabled: false,
    shuffleOnly: false,
    path: `/playlists/${id}.pnmx`,
  };
}

describe("playlist actions", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.resetAllMocks();
    listPlaylists.mockResolvedValue([]);
    setAppPreferences.mockImplementation(async (preferences) => preferences);
  });

  it("renames through the backend and says so", async () => {
    const { rename } = usePlaylistActions();
    const ui = useUiStore();

    await rename("pl-1", "Late Night");

    expect(updatePlaylist).toHaveBeenCalledWith("pl-1", "Late Night");
    expect(ui.toast?.message).toContain("Late Night");
  });

  it("reports a rename that failed instead of pretending it worked", async () => {
    updatePlaylist.mockRejectedValue(new Error("read-only folder"));
    const { rename } = usePlaylistActions();
    const ui = useUiStore();

    await rename("pl-1", "Late Night");

    expect(ui.toast?.kind).toBe("error");
    expect(ui.toast?.message).toContain("read-only folder");
  });

  it("exports to the file the user picked", async () => {
    save.mockResolvedValue("/home/me/Late Night.pnmx");
    const { share } = usePlaylistActions();

    await share("pl-1", "Late Night");

    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({ defaultPath: "Late Night.pnmx" }),
    );
    expect(exportPlaylist).toHaveBeenCalledWith("pl-1", "/home/me/Late Night.pnmx");
  });

  it("writes nothing when the save dialog is dismissed", async () => {
    save.mockResolvedValue(null);
    const { share } = usePlaylistActions();

    await share("pl-1", "Late Night");

    expect(exportPlaylist).not.toHaveBeenCalled();
  });

  it("opens what an import became", async () => {
    open.mockResolvedValue("/home/me/shared.pnmx");
    importPlaylist.mockResolvedValue("pl-new");
    const { importFile } = usePlaylistActions();

    await importFile();

    expect(importPlaylist).toHaveBeenCalledWith("/home/me/shared.pnmx");
    expect(push).toHaveBeenCalledWith({ name: "playlist", params: { id: "pl-new" } });
  });

  /**
   * The order is one person's arrangement of their own sidebar, so it is a
   * preference rather than something written into the shared playlist files.
   */
  it("stores the dragged order as a preference", async () => {
    const playlists = usePlaylistStore();
    playlists.summaries = [summary("a", "A"), summary("b", "B"), summary("c", "C")];
    listPlaylists.mockResolvedValue([summary("c", "C"), summary("a", "A"), summary("b", "B")]);
    const { reorder } = usePlaylistActions();

    const pending = reorder(2, 0);
    // Applied before the write finishes, so the row lands where it was dropped
    // rather than springing back until the backend answers.
    expect(playlists.summaries.map((playlist) => playlist.id)).toEqual(["c", "a", "b"]);
    await pending;

    expect(playlists.summaries.map((playlist) => playlist.id)).toEqual(["c", "a", "b"]);
    expect(setAppPreferences).toHaveBeenLastCalledWith(
      expect.objectContaining({ playlistOrder: ["c", "a", "b"] }),
    );
    expect(useSettingsStore().preferences.playlistOrder).toEqual(["c", "a", "b"]);
  });
});
