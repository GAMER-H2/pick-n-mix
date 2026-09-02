import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import MasterMixModal from "../MasterMixModal.vue";
import { useMasterMixStore } from "@/stores/masterMix";
import type { MasterMix, MasterMixView } from "@/lib/types";

const masterMix = vi.fn();
const setMasterMix = vi.fn();
const entryWaveform = vi.fn();
const playMasterMix = vi.fn();
const stopMasterMix = vi.fn();

vi.mock("@/lib/api", () => ({
  masterMix: (...args: unknown[]) => masterMix(...args),
  setMasterMix: (...args: unknown[]) => setMasterMix(...args),
  setMasterMixEnabled: vi.fn(),
  resetMasterMix: vi.fn(),
  entryWaveform: (...args: unknown[]) => entryWaveform(...args),
  playMasterMix: (...args: unknown[]) => playMasterMix(...args),
  stopMasterMix: (...args: unknown[]) => stopMasterMix(...args),
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
    playMasterMix.mockResolvedValue(180);
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

  it("adds an empty lane for imported audio", async () => {
    const { wrapper, store } = await open();
    await wrapper.find(".mm__add-lane button").trigger("click");
    expect(store.mix.lanes).toHaveLength(3);
    expect(store.mix.lanes[2].blocks).toEqual([]);
  });

  it("deletes a lane and everything on it", async () => {
    const { wrapper, store } = await open();
    await wrapper.findAll(".mm__ms--drop")[0].trigger("click");
    expect(store.mix.lanes).toHaveLength(1);
    expect(store.mix.lanes[0].id).toBe("l1");
  });

  it("switches tools and disables the one that is not built yet", async () => {
    const { wrapper, store } = await open();
    const tools = wrapper.findAll(".mm__tool");
    expect(tools).toHaveLength(3);
    expect((tools[0].element as HTMLButtonElement).disabled).toBe(true);

    await tools[1].trigger("click");
    expect(store.tool).toBe("blade");
    await tools[2].trigger("click");
    expect(store.tool).toBe("select");
  });

  it("auditions the mix from the playhead", async () => {
    const { wrapper, store } = await open();
    store.playhead = 30;
    await wrapper.findAll(".mm__transport .icon-button")[0].trigger("click");
    expect(playMasterMix).toHaveBeenCalledWith("pl_1", store.mix, 30);
  });

  it("stop rewinds to the beginning", async () => {
    const { wrapper, store } = await open();
    store.playhead = 30;
    store.previewing = true;
    await wrapper.findAll(".mm__transport .icon-button")[1].trigger("click");
    expect(stopMasterMix).toHaveBeenCalled();
    expect(store.playhead).toBe(0);
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
