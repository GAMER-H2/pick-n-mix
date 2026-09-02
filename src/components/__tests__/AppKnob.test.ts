import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import AppKnob from "../AppKnob.vue";

describe("AppKnob", () => {
  it("points at the same angle as the value arc", async () => {
    const wrapper = mount(AppKnob, { props: { modelValue: 0, label: "Mix" } });

    expect(wrapper.get("circle").attributes("transform")).toBe("rotate(135 23 23)");
    expect(wrapper.get("line").attributes("transform")).toBe("rotate(225 23 23)");

    await wrapper.setProps({ modelValue: 0.5 });
    expect(wrapper.get("line").attributes("transform")).toBe("rotate(360 23 23)");
  });
});
