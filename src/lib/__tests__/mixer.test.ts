import { describe, expect, it } from "vitest";
import { symmetricCurve } from "../crossfadeCurve";
import {
  POPOVER_SECTIONS,
  REVERB_MACRO_MAX_MIX,
  defaultBands,
  hasGain,
  orderedSections,
  overlay,
  pitchRatio,
  presetSections,
  resolve,
  reverbAmount,
  reverbForAmount,
  tempoPercent,
} from "../mixer";
import type { MixerSettings } from "../types";

/**
 * The frontend resolves the cascade locally so sliders respond immediately.
 * These assertions mirror the Rust tests in `audio/params.rs`; if the two ever
 * disagree, a control would show one value while the engine played another.
 */
describe("mixer cascade", () => {
  const global: MixerSettings = {
    reverb: { enabled: true, size: 0.5, damping: 0.5, width: 1, mix: 0.1, predelayMs: 0 },
  };
  const playlist: MixerSettings = {
    reverb: { enabled: true, size: 0.5, damping: 0.5, width: 1, mix: 0.5, predelayMs: 0 },
  };
  const track: MixerSettings = {
    delay: { enabled: true, timeMs: 400, feedback: 0.3, mix: 0.2, toneHz: 6000, spread: 0 },
  };

  it("lets the innermost layer win section by section", () => {
    const fx = resolve([global, playlist, track]);
    expect(fx.reverb.mix).toBe(0.5);
    expect(fx.delay.enabled).toBe(true);
  });

  it("falls through to the layer below for untouched sections", () => {
    const fx = resolve([global, {}, {}]);
    expect(fx.reverb.mix).toBe(0.1);
  });

  it("cascades panning as one section", () => {
    const fx = resolve([
      { panning: { mode: "monoPan", position: -0.5, width: 1 } },
      {},
      { panning: { mode: "trueStereo", position: 0.25, width: 0.4 } },
    ]);
    expect(fx.panning).toEqual({ mode: "trueStereo", position: 0.25, width: 0.4 });
    expect(presetSections({ panning: fx.panning })).toContain("panning");
  });

  it("allows a playlist crossfade but ignores an entry crossfade", () => {
    const fx = resolve([
      { crossfade: { lengthSecs: 1, curve: { ...symmetricCurve(1) } } },
      { crossfade: { lengthSecs: 2, curve: { ...symmetricCurve(2) } } },
      { crossfade: { lengthSecs: 3, curve: { ...symmetricCurve(3) } } },
    ]);
    expect(fx.crossfade.lengthSecs).toBe(2);
  });

  it("fills in defaults when no layer mentions a section", () => {
    const fx = resolve([]);
    expect(fx.enabled).toBe(true);
    expect(fx.panning).toEqual({ mode: "stereoBalance", position: 0, width: 1 });
    expect(fx.eq.bands).toHaveLength(8);
    expect(fx.reverb.enabled).toBe(false);
    expect(fx.filters).toEqual([]);
  });

  /**
   * Mirrors `the_default_pass_filters_ship_disabled` in `audio/params.rs`. A
   * pass filter has no flat setting, so an enabled one in the defaults would
   * change the sound of every existing mix.
   */
  it("ships the eight Logic bands with the pass filters disabled", () => {
    const bands = defaultBands();
    expect(bands).toHaveLength(8);

    for (const band of bands) {
      if (!hasGain(band.kind)) expect(band.enabled).toBe(false);
    }
    expect(bands.filter((b) => b.enabled).every((b) => b.gainDb === 0)).toBe(true);

    // The simple mixer draws a fader per gain-bearing band, so it still shows
    // the six it always did.
    expect(bands.filter((b) => hasGain(b.kind))).toHaveLength(6);
  });

  it("treats null and undefined as absent rather than as a value", () => {
    const merged = overlay([global, { reverb: null }]);
    expect((merged.reverb as { mix: number }).mix).toBe(0.1);
  });

  it("reports only the sections a preset actually touches", () => {
    expect(presetSections(track)).toEqual(["delay"]);
    expect(presetSections({})).toEqual([]);
  });
});

