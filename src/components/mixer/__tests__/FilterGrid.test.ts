import { beforeEach, describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import FilterGrid from "../FilterGrid.vue";
import AppKnob from "@/components/AppKnob.vue";
import { useMixerStore } from "@/stores/mixer";

const activeSettings = [
  { id: "rain", enabled: true, volume: 0.35, toneHz: 20_000 },
  { id: "custom-wind", enabled: true, volume: 0.6, toneHz: 20_000 },
];

describe("FilterGrid", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    const mixer = useMixerStore();
    mixer.filters = [
      { id: "rain", name: "Rain", builtIn: true, available: true, path: "/app/rain.mp3" },
      { id: "custom-wind", name: "Custom Wind", builtIn: false, available: true, path: "/data/wind.mp3" },
    ];
  });

  it("shows compact knobs below active atmospheres and marks only built-ins for visual treatment", async () => {
    const wrapper = mount(FilterGrid, { props: { settings: activeSettings } });

    expect(wrapper.get(".chip.is-built-in.is-on").text()).toBe("Rain");
    expect(wrapper.findAll(".chip.is-on")).toHaveLength(2);
    expect(wrapper.find("[aria-label='Rain volume']").exists()).toBe(true);
    expect(wrapper.find("[aria-label='Custom Wind volume']").exists()).toBe(true);

    const knobs = wrapper.findAllComponents(AppKnob);
    await knobs[0].vm.$emit("update:modelValue", 0.75);
    expect(wrapper.emitted("volume")).toEqual([["rain", 0.75]]);
  });
});
