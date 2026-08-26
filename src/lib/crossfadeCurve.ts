/**
 * Mirrors `src-tauri/src/audio/crossfade.rs` exactly — same formulas, same
 * clamping rules. Kept in step deliberately: this is what the graph editor
 * draws, and a graph that disagrees with the audio engine would be worse than
 * no graph at all.
 */

import type { CrossfadeCurve, CrossfadeSettings } from "./types";

export function symmetricCurve(lengthSecs: number): CrossfadeCurve {
  const length = Math.max(0, lengthSecs);
  return { fadeOutStart: -length, fadeOutEnd: 0, fadeInStart: -length, fadeInEnd: 0 };
}

export function defaultCrossfade(): CrossfadeSettings {
  return { lengthSecs: 0, curve: symmetricCurve(0) };
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

  return { fadeOutStart, fadeOutEnd, fadeInStart, fadeInEnd };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
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
  return easeDown(inverseLerp(curve.fadeOutStart, curve.fadeOutEnd, x));
}

/** Equal-power gain for the incoming song at time `x` (seconds). */
export function gainIn(curve: CrossfadeCurve, x: number): number {
  return easeUp(inverseLerp(curve.fadeInStart, curve.fadeInEnd, x));
}

/** Percentage change in tempo has no bearing here; this is a straight
 * seconds label, e.g. for the simple slider. */
export function formatSeconds(secs: number): string {
  if (secs <= 0.001) return "Off";
  return secs < 10 ? `${secs.toFixed(1)}s` : `${Math.round(secs)}s`;
}
