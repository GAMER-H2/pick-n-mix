import { describe, expect, it } from "vitest";
import {
  clampCurve,
  gainIn,
  gainOut,
  symmetricCurve,
} from "../crossfadeCurve";

/**
 * These mirror the assertions in `src-tauri/src/audio/crossfade.rs` almost
 * line for line. If the two ever disagree, the graph would be drawing a
 * curve different from the one actually applied to the audio — this is the
 * closest thing to a shared fixture across the language boundary.
 */
describe("crossfade curve", () => {
  it("is full at the start and silent at the boundary", () => {
    const curve = symmetricCurve(5);
    expect(curve).toEqual({
      fadeOutStart: -5,
      fadeOutEnd: 0,
      fadeInStart: -5,
      fadeInEnd: 0,
      fadeOutShape: 1,
      fadeInShape: 1,
    });

    expect(gainOut(curve, -5)).toBeCloseTo(1, 6);
    expect(gainOut(curve, 0)).toBeCloseTo(0, 6);
    expect(gainIn(curve, -5)).toBeCloseTo(0, 6);
    expect(gainIn(curve, 0)).toBeCloseTo(1, 6);
  });

  it("is equal power at every point", () => {
    const curve = symmetricCurve(6);
    for (let x = -6; x <= 0; x += 0.3) {
      const power = gainOut(curve, x) ** 2 + gainIn(curve, x) ** 2;
      expect(power).toBeCloseTo(1, 3);
    }
  });

  it("stays flat outside the window", () => {
    const curve = symmetricCurve(4);
    expect(gainOut(curve, -10)).toBe(1);
    expect(gainOut(curve, 1)).toBeCloseTo(0, 6);
    expect(gainIn(curve, -10)).toBe(0);
    expect(gainIn(curve, 10)).toBe(1);
  });

  it("adjusts each envelope when its shape changes", () => {
    const normal = symmetricCurve(4);
    const shaped = { ...normal, fadeOutShape: 2, fadeInShape: 0.5 };

    expect(gainOut(shaped, -2)).toBeGreaterThan(gainOut(normal, -2));
    expect(gainIn(shaped, -2)).toBeGreaterThan(gainIn(normal, -2));
  });

  it("never lets the outgoing song play past its own end", () => {
    const broken = {
      fadeOutStart: 2,
      fadeOutEnd: 3,
      fadeInStart: 1,
      fadeInEnd: 4,
      fadeOutShape: 1,
      fadeInShape: 1,
    };
    const clamped = clampCurve(broken, 5);
    expect(clamped.fadeOutEnd).toBeLessThanOrEqual(0);
    expect(clamped.fadeOutStart).toBeLessThanOrEqual(clamped.fadeOutEnd);
    expect(clamped.fadeInStart).toBeLessThanOrEqual(0);
  });

  it("zero length collapses every point to the boundary", () => {
    expect(clampCurve(symmetricCurve(10), 0)).toEqual(symmetricCurve(0));
  });

  it("does not forbid a gap or an overlap", () => {
    const gap = clampCurve(
      {
        fadeOutStart: -3,
        fadeOutEnd: -1,
        fadeInStart: 1,
        fadeInEnd: 3,
        fadeOutShape: 1,
        fadeInShape: 1,
      },
      5,
    );
    expect(gainOut(gap, gap.fadeOutEnd)).toBeCloseTo(0, 5);
    expect(gainIn(gap, gap.fadeOutEnd)).toBeCloseTo(0, 5);

    const overlap = clampCurve(
      {
        fadeOutStart: -4,
        fadeOutEnd: -0.5,
        fadeInStart: -3.5,
        fadeInEnd: 0,
        fadeOutShape: 1,
        fadeInShape: 1,
      },
      5,
    );
    expect(gainOut(overlap, -2)).toBeGreaterThan(0);
    expect(gainIn(overlap, -2)).toBeGreaterThan(0);
  });
});