describe("varispeed", () => {
  it("maps octaves onto doubling and halving", () => {
    expect(pitchRatio({ semitones: 12, cents: 0 })).toBeCloseTo(2, 10);
    expect(pitchRatio({ semitones: -12, cents: 0 })).toBeCloseTo(0.5, 10);
    expect(pitchRatio({ semitones: 0, cents: 0 })).toBe(1);
  });

  it("includes cents in the ratio", () => {
    expect(pitchRatio({ semitones: 0, cents: 100 })).toBeCloseTo(
      pitchRatio({ semitones: 1, cents: 0 }),
      10,
    );
  });

  it("clamps to the range the engine's resampler was built for", () => {
    expect(pitchRatio({ semitones: 48, cents: 0 })).toBe(4);
    expect(pitchRatio({ semitones: -48, cents: 0 })).toBe(0.25);
  });

  it("expresses the tempo change as a percentage", () => {
    expect(tempoPercent({ semitones: 12, cents: 0 })).toBeCloseTo(100, 6);
    expect(tempoPercent({ semitones: 0, cents: 0 })).toBe(0);
  });
});

/**
 * Which controls a surface shows, and in what order.
 *
 * The rule that matters is the last one: a list saved before a section existed
 * must not hide that section for ever.
 */
describe("section order and visibility", () => {
  it("follows the saved order, then the defaults for anything it does not name", () => {
    expect(orderedSections(POPOVER_SECTIONS, ["eq", "reverb"], [])).toEqual([
      "eq",
      "reverb",
      "pitch",
      "normalisation",
      "crossfade",
      "filters",
    ]);
  });

  it("ignores ids it does not know, and repeats of ones it does", () => {
    expect(orderedSections(["eq", "pitch"], ["pitch", "nonsense", "pitch"], [])).toEqual([
      "pitch",
      "eq",
    ]);
  });

  it("drops the hidden ones without losing their place for the rest", () => {
    expect(orderedSections(POPOVER_SECTIONS, ["filters"], ["pitch", "eq"])).toEqual([
      "filters",
      "reverb",
      "normalisation",
      "crossfade",
    ]);
    expect(orderedSections(POPOVER_SECTIONS, [], POPOVER_SECTIONS)).toEqual([]);
  });
});

/**
 * The popover's one reverb control.
 *
 * A slider that only moved the wet/dry balance made a reverb louder without
 * making it any bigger, so this sweeps the whole effect. What matters is that
 * every part of it opens out together, and that the slider still knows where
 * it is afterwards.
 */
describe("the one-slider reverb", () => {
  it("is off at the bottom and fully open at the top", () => {
    const off = reverbForAmount(0);
    expect(off.enabled).toBe(false);
    expect(off.mix).toBe(0);

    const full = reverbForAmount(1);
    expect(full.enabled).toBe(true);
    expect(full.mix).toBeCloseTo(REVERB_MACRO_MAX_MIX, 6);
    expect(full.width).toBeCloseTo(1, 6);
  });

  it("grows the room and widens the tail as it rises, and opens up the top end", () => {
    const steps = [0.2, 0.4, 0.6, 0.8, 1].map(reverbForAmount);
    for (let i = 1; i < steps.length; i += 1) {
      expect(steps[i].size).toBeGreaterThan(steps[i - 1].size);
      expect(steps[i].width).toBeGreaterThan(steps[i - 1].width);
      expect(steps[i].mix).toBeGreaterThan(steps[i - 1].mix);
      // Damping falls: a big room that ate its own high end would sound
      // smaller, not larger.
      expect(steps[i].damping).toBeLessThan(steps[i - 1].damping);
    }
  });

  it("stays inside the ranges the engine clamps to", () => {
    for (let a = 0; a <= 1.0001; a += 0.05) {
      const reverb = reverbForAmount(a);
      for (const value of [reverb.size, reverb.damping, reverb.width, reverb.mix]) {
        expect(value).toBeGreaterThanOrEqual(0);
        expect(value).toBeLessThanOrEqual(1);
      }
      expect(reverb.predelayMs).toBeGreaterThanOrEqual(0);
      expect(reverb.predelayMs).toBeLessThanOrEqual(250);
    }
  });

  it("reads back the position it was set to", () => {
    for (const amount of [0, 0.25, 0.5, 0.75, 1]) {
      expect(reverbAmount(reverbForAmount(amount))).toBeCloseTo(amount, 6);
    }
  });

  it("puts a hand-set reverb somewhere sensible, and a bypassed one at zero", () => {
    const byHand = { enabled: true, size: 0.1, damping: 0.9, width: 0.2, mix: 1, predelayMs: 120 };
    expect(reverbAmount(byHand)).toBe(1);
    expect(reverbAmount({ ...byHand, enabled: false })).toBe(0);
    expect(reverbAmount({ ...byHand, mix: 0.3 })).toBeCloseTo(0.5, 6);
  });
});
