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
  ChainStage,
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

/**
 * The stages of the effect chain, in the order the engine applies them unless
 * told otherwise. Mirrors `DEFAULT_CHAIN_ORDER` in `audio/params.rs`.
 *
 * This is the chain as it was before the order could be moved, so every mix,
 * preset and playlist saved until now goes on sounding as it did.
 */
export const DEFAULT_CHAIN_ORDER: ChainStage[] = ["eq", "delay", "reverb", "lofi", "panning"];

/**
 * The sections that are *not* stages of the chain, in the order the panel
 * shows them below it.
 *
 * Pitch is varispeed applied as the file is decoded, normalisation is a gain
 * ride after the chain (the limiter itself is on the master bus), a crossfade
 * belongs to the join between two songs, and the ambience beds are laid over
 * the top. None of them has a place in the chain to be moved to, so none of
 * them is offered one.
 */
export const FIXED_SECTIONS: Section[] = ["pitch", "normalisation", "crossfade", "filters"];

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
  chainOrder: (): string[] => [...DEFAULT_CHAIN_ORDER],
  layoutOrder: (): string[] => [...FIXED_SECTIONS],
  crossfade: () => defaultCrossfade(),
};

export type Section = keyof typeof DEFAULTS;

/**
 * A saved chain order turned into a complete one: unknown ids are dropped and
 * stages the list does not mention keep their default place, so a layer
 * written before a stage existed still gets it. The same rule the sidebar's
 * section order follows, and the same one `ChainStage::order` applies in the
 * backend — the two must agree or the panel would draw an order the engine is
 * not playing.
 */
export function chainOrder(saved: readonly string[] | null | undefined): ChainStage[] {
  const out: ChainStage[] = [];
  for (const id of saved ?? []) {
    const stage = DEFAULT_CHAIN_ORDER.find((candidate) => candidate === id);
    if (stage && !out.includes(stage)) out.push(stage);
  }
  for (const stage of DEFAULT_CHAIN_ORDER) {
    if (!out.includes(stage)) out.push(stage);
  }
  return out;
}

/**
 * The same list with one entry moved, ready to be written back as a section.
 *
 * Shared by both orders the panel can be dragged into: the chain's, which the
 * engine plays, and the layout of the sections outside it, which is only ever
 * about where they are on the panel.
 */
export function withStageMoved<T extends string>(
  order: readonly T[],
  from: number,
  to: number,
): T[] {
  const next = [...order];
  if (from === to || from < 0 || from >= next.length) return next;
  const target = Math.min(Math.max(to, 0), next.length - 1);
  const [moved] = next.splice(from, 1);
  next.splice(target, 0, moved);
  return next;
}

/**
 * What the master mixer's rack can hold: the chain's own stages, and the three
 * settings that belong to a region without being part of its chain.
 *
 * A crossfade is missing on purpose — it describes the join between two
 * playlist entries, which a region on a timeline does not have, and the
 * backend ignores one set on a block layer anyway.
 */
export type DeviceSection = ChainStage | "pitch" | "normalisation" | "filters";

/** The devices that are not chain stages, and so cannot be moved along it. */
export const PINNED_DEVICES: DeviceSection[] = ["pitch", "normalisation", "filters"];

export const RACK_DEVICES: DeviceSection[] = [...DEFAULT_CHAIN_ORDER, ...PINNED_DEVICES];

/** Whether a device has a switch of its own, as opposed to only settings. */
export function hasEnableSwitch(section: DeviceSection): boolean {
  return ["eq", "delay", "reverb", "lofi", "normalisation"].includes(section);
}

/**
 * A device as it should arrive when it is added.
 *
 * Switched on, unlike the stored defaults: several effects default to bypassed
 * so that an untouched mix stays untouched, but a device the user has just
 * gone to a menu and asked for should do something.
 */
export function deviceDefault(section: DeviceSection): MixerSettings[DeviceSection] {
  const value = DEFAULTS[section]();
  if (value && typeof value === "object" && !Array.isArray(value) && "enabled" in value) {
    return { ...value, enabled: true };
  }
  return value;
}

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
  "chainOrder",
  "layoutOrder",
];

/**
 * What each section is called wherever it is named: the advanced panel's own
 * headings, and the list in Settings ▸ Mixer that decides which of them the
 * sidebar shows. One list, so a heading and its settings row can never drift
 * apart.
 */
