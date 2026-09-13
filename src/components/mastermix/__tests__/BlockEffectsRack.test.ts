import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import BlockEffectsRack from "../BlockEffectsRack.vue";
import { useMasterMixStore } from "@/stores/masterMix";
import { useMixerStore } from "@/stores/mixer";
import { useSettingsStore } from "@/stores/settings";
import type { ChainLevelFrame, OutputLevelFrame } from "@/lib/types";

const setChainMeterBlock = vi.fn((_blockId: string | null) => Promise.resolve());
const chainLevelFrame = vi.fn();

vi.mock("@/lib/api", () => ({
  setChainMeterBlock: (blockId: string | null) => setChainMeterBlock(blockId),
  chainLevelFrame: () => chainLevelFrame(),
  setOutputMeterEnabled: () => Promise.resolve(),
  // The expanded EQ draws a live spectrum and can solo a band.
  setAnalyserEnabled: () => Promise.resolve(),
  analyserFrame: () => Promise.resolve({ bins: [], minHz: 20, maxHz: 20000, floorDb: -90 }),
  setEqSolo: () => Promise.resolve(),
  outputLevelFrame: () =>
    Promise.resolve({ levelsDb: [-60, -60], peaksDb: [-60, -60], floorDb: -60 }),
  savePreset: vi.fn(),
  deletePreset: vi.fn(),
}));

function level(db: number): OutputLevelFrame {
  return { levelsDb: [db, db], peaksDb: [db, db], floorDb: -60 };
}

/** One reading per tap on the engine's chain: in, then after each of the five. */
const stages: ChainLevelFrame = {
  blockId: "blk",
  stages: [level(-1), level(-2), level(-3), level(-4), level(-5), level(-6)],
};

/**
 * Mounted racks are tracked and unmounted between tests: a rack left up goes
 * on polling for levels, and the next test would see its calls as its own.
 */
let mounted: ReturnType<typeof mount>[] = [];

function rack() {
  const wrapper = mount(BlockEffectsRack, {
    props: { blockId: "blk", blockName: "First Song" },
  });
  mounted.push(wrapper);
  return wrapper;
}

