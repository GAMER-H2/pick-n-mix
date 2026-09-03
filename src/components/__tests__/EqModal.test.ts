import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount, type VueWrapper } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import EqModal from "../mixer/EqModal.vue";
import { defaultBands } from "@/lib/mixer";
import type { Eq } from "@/lib/types";

const setAnalyserEnabled = vi.fn();
const analyserFrame = vi.fn();

vi.mock("@/lib/api", () => ({
  setAnalyserEnabled: (...args: unknown[]) => setAnalyserEnabled(...args),
  analyserFrame: (...args: unknown[]) => analyserFrame(...args),
}));

function eq(overrides: Partial<Eq> = {}): Eq {
  return { enabled: true, preampDb: 0, bands: defaultBands(), ...overrides };
}

/**
 * The graph is 100x100 in its own space; the component reads the rendered box
 * to convert pointer positions, which happy-dom reports as zero-sized. Give it
 * a real one so drags can be exercised.
 */
function stubPlotBox(wrapper: VueWrapper) {
  const plot = wrapper.find(".plot").element as HTMLElement;
  plot.getBoundingClientRect = () =>
    ({ left: 0, top: 0, width: 1000, height: 400, right: 1000, bottom: 400 }) as DOMRect;

  // happy-dom does not implement pointer capture, which the drag relies on to
  // keep receiving moves once the pointer leaves the node.
  for (const node of wrapper.findAll(".node")) {
    const el = node.element as HTMLElement;
    el.setPointerCapture = () => {};
    el.releasePointerCapture = () => {};
  }
  return plot;
}

function mountModal(value: Eq = eq()) {
  return mount(EqModal, {
    props: { eq: value, targetLabel: "All Playback", sampleRate: 48000 },
    attachTo: document.body,
    // BaseModal teleports to <body>; render it inline so queries reach the dialog.
    global: { stubs: { teleport: true } },
  });
}

/** The last `change` payload the component emitted. */
function lastChange(wrapper: VueWrapper): Eq {
  const events = wrapper.emitted("change");
  if (!events) throw new Error("no change was emitted");
  return events[events.length - 1][0] as Eq;
}

