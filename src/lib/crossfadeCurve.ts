/**
 * Mirrors `src-tauri/src/audio/crossfade.rs` exactly — same formulas, same
 * clamping rules. Kept in step deliberately: this is what the graph editor
 * draws, and a graph that disagrees with the audio engine would be worse than
 * no graph at all.
 */

import type { CrossfadeCurve, CrossfadeSettings } from "./types";

export const MIN_FADE_SHAPE = 0.15;
export const MAX_FADE_SHAPE = 6;
const DEFAULT_FADE_SHAPE = 1;

export function symmetricCurve(lengthSecs: number): CrossfadeCurve {
  const length = Math.max(0, lengthSecs);
  return {
    fadeOutStart: -length,
    fadeOutEnd: 0,
    fadeInStart: -length,
    fadeInEnd: 0,
    fadeOutShape: DEFAULT_FADE_SHAPE,
    fadeInShape: DEFAULT_FADE_SHAPE,
  };
}

export function defaultCrossfade(): CrossfadeSettings {
  return { lengthSecs: 0, curve: symmetricCurve(0) };
}

/** Mirror `CrossfadeSettings::with_length` in the backend. */
export function withCrossfadeLength(
  settings: CrossfadeSettings,
  lengthSecs: number,
): CrossfadeSettings {
  const length = Math.max(0, lengthSecs);
  const symmetric = symmetricCurve(settings.lengthSecs);
  if (JSON.stringify(settings.curve) === JSON.stringify(symmetric)) {
    return { lengthSecs: length, curve: symmetricCurve(length) };
  }

  const scale = length / Math.max(settings.lengthSecs, 1e-6);
  return {
    lengthSecs: length,
    curve: clampCurve(
      {
        fadeOutStart: settings.curve.fadeOutStart * scale,
        fadeOutEnd: settings.curve.fadeOutEnd * scale,
        fadeInStart: settings.curve.fadeInStart * scale,
        fadeInEnd: settings.curve.fadeInEnd * scale,
        fadeOutShape: settings.curve.fadeOutShape,
        fadeInShape: settings.curve.fadeInShape,
      },
      length,
    ),
  };
}

/**
 * Keep the curve physically playable and internally ordered after an edit:
 * each song's own two points stay ordered, the outgoing song's points stay at
 * or before its own end, and no point sits further than `lengthSecs` before
 * the boundary.
 */
export function clampCurve(curve: CrossfadeCurve, lengthSecs: number): CrossfadeCurve {
  const length = Math.max(0, lengthSecs);
  const floor = -length;

  const fadeOutEnd = clamp(curve.fadeOutEnd, floor, 0);
  const fadeOutStart = clamp(curve.fadeOutStart, floor, fadeOutEnd);

  const fadeInStart = clamp(curve.fadeInStart, floor, 0);
  const fadeInEnd = clamp(curve.fadeInEnd, fadeInStart, length);

  return {
    fadeOutStart,
    fadeOutEnd,
    fadeInStart,
    fadeInEnd,
    fadeOutShape: clampShape(curve.fadeOutShape),
    fadeInShape: clampShape(curve.fadeInShape),
  };
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function clampShape(shape: number | undefined): number {
  return Number.isFinite(shape) ? clamp(shape as number, MIN_FADE_SHAPE, MAX_FADE_SHAPE) : DEFAULT_FADE_SHAPE;
}

/** 0 before `start`, 1 after `end`, clamped. A degenerate window (start ==
 * end) snaps straight to the far side rather than dividing by zero. */
function inverseLerp(start: number, end: number, x: number): number {
  if (end <= start) return x < start ? 0 : 1;
  return clamp((x - start) / (end - start), 0, 1);
}

const HALF_PI = Math.PI / 2;

function easeDown(t: number): number {
  return Math.cos(t * HALF_PI);
}

function easeUp(t: number): number {
  return Math.sin(t * HALF_PI);
}

/** Equal-power gain for the outgoing song at time `x` (seconds). */
export function gainOut(curve: CrossfadeCurve, x: number): number {
  const time = inverseLerp(curve.fadeOutStart, curve.fadeOutEnd, x);
  return easeDown(time ** clampShape(curve.fadeOutShape));
}

/** Equal-power gain for the incoming song at time `x` (seconds). */
export function gainIn(curve: CrossfadeCurve, x: number): number {
  const time = inverseLerp(curve.fadeInStart, curve.fadeInEnd, x);
  return easeUp(time ** clampShape(curve.fadeInShape));
}

/** Percentage change in tempo has no bearing here; this is a straight
 * seconds label, e.g. for the simple slider. */
export function formatSeconds(secs: number): string {
  if (secs <= 0.001) return "Off";
  return secs < 10 ? `${secs.toFixed(1)}s` : `${Math.round(secs)}s`;
}
