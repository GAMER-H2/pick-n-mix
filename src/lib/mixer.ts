/**
 * The mixer cascade, mirrored on the frontend.
 *
 * The backend is authoritative, but the UI needs to resolve layers locally so
 * a slider can move on the same frame it is dragged rather than waiting for a
 * round trip.
 */

import { defaultCrossfade } from "./crossfadeCurve";
import type {
  BandKind,
  CrossfadeSettings,
  Delay,
  Eq,
  EqBand,
  FilterSetting,
  Lofi,
  MixerSettings,
  Normalisation,
  Panning,
  Pitch,
  ResolvedMixer,
  Reverb,
} from "./types";

/**
 * The eight bands of a Logic-style channel EQ, mirroring `default_bands()` in
 * `audio/params.rs`.
 *
 * The two pass filters ship disabled: unlike a shelf or a peak, a pass filter
 * has no flat setting — it always cuts — so an enabled one would quietly
 * change the sound of every existing mix.
 */
const DEFAULT_BAND_LAYOUT: readonly { kind: BandKind; freq: number }[] = [
  { kind: "highPass", freq: 30 },
  { kind: "lowShelf", freq: 80 },
  { kind: "peak", freq: 200 },
  { kind: "peak", freq: 500 },
  { kind: "peak", freq: 1200 },
  { kind: "peak", freq: 3500 },
  { kind: "highShelf", freq: 10000 },
  { kind: "lowPass", freq: 18000 },
];

/** Band kinds whose gain does anything; a pass filter's is ignored. */
export const GAIN_BEARING_KINDS: readonly BandKind[] = ["lowShelf", "peak", "highShelf"];

export function hasGain(kind: BandKind): boolean {
  return GAIN_BEARING_KINDS.includes(kind);
}

export function defaultBands(): EqBand[] {
  return DEFAULT_BAND_LAYOUT.map(({ kind, freq }) => ({
    kind,
    freq,
    gainDb: 0,
    q: 0.71,
    enabled: hasGain(kind),
  }));
}

export const DEFAULTS = {
  pitch: (): Pitch => ({ semitones: 0, cents: 0 }),
  panning: (): Panning => ({ mode: "stereoBalance", position: 0, width: 1 }),
  eq: (): Eq => ({ enabled: true, preampDb: 0, bands: defaultBands() }),
  reverb: (): Reverb => ({
    enabled: false,
    size: 0.5,
    damping: 0.5,
    width: 1,
    mix: 0.25,
    predelayMs: 0,
  }),
  delay: (): Delay => ({
    enabled: false,
    timeMs: 350,
    feedback: 0.35,
    mix: 0.25,
    toneHz: 6000,
    spread: 0,
  }),
  normalisation: (): Normalisation => ({
    enabled: false,
    targetDb: -14,
    gainDb: 0,
    limiterEnabled: true,
    limiterCeilingDb: -0.3,
    limiterReleaseMs: 120,
  }),
  lofi: (): Lofi => ({ enabled: false, sampleRateHz: 44100, bitDepth: 16, mix: 1 }),
  filters: (): FilterSetting[] => [],
  crossfade: () => defaultCrossfade(),
};

export type Section = keyof typeof DEFAULTS;

export const SECTIONS: Section[] = [
  "pitch",
  "panning",
  "eq",
  "reverb",
  "delay",
  "normalisation",
  "lofi",
  "crossfade",
  "filters",
];

/** Layer settings, later entries winning section by section. */
export function overlay(layers: (MixerSettings | null | undefined)[]): MixerSettings {
  const out: MixerSettings = {};
  for (const layer of layers) {
    if (!layer) continue;
    for (const [key, value] of Object.entries(layer)) {
      if (value !== null && value !== undefined) out[key] = value;
    }
  }
  return out;
}

/** Collapse a cascade into fully-populated values. */
export function resolve(layers: (MixerSettings | null | undefined)[]): ResolvedMixer {
  const merged = overlay(layers);
  // A crossfade belongs to the playlist transition, so entry-level layers do
  // not participate even if older data happens to contain that field.
  const crossfade = overlay(layers.slice(0, 2)).crossfade as CrossfadeSettings | undefined;
  return {
    enabled: merged.enabled ?? true,
    pitch: (merged.pitch as Pitch) ?? DEFAULTS.pitch(),
    panning: (merged.panning as Panning) ?? DEFAULTS.panning(),
    eq: (merged.eq as Eq) ?? DEFAULTS.eq(),
    reverb: (merged.reverb as Reverb) ?? DEFAULTS.reverb(),
    delay: (merged.delay as Delay) ?? DEFAULTS.delay(),
    normalisation: (merged.normalisation as Normalisation) ?? DEFAULTS.normalisation(),
    lofi: (merged.lofi as Lofi) ?? DEFAULTS.lofi(),
    crossfade: crossfade ?? DEFAULTS.crossfade(),
    filters: (merged.filters as FilterSetting[]) ?? DEFAULTS.filters(),
  };
}

/**
 * How much of an effect is actually being heard.
 *
 * A disabled effect still remembers its wet mix so that switching it back on
 * restores the old setting, but a control that reads "25%" while the effect is
 * bypassed is simply lying. Readouts use this; stored values keep the mix.
 */
export function audibleMix(section: { enabled: boolean; mix: number }): number {
  return section.enabled ? section.mix : 0;
}

/** Playback rate for a pitch setting; pitch and tempo move together. */
export function pitchRatio(pitch: Pitch): number {
  const semis = pitch.semitones + pitch.cents / 100;
  return Math.min(4, Math.max(0.25, Math.pow(2, semis / 12)));
}

/** Percentage change in tempo, which is what the varispeed readout shows. */
export function tempoPercent(pitch: Pitch): number {
  return (pitchRatio(pitch) - 1) * 100;
}

export function isSectionOverridden(layer: MixerSettings | null, section: Section): boolean {
  return !!layer && layer[section] !== null && layer[section] !== undefined;
}

/** Sections a preset touches, so the UI can say what it will change. */
export function presetSections(settings: MixerSettings): Section[] {
  return SECTIONS.filter((s) => settings[s] !== null && settings[s] !== undefined);
}

export function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

/** Map a knob's 0..1 travel onto a frequency range that feels linear by ear. */
export function toLogScale(value01: number, min: number, max: number): number {
  return min * Math.pow(max / min, Math.min(1, Math.max(0, value01)));
}

export function fromLogScale(value: number, min: number, max: number): number {
  return Math.log(Math.min(max, Math.max(min, value)) / min) / Math.log(max / min);
}
