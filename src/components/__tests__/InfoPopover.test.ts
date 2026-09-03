import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import InfoPopover from "../InfoPopover.vue";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";
import type { MasterMixNowPlaying } from "@/lib/types";

vi.mock("@/lib/api", () => ({}));

function playingMix(): MasterMixNowPlaying {
  return {
    playlistId: "pl_1",
    name: "Evening",
    description: "For the motorway at 2am",
    artwork: null,
    artworkIds: [],
    trackCount: 3,
    durationSecs: 600,
    laneCount: 3,
    blockCount: 5,
    chapters: [],
  };
}

function mountPopover() {
  return mount(InfoPopover, {
    global: { stubs: { Artwork: true, PlaylistArtwork: true, PnmIcon: true } },
  });
}

describe("InfoPopover for a playing mix", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("describes the playlist, the stream built from it, and the output", () => {
    const player = usePlayerStore();
    const ui = useUiStore();
    player.masterMix = playingMix();
    player.snapshot = {
      ...player.snapshot,
      durationSecs: 600,
      deviceName: "Speakers",
      deviceSampleRate: 48000,
      stream: {
        sampleRate: 48000,
        channels: 2,
        durationSecs: 600,
        codec: "master mix",
        bitsPerSample: 32,
        bitrateKbps: null,
      },
    };
    ui.infoMixOpen = true;

    const text = mountPopover().text();
    // The playlist itself.
    expect(text).toContain("Evening");
    expect(text).toContain("For the motorway at 2am");
    expect(text).toContain("5 regions on 3 tracks");
    // The combined audio the engine is playing.
    expect(text).toContain("Mixed Audio");
    expect(text).toContain("master mix");
    expect(text).toContain("48 kHz");
    // And the ordinary playback details.
    expect(text).toContain("Speakers");
  });

  it("shows nothing once the mix has stopped playing", () => {
    const ui = useUiStore();
    ui.infoMixOpen = true;
    expect(mountPopover().find(".info").exists()).toBe(false);
  });
});
