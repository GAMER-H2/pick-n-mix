import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import StereoLevelMeter from "../StereoLevelMeter.vue";
import type { OutputLevelFrame } from "@/lib/types";

const setOutputMeterEnabled = vi.fn();
const meterFrames = vi.fn();

vi.mock("@/lib/api", () => ({
  setOutputMeterEnabled: (...args: unknown[]) => setOutputMeterEnabled(...args),
  meterFrames: (...args: unknown[]) => meterFrames(...args),
}));

const silent: OutputLevelFrame = {
  levelsDb: [-60, -60],
  peaksDb: [-60, -60],
  floorDb: -60,
};

function reads(output: OutputLevelFrame) {
  meterFrames.mockResolvedValue({ output, chain: null });
}

/**
 * Let a reading arrive and be drawn.
 *
 * Reading and drawing are separated — see `useMeterFeed` — so a meter shows
 * nothing until an animation frame has run, however many replies have landed.
 */
async function drawnFrame() {
  // A reading issued before this point may still be in flight and would land
  // first, so wait for one asked for since — and then for the one after it,
  // which is only asked for once the first has landed.
  const from = meterFrames.mock.calls.length;
  for (let tries = 0; tries < 60 && meterFrames.mock.calls.length < from + 2; tries += 1) {
    await new Promise((resolve) => setTimeout(resolve, 2));
  }
  await flushPromises();
  await new Promise((resolve) => requestAnimationFrame(() => resolve(null)));
  await flushPromises();
}

describe("StereoLevelMeter", () => {
  beforeEach(() => {
    setOutputMeterEnabled.mockReset().mockResolvedValue(undefined);
    meterFrames.mockReset();
    reads(silent);
  });

  it("reads the engine's meters only for its mounted lifetime", async () => {
    const wrapper = mount(StereoLevelMeter);
    await drawnFrame();

    expect(setOutputMeterEnabled).toHaveBeenCalledWith(true);
    // The master meter has no rack beside it, so it does not ask for the taps.
    expect(meterFrames).toHaveBeenCalledWith(false);

    wrapper.unmount();
    await flushPromises();
    expect(setOutputMeterEnabled).toHaveBeenLastCalledWith(false);

    const calls = meterFrames.mock.calls.length;
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(meterFrames).toHaveBeenCalledTimes(calls);
  });

  it("draws independent accessible levels and held peaks", async () => {
    reads({ levelsDb: [-30, -12], peaksDb: [-6, -3], floorDb: -60 });

    const wrapper = mount(StereoLevelMeter);
    await drawnFrame();
    const meters = wrapper.findAll('[role="meter"]');

    expect(meters).toHaveLength(2);
    expect(meters[0].attributes("aria-label")).toBe("Left output level");
    expect(meters[1].attributes("aria-label")).toBe("Right output level");
    expect(meters[0].attributes("aria-valuenow")).toBe("-30");
    expect(meters[1].attributes("aria-valuenow")).toBe("-12");
    expect(meters[0].attributes("aria-valuetext")).toContain("peak -6.0 dBFS");

    // The mask covers everything above the level: half the scale at -30 dB of
    // a -60 dB floor, a fifth of it at -12 dB.
    const masks = wrapper.findAll(".level-meter__mask");
    expect(masks[0].attributes("style")).toContain("scaleY(0.5000)");
    expect(masks[1].attributes("style")).toContain("scaleY(0.2000)");

    const rails = wrapper.findAll(".level-meter__peak-rail");
    expect(rails[0].attributes("style")).toContain("translateY(-90.00%)");
    expect(rails[1].attributes("style")).toContain("translateY(-95.00%)");

    wrapper.unmount();
  });

  it("clamps display geometry while marking a peak at or above zero", async () => {
    reads({ levelsDb: [1.5, -90], peaksDb: [2.0, -90], floorDb: -60 });

    const wrapper = mount(StereoLevelMeter);
    await drawnFrame();
    const meters = wrapper.findAll('[role="meter"]');

    expect(meters[0].attributes("aria-valuenow")).toBe("0");
    // Clamped for drawing, reported as it came: the reading is over, and
    // saying so is the point of a meter.
    expect(meters[0].attributes("aria-valuetext")).toContain("1.5 dBFS");

    const masks = wrapper.findAll(".level-meter__mask");
    expect(masks[0].attributes("style")).toContain("scaleY(0.0000)");
    expect(masks[1].attributes("style")).toContain("scaleY(1.0000)");
    expect(wrapper.findAll(".level-meter__peak")[0].classes()).toContain("is-over");
    expect(wrapper.findAll(".level-meter__peak")[1].classes()).not.toContain("is-over");

    wrapper.unmount();
  });

  it("keeps reading after a transient failure", async () => {
    meterFrames
      .mockRejectedValueOnce(new Error("engine starting"))
      .mockResolvedValue({
        output: { levelsDb: [-18, -24], peaksDb: [-12, -18], floorDb: -60 },
        chain: null,
      });

    const wrapper = mount(StereoLevelMeter);
    // Long enough for the reader to come back round after the failed reply.
    await new Promise((resolve) => setTimeout(resolve, 40));
    await drawnFrame();

    expect(meterFrames.mock.calls.length).toBeGreaterThan(1);
    expect(wrapper.findAll('[role="meter"]')[0].attributes("aria-valuenow")).toBe("-18");

    wrapper.unmount();
  });

  /** Given a reading by its parent, it draws that and asks the engine nothing. */
  it("draws what it is handed when it is driven", async () => {
    const wrapper = mount(StereoLevelMeter, { props: { driven: true, compact: true } });
    await flushPromises();

    expect(meterFrames).not.toHaveBeenCalled();
    expect(setOutputMeterEnabled).not.toHaveBeenCalled();

    (wrapper.vm as unknown as { apply: (frame: OutputLevelFrame) => void }).apply({
      levelsDb: [-30, -30],
      peaksDb: [-30, -30],
      floorDb: -60,
    });

    expect(wrapper.findAll('[role="meter"]')[0].attributes("aria-valuenow")).toBe("-30");
    expect(wrapper.findAll(".level-meter__mask")[0].attributes("style")).toContain(
      "scaleY(0.5000)",
    );

    wrapper.unmount();
  });
});
