import { defaultBands } from "./mixer";
import type { Eq, EqBand } from "./types";

export interface EqPreset {
  id: string;
  name: string;
  eq: Eq;
}

function shapedEq(
  gains: Partial<Record<number, number>>,
  preampDb = 0,
): Eq {
  const bands = defaultBands().map((band, index) => ({
    ...band,
    gainDb: gains[index] ?? band.gainDb,
  }));
  return { enabled: true, preampDb, bands };
}

function phoneEq(): Eq {
  const bands: EqBand[] = [
    { kind: "highPass", freq: 300, gainDb: 0, q: 0.8, enabled: true },
    { kind: "peak", freq: 1700, gainDb: 4, q: 1.1, enabled: true },
    { kind: "lowPass", freq: 4500, gainDb: 0, q: 0.8, enabled: true },
  ];
  return { enabled: true, preampDb: -1, bands };
}

/** Built-in shapes for the expanded EQ. These affect only the EQ section. */
export const EQ_PRESETS: readonly EqPreset[] = [
  { id: "eq-flat", name: "Flat", eq: shapedEq({}) },
  { id: "eq-bass-boost", name: "Bass Boost", eq: shapedEq({ 1: 5, 2: 2 }, -3) },
  { id: "eq-bass-cut", name: "Bass Cut", eq: shapedEq({ 1: -5, 2: -2 }) },
  { id: "eq-treble-boost", name: "Treble Boost", eq: shapedEq({ 5: 2, 6: 5 }, -3) },
  { id: "eq-treble-cut", name: "Treble Cut", eq: shapedEq({ 5: -2, 6: -5 }) },
  {
    id: "eq-vocal-presence",
    name: "Vocal Presence",
    eq: shapedEq({ 1: -2.5, 2: -1.5, 4: 2, 5: 3, 6: 1 }, -1),
  },
  { id: "eq-loudness", name: "Loudness", eq: shapedEq({ 1: 4, 3: -1.5, 4: -1.5, 6: 3 }, -2) },
  { id: "eq-telephone", name: "Telephone", eq: phoneEq() },
];

export function eqValuesEqual(left: Eq, right: Eq): boolean {
  return left.enabled === right.enabled
    && left.preampDb === right.preampDb
    && left.bands.length === right.bands.length
    && left.bands.every((band, index) => {
      const other = right.bands[index];
      return band.kind === other.kind
        && band.freq === other.freq
        && band.gainDb === other.gainDb
        && band.q === other.q
        && band.enabled === other.enabled;
    });
}

export function matchingEqPresetId(eq: Eq): string {
  return EQ_PRESETS.find((preset) => eqValuesEqual(preset.eq, eq))?.id ?? "";
}

export function eqPresetById(id: string): Eq | null {
  const preset = EQ_PRESETS.find((item) => item.id === id);
  if (!preset) return null;
  return {
    ...preset.eq,
    bands: preset.eq.bands.map((band) => ({ ...band })),
  };
}
