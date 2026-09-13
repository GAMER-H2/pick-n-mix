import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import StereoLevelMeter from "../StereoLevelMeter.vue";
import type { OutputLevelFrame } from "@/lib/types";

const setOutputMeterEnabled = vi.fn();
const outputLevelFrame = vi.fn();

vi.mock("@/lib/api", () => ({
  setOutputMeterEnabled: (...args: unknown[]) => setOutputMeterEnabled(...args),
  outputLevelFrame: (...args: unknown[]) => outputLevelFrame(...args),
}));

const silent: OutputLevelFrame = {
  levelsDb: [-60, -60],
  peaksDb: [-60, -60],
  floorDb: -60,
};

let nextFrameId = 1;
let frameCallbacks = new Map<number, FrameRequestCallback>();
const requestFrame = vi.fn((callback: FrameRequestCallback): number => {
  const id = nextFrameId;
  nextFrameId += 1;
  frameCallbacks.set(id, callback);
  return id;
});
const cancelFrame = vi.fn((id: number): void => {
  frameCallbacks.delete(id);
});

async function runNextFrame() {
  const next = frameCallbacks.entries().next().value as
    | [number, FrameRequestCallback]
    | undefined;
  if (!next) throw new Error("no animation frame was scheduled");
  frameCallbacks.delete(next[0]);
  next[1](performance.now());
  await flushPromises();
}

describe("StereoLevelMeter", () => {
  beforeEach(() => {
    setOutputMeterEnabled.mockReset().mockResolvedValue(undefined);
    outputLevelFrame.mockReset().mockResolvedValue(silent);
    nextFrameId = 1;
    frameCallbacks = new Map();
    requestFrame.mockClear();
    cancelFrame.mockClear();
    vi.stubGlobal("requestAnimationFrame", requestFrame);
    vi.stubGlobal("cancelAnimationFrame", cancelFrame);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("enables polling only for its mounted lifetime", async () => {
    const wrapper = mount(StereoLevelMeter);
    await flushPromises();

    expect(setOutputMeterEnabled).toHaveBeenCalledWith(true);
    expect(outputLevelFrame).toHaveBeenCalledTimes(1);
    expect(requestFrame).toHaveBeenCalledTimes(1);

    wrapper.unmount();
    expect(cancelFrame).toHaveBeenCalledWith(1);
    expect(setOutputMeterEnabled).toHaveBeenLastCalledWith(false);
  });

  it("renders independent accessible levels and held peaks", async () => {
    outputLevelFrame.mockResolvedValue({
      levelsDb: [-30, -12],
      peaksDb: [-6, -3],
      floorDb: -60,
    } satisfies OutputLevelFrame);

    const wrapper = mount(StereoLevelMeter);
    await flushPromises();
    const meters = wrapper.findAll('[role="meter"]');

    expect(meters).toHaveLength(2);
    expect(meters[0].attributes("aria-label")).toBe("Left output level");
    expect(meters[1].attributes("aria-label")).toBe("Right output level");
    expect(meters[0].attributes("aria-valuenow")).toBe("-30");
    expect(meters[1].attributes("aria-valuenow")).toBe("-12");
    expect(meters[0].attributes("aria-valuetext")).toContain("peak -6.0 dBFS");

    const fills = wrapper.findAll(".level-meter__fill");
    expect(fills[0].attributes("style")).toContain("inset(50.00% 0 0)");
    expect(fills[1].attributes("style")).toContain("inset(20.00% 0 0)");
    const peaks = wrapper.findAll(".level-meter__peak");
    expect(peaks[0].attributes("style")).toContain("90.00%");
    expect(peaks[1].attributes("style")).toContain("95.00%");

    wrapper.unmount();
  });

  it("clamps display geometry while marking a peak at or above zero", async () => {
    outputLevelFrame.mockResolvedValue({
      levelsDb: [1.5, -90],
      peaksDb: [2.0, -90],
      floorDb: -60,
    } satisfies OutputLevelFrame);

    const wrapper = mount(StereoLevelMeter);
    await flushPromises();
    const meters = wrapper.findAll('[role="meter"]');

    expect(meters[0].attributes("aria-valuenow")).toBe("0");
    expect(meters[0].attributes("aria-valuetext")).toContain("1.5 dBFS");
    expect(wrapper.findAll(".level-meter__fill")[0].attributes("style")).toContain(
      "inset(0.00% 0 0)",
    );
    expect(wrapper.findAll(".level-meter__peak")[0].classes()).toContain("is-over");
    expect(wrapper.findAll(".level-meter__fill")[1].attributes("style")).toContain(
      "inset(100.00% 0 0)",
    );

    wrapper.unmount();
  });

  it("keeps polling after a transient frame failure", async () => {
    outputLevelFrame
      .mockRejectedValueOnce(new Error("engine starting"))
      .mockResolvedValueOnce({
        levelsDb: [-18, -24],
        peaksDb: [-12, -18],
        floorDb: -60,
      } satisfies OutputLevelFrame);

    const wrapper = mount(StereoLevelMeter);
    await flushPromises();
    expect(requestFrame).toHaveBeenCalledTimes(1);

    await runNextFrame();
    expect(outputLevelFrame).toHaveBeenCalledTimes(2);
    expect(wrapper.findAll('[role="meter"]')[0].attributes("aria-valuenow")).toBe("-18");

    wrapper.unmount();
  });
});
