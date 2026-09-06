import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import Sidebar from "../layout/Sidebar.vue";
import { usePlaylistStore } from "@/stores/playlists";
import { useUiStore } from "@/stores/ui";
import type { PlaylistSummary } from "@/lib/types";

const importPlaylist = vi.fn();
const open = vi.fn();

vi.mock("@/lib/api", () => ({
  listPlaylists: vi.fn().mockResolvedValue([]),
  getPlaylist: vi.fn().mockResolvedValue(null),
  updatePlaylist: vi.fn(),
  deletePlaylist: vi.fn(),
  exportPlaylist: vi.fn(),
  importPlaylist: (...args: unknown[]) => importPlaylist(...args),
  setAppPreferences: vi.fn(),
  mixerState: vi.fn(),
  filtersDirectory: vi.fn(),
  homeShelves: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => open(...args),
  save: vi.fn(),
}));

vi.mock("vue-router", () => ({
  useRoute: () => ({ name: "home", params: {} }),
  useRouter: () => ({ push: vi.fn(), back: vi.fn(), forward: vi.fn() }),
}));

/** Rows are links; without a router they only need to be an element. */
const RouterLink = { props: ["to"], template: "<a><slot /></a>" };

function summary(id: string, name: string): PlaylistSummary {
  return {
    id,
    name,
    description: "",
    trackCount: 3,
    artwork: null,
    artworkIds: [],
    hasMixer: false,
    hasMasterMix: false,
    masterMixEnabled: false,
    shuffleOnly: false,
    path: `/playlists/${id}.pnmx`,
  };
}

function mountSidebar() {
  const playlists = usePlaylistStore();
  playlists.summaries = [summary("a", "Morning"), summary("b", "Late Night")];
  return mount(Sidebar, { global: { stubs: { RouterLink } } });
}

describe("Sidebar playlists", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("offers rename, share and delete on a playlist row", async () => {
    const wrapper = mountSidebar();
    const ui = useUiStore();

    await wrapper.findAll("[data-row]")[1].trigger("contextmenu");

    expect(ui.contextMenu?.items?.map((item) => item.label)).toEqual([
      "Rename…",
      "Share…",
      "Delete Playlist",
    ]);
  });

  /** The row's own button, for anyone who does not think to right-click. */
  it("opens the same menu from the row's more button", async () => {
    const wrapper = mountSidebar();
    const ui = useUiStore();

    const more = wrapper
      .findAll("button")
      .find((button) => button.attributes("aria-label") === "More options for Morning");
    if (!more) throw new Error("Missing the row's more button");
    await more.trigger("click");

    expect(ui.contextMenu?.items).toHaveLength(3);
  });

  it("asks before deleting, rather than deleting on the click", async () => {
    const wrapper = mountSidebar();
    const ui = useUiStore();

    await wrapper.findAll("[data-row]")[0].trigger("contextmenu");
    const remove = ui.contextMenu?.items?.find((item) => item.label === "Delete Playlist");
    remove?.action();

    expect(ui.playlistDelete).toEqual({ id: "a", name: "Morning" });
  });

  it("every playlist row has a grip to reorder it by", () => {
    const wrapper = mountSidebar();

    const grips = wrapper
      .findAll("button")
      .filter((button) => button.attributes("aria-label")?.startsWith("Drag to reorder"));
    expect(grips).toHaveLength(2);
  });

  /* A badge per state, and only for the state the playlist is actually in. */
  it("badges a playlist whose master mix is what plays", () => {
    const playlists = usePlaylistStore();
    playlists.summaries = [
      { ...summary("a", "Morning"), hasMasterMix: true, masterMixEnabled: true },
      { ...summary("b", "Late Night"), hasMasterMix: true, masterMixEnabled: false },
    ];
    const wrapper = mount(Sidebar, { global: { stubs: { RouterLink } } });

    const badges = wrapper
      .findAll("[data-row] svg")
      .filter((icon) => icon.attributes("title") === "This playlist plays as a master mix");
    expect(badges).toHaveLength(1);
  });

  it("imports a playlist file the user chose", async () => {
    open.mockResolvedValue("/home/me/shared.pnmx");
    importPlaylist.mockResolvedValue("pl-new");
    const wrapper = mountSidebar();

    const button = wrapper
      .findAll("button")
      .find((candidate) => candidate.attributes("aria-label") === "Import a playlist file");
    if (!button) throw new Error("Missing the import button");
    await button.trigger("click");
    await new Promise((resolve) => window.setTimeout(resolve, 0));

    expect(importPlaylist).toHaveBeenCalledWith("/home/me/shared.pnmx");
  });
});