describe("EqModal", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    setAnalyserEnabled.mockReset().mockResolvedValue(undefined);
    analyserFrame.mockReset().mockResolvedValue({
      bins: new Array(96).fill(-90),
      minHz: 20,
      maxHz: 20000,
      floorDb: -90,
    });
  });

  it("draws a node per band", () => {
    const wrapper = mountModal();
    expect(wrapper.findAll(".node")).toHaveLength(8);
    wrapper.unmount();
  });

  it("runs the analyser only while it is open", async () => {
    const wrapper = mountModal();
    expect(setAnalyserEnabled).toHaveBeenCalledWith(true);

    wrapper.unmount();
    expect(setAnalyserEnabled).toHaveBeenLastCalledWith(false);
  });

  it("drags a node to a new frequency and gain", async () => {
    // Band 2 is the low shelf at 80 Hz, which has gain.
    const wrapper = mountModal();
    stubPlotBox(wrapper);

    const node = wrapper.findAll(".node")[1];
    await node.trigger("pointerdown", { pointerId: 1 });
    // Halfway across the log axis, and a quarter of the way up from centre.
    await node.trigger("pointermove", { pointerId: 1, clientX: 500, clientY: 100 });

    const next = lastChange(wrapper);
    // 20 Hz .. 20 kHz, so the midpoint of a log axis is ~632 Hz.
    expect(next.bands[1].freq).toBeGreaterThan(500);
    expect(next.bands[1].freq).toBeLessThan(800);
    expect(next.bands[1].gainDb).toBeGreaterThan(0);
    wrapper.unmount();
  });

  it("clamps a drag to the gain limit the faders also use", async () => {
    const wrapper = mountModal();
    stubPlotBox(wrapper);

    const node = wrapper.findAll(".node")[1];
    await node.trigger("pointerdown", { pointerId: 1 });
    // Well above the top of the plot.
    await node.trigger("pointermove", { pointerId: 1, clientX: 500, clientY: -400 });

    expect(lastChange(wrapper).bands[1].gainDb).toBe(12);
    wrapper.unmount();
  });

  /**
   * A pass filter's gain is ignored by the engine, so letting a drag set it
   * would show a value that does nothing.
   */
  it("does not change a pass filter's gain when dragged vertically", async () => {
    const wrapper = mountModal();
    stubPlotBox(wrapper);

    // Band 1 is the high-pass.
    const node = wrapper.findAll(".node")[0];
    await node.trigger("pointerdown", { pointerId: 1 });
    await node.trigger("pointermove", { pointerId: 1, clientX: 300, clientY: 40 });

    const next = lastChange(wrapper);
    expect(next.bands[0].gainDb).toBe(0);
    // The horizontal half of the drag still applies.
    expect(next.bands[0].freq).not.toBe(30);
    wrapper.unmount();
  });

  it("adjusts Q with the wheel", async () => {
    const wrapper = mountModal();
    const node = wrapper.findAll(".node")[2];

    await node.trigger("wheel", { deltaY: -100 });
    expect(lastChange(wrapper).bands[2].q).toBeGreaterThan(0.71);

    await node.trigger("wheel", { deltaY: 100 });
    wrapper.unmount();
  });

  it("flattens a band on double-click", async () => {
    const boosted = eq();
    boosted.bands[2] = { ...boosted.bands[2], gainDb: 9 };
    const wrapper = mountModal(boosted);

    await wrapper.findAll(".node")[2].trigger("dblclick");
    expect(lastChange(wrapper).bands[2].gainDb).toBe(0);
    wrapper.unmount();
  });

  it("toggles a band without touching the others", async () => {
    const wrapper = mountModal();
    await wrapper.findAll(".band__power")[2].trigger("click");

    const next = lastChange(wrapper);
    expect(next.bands[2].enabled).toBe(false);
    expect(next.bands[1].enabled).toBe(true);
    wrapper.unmount();
  });

  it("adds and removes bands, stopping at the engine's ceiling", async () => {
    const wrapper = mountModal();
    await wrapper.find(".bands__add").trigger("click");
    expect(lastChange(wrapper).bands).toHaveLength(9);

    // Twelve is `MAX_BANDS` in dsp.rs; past that the engine would ignore them.
    const full = mountModal(eq({ bands: new Array(12).fill(defaultBands()[2]) }));
    expect(full.find(".bands__add").exists()).toBe(false);
    full.unmount();

    await wrapper.findAll(".band__remove")[0].trigger("click");
    expect(lastChange(wrapper).bands).toHaveLength(7);
    wrapper.unmount();
  });

  it("keeps the last band, since an empty EQ has nothing to draw", () => {
    const single = mountModal(eq({ bands: [defaultBands()[2]] }));
    expect(single.find(".band__remove").exists()).toBe(false);
    single.unmount();
  });

  it("closes on Escape and on the close button", async () => {
    const wrapper = mountModal();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(wrapper.emitted("close")).toBeTruthy();
    wrapper.unmount();
  });

  it("bypasses without discarding the band settings", async () => {
    const wrapper = mountModal();
    const toggle = wrapper.find(".eq-modal__power input");
    await toggle.setValue(false);

    const next = lastChange(wrapper);
    expect(next.enabled).toBe(false);
    expect(next.bands).toHaveLength(8);
    wrapper.unmount();
  });

  it("applies an EQ-only preset", async () => {
    const wrapper = mountModal();
    await wrapper.get("[aria-haspopup='menu']").trigger("click");
    expect(wrapper.find("[role='menu']").exists()).toBe(true);
    const bassBoost = wrapper.findAll("[role='menuitem']").find((option) => option.text().includes("Bass Boost"));
    expect(bassBoost).toBeTruthy();
    await bassBoost!.trigger("click");

    const next = lastChange(wrapper);
    expect(next.bands[1].gainDb).toBe(5);
    expect(next.preampDb).toBe(-3);
    expect(next.enabled).toBe(true);
    wrapper.unmount();
  });

  it("resets to the eight-band default", async () => {
    const messy = eq({ preampDb: -6, bands: [defaultBands()[2]] });
    const wrapper = mountModal(messy);

    await wrapper.find(".eq-modal__link").trigger("click");
    const next = lastChange(wrapper);
    expect(next.bands).toHaveLength(8);
    expect(next.preampDb).toBe(0);
    expect(next.enabled).toBe(true);
    wrapper.unmount();
  });
});
