import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import MasterMixModal from "../MasterMixModal.vue";
import { useMasterMixStore } from "@/stores/masterMix";
import { useMixerStore } from "@/stores/mixer";
import { usePlayerStore } from "@/stores/player";
import { locate, moveBlock, updateLane } from "@/lib/masterMix";
import type { MasterMix, MasterMixView } from "@/lib/types";

const masterMix = vi.fn();
const setMasterMix = vi.fn();
const entryWaveform = vi.fn();
const beginMasterMixSession = vi.fn();
const endMasterMixSession = vi.fn();
const playMasterMix = vi.fn();
const setMasterMixPlaying = vi.fn();
const stopMasterMix = vi.fn();

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  masterMix: (...args: unknown[]) => masterMix(...args),
  setMasterMix: (...args: unknown[]) => setMasterMix(...args),
  setMasterMixEnabled: vi.fn(),
  resetMasterMix: vi.fn(),
  entryWaveform: (...args: unknown[]) => entryWaveform(...args),
  beginMasterMixSession: (...args: unknown[]) => beginMasterMixSession(...args),
  endMasterMixSession: (...args: unknown[]) => endMasterMixSession(...args),
  playMasterMix: (...args: unknown[]) =>
    typeof args[1] === "string"
      ? playMasterMix(...args)
      : playMasterMix(args[3], args[0], args[1], args[2]),
  setMasterMixPlaying: (...args: unknown[]) =>
    typeof args[0] === "string"
      ? setMasterMixPlaying(...args)
      : setMasterMixPlaying(args[1], args[0]),
  stopMasterMix: (...args: unknown[]) => stopMasterMix(...args),
  mixerState: vi.fn().mockResolvedValue({ global: {}, presets: [], filters: [] }),
  filtersDirectory: vi.fn().mockResolvedValue("/tmp/filters"),
  importMixAsset: vi.fn(),
  assetWaveform: vi.fn(),
  playbackState: vi.fn(),
  currentTrack: vi.fn(),
  queueState: vi.fn(),
  togglePlay: vi.fn(),
}));