describe("BlockEffectsRack", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    setChainMeterBlock.mockClear();
    chainLevelFrame.mockReset().mockResolvedValue(stages);

    const mixer = useMixerStore();
    mixer.target = { kind: "block", playlistId: "pl", blockId: "blk", name: "First Song" };
    // Two devices, which is what makes this block's layer have them at all.
    mixer.targetLayer = {
      reverb: { enabled: true, size: 0.5, damping: 0.5, width: 1, mix: 0.25, predelayMs: 0 },
      lofi: { enabled: true, sampleRateHz: 44100, bitDepth: 16, mix: 1 },
    };
  });

  afterEach(() => {
    for (const wrapper of mounted) wrapper.unmount();
    mounted = [];
  });

  it("draws only the devices the block has, in chain order, with a meter in every gap", () => {
    const wrapper = rack();

    expect(wrapper.findAll(".rack__device-name").map((n) => n.text())).toEqual([
      "Reverb",
      "Sample Rate",
    ]);
    // Before the first, between the two, after the last.
    expect(wrapper.findAll(".level-meter.is-compact")).toHaveLength(3);
  });

  /**
   * The meters read the engine's own chain, which has every stage in it
   * whether or not the rack shows a device for it. A gap that spans a stage
   * with no device must still read the level at the right place.
   */
  it("reads each gap from the matching tap on the whole chain", async () => {
    const store = useMasterMixStore();
    store.previewing = true;
    const wrapper = rack();
    await flushPromises();

    const meters = wrapper
      .findAll(".level-meter.is-compact [role='meter']")
      .filter((_, index) => index % 2 === 0);
    // Default chain: eq, delay, reverb, lofi, panning. Entering the reverb is
    // tap 2, between reverb and lo-fi tap 3, leaving lo-fi tap 4.
    expect(meters.map((meter) => meter.attributes("aria-valuenow"))).toEqual([
      "-3",
      "-4",
      "-5",
    ]);
    expect(meters[0].attributes("aria-label")).toBe("Left output level entering Reverb");
    expect(meters[1].attributes("aria-label")).toBe(
      "Left output level between Reverb and Sample Rate",
    );
    expect(meters[2].attributes("aria-label")).toBe("Left output level leaving Sample Rate");
  });

  it("meters the selected block, and only while the mix is sounding", async () => {
    const store = useMasterMixStore();
    const wrapper = rack();
    await flushPromises();

    expect(setChainMeterBlock).toHaveBeenCalledWith("blk");
    // Stopped, the engine's meters are at the floor and there is nothing to ask for.
    expect(chainLevelFrame).not.toHaveBeenCalled();

    store.previewing = true;
    await flushPromises();
    expect(chainLevelFrame).toHaveBeenCalled();

    wrapper.unmount();
    expect(setChainMeterBlock).toHaveBeenLastCalledWith(null);
  });

  it("ignores a frame published for another block", async () => {
    chainLevelFrame.mockResolvedValue({ ...stages, blockId: "other" });
    const store = useMasterMixStore();
    store.previewing = true;
    const wrapper = rack();
    await flushPromises();

    // Still at the floor: the readings belong to a region no longer selected.
    const meter = wrapper.get(".level-meter.is-compact [role='meter']");
    expect(meter.attributes("aria-valuenow")).toBe("-60");
  });

  it("moves a device along the chain, writing the whole order", async () => {
    const mixer = useMixerStore();
    const wrapper = rack();

    await wrapper
      .get("[aria-label='Move Sample Rate earlier in the chain']")
      .trigger("click");
    await flushPromises();

    expect(mixer.targetLayer.chainOrder).toEqual(["eq", "delay", "lofi", "reverb", "panning"]);
    expect(wrapper.findAll(".rack__device-name").map((n) => n.text())).toEqual([
      "Sample Rate",
      "Reverb",
    ]);
  });

  /**
   * Pitch, normalisation and the beds are not stages of the chain, so moving
   * one is about where it sits in the row and nothing else.
   */
  it("moves a device outside the chain without touching the chain order", async () => {
    const mixer = useMixerStore();
    mixer.targetLayer = {
      ...mixer.targetLayer,
      pitch: { semitones: 0, cents: 0 },
      filters: [],
    };
    const wrapper = rack();
    expect(
      wrapper.findAll(".rack__device--pinned .rack__device-name").map((n) => n.text()),
    ).toEqual(["Pitch", "Atmospheres"]);

    await wrapper.get("[aria-label='Move Atmospheres left']").trigger("click");
    await flushPromises();

    // It lands where the user saw it land — in front of Pitch — which in the
    // whole order means ahead of the sections the rack never showed.
    expect(mixer.targetLayer.layoutOrder).toEqual([
      "filters",
      "pitch",
      "normalisation",
      "crossfade",
    ]);
    expect(mixer.targetLayer.chainOrder).toBeUndefined();
    expect(
      wrapper.findAll(".rack__device--pinned .rack__device-name").map((n) => n.text()),
    ).toEqual(["Atmospheres", "Pitch"]);
  });

  it("opens the full equaliser from the EQ device", async () => {
    const mixer = useMixerStore();
    mixer.targetLayer = {
      ...mixer.targetLayer,
      eq: { enabled: true, preampDb: 0, bands: [] },
    };
    const wrapper = rack();
    expect(wrapper.findComponent({ name: "EqModal" }).exists()).toBe(false);

    await wrapper.get("[aria-label='Expand EQ']").trigger("click");
    expect(wrapper.findComponent({ name: "EqModal" }).exists()).toBe(true);
  });

  it("adds a device from the title-bar menu and prevents adding it twice", async () => {
    const mixer = useMixerStore();
    mixer.targetLayer = {};
    const wrapper = rack();

    expect(wrapper.text()).toContain("This block has no effects yet");
    await wrapper.get(".effects-menu__trigger").trigger("click");
    const delay = wrapper
      .findAll(".effects-menu__menu [role='menuitem']")
      .find((item) => item.text() === "Delay");
    await delay?.trigger("click");
    await flushPromises();

    expect(mixer.targetLayer.delay).toMatchObject({ enabled: true });
    expect(wrapper.findAll(".rack__device-name").map((name) => name.text())).toEqual(["Delay"]);

    await wrapper.get(".effects-menu__trigger").trigger("click");
    const addedDelay = wrapper
      .findAll(".effects-menu__menu [role='menuitem']")
      .find((item) => item.text() === "Delay");
    expect((addedDelay?.element as HTMLButtonElement).disabled).toBe(true);
  });

  it("applies presets in the rack and can hide only its built-in choices", async () => {
    const mixer = useMixerStore();
    mixer.presets = [
      {
        id: "built-in",
        name: "Built In Mix",
        builtIn: true,
        kind: "mixer",
        settings: {
                  delay: {
                    enabled: true,
                    timeMs: 300,
                    feedback: 0.2,
                    mix: 0.3,
                    toneHz: 8000,
                    spread: 0,
                  },
                },
      },
      {
        id: "custom",
        name: "My Mix",
        builtIn: false,
        kind: "mixer",
        settings: { pitch: { semitones: 2, cents: 0 } },
      },
    ];
    const wrapper = rack();

    await wrapper.get(".preset__button").trigger("click");
    expect(wrapper.text()).toContain("Built In Mix");
    expect(wrapper.text()).toContain("My Mix");

    await wrapper.findAll("[role='menuitem']").find((item) => item.text() === "My Mix")?.trigger("click");
    await flushPromises();
    expect(mixer.targetLayer.pitch).toEqual({ semitones: 2, cents: 0 });

    useSettingsStore().preferences.hideBuiltInMasterMixerPresets = true;
    await wrapper.vm.$nextTick();
    await wrapper.get(".preset__button").trigger("click");
    expect(wrapper.text()).not.toContain("Built In Mix");
    expect(wrapper.text()).toContain("My Mix");
  });

  it("takes a device off the block when it is removed", async () => {
    const mixer = useMixerStore();
    const wrapper = rack();

    await wrapper.get("[aria-label='Remove Reverb from this block']").trigger("click");
    await flushPromises();

    expect(mixer.targetLayer.reverb).toBeUndefined();
    expect(wrapper.findAll(".rack__device-name").map((n) => n.text())).toEqual(["Sample Rate"]);
  });
});
