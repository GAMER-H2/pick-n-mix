import { describe, expect, it } from "vitest";
import fixture from "./fixtures/eq-coefficients.json";
import { coefficients, logFrequencies, magnitudeDb, summedResponse } from "../eqCurve";
import { defaultBands } from "../mixer";
import type { BandKind, Eq } from "../types";

/**
 * The graph re-derives the engine's filter designs so it can redraw at pointer
 * speed. That duplicates the formulas in `audio/dsp.rs`, so both sides assert
 * against the same fixture — see `src-tauri/tests/eq_parity.rs`, which
 * generates it.
 */
describe("biquad coefficients", () => {
  it("match the fixture the engine generated", () => {
    expect(fixture.cases.length).toBeGreaterThan(0);

    for (const c of fixture.cases) {
      const got = coefficients(
        c.kind as BandKind,
        c.sampleRate,
        c.freq,
        c.gainDb,
        c.q,
      );
      // f32 on the Rust side against f64 here, so compare at single precision.
      for (const key of ["b0", "b1", "b2", "a1", "a2"] as const) {
        expect(got[key], `${c.name}: ${key}`).toBeCloseTo(c.coefficients[key], 5);
      }
    }
  });

  it("covers every band kind, so no design goes unpinned", () => {
    const kinds = new Set(fixture.cases.map((c) => c.kind));
    expect(kinds).toEqual(
      new Set(["peak", "lowShelf", "highShelf", "lowPass", "highPass"]),
    );
  });
});

describe("frequency response", () => {
  const RATE = 48000;

  function flatEq(bands: Eq["bands"]): Eq {
    return { enabled: true, preampDb: 0, bands };
  }

  it("is flat when every band is flat", () => {
    const freqs = logFrequencies(20, 20000, 64);
    const response = summedResponse(flatEq(defaultBands()), freqs, RATE);
    // The defaults are all 0 dB peaks/shelves with the pass filters disabled.
    for (const db of response) expect(db).toBeCloseTo(0, 6);
  });

  it("puts a peak band's boost at its own centre frequency", () => {
    const eq = flatEq([
      { kind: "peak", freq: 1000, gainDb: 6, q: 2, enabled: true },
    ]);
    const [atCentre] = summedResponse(eq, [1000], RATE);
    expect(atCentre).toBeCloseTo(6, 2);

    // A high-Q bell is local: two octaves out it has essentially decayed.
    const [farBelow] = summedResponse(eq, [250], RATE);
    expect(Math.abs(farBelow)).toBeLessThan(1);
  });

  it("cuts as well as it boosts", () => {
    const eq = flatEq([
      { kind: "peak", freq: 1000, gainDb: -8, q: 1.5, enabled: true },
    ]);
    expect(summedResponse(eq, [1000], RATE)[0]).toBeCloseTo(-8, 2);
  });

  it("cascades bands additively in dB", () => {
    const low: Eq["bands"][number] = {
      kind: "peak", freq: 200, gainDb: 4, q: 1, enabled: true,
    };
    const high: Eq["bands"][number] = {
      kind: "peak", freq: 5000, gainDb: 3, q: 1, enabled: true,
    };
    const freqs = logFrequencies(20, 20000, 96);

    const both = summedResponse(flatEq([low, high]), freqs, RATE);
    const apart = summedResponse(flatEq([low]), freqs, RATE).map(
      (db, i) => db + summedResponse(flatEq([high]), freqs, RATE)[i],
    );
    both.forEach((db, i) => expect(db).toBeCloseTo(apart[i], 6));
  });

  it("ignores disabled bands and a disabled section, as the engine does", () => {
    const band: Eq["bands"][number] = {
      kind: "peak", freq: 1000, gainDb: 12, q: 1, enabled: false,
    };
    expect(summedResponse(flatEq([band]), [1000], RATE)[0]).toBeCloseTo(0, 6);

    const off: Eq = {
      enabled: false,
      preampDb: 0,
      bands: [{ ...band, enabled: true }],
    };
    expect(summedResponse(off, [1000], RATE)[0]).toBeCloseTo(0, 6);
  });

  it("applies the preamp as a flat offset", () => {
    const freqs = logFrequencies(20, 20000, 32);
    const eq: Eq = { enabled: true, preampDb: -3, bands: [] };
    for (const db of summedResponse(eq, freqs, RATE)) expect(db).toBeCloseTo(-3, 6);
  });

  it("rolls a high-pass off below its corner and passes above it", () => {
    const eq = flatEq([
      { kind: "highPass", freq: 200, gainDb: 0, q: 0.71, enabled: true },
    ]);
    // ~-3 dB at the corner for a Butterworth-ish Q, steep below, flat above.
    expect(summedResponse(eq, [200], RATE)[0]).toBeCloseTo(-3, 0);
    expect(summedResponse(eq, [25], RATE)[0]).toBeLessThan(-30);
    expect(summedResponse(eq, [4000], RATE)[0]).toBeCloseTo(0, 1);
  });

  it("rolls a low-pass off above its corner", () => {
    const eq = flatEq([
      { kind: "lowPass", freq: 2000, gainDb: 0, q: 0.71, enabled: true },
    ]);
    expect(summedResponse(eq, [2000], RATE)[0]).toBeCloseTo(-3, 0);
    expect(summedResponse(eq, [200], RATE)[0]).toBeCloseTo(0, 1);
    expect(summedResponse(eq, [16000], RATE)[0]).toBeLessThan(-30);
  });

  it("approaches a shelf's full gain well past its corner", () => {
    const eq = flatEq([
      { kind: "lowShelf", freq: 200, gainDb: 6, q: 0.71, enabled: true },
    ]);
    expect(summedResponse(eq, [25], RATE)[0]).toBeCloseTo(6, 1);
    expect(summedResponse(eq, [8000], RATE)[0]).toBeCloseTo(0, 1);
  });
});

describe("logFrequencies", () => {
  it("spans the range inclusively with even spacing in log space", () => {
    const freqs = logFrequencies(20, 20000, 4);
    expect(freqs[0]).toBeCloseTo(20, 6);
    expect(freqs[freqs.length - 1]).toBeCloseTo(20000, 6);

    const ratios = freqs.slice(1).map((f, i) => f / freqs[i]);
    for (const r of ratios) expect(r).toBeCloseTo(ratios[0], 6);
  });
});

describe("magnitudeDb", () => {
  it("reports unity for a bypass biquad", () => {
    const bypass = { b0: 1, b1: 0, b2: 0, a1: 0, a2: 0 };
    for (const f of [20, 200, 2000, 20000]) {
      expect(magnitudeDb(bypass, f, 48000)).toBeCloseTo(0, 9);
    }
  });
});
