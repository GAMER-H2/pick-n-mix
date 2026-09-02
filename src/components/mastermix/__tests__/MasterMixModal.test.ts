import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import MasterMixModal from "../MasterMixModal.vue";
import { useMasterMixStore } from "@/stores/masterMix";
import { useMixerStore } from "@/stores/mixer";
import { usePlayerStore } from "@/stores/player";
import { locate } from "@/lib/masterMix";
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

  it("persists a chosen lane colour", async () => {
    const { wrapper, store } = await open();
    await wrapper.findAll(".mm__lane-swatch")[0].trigger("click");
    await wrapper.findAll(".mm__palette-color")[4].trigger("click");
    expect(store.mix.lanes[0].colorHue).toBe(165);
    expect(wrapper.find(".mm__palette").exists()).toBe(false);
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

  it("stop rewinds to the beginning", async () => {
    const { wrapper, store } = await open();
    store.playhead = 30;
    store.previewing = true;
    await wrapper.vm.$nextTick();
    await wrapper.get('[aria-label="Stop"]').trigger("click");
    await flushPromises();
    expect(stopMasterMix).toHaveBeenCalledWith("session_1");
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

  it("rebuilds an active preview when mute changes", async () => {
    const { wrapper, store } = await open();
    store.playhead = 15;
    await store.play(store.playhead);
    await wrapper.findAll(".mm__lane")[0].findAll(".mm__ms")[0].trigger("click");
    expect(playMasterMix).toHaveBeenCalledTimes(2);
    expect(playMasterMix).toHaveBeenLastCalledWith("session_1", "pl_1", store.mix, 15);
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
    expect(wrapper.text()).toContain("DJ Advanced Mixer");
    expect(wrapper.text()).not.toContain("Atmospheres");
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
