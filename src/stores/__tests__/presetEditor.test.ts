import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useMixerStore } from "@/stores/mixer";
import { usePresetEditorStore } from "@/stores/presetEditor";

const savePreset = vi.fn();
const updatePreset = vi.fn();

vi.mock("@/lib/api", () => ({
  savePreset: (...args: unknown[]) => savePreset(...args),
  updatePreset: (...args: unknown[]) => updatePreset(...args),
}));

describe("preset editor", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.resetAllMocks();
  });

  it("keeps edits isolated until a custom preset is explicitly saved", async () => {
    const editor = usePresetEditorStore();
    const mixer = useMixerStore();
    const preset = {
      id: "custom-1",
      name: "My Preset",
      builtIn: false,
      settings: { enabled: true, preset: "display-only" },
    };
    editor.open(preset);
    editor.setEnabled(false);

    expect(updatePreset).not.toHaveBeenCalled();
    expect(mixer.targetLayer).toEqual({});

    updatePreset.mockResolvedValue([{ ...preset, settings: { enabled: false } }]);
    await editor.save();

    expect(updatePreset).toHaveBeenCalledWith("custom-1", "My Preset", { enabled: false });
    expect(editor.dirty).toBe(false);
  });

  it("saves an edited built-in as a custom copy", async () => {
    const editor = usePresetEditorStore();
    editor.open({ id: "flat", name: "Flat", builtIn: true, settings: { enabled: true } });
    editor.setSection("pitch", { semitones: 2, cents: 0 });
    savePreset.mockResolvedValue([{
      id: "custom-flat",
      name: "Flat Custom",
      builtIn: false,
      settings: { enabled: true, pitch: { semitones: 2, cents: 0 } },
    }]);

    await editor.save();

    expect(savePreset).toHaveBeenCalledWith("Flat Custom", {
      enabled: true,
      pitch: { semitones: 2, cents: 0 },
    });
    expect(updatePreset).not.toHaveBeenCalled();
    expect(editor.session?.sourceBuiltIn).toBe(false);
  });
});
