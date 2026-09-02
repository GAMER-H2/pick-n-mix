import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import NowPlayingBar from "../NowPlayingBar.vue";
import { useMixerStore } from "@/stores/mixer";

const mixerState = vi.fn();

vi.mock("vue-router", () => ({
  useRoute: () => ({ name: "home" }),
  useRouter: () => ({ push: vi.fn(), back: vi.fn() }),
}));

vi.mock("@/lib/api", () => ({
  mixerState: (...args: unknown[]) => mixerState(...args),
}));

describe("NowPlayingBar mixer button", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    mixerState.mockReset().mockResolvedValue({ global: {}, presets: [], filters: [] });
  });

  it("toggles the advanced mixer on repeated Shift-clicks", async () => {
    const wrapper = mount(NowPlayingBar, {
      global: {
        stubs: {
          Artwork: true,
          AppSlider: true,
          MixerPopover: true,
          InfoPopover: true,
          PnmIcon: true,
          Teleport: true,
        },
      },
    });
    const mixer = useMixerStore();
    const button = wrapper.get("[aria-label='DJ Mixer']");

    await button.trigger("click", { shiftKey: true });
    await flushPromises();
    expect(mixer.panelOpen).toBe(true);
    expect(mixer.popoverOpen).toBe(false);

    await button.trigger("click", { shiftKey: true });
    await flushPromises();
    expect(mixer.panelOpen).toBe(false);
  });
});
