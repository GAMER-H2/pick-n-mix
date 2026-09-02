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
const beginMasterMixSession = vi.fn();
const endMasterMixSession = vi.fn();
const playMasterMix = vi.fn();
const setMasterMixPlaying = vi.fn();
const stopMasterMix = vi.fn();

vi.mock("@/lib/api", () => ({
  masterMix: (...args: unknown[]) => masterMix(...args),
  setMasterMix: (...args: unknown[]) => setMasterMix(...args),
  setMasterMixEnabled: (...args: unknown[]) => setMasterMixEnabled(...args),
  resetMasterMix: (...args: unknown[]) => resetMasterMix(...args),
  entryWaveform: (...args: unknown[]) => entryWaveform(...args),
  beginMasterMixSession: (...args: unknown[]) => beginMasterMixSession(...args),
  endMasterMixSession: (...args: unknown[]) => endMasterMixSession(...args),
  playMasterMix: (...args: unknown[]) => playMasterMix(...args),
  setMasterMixPlaying: (...args: unknown[]) => setMasterMixPlaying(...args),
  stopMasterMix: (...args: unknown[]) => stopMasterMix(...args),
  assetWaveform: vi.fn(),
  importMixAsset: vi.fn(),
  bounceMasterMix: vi.fn(),
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
      {
        index: 0,
        title: "Song",
        artist: "Artist",
        artworkId: null,
        durationSecs: 100,
        available: true,
      },
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
    setMasterMix.mockImplementation(async (_id: string, sent: MasterMix) =>
      view({ mix: sent }),
    );
    setMasterMixEnabled.mockImplementation(
      async (_id: string, enabled: boolean) =>
        view({ mix: { ...mix(), enabled } }),
    );
    resetMasterMix.mockResolvedValue(view());
    entryWaveform.mockResolvedValue({
      peaks: [1, 2, 3],
      peaksPerSec: 25,
      durationSecs: 100,
    });
    beginMasterMixSession.mockResolvedValue("session-1");
    endMasterMixSession.mockResolvedValue(true);
    playMasterMix.mockResolvedValue(100);
    setMasterMixPlaying.mockImplementation(async (playing: boolean) => playing);
    stopMasterMix.mockResolvedValue(undefined);
  });

  it("loads a playlist's arrangement and its entries", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    expect(store.open).toBe(true);
    expect(store.playlistName).toBe("Evening");
    expect(store.mix.lanes).toHaveLength(1);
    expect(store.duration).toBe(100);
    expect(beginMasterMixSession).toHaveBeenCalledTimes(1);
    expect(beginMasterMixSession.mock.invocationCallOrder[0]).toBeLessThan(
      masterMix.mock.invocationCallOrder[0],
    );
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
    expect(endMasterMixSession).toHaveBeenCalledWith("session-1");
    expect(stopMasterMix).not.toHaveBeenCalled();
    expect(store.open).toBe(false);
  });

  it("auditions the arrangement in hand, not the one last saved", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    store.commit(moveBlock(store.mix, "a", 30));

    await store.play(12);
    expect(playMasterMix).toHaveBeenCalledWith(
      "pl_1",
      store.mix,
      12,
      "session-1",
    );
    expect(playMasterMix.mock.calls[0][1].lanes[0].blocks[0].startSecs).toBe(
      30,
    );
    expect(store.previewing).toBe(true);
    await vi.runAllTimersAsync();
  });

  it("pauses and resumes an audition without replacing its timeline", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    await store.play(12);

    await store.pause();
    expect(setMasterMixPlaying).toHaveBeenLastCalledWith(false, "session-1");
    expect(store.previewing).toBe(true);
    expect(store.previewPaused).toBe(true);

    await store.resume();
    expect(setMasterMixPlaying).toHaveBeenLastCalledWith(true, "session-1");
    expect(playMasterMix).toHaveBeenCalledTimes(1);
    expect(store.previewPaused).toBe(false);
  });

  it("keeps the absolute playhead while rebuilding a preview", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    await store.play(24);
    store.playhead = 24;
    playMasterMix.mockImplementationOnce(async () => {
      store.playhead = 0;
      return 100;
    });

    await store.reloadPreview();

    expect(playMasterMix).toHaveBeenLastCalledWith(
      "pl_1",
      store.mix,
      24,
      "session-1",
    );
    expect(store.playhead).toBe(24);
  });

  it("ignores a play completion after close has restored the session", async () => {
    let finishPlay!: () => void;
    playMasterMix.mockImplementationOnce(
      () => new Promise<number>((resolve) => (finishPlay = () => resolve(100))),
    );
    const store = useMasterMixStore();
    await store.openFor("pl_1");

    const playing = store.play(10);
    await store.close();
    finishPlay();
    await playing;

    expect(endMasterMixSession).toHaveBeenCalledWith("session-1");
    expect(store.previewing).toBe(false);
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

  it("stop silences the audition without ending and restoring the modal session", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    await store.play(8);

    await store.stop();

    expect(stopMasterMix).toHaveBeenCalledWith("session-1");
    expect(endMasterMixSession).not.toHaveBeenCalled();
    expect(store.previewing).toBe(false);
  });

  it("fetches each song's waveform once", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    await Promise.all([store.loadWaveform(0), store.loadWaveform(0)]);
    await store.loadWaveform(0);
    expect(entryWaveform).toHaveBeenCalledTimes(1);
    expect(store.waveforms[0].peaks).toEqual([1, 2, 3]);
  });

  it("keeps horizontal and vertical zoom inside usable limits", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    for (let i = 0; i < 50; i += 1) {
      store.zoom(2);
      store.zoomTracks(2);
    }
    expect(store.pixelsPerSecond).toBeLessThanOrEqual(400);
    expect(store.laneHeight).toBeLessThanOrEqual(180);
    for (let i = 0; i < 100; i += 1) {
      store.zoom(0.5);
      store.zoomTracks(0.5);
    }
    expect(store.pixelsPerSecond).toBeGreaterThanOrEqual(0.5);
    expect(store.laneHeight).toBeGreaterThanOrEqual(48);
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
