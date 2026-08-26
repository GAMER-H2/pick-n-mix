import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import ContextMenu from "../ContextMenu.vue";
import { useUiStore } from "@/stores/ui";
import type { Track } from "@/lib/types";

const playNext = vi.fn().mockResolvedValue(undefined);
const addToQueue = vi.fn().mockResolvedValue(undefined);

vi.mock("@/lib/api", () => ({
  playNext: (...args: unknown[]) => playNext(...args),
  addToQueue: (...args: unknown[]) => addToQueue(...args),
  removeFromPlaylist: vi.fn().mockResolvedValue(undefined),
  enrichTrack: vi.fn().mockResolvedValue(null),
  listPlaylists: vi.fn().mockResolvedValue([]),
  getPlaylist: vi.fn().mockResolvedValue(null),
  queueState: vi.fn().mockResolvedValue({
    items: [],
    currentIndex: null,
    upcoming: [],
    shuffle: false,
    repeat: "off",
    context: null,
  }),
  playbackState: vi.fn(),
  currentTrack: vi.fn(),
  mixerState: vi.fn(),
  filtersDirectory: vi.fn(),
  setPlaylistEntryMixer: vi.fn(),
}));

vi.mock("vue-router", () => ({ useRouter: () => ({ push: vi.fn() }) }));

function track(id: string, title: string): Track {
  return {
    id,
    sourceId: "local",
    location: `/m/${id}.flac`,
    title,
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
    fileSize: 1,
    format: "FLAC",
    artworkId: null,
    musicbrainzRecordingId: null,
    musicbrainzReleaseId: null,
    gainDb: null,
    addedAt: 0,
  };
}

function labelsOf(wrapper: ReturnType<typeof mount>) {
  return wrapper.findAll("button").map((b) => b.text());
}

async function clickItem(label: string, tracks: Track[]) {
  const ui = useUiStore();
  const wrapper = mount(ContextMenu);
  ui.openContextMenu({ x: 10, y: 10, tracks });
  await wrapper.vm.$nextTick();

  const button = wrapper.findAll("button").find((b) => b.text().includes(label));
  expect(button, `no menu item labelled "${label}"`).toBeTruthy();
  await button!.trigger("click");
  await new Promise((resolve) => setTimeout(resolve, 0));
  return { ui, wrapper };
}

describe("context menu actions", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    playNext.mockClear();
    addToQueue.mockClear();
  });

  /**
   * The menu closes before its action is awaited, so anything the action reads
   * out of the store at call time is already gone. That silently sent an empty
   * list, which made Play Next and Add to Queue do nothing at all.
   */
  it("sends the track ids even though the menu has already closed", async () => {
    const { ui } = await clickItem("Play Next", [track("t1", "One")]);
    expect(ui.contextMenu).toBeNull();
    expect(playNext).toHaveBeenCalledWith(["t1"]);
  });

  it("sends every selected id for a multi-track selection", async () => {
    await clickItem("Add to Queue", [track("t1", "One"), track("t2", "Two")]);
    expect(addToQueue).toHaveBeenCalledWith(["t1", "t2"]);
  });

  it("never calls a queue command with an empty list", async () => {
    await clickItem("Play Next", [track("t1", "One")]);
    expect(playNext.mock.calls.every((call) => (call[0] as string[]).length > 0)).toBe(true);
  });

  it("offers no album link for a track with no album", async () => {
    const ui = useUiStore();
    const wrapper = mount(ContextMenu);
    ui.openContextMenu({ x: 0, y: 0, tracks: [{ ...track("t1", "One"), album: "" }] });
    await wrapper.vm.$nextTick();

    const labels = labelsOf(wrapper);
    expect(labels.some((l) => l.includes("Go to Album"))).toBe(false);
    expect(labels.some((l) => l.includes("Go to Artist"))).toBe(true);
  });

  it("offers mixer settings only inside a playlist", async () => {
    const ui = useUiStore();
    const wrapper = mount(ContextMenu);

    ui.openContextMenu({ x: 0, y: 0, tracks: [track("t1", "One")] });
    await wrapper.vm.$nextTick();
    expect(labelsOf(wrapper).some((l) => l.includes("Mixer"))).toBe(false);

    ui.openContextMenu({
      x: 0,
      y: 0,
      tracks: [track("t1", "One")],
      playlistId: "p1",
      entryIndex: 2,
    });
    await wrapper.vm.$nextTick();
    expect(labelsOf(wrapper).some((l) => l.includes("Mixer"))).toBe(true);
  });
});
