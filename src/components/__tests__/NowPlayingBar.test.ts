import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import NowPlayingBar from "../layout/NowPlayingBar.vue";
import { useMixerStore } from "@/stores/mixer";
import { usePlayerStore } from "@/stores/player";
import type { MasterMixNowPlaying } from "@/lib/types";

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

function playingMix(): MasterMixNowPlaying {
  return {
    playlistId: "pl_1",
    name: "Evening",
    description: "",
    artwork: "art_1.jpg",
    artworkIds: [],
    trackCount: 3,
    durationSecs: 600,
    laneCount: 3,
    blockCount: 3,
    chapters: [
      // At the very start, so it sits under the parked handle and is dropped.
      { startSecs: 0, title: "One", artist: "A" },
      { startSecs: 200, title: "Two", artist: "B" },
      // Two seconds after the last: a crossfaded join, one smudge on screen.
      { startSecs: 202, title: "Three", artist: "C" },
      { startSecs: 400, title: "Four", artist: "D" },
    ],
  };
}

describe("NowPlayingBar while a master mix plays", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    mixerState.mockReset().mockResolvedValue({ global: {}, presets: [], filters: [] });
  });

  function mountBar() {
    return mount(NowPlayingBar, {
      global: {
        stubs: {
          Artwork: true,
          MixerPopover: true,
          InfoPopover: true,
          PnmIcon: true,
          Teleport: true,
        },
      },
    });
  }

  it("shows the playlist in place of a song, with a mix badge", async () => {
    const player = usePlayerStore();
    player.masterMix = playingMix();
    player.snapshot = { ...player.snapshot, durationSecs: 600 };

    const wrapper = mountBar();
    await flushPromises();

    expect(wrapper.text()).toContain("Evening");
    expect(wrapper.text()).toContain("Master mix");
    expect(wrapper.text()).toContain("3 songs");
    expect(wrapper.text()).not.toContain("Nothing Playing");
    expect(wrapper.find(".bar__badge").exists()).toBe(true);
    // Nothing is "loaded" in the queue sense, but the transport still works.
    expect(wrapper.get(".bar__play").attributes("disabled")).toBeUndefined();
  });

  it("marks each song on the scrubber, skipping the ones too close to read", async () => {
    const player = usePlayerStore();
    player.masterMix = playingMix();
    player.snapshot = { ...player.snapshot, durationSecs: 600 };

    const wrapper = mountBar();
    await flushPromises();

    // 0 is at the very start, 202 is within a fiftieth of the timeline of 200.
    expect(wrapper.findAll(".slider__marker")).toHaveLength(2);
  });

  it("has no marks when an ordinary track is playing", async () => {
    const wrapper = mountBar();
    await flushPromises();
    expect(wrapper.findAll(".slider__marker")).toHaveLength(0);
    expect(wrapper.text()).toContain("Nothing Playing");
  });
});
