import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import HomeView from "./HomeView.vue";
import MixCard from "@/components/media/MixCard.vue";
import { useUiStore } from "@/stores/ui";
import type { HomeShelves, ResolvedPlaylist, Track } from "@/lib/types";

const homeShelves = vi.fn();
const listPinnedMixes = vi.fn();
const mixTracks = vi.fn();
const getTrack = vi.fn();
const getPlaylist = vi.fn();
const listTracks = vi.fn();
const setShuffle = vi.fn();
const playTracks = vi.fn();

vi.mock("vue-router", () => ({
  useRouter: () => ({ push: vi.fn() }),
}));

vi.mock("@/lib/api", () => ({
  homeShelves: (...args: unknown[]) => homeShelves(...args),
  listPinnedMixes: (...args: unknown[]) => listPinnedMixes(...args),
  mixTracks: (...args: unknown[]) => mixTracks(...args),
  getTrack: (...args: unknown[]) => getTrack(...args),
  getPlaylist: (...args: unknown[]) => getPlaylist(...args),
  playMix: vi.fn(),
  listTracks: (...args: unknown[]) => listTracks(...args),
  listAlbums: vi.fn(() => []),
  listArtists: vi.fn(() => []),
  listFolders: vi.fn(() => []),
  setShuffle: (...args: unknown[]) => setShuffle(...args),
  playTracks: (...args: unknown[]) => playTracks(...args),
}));

function track(id: string): Track {
  return {
    id,
    sourceId: "local",
    location: `/music/${id}.flac`,
    title: id,
    artist: "Artist",
    albumArtist: "Artist",
    album: "Album",
    trackNumber: 1,
    discNumber: 1,
    year: 2026,
    genre: null,
    durationSecs: 180,
    sampleRate: 48000,
    channels: 2,
    bitsPerSample: 24,
    bitrateKbps: null,
    fileSize: 100,
    format: "flac",
    artworkId: null,
    musicbrainzRecordingId: null,
    musicbrainzReleaseId: null,
    gainDb: null,
    addedAt: 0,
    fileCount: 1,
    missingFileCount: 0,
    effectiveFileId: id,
    preferredFileId: null,
  };
}

const shelves: HomeShelves = {
  mixes: [{
    kind: "replay",
    name: "Replay Mix",
    description: "Recent favourites",
    trackCount: 5,
    artworkIds: [],
    pinned: false,
  }],
  picks: [{
    kind: "song",
    id: "pick",
    title: "Pick",
    subtitle: "Artist",
    artworkId: null,
    reason: "Because you played it",
    trackIds: ["pick"],
  }],
  recentPlaylists: [{
    id: "playlist",
    name: "Playlist",
    description: "",
    trackCount: 1,
    artwork: null,
    artworkIds: [],
    hasMixer: false,
    hasMasterMix: false,
    masterMixEnabled: false,
    shuffleOnly: false,
    path: "/playlist.pnmx",
  }],
  playTotal: 10,
};

describe("HomeView context menus", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    homeShelves.mockReset().mockResolvedValue(shelves);
    listPinnedMixes.mockReset().mockResolvedValue([]);
    mixTracks.mockReset().mockResolvedValue([track("mix")]);
    getTrack.mockReset().mockResolvedValue(track("pick"));
    getPlaylist.mockReset().mockResolvedValue({
      items: [{ track: track("playlist-track") }],
    } as ResolvedPlaylist);
    listTracks.mockReset().mockResolvedValue([]);
    setShuffle.mockReset().mockResolvedValue(undefined);
    playTracks.mockReset().mockResolvedValue(undefined);
  });

  it("opens the shared menu for mixes, picks, and recent playlists", async () => {
    const wrapper = mount(HomeView, {
      global: {
        stubs: { Artwork: true, PnmIcon: true, RouterLink: true },
      },
    });
    await flushPromises();
    const ui = useUiStore();

    wrapper.getComponent(MixCard).vm.$emit("menu", new MouseEvent("contextmenu", { clientX: 10, clientY: 20 }));
    await flushPromises();
    expect(ui.contextMenu?.tracks.map((item) => item.id)).toEqual(["mix"]);

    await wrapper.get(".pick").trigger("contextmenu", { clientX: 30, clientY: 40 });
    await flushPromises();
    expect(ui.contextMenu?.tracks.map((item) => item.id)).toEqual(["pick"]);

    await wrapper.get(".card").trigger("contextmenu", { clientX: 50, clientY: 60 });
    await flushPromises();
    expect(ui.contextMenu?.tracks.map((item) => item.id)).toEqual(["playlist-track"]);
  });
});

describe("HomeView shuffle library", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    homeShelves.mockReset().mockResolvedValue(shelves);
    listPinnedMixes.mockReset().mockResolvedValue([]);
    mixTracks.mockReset().mockResolvedValue([]);
    getTrack.mockReset().mockResolvedValue(null);
    getPlaylist.mockReset().mockResolvedValue(null);
    listTracks.mockReset().mockResolvedValue([]);
    setShuffle.mockReset().mockResolvedValue(undefined);
    playTracks.mockReset().mockResolvedValue(undefined);
  });

  it("disables the button while the library is empty", async () => {
    const wrapper = mount(HomeView, {
      global: { stubs: { Artwork: true, PnmIcon: true, RouterLink: true } },
    });
    await flushPromises();
    const button = wrapper.get(".shelf__title button");
    expect(button.attributes("disabled")).toBeDefined();
    await button.trigger("click");
    expect(setShuffle).not.toHaveBeenCalled();
    expect(playTracks).not.toHaveBeenCalled();
  });

  it("turns shuffle on and plays the whole library through the backend", async () => {
    listTracks.mockResolvedValue([track("a"), track("b")]);
    const wrapper = mount(HomeView, {
      global: { stubs: { Artwork: true, PnmIcon: true, RouterLink: true } },
    });
    await flushPromises();

    const button = wrapper.get(".shelf__title button");
    expect(button.attributes("disabled")).toBeUndefined();
    await button.trigger("click");
    await flushPromises();

    expect(setShuffle).toHaveBeenCalledWith(true);
    expect(playTracks).toHaveBeenCalledTimes(1);
    const payload = playTracks.mock.calls[0][0];
    expect(payload.trackIds).toEqual(["a", "b"]);
    // The opener is a random pick, so any index in the list is valid.
    expect(payload.startIndex).toBeTypeOf("number");
    expect(payload.startIndex).toBeGreaterThanOrEqual(0);
    expect(payload.startIndex).toBeLessThan(2);
    expect(payload.context).toEqual({ kind: "library", id: "library", name: "Library" });
  });

  it("reports a failure as an error toast", async () => {
    listTracks.mockResolvedValue([track("a")]);
    setShuffle.mockRejectedValue(new Error("engine offline"));
    const wrapper = mount(HomeView, {
      global: { stubs: { Artwork: true, PnmIcon: true, RouterLink: true } },
    });
    await flushPromises();

    await wrapper.get(".shelf__title button").trigger("click");
    await flushPromises();
    expect(useUiStore().toast?.kind).toBe("error");
  });
});
