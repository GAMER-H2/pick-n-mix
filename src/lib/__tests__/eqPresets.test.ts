import { describe, expect, it } from "vitest";
import { EQ_PRESETS, eqPresetById, matchingEqPresetId } from "../eqPresets";

describe("EQ presets", () => {
  it("provides useful built-in curves without exceeding engine limits", () => {
    expect(EQ_PRESETS.map((preset) => preset.name)).toEqual([
      "Flat",
      "Bass Boost",
      "Bass Cut",
      "Treble Boost",
      "Treble Cut",
      "Vocal Presence",
      "Loudness",
      "Telephone",
    ]);
    expect(EQ_PRESETS.every((preset) => preset.eq.bands.length <= 12)).toBe(true);
  });

  it("returns a fresh EQ value each time", () => {
    const first = eqPresetById("eq-bass-boost");
    const second = eqPresetById("eq-bass-boost");
    expect(first).not.toBeNull();
    expect(second).not.toBeNull();

    first!.bands[1].gainDb = 0;
    expect(second!.bands[1].gainDb).toBe(5);
    expect(eqPresetById("missing")).toBeNull();
  });

  it("recognises presets and labels edited curves as custom", () => {
    const preset = eqPresetById("eq-vocal-presence")!;
    expect(matchingEqPresetId(preset)).toBe("eq-vocal-presence");

    preset.bands[2].gainDb += 0.5;
    expect(matchingEqPresetId(preset)).toBe("");
  });
});
