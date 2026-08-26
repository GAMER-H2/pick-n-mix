import { describe, expect, it } from "vitest";
import { symmetricCurve } from "../crossfadeCurve";
import { overlay, pitchRatio, presetSections, resolve, tempoPercent } from "../mixer";
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
    expect(fx.eq.bands).toHaveLength(6);
    expect(fx.reverb.enabled).toBe(false);
    expect(fx.filters).toEqual([]);
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
