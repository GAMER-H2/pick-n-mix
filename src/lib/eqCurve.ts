/**
 * The EQ's frequency response, evaluated in the browser.
 *
 * The graph has to redraw while a node is being dragged, so the maths lives
 * here rather than being asked of the engine. That means these coefficient
 * formulas are a *second* copy of the ones in `audio/dsp.rs` — they are the
 * RBJ audio-EQ-cookbook designs, and the two must agree or the curve would
 * draw something the engine is not playing. `__tests__/eqCurve.test.ts`
 * pins them against a fixture shared with the Rust test of the same name.
 */

import type { BandKind, Eq, EqBand } from "./types";

/** Matches `Biquad::set`, which clamps to keep the bilinear transform sane. */
const MIN_FREQ = 10;
const MIN_Q = 0.05;

export interface Biquad {
  b0: number;
  b1: number;
  b2: number;
  a1: number;
  a2: number;
}

/**
 * Direct-form coefficients for one band, normalised by `a0`.
 *
 * Mirrors `Biquad::set` in `audio/dsp.rs`, including its clamps: the gain
 * term is `10^(gainDb/40)` because a shelf/peak splits its gain across the
 * numerator and denominator.
 */
export function coefficients(
  kind: BandKind,
  sampleRate: number,
  freq: number,
  gainDb: number,
  q: number,
): Biquad {
  const f = Math.min(Math.max(freq, MIN_FREQ), sampleRate * 0.49);
  const qq = Math.max(q, MIN_Q);
  const a = Math.pow(10, gainDb / 40);
  const w0 = (2 * Math.PI * f) / sampleRate;
  const sinW0 = Math.sin(w0);
  const cosW0 = Math.cos(w0);
  const alpha = sinW0 / (2 * qq);

  let b0: number, b1: number, b2: number, a0: number, a1: number, a2: number;

  switch (kind) {
    case "peak":
      b0 = 1 + alpha * a;
      b1 = -2 * cosW0;
      b2 = 1 - alpha * a;
      a0 = 1 + alpha / a;
      a1 = -2 * cosW0;
      a2 = 1 - alpha / a;
      break;
    case "lowShelf": {
      const twoSqrtAAlpha = 2 * Math.sqrt(a) * alpha;
      b0 = a * (a + 1 - (a - 1) * cosW0 + twoSqrtAAlpha);
      b1 = 2 * a * (a - 1 - (a + 1) * cosW0);
      b2 = a * (a + 1 - (a - 1) * cosW0 - twoSqrtAAlpha);
      a0 = a + 1 + (a - 1) * cosW0 + twoSqrtAAlpha;
      a1 = -2 * (a - 1 + (a + 1) * cosW0);
      a2 = a + 1 + (a - 1) * cosW0 - twoSqrtAAlpha;
      break;
    }
    case "highShelf": {
      const twoSqrtAAlpha = 2 * Math.sqrt(a) * alpha;
      b0 = a * (a + 1 + (a - 1) * cosW0 + twoSqrtAAlpha);
      b1 = -2 * a * (a - 1 + (a + 1) * cosW0);
      b2 = a * (a + 1 + (a - 1) * cosW0 - twoSqrtAAlpha);
      a0 = a + 1 - (a - 1) * cosW0 + twoSqrtAAlpha;
      a1 = 2 * (a - 1 - (a + 1) * cosW0);
      a2 = a + 1 - (a - 1) * cosW0 - twoSqrtAAlpha;
      break;
    }
    case "lowPass":
      b0 = (1 - cosW0) / 2;
      b1 = 1 - cosW0;
      b2 = (1 - cosW0) / 2;
      a0 = 1 + alpha;
      a1 = -2 * cosW0;
      a2 = 1 - alpha;
      break;
    case "highPass":
      b0 = (1 + cosW0) / 2;
      b1 = -(1 + cosW0);
      b2 = (1 + cosW0) / 2;
      a0 = 1 + alpha;
      a1 = -2 * cosW0;
      a2 = 1 - alpha;
      break;
  }

  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 };
}

/**
 * Magnitude of `H(z)` at `freq`, in dB.
 *
 * Evaluated on the unit circle at `z = e^{jw}`. Written out in terms of
 * `cos w` and `cos 2w` rather than with a complex-number type, which keeps
 * this allocation-free — it runs a few hundred times per redraw.
 */
export function magnitudeDb(biquad: Biquad, freq: number, sampleRate: number): number {
  const w = (2 * Math.PI * freq) / sampleRate;
  const cosW = Math.cos(w);
  const sinW = Math.sin(w);
  const cos2W = Math.cos(2 * w);
  const sin2W = Math.sin(2 * w);

  const numRe = biquad.b0 + biquad.b1 * cosW + biquad.b2 * cos2W;
  const numIm = -(biquad.b1 * sinW + biquad.b2 * sin2W);
  const denRe = 1 + biquad.a1 * cosW + biquad.a2 * cos2W;
  const denIm = -(biquad.a1 * sinW + biquad.a2 * sin2W);

  const numSq = numRe * numRe + numIm * numIm;
  const denSq = denRe * denRe + denIm * denIm;
  if (denSq === 0) return 0;

  // 10·log10 of a squared magnitude is the same as 20·log10 of the magnitude,
  // and skips a square root.
  return 10 * Math.log10(Math.max(numSq / denSq, 1e-12));
}

/** One band's own contribution, in dB, at each of `freqs`. */
export function bandResponse(
  band: EqBand,
  freqs: readonly number[],
  sampleRate: number,
): number[] {
  const biquad = coefficients(band.kind, sampleRate, band.freq, band.gainDb, band.q);
  return freqs.map((f) => magnitudeDb(biquad, f, sampleRate));
}

/**
 * The whole EQ's response, in dB, at each of `freqs`.
 *
 * Cascaded filters multiply, so in dB they add. Disabled bands and a disabled
 * EQ contribute nothing, matching `Eq::update`/`Eq::process` in `dsp.rs`,
 * which skips disabled bands entirely and bypasses when the section is off.
 */
export function summedResponse(
  eq: Eq,
  freqs: readonly number[],
  sampleRate: number,
): number[] {
  const total = new Array<number>(freqs.length).fill(0);
  if (!eq.enabled) return total;

  for (const band of eq.bands) {
    if (!band.enabled) continue;
    const biquad = coefficients(band.kind, sampleRate, band.freq, band.gainDb, band.q);
    for (let i = 0; i < freqs.length; i += 1) {
      total[i] += magnitudeDb(biquad, freqs[i], sampleRate);
    }
  }

  // The preamp is a flat trim applied after the chain.
  if (eq.preampDb !== 0) {
    for (let i = 0; i < total.length; i += 1) total[i] += eq.preampDb;
  }
  return total;
}

/** `count` frequencies spread evenly on a log scale, for the graph's x axis. */
export function logFrequencies(min: number, max: number, count: number): number[] {
  const logMin = Math.log10(min);
  const logMax = Math.log10(max);
  const out = new Array<number>(count);
  for (let i = 0; i < count; i += 1) {
    out[i] = Math.pow(10, logMin + ((logMax - logMin) * i) / (count - 1));
  }
  return out;
}