export const SECTION_LABELS: Record<Section, string> = {
  eq: "EQ",
  pitch: "Pitch",
  reverb: "Reverb",
  delay: "Delay",
  normalisation: "Normalisation",
  panning: "Panning",
  crossfade: "Crossfade",
  filters: "Atmospheres",
  lofi: "Sample Rate",
  chainOrder: "Signal Chain",
  layoutOrder: "Outside the Chain",
};

/**
 * The sections the compact popover offers, in the order it offers them by
 * default. Shorter than the sidebar's list on purpose: the popover is the
 * quick view, and the panel behind it is where everything else lives.
 */
export const POPOVER_SECTIONS: Section[] = [
  "reverb",
  "pitch",
  "eq",
  "normalisation",
  "crossfade",
  "filters",
];

/**
 * A surface's sections, in the user's order and without the ones they have
 * hidden.
 *
 * Ids the preference does not mention keep their place in `defaults`, so a
 * section added by a later version appears rather than silently vanishing
 * from a list saved before it existed — the same rule the sidebar's playlist
 * order follows.
 */
export function orderedSections(
  defaults: Section[],
  order: readonly string[],
  hidden: readonly string[],
): Section[] {
  const known = new Set<string>(defaults);
  const placed = new Set<string>();
  const out: Section[] = [];
  for (const id of order) {
    if (known.has(id) && !placed.has(id)) {
      placed.add(id);
      out.push(id as Section);
    }
  }
  for (const id of defaults) {
    if (!placed.has(id)) out.push(id);
  }
  return out.filter((id) => !hidden.includes(id));
}

/**
 * The sections the sidebar draws, in the order it draws them: the chain's own
 * stages in their default order, then the sections that are not part of it.
 *
 * The panel itself draws the stages in whatever order the layer being edited
 * resolves to — this is the list of what it can draw, and the fallback order
 * for a layer that has never been reordered.
 */
export const SIDEBAR_SECTIONS: Section[] = [...DEFAULT_CHAIN_ORDER, ...FIXED_SECTIONS];

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
    chainOrder: chainOrder(merged.chainOrder as string[] | undefined),
    layoutOrder: orderedSections(FIXED_SECTIONS, (merged.layoutOrder as string[]) ?? [], []),
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

/*
 * The popover's one reverb control.
 *
 * The compact view has room for a single slider, and a slider that only moved
 * the wet/dry balance was a poor use of it: a reverb that gets louder without
 * getting any bigger just sounds like the same small room turned up. This
 * sweeps the whole effect instead — a short, dark, narrow room at the bottom,
 * a long, bright, wide hall at the top — so the tail genuinely opens out as
 * the slider rises. The panel still has every parameter on its own knob, and
 * these are the values it shows.
 */

/**
 * The wettest the macro goes. Past this the dry signal sits so far under the
 * tail that the song sounds like it is playing in the next room.
 */
export const REVERB_MACRO_MAX_MIX = 0.6;

function clamp01(value: number): number {
  return Math.min(1, Math.max(0, Number.isFinite(value) ? value : 0));
}

/** The whole reverb, swept from one 0..1 control. */
export function reverbForAmount(amount: number): Reverb {
  const a = clamp01(amount);
  return {
    enabled: a > 0.001,
    // Freeverb's room size: a small booth up to a hall with a long tail.
    size: 0.3 + a * 0.65,
    // Damping *falls* as the room grows: a big space that swallowed its own
    // high end as fast as a small one would sound like a blanket, not a hall.
    damping: 0.8 - a * 0.55,
    // The tail is collapsed toward mono at the bottom and fully spread at the
    // top, which is what makes the reverb read as wider the further it goes.
    width: 0.55 + a * 0.45,
    mix: a * REVERB_MACRO_MAX_MIX,
    // A little pre-delay keeps the source in front of the space it is in.
    predelayMs: Math.round(a * 30),
  };
}

/**
 * Where the slider sits for a given reverb, read from the one value that is
 * monotonic in the macro. Hand-set values from the panel therefore still put
 * the slider somewhere sensible rather than resetting it.
 */
export function reverbAmount(reverb: Reverb): number {
  if (!reverb.enabled) return 0;
  return clamp01(reverb.mix / REVERB_MACRO_MAX_MIX);
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
