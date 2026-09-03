import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useMixerStore } from "../mixer";
import type { MixerSettings } from "@/lib/types";

const mixerState = vi.fn();

vi.mock("@/lib/api", () => ({
  mixerState: (...args: unknown[]) => mixerState(...args),
  filtersDirectory: vi.fn().mockResolvedValue("/tmp/filters"),
  setGlobalMixer: vi.fn(),
  setPlaylistMixer: vi.fn(),
  setPlaylistEntryMixer: vi.fn(),
  mixerLayers: vi.fn(),
  savePreset: vi.fn(),
  deletePreset: vi.fn(),
}));

/** A global layer with something audible in it, to be ignored or not. */
function global(): MixerSettings {
  return {
    reverb: { enabled: true, size: 0.5, damping: 0.5, width: 1, mix: 0.4, predelayMs: 0 },
  };
}

describe("what a master mix inherits", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    mixerState.mockReset().mockResolvedValue({
      global: global(),
      presets: [],
      filters: [],
    });
  });

  // A mix has to sound the same whatever the DJ mixer downstairs is set to,
  // and the panel has to say so — see `build_plan` in `commands.rs`.
  it("a block on the timeline does not inherit the global mixer", async () => {
    const mixer = useMixerStore();
    await mixer.editMixBlock("pl_1", "blk_1", "First Song", null, null);

    expect(mixer.effective.reverb.enabled).toBe(false);
  });

  it("a playlist that plays as a mix does not show global settings underneath", async () => {
    const mixer = useMixerStore();
    await mixer.editPlaylist("pl_1", "Evening", null, true);
    expect(mixer.effective.reverb.enabled).toBe(false);

    // An ordinary playlist still layers over global, as it always has.
    await mixer.editPlaylist("pl_1", "Evening", null);
    expect(mixer.effective.reverb.enabled).toBe(true);
  });
});
