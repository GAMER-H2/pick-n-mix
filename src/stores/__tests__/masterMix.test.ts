import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useMasterMixStore } from "../masterMix";
import { deleteBlocks, locate, moveBlock } from "@/lib/masterMix";
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

  it("does not chase the playhead until it is asked to", async () => {
    // Following moves the timeline under the pointer, which is worse while an
    // edit is being lined up than losing sight of where the music has got to.
    const store = useMasterMixStore();
    expect(store.followPlayhead).toBe(false);
    expect(store.snapping).toBe(true);
    expect(store.gridSnapping).toBe(false);
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

  /**
   * The bug this exists for: the engine is polled five times a second, so a
   * snapshot taken *before* a seek can be delivered *after* it. Believing it
   * threw the playhead back to the old position for a frame — which is what
   * made the playhead look like it would not stay where it was put.
   */
  it("ignores an engine position left over from before a seek", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    await store.play(60);
    expect(store.playhead).toBe(60);

    store.applyEnginePosition(3.5);
    expect(store.playhead).toBe(60);

    // The first report that has caught up is believed, and so is every one
    // after it.
    store.applyEnginePosition(60.2);
    expect(store.playhead).toBe(60.2);
    store.applyEnginePosition(60.4);
    expect(store.playhead).toBe(60.4);
  });

  it("takes engine positions no further behind than the polling interval", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    await store.play(60);

    // The engine reports where the audio is, which can be a hair short of the
    // requested position without being a stale report at all.
    store.applyEnginePosition(59.9);
    expect(store.playhead).toBe(59.9);
  });

  it("dropping the playhead by hand abandons a position the engine still owes", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    await store.play(60);

    store.setPlayhead(10);
    expect(store.playhead).toBe(10);
    // Nothing is being waited for any more, so the next report is followed.
    store.applyEnginePosition(10.2);
    expect(store.playhead).toBe(10.2);
  });

  it("ignores engine positions entirely when nothing is being auditioned", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    store.setPlayhead(30);

    store.applyEnginePosition(0);
    expect(store.playhead).toBe(30);
  });

  it("a rebuild does not move where stop will return to", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    await store.play(12);
    store.playhead = 45;

    await store.reloadPreview();

    expect(store.playhead).toBe(45);
    expect(store.playStartSecs).toBe(12);
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

  /**
   * Pitching a region up makes it cover more of the song per second, so the
   * region shrinks to keep the same audio under it. The first write only
   * records the speed — there is nothing yet to compare against.
   */
  it("resizes a block when its resolved speed changes", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    store.noteBlockSpeed("a", 1);

    store.setBlockMixer("a", { pitch: { semitones: 12, cents: 0 } }, 2);

    expect(locate(store.mix, "a")!.block.durationSecs).toBe(50);
  });

  it("leaves a block alone the first time its speed is seen", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");

    store.setBlockMixer("a", { pitch: { semitones: 12, cents: 0 } }, 2);

    expect(locate(store.mix, "a")!.block.durationSecs).toBe(100);
    // But the speed is now known, so the next change does resize it.
    store.setBlockMixer("a", {}, 1);
    expect(locate(store.mix, "a")!.block.durationSecs).toBe(200);
  });

  it("writes a block mixer with no speed at all without touching its length", async () => {
    const store = useMasterMixStore();
    await store.openFor("pl_1");
    store.noteBlockSpeed("a", 1);

    store.setBlockMixer("a", { enabled: false });

    expect(locate(store.mix, "a")!.block.durationSecs).toBe(100);
  });
});
