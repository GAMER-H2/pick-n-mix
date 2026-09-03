import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import AppKnob from "../ui/AppKnob.vue";

describe("AppKnob", () => {
  it("points at the same angle as the value arc", async () => {
    const wrapper = mount(AppKnob, { props: { modelValue: 0, label: "Mix" } });

    expect(wrapper.get("circle").attributes("transform")).toBe("rotate(135 23 23)");
    expect(wrapper.get("line").attributes("transform")).toBe("rotate(225 23 23)");

    await wrapper.setProps({ modelValue: 0.5 });
    expect(wrapper.get("line").attributes("transform")).toBe("rotate(360 23 23)");
  });

  it("snaps to a detent unless Shift is held", async () => {
    const wrapper = mount(AppKnob, {
      props: { modelValue: 0.25, min: -1, max: 1, label: "Pan", detents: [0] },
    });
    const dial = wrapper.get("[role='slider']");

    await dial.trigger("wheel", { deltaY: 12, shiftKey: false });
    const updates = wrapper.emitted("update:modelValue") ?? [];
    expect(updates[updates.length - 1]).toEqual([0]);

    await dial.trigger("wheel", { deltaY: 12, shiftKey: true });
    const fineUpdates = wrapper.emitted("update:modelValue") ?? [];
    expect(fineUpdates[fineUpdates.length - 1]?.[0]).toBeCloseTo(0.202);
  });
});
