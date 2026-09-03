import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import AdvancedMixer from "../AdvancedMixer.vue";
import AppKnob from "@/components/ui/AppKnob.vue";
import { useMixerStore } from "@/stores/mixer";

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
    const wrapper = mount(AdvancedMixer);
    const mixer = useMixerStore();
    const section = wrapper.get("[data-testid='panning-section']");

    expect(wrapper.get<HTMLSelectElement>("[aria-label='Panning mode']").element.value).toBe(
      "stereoBalance",
    );
    const balanceKnobs = section.findAllComponents(AppKnob);
    expect(balanceKnobs).toHaveLength(1);
    expect(balanceKnobs[0].props("label")).toBe("Balance");

    await wrapper.get("[aria-label='Panning mode']").setValue("trueStereo");
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
