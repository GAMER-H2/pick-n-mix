import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useMasterMixStore } from "../masterMix";
import { deleteBlocks, moveBlock } from "@/lib/masterMix";
import type { MasterMix, MasterMixView } from "@/lib/types";

const masterMix = vi.fn();
const setMasterMix = vi.fn();
const setMasterMixEnabled = vi.fn();
const resetMasterMix = vi.fn();
const entryWaveform = vi.fn();
const playMasterMix = vi.fn();
const stopMasterMix = vi.fn();

vi.mock("@/lib/api", () => ({
  masterMix: (...args: unknown[]) => masterMix(...args),
  setMasterMix: (...args: unknown[]) => setMasterMix(...args),
  setMasterMixEnabled: (...args: unknown[]) => setMasterMixEnabled(...args),
  resetMasterMix: (...args: unknown[]) => resetMasterMix(...args),
  entryWaveform: (...args: unknown[]) => entryWaveform(...args),
  playMasterMix: (...args: unknown[]) => playMasterMix(...args),
  stopMasterMix: (...args: unknown[]) => stopMasterMix(...args),
}));

function mix(): MasterMix {
  return {
    enabled: true,
    revision: 1,
    lanes: [
      {
        id: "l0",
        name: "One",
        muted: false,
        soloed: false,
        gainDb: 0,
        blocks: [
          {
            id: "a",
            source: { kind: "entry", index: 0 },
            startSecs: 0,
            offsetSecs: 0,
            durationSecs: 100,
            gainDb: 0,
            fadeInSecs: 0,
            fadeOutSecs: 0,
            mixer: null,
            automation: [],
          },
        ],
      },
    ],
  };
}

function view(overrides: Partial<MasterMixView> = {}): MasterMixView {
  return {
    playlistId: "pl_1",
    playlistName: "Evening",
    mix: mix(),
    entries: [
      { index: 0, title: "Song", artist: "Artist", artworkId: null, durationSecs: 100, available: true },
    ],
    durationSecs: 100,
    saved: true,
    ...overrides,
  };
}

describe("master mix store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    vi.useFakeTimers();
    masterMix.mockResolvedValue(view());
    setMasterMix.mockImplementation(async (_id: string, sent: MasterMix) => view({ mix: sent }));
    setMasterMixEnabled.mockImplementation(async (_id: string, enabled: boolean) =>
      view({ mix: { ...mix(), enabled } }),
    );
    resetMasterMix.mockResolvedValue(view());
    entryWaveform.mockResolvedValue({ peaks: [1, 2, 3], peaksPerSec: 25, durationSecs: 100 });
    playMasterMix.mockResolvedValue(100);
    stopMasterMix.mockResolvedValue(undefined);
  });

  it("loads a playlist's arrangement and its entries", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    expect(store.open).toBe(true);
    expect(store.playlistName).toBe("Evening");
    expect(store.mix.lanes).toHaveLength(1);
    expect(store.duration).toBe(100);
  });

  it("surfaces a load failure instead of showing an empty timeline", async () => {
    masterMix.mockRejectedValue(new Error("playlist not found"));
    const store = useMasterMixStore();
    await store.openFor("pl_missing");
    expect(store.error).toContain("playlist not found");
    expect(store.loading).toBe(false);
  });

  it("batches edits into one save rather than one per gesture", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");

    store.commit(moveBlock(store.mix, "a", 10));
    store.commit(moveBlock(store.mix, "a", 20));
    expect(setMasterMix).not.toHaveBeenCalled();

    await vi.runAllTimersAsync();
    expect(setMasterMix).toHaveBeenCalledTimes(1);
    expect(setMasterMix.mock.calls[0][1].lanes[0].blocks[0].startSecs).toBe(20);
  });

  it("undoes and redoes whole arrangements", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");

    store.commit(moveBlock(store.mix, "a", 42));
    expect(store.mix.lanes[0].blocks[0].startSecs).toBe(42);
    expect(store.canUndo).toBe(true);

    store.undo();
    expect(store.mix.lanes[0].blocks[0].startSecs).toBe(0);
    expect(store.canRedo).toBe(true);

    store.redo();
    expect(store.mix.lanes[0].blocks[0].startSecs).toBe(42);
    await vi.runAllTimersAsync();
  });

  it("drops selected ids that an undo removed from the timeline", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    store.select(["a"]);

    store.commit(deleteBlocks(store.mix, ["a"]));
    store.undo();
    expect(store.selection).toEqual(["a"]);

    store.redo();
    expect(store.selection).toEqual([]);
    await vi.runAllTimersAsync();
  });

  it("writes a pending edit before the modal closes", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    store.commit(moveBlock(store.mix, "a", 5));

    await store.close();
    expect(setMasterMix).toHaveBeenCalledTimes(1);
    expect(store.open).toBe(false);
  });

  it("auditions the arrangement in hand, not the one last saved", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    store.commit(moveBlock(store.mix, "a", 30));

    await store.play(12);
    expect(playMasterMix).toHaveBeenCalledWith("pl_1", store.mix, 12);
    expect(playMasterMix.mock.calls[0][1].lanes[0].blocks[0].startSecs).toBe(30);
    expect(store.previewing).toBe(true);
    await vi.runAllTimersAsync();
  });

  it("stops auditioning when the engine says the mix has run out", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    await store.play(0);
    store.previewEnded();
    expect(store.previewing).toBe(false);
  });

  it("does not send a stop for a mix it is not playing", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    await store.stop();
    expect(stopMasterMix).not.toHaveBeenCalled();
  });

  it("fetches each song's waveform once", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    await Promise.all([store.loadWaveform(0), store.loadWaveform(0)]);
    await store.loadWaveform(0);
    expect(entryWaveform).toHaveBeenCalledTimes(1);
    expect(store.waveforms[0].peaks).toEqual([1, 2, 3]);
  });

  it("keeps zoom inside usable limits", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    for (let i = 0; i < 50; i += 1) store.zoom(2);
    expect(store.pixelsPerSecond).toBeLessThanOrEqual(400);
    for (let i = 0; i < 100; i += 1) store.zoom(0.5);
    expect(store.pixelsPerSecond).toBeGreaterThanOrEqual(0.5);
  });

  it("reports a failed save without throwing away the edit", async () => {
    setMasterMix.mockRejectedValue(new Error("disk full"));
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    store.commit(moveBlock(store.mix, "a", 8));
    await vi.runAllTimersAsync();

    expect(store.error).toContain("disk full");
    expect(store.mix.lanes[0].blocks[0].startSecs).toBe(8);
  });
});
