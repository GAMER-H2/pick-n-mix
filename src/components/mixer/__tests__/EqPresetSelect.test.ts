import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import EqPresetSelect from "../EqPresetSelect.vue";
import { defaultBands } from "@/lib/mixer";
import { useMixerStore } from "@/stores/mixer";
import type { Eq, Preset } from "@/lib/types";

const savePreset = vi.fn();
const deletePreset = vi.fn();

vi.mock("@/lib/api", () => ({
  savePreset: (...args: unknown[]) => savePreset(...args),
  deletePreset: (...args: unknown[]) => deletePreset(...args),
}));

function eq(gainDb = 0): Eq {
  const bands = defaultBands();
  bands[1] = { ...bands[1], gainDb };
  return { enabled: true, preampDb: 0, bands };
}

function customPreset(): Preset {
  return {
    id: "eq-custom",
    name: "My Curve",
    builtIn: false,
    kind: "eq",
    settings: { eq: eq(3) },
  };
}

describe("EqPresetSelect", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    savePreset.mockReset().mockResolvedValue([customPreset()]);
    deletePreset.mockReset().mockResolvedValue([]);
    useMixerStore().presets = [customPreset(), {
      id: "mixer-custom",
      name: "Whole Mixer",
      builtIn: false,
      kind: "mixer",
      settings: {},
    }];
  });

  it("groups custom EQ presets separately and applies one", async () => {
    const wrapper = mount(EqPresetSelect, { props: { eq: eq() } });
    await wrapper.get(".preset__button").trigger("click");

    expect(wrapper.text()).toContain("Built In");
    expect(wrapper.text()).toContain("Yours");
    expect(wrapper.text()).toContain("My Curve");
    expect(wrapper.text()).not.toContain("Whole Mixer");

    const custom = wrapper.findAll("[role='menuitem']").find((item) => item.text().includes("My Curve"));
    await custom!.trigger("click");
    expect((wrapper.emitted("change")![0][0] as Eq).bands[1].gainDb).toBe(3);
  });

  it("saves the current curve as an EQ preset and deletes custom curves", async () => {
    const current = eq(5);
    const wrapper = mount(EqPresetSelect, { props: { eq: current } });
    await wrapper.get(".preset__button").trigger("click");
    const saveCurrent = wrapper.findAll("[role='menuitem']").find((item) =>
      item.text().includes("Save current EQ"),
    );
    await saveCurrent!.trigger("click");
    await wrapper.get("[aria-label='EQ preset name']").setValue("Heavy Bass");
    await wrapper.find(".preset__save .pill-button").trigger("click");

    expect(savePreset).toHaveBeenCalledWith("Heavy Bass", { eq: current }, "eq");

    await wrapper.get("[title='Delete EQ preset']").trigger("click");
    expect(deletePreset).toHaveBeenCalledWith("eq-custom");
  });
});
