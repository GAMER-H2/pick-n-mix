import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import HomeView from "./HomeView.vue";
import MixCard from "@/components/home/MixCard.vue";
import { useUiStore } from "@/stores/ui";
import type { HomeShelves, ResolvedPlaylist, Track } from "@/lib/types";

const homeShelves = vi.fn();
const listPinnedMixes = vi.fn();
const mixTracks = vi.fn();
const getTrack = vi.fn();
const getPlaylist = vi.fn();

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
