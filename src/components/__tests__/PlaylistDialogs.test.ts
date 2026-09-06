import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PlaylistDialogs from "../dialogs/PlaylistDialogs.vue";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";
import type { Track } from "@/lib/types";

const createPlaylist = vi.fn();
const addToPlaylist = vi.fn();
const updatePlaylist = vi.fn();
const deletePlaylist = vi.fn();
const push = vi.fn();

vi.mock("@/lib/api", () => ({
  createPlaylist: (...args: unknown[]) => createPlaylist(...args),
  addToPlaylist: (...args: unknown[]) => addToPlaylist(...args),
  updatePlaylist: (...args: unknown[]) => updatePlaylist(...args),
  deletePlaylist: (...args: unknown[]) => deletePlaylist(...args),
  listPlaylists: vi.fn().mockResolvedValue([]),
  getPlaylist: vi.fn().mockResolvedValue(null),
  setAppPreferences: vi.fn(),
  queueState: vi.fn(),
  playbackState: vi.fn(),
  currentTrack: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn(), open: vi.fn() }));
vi.mock("vue-router", () => ({ useRouter: () => ({ push }) }));

function track(id: string): Track {
  return {
    id,
    sourceId: "local",
    location: `/m/${id}.flac`,
    title: `Song ${id}`,
    artist: "Artist",
    albumArtist: "Artist",
    album: "Album",
    trackNumber: 1,
    discNumber: 1,
    year: 2020,
    genre: null,
    durationSecs: 200,
    sampleRate: 44100,
    channels: 2,
    bitsPerSample: 16,
    bitrateKbps: 900,
    fileSize: 1000,
    format: "FLAC",
    artworkId: null,
    musicbrainzRecordingId: null,
    musicbrainzReleaseId: null,
    gainDb: null,
    addedAt: 1,
    fileCount: 1,
    missingFileCount: 0,
    effectiveFileId: "f",
    preferredFileId: null,
  };
}

function mountDialogs() {
  // The dialogs teleport to <body>; render them inline so queries reach them.
  return mount(PlaylistDialogs, { global: { stubs: { teleport: true } } });
}

async function settle() {
  await Promise.resolve();
  await new Promise((resolve) => window.setTimeout(resolve, 0));
}

describe("saving the queue as a playlist", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    createPlaylist.mockResolvedValue({ id: "pl-new", name: "Queue" });
  });

  function queueOf(...items: Track[]) {
    const player = usePlayerStore();
    player.queue = {
      items: items.map((item) => ({ kind: "track" as const, track: item })),
      currentIndex: 0,
      upcoming: [],
      shuffle: false,
      repeat: "off",
      context: { kind: "playlist", id: "pl-1", name: "Morning" },
    };
    return player;
  }

  it("suggests where the queue came from, and saves every song in order", async () => {
    queueOf(track("a"), track("b"));
    const ui = useUiStore();
    ui.saveQueueOpen = true;
    const wrapper = mountDialogs();

    const field = wrapper.get<HTMLInputElement>("input[aria-label='Playlist name']");
    expect(field.element.value).toBe("Morning");

    await field.setValue("Tuesday");
    const save = wrapper.findAll("button").find((button) => button.text() === "Save");
    await save?.trigger("click");
    await settle();

    expect(createPlaylist).toHaveBeenCalledWith("Tuesday", undefined);
    expect(addToPlaylist).toHaveBeenCalledWith("pl-new", ["a", "b"]);
    expect(push).toHaveBeenCalledWith({ name: "playlist", params: { id: "pl-new" } });
    expect(ui.saveQueueOpen).toBe(false);
  });

  /**
   * A queued mix is one indivisible block, not the songs inside it, so it
   * cannot go into a playlist as songs — and the user is told rather than
   * being left to count the difference.
   */
  it("says so when a queued mix could not be included", async () => {
    const player = queueOf(track("a"));
    player.queue.items.push({
      kind: "mix",
      mix: {
        playlistId: "pl-2",
        name: "Mixed",
        artwork: null,
        artworkIds: [],
        durationSecs: 600,
        chapters: [],
      },
    });
    const ui = useUiStore();
    ui.saveQueueOpen = true;
    const wrapper = mountDialogs();

    await wrapper.findAll("button").find((button) => button.text() === "Save")?.trigger("click");
    await settle();

    expect(addToPlaylist).toHaveBeenCalledWith("pl-new", ["a"]);
    expect(ui.toast?.message).toContain("cannot be saved as songs");
  });
});

describe("renaming and deleting", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("renames the playlist the dialog was opened for", async () => {
    const ui = useUiStore();
    ui.playlistRename = { id: "pl-1", name: "Morning" };
    const wrapper = mountDialogs();

    const field = wrapper.get<HTMLInputElement>("input[aria-label='Playlist name']");
    expect(field.element.value).toBe("Morning");
    await field.setValue("Mornings");
    await wrapper.findAll("button").find((button) => button.text() === "Rename")?.trigger("click");
    await settle();

    expect(updatePlaylist).toHaveBeenCalledWith("pl-1", "Mornings");
    expect(ui.playlistRename).toBeNull();
  });

  it("deletes only once the confirmation is accepted", async () => {
    const ui = useUiStore();
    ui.playlistDelete = { id: "pl-1", name: "Morning" };
    const wrapper = mountDialogs();

    expect(wrapper.text()).toContain("Morning");
    expect(deletePlaylist).not.toHaveBeenCalled();

    await wrapper.findAll("button").find((button) => button.text() === "Delete")?.trigger("click");
    await settle();

    expect(deletePlaylist).toHaveBeenCalledWith("pl-1");
    expect(ui.playlistDelete).toBeNull();
  });
});
