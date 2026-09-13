import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PlaylistView from "./PlaylistView.vue";
import type { MasterMix, ResolvedPlaylist, Track } from "@/lib/types";

const getPlaylist = vi.fn();
const playPlaylist = vi.fn();
const setShuffle = vi.fn();

vi.mock("vue-router", () => ({
  useRoute: () => ({ params: { id: "playlist-1" }, query: {} }),
  useRouter: () => ({ replace: vi.fn(), push: vi.fn() }),
}));

vi.mock("@/lib/api", () => ({
  getPlaylist: (...args: unknown[]) => getPlaylist(...args),
  playPlaylist: (...args: unknown[]) => playPlaylist(...args),
  setShuffle: (...args: unknown[]) => setShuffle(...args),
}));

function track(): Track {
  return {
    id: "track-1",
    sourceId: "local",
    location: "/music/track.flac",
    title: "Track",
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
    effectiveFileId: "file-1",
    preferredFileId: null,
  };
}

function playlist(masterMix: MasterMix | null): ResolvedPlaylist {
  const resolvedTrack = track();
  return {
    id: "playlist-1",
    name: "Playlist",
    description: "",
    artwork: null,
    createdAt: 0,
    updatedAt: 0,
    shuffleOnly: false,
    mixer: null,
    masterMix,
    items: [{
      index: 0,
      track: resolvedTrack,
      entry: {
        title: resolvedTrack.title,
        artist: resolvedTrack.artist,
        album: resolvedTrack.album,
        albumArtist: resolvedTrack.albumArtist,
        durationSecs: resolvedTrack.durationSecs,
        trackNumber: resolvedTrack.trackNumber,
        discNumber: resolvedTrack.discNumber,
        year: resolvedTrack.year,
        musicbrainzRecordingId: resolvedTrack.musicbrainzRecordingId,
        localPath: resolvedTrack.location,
        mixer: null,
        addedAt: 0,
      },
    }],
    missingCount: 0,
  };
}

const enabledMasterMix: MasterMix = {
  enabled: true,
  revision: 1,
  bpm: 120,
  beatsPerBar: 4,
  lanes: [],
};

function mountView() {
  return mount(PlaylistView, {
    global: {
      stubs: {
        PlaylistArtwork: true,
        PnmIcon: true,
        TrackList: true,
        SearchField: true,
      },
    },
  });
}

function shuffleButton(wrapper: ReturnType<typeof mount>) {
  return wrapper.findAll("button").find((button) => button.text() === "Shuffle");
}

describe("PlaylistView master mix shuffle guard", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    getPlaylist.mockReset();
    playPlaylist.mockReset().mockResolvedValue(undefined);
    setShuffle.mockReset().mockResolvedValue(undefined);
  });

  it("disables shuffle without disabling Play when the master mix is enabled", async () => {
    getPlaylist.mockResolvedValue(playlist(enabledMasterMix));
    const wrapper = mountView();
    await flushPromises();

    expect(shuffleButton(wrapper)?.attributes("disabled")).toBeDefined();
    const playButton = wrapper.findAll("button").find((button) => button.text() === "Play");
    expect(playButton?.attributes("disabled")).toBeUndefined();

    await shuffleButton(wrapper)?.trigger("click");
    await flushPromises();
    expect(setShuffle).not.toHaveBeenCalled();
    expect(playPlaylist).not.toHaveBeenCalled();
  });

  it("keeps ordinary playlist shuffle behavior unchanged", async () => {
    getPlaylist.mockResolvedValue(playlist(null));
    const wrapper = mountView();
    await flushPromises();

    expect(shuffleButton(wrapper)?.attributes("disabled")).toBeUndefined();
    await shuffleButton(wrapper)?.trigger("click");
    await flushPromises();

    expect(setShuffle).toHaveBeenCalledWith(true);
    expect(playPlaylist).toHaveBeenCalledWith("playlist-1", 0);
  });
});