function twoLaneMix(): MasterMix {
  return {
    enabled: true,
    revision: 1,
    lanes: [
      {
        id: "l0",
        name: "First Song",
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
      {
        id: "l1",
        name: "Second Song",
        muted: false,
        soloed: false,
        gainDb: 0,
        blocks: [
          {
            id: "b",
            source: { kind: "entry", index: 1 },
            startSecs: 100,
            offsetSecs: 0,
            durationSecs: 80,
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

function view(): MasterMixView {
  return {
    playlistId: "pl_1",
    playlistName: "Evening",
    mix: twoLaneMix(),
    entries: [
      { index: 0, title: "First Song", artist: "A", artworkId: null, durationSecs: 100, available: true },
      { index: 1, title: "Second Song", artist: "B", artworkId: null, durationSecs: 80, available: true },
    ],
    durationSecs: 180,
    saved: true,
  };
}

/** Mount with the arrangement already loaded, as it is when the modal opens. */
async function open() {
  const store = useMasterMixStore();
  await store.openFor("pl_1");
  const wrapper = mount(MasterMixModal);
  await wrapper.vm.$nextTick();
  return { wrapper, store };
}

describe("MasterMixModal", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    masterMix.mockResolvedValue(view());
    setMasterMix.mockImplementation(async (_id: string, sent: MasterMix) => ({
      ...view(),
      mix: sent,
    }));
    entryWaveform.mockResolvedValue({ peaks: [], peaksPerSec: 25, durationSecs: 100 });
    beginMasterMixSession.mockResolvedValue("session_1");
    endMasterMixSession.mockResolvedValue(undefined);
    playMasterMix.mockResolvedValue(180);
    setMasterMixPlaying.mockImplementation(async (_sessionId: string, playing: boolean) => playing);
  });

  it("draws a lane per song and a block for each", async () => {
    const { wrapper } = await open();
    expect(wrapper.findAll(".mm__lane")).toHaveLength(2);
    expect(wrapper.findAll(".block")).toHaveLength(2);
    expect(wrapper.text()).toContain("First Song");
    expect(wrapper.text()).toContain("Second Song");
  });

  it("shows the playlist name and a summary of the arrangement", async () => {
    const { wrapper } = await open();
    expect(wrapper.text()).toContain("Evening");
    expect(wrapper.text()).toContain("2 tracks · 2 blocks");
  });

  it("mutes and solos a lane, and records it as an undoable edit", async () => {
    const { wrapper, store } = await open();
    const lane = wrapper.findAll(".mm__lane")[0];

    await lane.findAll(".mm__ms")[0].trigger("click");
    expect(store.mix.lanes[0].muted).toBe(true);
    expect(lane.classes()).toContain("is-muted");
    expect(lane.findAll(".mm__ms")[0].attributes("aria-label")).toBe("Unmute First Song");

    await lane.findAll(".mm__ms")[1].trigger("click");
    expect(store.mix.lanes[0].soloed).toBe(true);

    store.undo();
    expect(store.mix.lanes[0].soloed).toBe(false);
    expect(store.mix.lanes[0].muted).toBe(true);
  });

  it("splits a block where the blade is clicked", async () => {
    const { wrapper, store } = await open();
    store.tool = "blade";
    await wrapper.vm.$nextTick();

    // The timeline starts after the 190px lane header, at 8 px per second,
    // so an x of 510 is 40 seconds in.
    await wrapper.findAll(".block")[0].trigger("pointerdown", {
      button: 0,
      clientX: 510,
      clientY: 60,
    });

    expect(store.mix.lanes[0].blocks).toHaveLength(2);
    expect(store.mix.lanes[0].blocks[1].startSecs).toBe(40);
    expect(store.mix.lanes[0].blocks[1].offsetSecs).toBe(40);
  });

  it("puts the blade cut on the nearest edge when snapping is on", async () => {
    const { wrapper, store } = await open();
    // The second song starts at 60 s rather than butting the first's end, so
    // there is an edge inside the first block to snap the cut to.
    store.mix = moveBlock(store.mix, "b", 60);
    store.tool = "blade";
    await wrapper.vm.$nextTick();

    // 190px of lane header, 8px per second: 60.5 seconds in, half a second
    // past the edge and well inside the seven-pixel tolerance.
    await wrapper.findAll(".block")[0].trigger("pointerdown", {
      button: 0,
      clientX: 190 + 60.5 * 8,
      clientY: 60,
    });

    expect(store.mix.lanes[0].blocks[1].startSecs).toBe(60);
  });

  it("cuts exactly where the blade was clicked when Alt inverts snapping", async () => {
    const { wrapper, store } = await open();
    store.mix = moveBlock(store.mix, "b", 60);
    store.tool = "blade";
    await wrapper.vm.$nextTick();

    await wrapper.findAll(".block")[0].trigger("pointerdown", {
      button: 0,
      clientX: 190 + 60.5 * 8,
      clientY: 60,
      altKey: true,
    });

    expect(store.mix.lanes[0].blocks[1].startSecs).toBeCloseTo(60.5, 6);
  });

  it("cuts on the playhead, which is where the join being worked on is", async () => {
    const { wrapper, store } = await open();
    store.setPlayhead(30);
    store.tool = "blade";
    await wrapper.vm.$nextTick();

    await wrapper.findAll(".block")[0].trigger("pointerdown", {
      button: 0,
      clientX: 190 + 30.4 * 8,
      clientY: 60,
    });

    expect(store.mix.lanes[0].blocks[1].startSecs).toBe(30);
  });

  it("draws a line at whatever the blade is about to cut on", async () => {
    const { wrapper, store } = await open();
    store.mix = moveBlock(store.mix, "b", 60);
    store.tool = "blade";
    await wrapper.vm.$nextTick();

    // Hovering near the second song's edge: the line appears at the edge, not
    // under the pointer.
    await wrapper.get(".mm__body").trigger("pointermove", {
      clientX: 190 + 60.5 * 8,
      clientY: 60,
    });
    const line = wrapper.get(".mm__snapline");
    expect(line.attributes("style")).toContain(`${190 + 60 * 8}px`);

    expect(line.classes()).toContain("is-locked");

    // Out in the open the line follows the cursor instead — the blade always
    // says where it will cut — but it does not claim to have locked on.
    await wrapper.get(".mm__body").trigger("pointermove", {
      clientX: 190 + 40 * 8,
      clientY: 60,
    });
    const free = wrapper.get(".mm__snapline");
    expect(free.attributes("style")).toContain(`${190 + 40 * 8}px`);
    expect(free.classes()).not.toContain("is-locked");
  });

  it("draws the blade's line at the cursor, unlocked, while snapping is off", async () => {
    const { wrapper, store } = await open();
    store.mix = moveBlock(store.mix, "b", 60);
    store.tool = "blade";
    store.snapping = false;
    await wrapper.vm.$nextTick();

    await wrapper.get(".mm__body").trigger("pointermove", {
      clientX: 190 + 60.5 * 8,
      clientY: 60,
    });
    const line = wrapper.get(".mm__snapline");
    expect(line.attributes("style")).toContain(`${190 + 60.5 * 8}px`);
    expect(line.classes()).not.toContain("is-locked");
  });

  it("has no blade line when the pointer is over the track headers", async () => {
    const { wrapper, store } = await open();
    store.tool = "blade";
    await wrapper.vm.$nextTick();

    await wrapper.get(".mm__body").trigger("pointermove", { clientX: 40, clientY: 60 });
    expect(wrapper.find(".mm__snapline").exists()).toBe(false);
  });

  it("offers the ruler grid only while snapping is on", async () => {
    const { wrapper, store } = await open();
    const grid = wrapper.findAll(".mm__toggle").find((b) => b.text() === "Grid")!;
    expect(store.gridSnapping).toBe(false);

    await grid.trigger("click");
    expect(store.gridSnapping).toBe(true);

    store.snapping = false;
    await wrapper.vm.$nextTick();
    expect(grid.attributes("disabled")).toBeDefined();
  });

  it("takes a level typed into a lane's readout, as one undoable edit", async () => {
    const { wrapper, store } = await open();
    const field = wrapper.findAll(".mm__lane-db")[0];

    await field.trigger("focus");
    await field.setValue("-6.5");
    await field.trigger("blur");

    expect(store.mix.lanes[0].gainDb).toBe(-6.5);
    store.undo();
    expect(store.mix.lanes[0].gainDb).toBe(0);
  });

  it("clamps a typed level, and ignores one that is not a number", async () => {
    const { wrapper, store } = await open();
    const field = wrapper.findAll(".mm__lane-db")[0];

    await field.trigger("focus");
    await field.setValue("40");
    await field.trigger("blur");
    expect(store.mix.lanes[0].gainDb).toBe(12);

    await field.trigger("focus");
    await field.setValue("loud please");
    await field.trigger("blur");
    expect(store.mix.lanes[0].gainDb).toBe(12);
  });

  it("selects a block with the pointer rather than splitting it", async () => {
    const { wrapper, store } = await open();
    await wrapper.findAll(".block")[0].trigger("pointerdown", {
      button: 0,
      clientX: 510,
      clientY: 60,
    });
    expect(store.selection).toEqual(["a"]);
    expect(store.mix.lanes[0].blocks).toHaveLength(1);
  });

  it("duplicates the selected block as an undoable edit and selects the copy", async () => {
    const { wrapper, store } = await open();
    await wrapper.findAll(".block")[0].trigger("pointerdown", {
      button: 0,
      clientX: 510,
      clientY: 60,
    });
    await wrapper.get(".mm__duplicate-button").trigger("click");

    expect(store.mix.lanes[0].blocks).toHaveLength(2);
    const duplicate = store.mix.lanes[0].blocks[1];
    expect(duplicate.id).not.toBe("a");
    expect(duplicate.startSecs).toBe(100);
    expect(store.selection).toEqual([duplicate.id]);
    store.undo();
    expect(store.mix.lanes[0].blocks).toHaveLength(1);
  });

  it("duplicates with Cmd/Ctrl+D", async () => {
    const { wrapper, store } = await open();
    store.select(["a"]);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "d", metaKey: true }));
    await wrapper.vm.$nextTick();
    expect(store.mix.lanes[0].blocks).toHaveLength(2);
  });

  it("adds an empty lane for imported audio", async () => {
    const { wrapper, store } = await open();
    await wrapper.find(".mm__add-lane button").trigger("click");
    expect(store.mix.lanes).toHaveLength(3);
    expect(store.mix.lanes[2].blocks).toEqual([]);
  });

  it("renames a lane on Enter and cancels a rename with Escape", async () => {
    const { wrapper, store } = await open();
    await wrapper.findAll(".mm__lane-name")[0].trigger("dblclick");
    const input = wrapper.get(".mm__lane-name-input");
    await input.setValue("Warm-up");
    await input.trigger("keydown", { key: "Enter" });
    expect(store.mix.lanes[0].name).toBe("Warm-up");

    await wrapper.findAll(".mm__lane-name")[0].trigger("dblclick");
    const cancelInput = wrapper.get(".mm__lane-name-input");
    await cancelInput.setValue("Discarded");
    await cancelInput.trigger("keydown", { key: "Escape" });
    expect(store.mix.lanes[0].name).toBe("Warm-up");
  });

  /**
   * The picker is drawn in a layer of its own rather than inside the lane
   * header, which is what stops the lanes below painting over it — so it is
   * found in the document, not in the modal's own subtree.
   */
  it("persists a chosen lane colour from a picker above the timeline", async () => {
    const { wrapper, store } = await open();
    await wrapper.findAll(".mm__lane-swatch")[0].trigger("click");

    const palette = document.body.querySelector(".mm__palette");
    expect(palette).not.toBeNull();
    const swatches = palette!.querySelectorAll<HTMLElement>(".mm__palette-color");
    swatches[4].click();
    await wrapper.vm.$nextTick();

    expect(store.mix.lanes[0].colorHue).toBe(165);
    expect(document.body.querySelector(".mm__palette")).toBeNull();
  });

  it("deletes a lane and everything on it", async () => {
    const { wrapper, store } = await open();
    await wrapper.findAll(".mm__ms--drop")[0].trigger("click");
    expect(store.mix.lanes).toHaveLength(1);
    expect(store.mix.lanes[0].id).toBe("l1");
  });

  it("switches between pointer, blade and automation tools", async () => {
    const { wrapper, store } = await open();
    const tools = wrapper.findAll(".mm__tool");
    expect(tools).toHaveLength(3);
    expect((tools[0].element as HTMLButtonElement).disabled).toBe(false);

    await tools[0].trigger("click");
    expect(store.tool).toBe("automation");
    await tools[1].trigger("click");
    expect(store.tool).toBe("blade");
    await tools[2].trigger("click");
    expect(store.tool).toBe("select");
  });

  it("auditions the mix from the playhead", async () => {
    const { wrapper, store } = await open();
    store.playhead = 30;
    await wrapper.get('[aria-label="Play the mix"]').trigger("click");
    expect(playMasterMix).toHaveBeenCalledWith("session_1", "pl_1", store.mix, 30);
  });

  it("pauses and resumes without stopping or rebuilding the mix", async () => {
    const { wrapper, store } = await open();
    const player = usePlayerStore();
    await store.play(20);
    player.snapshot = { ...player.snapshot, playing: true };
    await wrapper.vm.$nextTick();

    await wrapper.get('[aria-label="Pause"]').trigger("click");
    expect(setMasterMixPlaying).toHaveBeenLastCalledWith("session_1", false);
    expect(store.previewPaused).toBe(true);
    expect(stopMasterMix).not.toHaveBeenCalled();

    await wrapper.get('[aria-label="Play the mix"]').trigger("click");
    expect(setMasterMixPlaying).toHaveBeenLastCalledWith("session_1", true);
    expect(playMasterMix).toHaveBeenCalledTimes(1);
    expect(store.previewPaused).toBe(false);
  });

  /** Logic's behaviour: stop parks where playing began, not at zero. */
  it("stop returns the playhead to where playing started", async () => {
    const { wrapper, store } = await open();
    store.playhead = 12;
    await store.play(12);
    store.playhead = 40;
    await wrapper.vm.$nextTick();

    await wrapper.get('[aria-label="Stop"]').trigger("click");
    await flushPromises();
    expect(stopMasterMix).toHaveBeenCalledWith("session_1");
    expect(store.playhead).toBe(12);
  });

  it("stopping while already parked at the start rewinds to the beginning", async () => {
    const { wrapper, store } = await open();
    await store.play(12);
    store.playhead = 40;
    await wrapper.vm.$nextTick();

    await wrapper.get('[aria-label="Stop"]').trigger("click");
    await flushPromises();
    expect(store.playhead).toBe(12);

    // Nothing has moved since, so the second press has nowhere to go but home.
    store.previewing = true;
    await wrapper.vm.$nextTick();
    await wrapper.get('[aria-label="Stop"]').trigger("click");
    await flushPromises();
    expect(store.playhead).toBe(0);
  });

  it("uses Option+wheel for continuous vertical zoom", async () => {
    const { wrapper, store } = await open();
    const timelineZoom = store.pixelsPerSecond;
    const before = store.laneHeight;
    await wrapper.get(".mm__body").trigger("wheel", { altKey: true, deltaY: -20 });
    expect(store.laneHeight).toBeGreaterThan(before);
    expect(store.pixelsPerSecond).toBe(timelineZoom);
  });

  it("uses a restrained continuous Cmd/Ctrl+wheel timeline zoom", async () => {
    const { wrapper, store } = await open();
    const before = store.pixelsPerSecond;
    await wrapper.get(".mm__body").trigger("wheel", { metaKey: true, deltaY: -20, clientX: 500 });
    expect(store.pixelsPerSecond).toBeGreaterThan(before);
    expect(store.pixelsPerSecond / before).toBeLessThan(1.05);
  });

  it("enlarges track lanes without changing timeline zoom", async () => {
    const { wrapper, store } = await open();
    const timelineZoom = store.pixelsPerSecond;
    const before = store.laneHeight;

    await wrapper.get('[aria-label="Increase track height"]').trigger("click");

    expect(store.laneHeight).toBeGreaterThan(before);
    expect(store.pixelsPerSecond).toBe(timelineZoom);
    expect(wrapper.find(".mm__lane").attributes("style")).toContain(`height: ${store.laneHeight}px`);
  });

  it("dims tracks silenced by another track's solo", async () => {
    const { wrapper } = await open();
    await wrapper.findAll(".mm__lane")[0].find(".mm__ms--solo").trigger("click");
    expect(wrapper.findAll(".mm__lane")[0].classes()).not.toContain("is-solo-silenced");
    expect(wrapper.findAll(".mm__lane")[1].classes()).toContain("is-solo-silenced");
  });

  /**
   * Edits are heard without touching the transport. The rebuild is debounced,
   * so a drag that ends in one commit costs one rebuild rather than one per
   * frame — hence the timer.
   */
  it("rebuilds an active preview when mute changes", async () => {
    vi.useFakeTimers();
    try {
      const { wrapper, store } = await open();
      store.playhead = 15;
      await store.play(store.playhead);

      await wrapper.findAll(".mm__lane")[0].findAll(".mm__ms")[0].trigger("click");
      await vi.advanceTimersByTimeAsync(300);

      expect(playMasterMix).toHaveBeenCalledTimes(2);
      expect(playMasterMix).toHaveBeenLastCalledWith("session_1", "pl_1", store.mix, 15);
    } finally {
      vi.useRealTimers();
    }
  });

  /** Moving a block is audible immediately: no nudging the playhead first. */
  it("rebuilds an active preview when a block is moved", async () => {
    vi.useFakeTimers();
    try {
      const { store } = await open();
      await store.play(15);

      store.commit(moveBlock(store.mix, "a", 4));
      await vi.advanceTimersByTimeAsync(300);

      expect(playMasterMix).toHaveBeenCalledTimes(2);
      expect(playMasterMix).toHaveBeenLastCalledWith("session_1", "pl_1", store.mix, 15);
    } finally {
      vi.useRealTimers();
    }
  });

  /** Renaming or recolouring changes nothing audible, so it costs no re-seek. */
  it("does not rebuild the preview for an edit that cannot be heard", async () => {
    vi.useFakeTimers();
    try {
      const { store } = await open();
      await store.play(15);

      store.commit(updateLane(store.mix, 0, { name: "Intro", colorHue: 205 }));
      await vi.advanceTimersByTimeAsync(300);

      expect(playMasterMix).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("opens an attached mixer and writes effects only to one selected block", async () => {
    const { wrapper, store } = await open();
    const button = wrapper.get(".mm__mixer-button");
    expect((button.element as HTMLButtonElement).disabled).toBe(true);

    await wrapper.findAll(".block")[0].trigger("pointerdown", {
      button: 0,
      clientX: 510,
      clientY: 60,
    });
    await button.trigger("click");
    await flushPromises();

    expect(wrapper.find(".mm__block-mixer").exists()).toBe(true);
    expect(wrapper.text()).toContain("Advanced DJ Mixer");
    // Both are per-voice on the timeline, so a block gets them like anything
    // else does.
    expect(wrapper.text()).toContain("Atmospheres");
    expect(wrapper.text()).toContain("Semitones");
    const blockMixer = useMixerStore();
    expect(blockMixer.target).toMatchObject({
      kind: "block",
      playlistId: "pl_1",
      blockId: "a",
    });

    await blockMixer.setEnabled(false);
    expect(locate(store.mix, "a")?.block.mixer?.enabled).toBe(false);
    expect(locate(store.mix, "b")?.block.mixer).toBeNull();
  });

  /**
   * The whole point of the space bar here: it pauses, leaving the timeline
   * loaded and the playhead where the music got to. Stopping is the button.
   */
  it("space pauses an audition rather than stopping it", async () => {
    const { wrapper, store } = await open();
    await store.play(20);
    await wrapper.vm.$nextTick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: " ", cancelable: true }));
    await flushPromises();

    expect(setMasterMixPlaying).toHaveBeenLastCalledWith("session_1", false);
    expect(stopMasterMix).not.toHaveBeenCalled();
    expect(store.previewPaused).toBe(true);
    expect(store.playhead).toBe(20);
  });

  it("dragging across empty track space selects what the box touches", async () => {
    const { wrapper, store } = await open();
    const track = wrapper.findAll(".mm__lane-track")[0];

    // The first lane, from 0s to 25s: the block starting at 0 is inside it,
    // the one on the lane below is not.
    await track.trigger("pointerdown", { button: 0, clientX: 190, clientY: 40 });
    window.dispatchEvent(
      new PointerEvent("pointermove", { clientX: 390, clientY: 60 } as PointerEventInit),
    );
    window.dispatchEvent(new PointerEvent("pointerup"));
    await wrapper.vm.$nextTick();

    expect(store.selection).toEqual(["a"]);
    expect(wrapper.find(".mm__marquee").exists()).toBe(false);
  });

  it("clicking empty track space with nothing dragged clears the selection", async () => {
    const { wrapper, store } = await open();
    store.select(["a", "b"]);

    await wrapper.findAll(".mm__lane-track")[0].trigger("pointerdown", {
      button: 0,
      clientX: 900,
      clientY: 40,
    });
    window.dispatchEvent(new PointerEvent("pointerup"));
    await wrapper.vm.$nextTick();

    expect(store.selection).toEqual([]);
  });

  it("arrow keys nudge a selection by one ruler division", async () => {
    const { wrapper, store } = await open();
    store.select(["a"]);
    await wrapper.vm.$nextTick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", cancelable: true }));
    await flushPromises();

    const moved = locate(store.mix, "a")!.block.startSecs;
    expect(moved).toBeGreaterThan(0);
    expect(store.canUndo).toBe(true);
  });

  it("arrow keys walk the playhead when nothing is selected", async () => {
    const { wrapper, store } = await open();
    store.setPlayhead(30);
    await wrapper.vm.$nextTick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", cancelable: true }));
    await flushPromises();

    expect(store.playhead).toBeLessThan(30);
    expect(store.canUndo).toBe(false);
  });

  it("Home and End park the playhead at either end of the arrangement", async () => {
    const { store } = await open();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "End", cancelable: true }));
    await flushPromises();
    expect(store.playhead).toBe(180);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Home", cancelable: true }));
    await flushPromises();
    expect(store.playhead).toBe(0);
  });

  it("Cmd/Ctrl+A selects every block on every lane", async () => {
    const { wrapper, store } = await open();
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "a", metaKey: true, cancelable: true }),
    );
    await wrapper.vm.$nextTick();
    expect(store.selection).toEqual(["a", "b"]);
  });

  /** The lane gain has always been in the file format with no way to set it. */
  it("the lane fader writes a gain and records the drag as one undo step", async () => {
    const { wrapper, store } = await open();
    const fader = wrapper.findAllComponents({ name: "AppSlider" })[0];

    fader.vm.$emit("start");
    fader.vm.$emit("update:modelValue", -6);
    fader.vm.$emit("update:modelValue", -4.5);
    fader.vm.$emit("end");
    await wrapper.vm.$nextTick();

    expect(store.mix.lanes[0].gainDb).toBe(-4.5);
    store.undo();
    expect(store.mix.lanes[0].gainDb).toBe(0);
  });

  it("moves the playhead when the ruler is clicked", async () => {
    const { wrapper, store } = await open();
    await wrapper.find(".mm__ruler").trigger("pointerdown", { clientX: 590, clientY: 10 });
    expect(store.playhead).toBe(50);
  });

  it("never moves the playhead past the end of the arrangement", async () => {
    const { wrapper, store } = await open();
    await wrapper.find(".mm__ruler").trigger("pointerdown", { clientX: 9000, clientY: 10 });
    expect(store.playhead).toBe(180);
  });
});
