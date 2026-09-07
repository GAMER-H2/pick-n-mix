import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import MixerPopover from "../MixerPopover.vue";
import AppSlider from "@/components/ui/AppSlider.vue";
import { useMixerStore } from "@/stores/mixer";
import { useSettingsStore } from "@/stores/settings";
import { REVERB_MACRO_MAX_MIX } from "@/lib/mixer";

const setGlobalMixer = vi.fn();

vi.mock("@/lib/api", () => ({
  setGlobalMixer: (...args: unknown[]) => setGlobalMixer(...args),
}));

function mountPopover() {
  return mount(MixerPopover, { global: { stubs: { teleport: true } } });
}

/** The row labels, top to bottom, as the popover currently draws them. */
function rowLabels(wrapper: ReturnType<typeof mountPopover>): string[] {
  return wrapper.findAll(".popover__label").map((label) => label.text());
}

describe("MixerPopover sections", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    setGlobalMixer.mockReset().mockResolvedValue(undefined);
  });

  it("draws its default rows in the drawing's order", () => {
    const wrapper = mountPopover();
    expect(rowLabels(wrapper)).toEqual([
      "Reverb",
      "Pitch",
      "EQ",
      "Normalisation",
      "Crossfade",
      "Atmospheres",
    ]);
  });

  it("hides the rows the preference names and keeps the rest in order", () => {
    useSettingsStore().preferences.hiddenPopoverSections = ["pitch", "crossfade"];
    expect(rowLabels(mountPopover())).toEqual(["Reverb", "EQ", "Normalisation", "Atmospheres"]);
  });

  it("lists them in the user's own order, with anything unnamed following", () => {
    useSettingsStore().preferences.popoverSectionOrder = ["filters", "eq"];
    expect(rowLabels(mountPopover())).toEqual([
      "Atmospheres",
      "EQ",
      "Reverb",
      "Pitch",
      "Normalisation",
      "Crossfade",
    ]);
  });

  it("says so rather than drawing an empty bubble when everything is hidden", () => {
    const settings = useSettingsStore();
    settings.preferences.hiddenPopoverSections = [
      "reverb",
      "pitch",
      "eq",
      "normalisation",
      "crossfade",
      "filters",
    ];
    const wrapper = mountPopover();
    expect(rowLabels(wrapper)).toEqual([]);
    expect(wrapper.find(".popover__empty").exists()).toBe(true);
    // The preset picker and the way out to the full panel are not part of the
    // arrangeable list: without them the bubble would be a dead end.
    expect(wrapper.find(".popover__advanced").exists()).toBe(true);
  });
});

/**
 * The one reverb slider writes a whole reverb, which is what the panel's knobs
 * then show — not just the wet/dry balance it used to move on its own.
 */
describe("MixerPopover reverb", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    setGlobalMixer.mockReset().mockResolvedValue(undefined);
  });

  it("opens the whole effect out as it rises", async () => {
    const wrapper = mountPopover();
    const mixer = useMixerStore();
    const slider = wrapper.findAllComponents(AppSlider)[0];

    slider.vm.$emit("update:modelValue", 0.8);
    await flushPromises();

    const reverb = mixer.effective.reverb;
    expect(reverb.enabled).toBe(true);
    expect(reverb.mix).toBeCloseTo(0.8 * REVERB_MACRO_MAX_MIX, 6);
    expect(reverb.size).toBeGreaterThan(0.5);
    expect(reverb.width).toBeGreaterThan(0.55);
    expect(reverb.predelayMs).toBeGreaterThan(0);

    // ...and the slider reads back where it was put, with the resulting room
    // named underneath it.
    expect(wrapper.get(".popover__value").text()).toBe("80%");
    expect(wrapper.get(".popover__note").text()).toContain("width");
  });

  it("bypasses the reverb at zero rather than leaving a silent tail running", async () => {
    const wrapper = mountPopover();
    const mixer = useMixerStore();
    const slider = wrapper.findAllComponents(AppSlider)[0];

    slider.vm.$emit("update:modelValue", 0.5);
    await flushPromises();
    slider.vm.$emit("update:modelValue", 0);
    await flushPromises();

    expect(mixer.effective.reverb.enabled).toBe(false);
    expect(wrapper.find(".popover__note").exists()).toBe(false);
  });
});
