import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import AdvancedMixer from "../AdvancedMixer.vue";
import AppKnob from "@/components/ui/AppKnob.vue";
import SelectMenu from "@/components/ui/SelectMenu.vue";
import { useMixerStore } from "@/stores/mixer";
import { useSettingsStore } from "@/stores/settings";
import { SECTION_LABELS } from "@/lib/mixer";

const setGlobalMixer = vi.fn();

vi.mock("@/lib/api", () => ({
  setGlobalMixer: (...args: unknown[]) => setGlobalMixer(...args),
}));

describe("AdvancedMixer panning", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    setGlobalMixer.mockReset().mockResolvedValue(undefined);
  });

  it("starts as identity balance and exposes true-stereo centre and width controls", async () => {
    // The select menu teleports to <body>; render it inline so the queries
    // below reach it.
    const wrapper = mount(AdvancedMixer, { global: { stubs: { teleport: true } } });
    const mixer = useMixerStore();
    const section = wrapper.get("[data-testid='panning-section']");

    const mode = section.getComponent(SelectMenu);
    expect(mode.props("modelValue")).toBe("stereoBalance");
    const balanceKnobs = section.findAllComponents(AppKnob);
    expect(balanceKnobs).toHaveLength(1);
    expect(balanceKnobs[0].props("label")).toBe("Balance");

    // Through the menu rather than the emitted event, so the wiring from the
    // trigger to the store is what is covered.
    await mode.get("[aria-label='Mode']").trigger("click");
    const trueStereo = mode
      .findAll("[role='menuitem']")
      .find((item) => item.text() === "True Stereo");
    await trueStereo?.trigger("click");
    await flushPromises();

    expect(mixer.targetLayer.panning).toEqual({
      mode: "trueStereo",
      position: 0,
      width: 1,
    });
    const trueStereoKnobs = section.findAllComponents(AppKnob);
    expect(trueStereoKnobs).toHaveLength(2);
    expect(trueStereoKnobs[0].props("label")).toBe("Centre");
    expect(trueStereoKnobs[1].props("label")).toBe("Width");
    expect(setGlobalMixer).toHaveBeenLastCalledWith({ panning: mixer.targetLayer.panning });
  });

  it("remains available when editing a timeline block", () => {
    const mixer = useMixerStore();
    mixer.target = { kind: "block", playlistId: "playlist", blockId: "block", name: "Clip" };

    const wrapper = mount(AdvancedMixer);

    expect(wrapper.find("[data-testid='panning-section']").exists()).toBe(true);
  });

  /**
   * Timeline voices carry their own varispeed and their own ambience beds, so
   * both are offered on a block. Crossfade still is not: it describes a join
   * between playlist entries, which a region on a timeline does not have.
   */
  it("offers pitch and atmospheres on a timeline block, but not crossfade", () => {
    const mixer = useMixerStore();
    mixer.target = { kind: "block", playlistId: "playlist", blockId: "block", name: "Clip" };

    const wrapper = mount(AdvancedMixer);

    expect(wrapper.text()).toContain("Semitones");
    expect(wrapper.text()).toContain("Atmospheres");
    expect(wrapper.text()).not.toContain("Crossfade");
  });
});

/**
 * Which sections the sidebar draws.
 *
 * Visibility only, and only in the sidebar: the settings themselves are
 * untouched, and the panels that edit a preset or a timeline block always show
 * everything so a hidden setting can still be reached.
 */
describe("AdvancedMixer sidebar sections", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    setGlobalMixer.mockReset().mockResolvedValue(undefined);
  });

  it("hides the sections the preference names, and only in the sidebar", () => {
    useSettingsStore().preferences.hiddenMixerSections = ["reverb", "lofi"];

    const sidebar = mount(AdvancedMixer, { props: { customisable: true } });
    expect(sidebar.text()).not.toContain("Reverb");
    expect(sidebar.text()).not.toContain("Sample Rate");
    expect(sidebar.text()).toContain("Delay");

    // The master mixer's block panel mounts the same component without asking
    // for the preference, and still has everything.
    const blockPanel = mount(AdvancedMixer);
    expect(blockPanel.text()).toContain("Reverb");
    expect(blockPanel.text()).toContain("Sample Rate");
  });

  it("shows every section when nothing is hidden", () => {
    useSettingsStore().preferences.hiddenMixerSections = [];
    const wrapper = mount(AdvancedMixer, { props: { customisable: true } });
    for (const label of Object.values(SECTION_LABELS)) {
      expect(wrapper.text()).toContain(label);
    }
  });
});
