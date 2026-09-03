import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { usePlayerStore } from "../player";

const setVolume = vi.fn().mockResolvedValue(undefined);
const playbackState = vi.fn();
const currentTrack = vi.fn();
const queueState = vi.fn();
const masterMixNowPlaying = vi.fn();

vi.mock("@/lib/api", () => ({
  setVolume: (...args: unknown[]) => setVolume(...args),
  playbackState: (...args: unknown[]) => playbackState(...args),
  currentTrack: (...args: unknown[]) => currentTrack(...args),
  queueState: (...args: unknown[]) => queueState(...args),
  masterMixNowPlaying: (...args: unknown[]) => masterMixNowPlaying(...args),
  togglePlay: vi.fn(),
  nextTrack: vi.fn(),
  previousTrack: vi.fn(),
  seek: vi.fn(),
  setShuffle: vi.fn(),
  setRepeat: vi.fn(),
  playTracks: vi.fn(),
}));

describe("mute", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    setVolume.mockClear();
  });

  it("restores the previous level when unmuted", async () => {
    const player = usePlayerStore();
    await player.setVolume(0.7);

    await player.toggleMute();
    expect(player.snapshot.volume).toBe(0);
    expect(player.muted).toBe(true);

    await player.toggleMute();
    expect(player.snapshot.volume).toBeCloseTo(0.7);
    expect(player.muted).toBe(false);
  });

  it("goes to full when unmuting something that was already silent", async () => {
    const player = usePlayerStore();
    await player.setVolume(0);

    // Muting silence is a no-op, so unmuting must not restore silence or the
    // button would look broken.
    await player.toggleMute();
    expect(player.snapshot.volume).toBe(1);
  });

  it("dragging the slider cancels the remembered level", async () => {
    const player = usePlayerStore();
    await player.setVolume(0.8);
    await player.toggleMute();

    await player.setVolume(0.3);
    expect(player.mutedFrom).toBeNull();

    await player.toggleMute();
    await player.toggleMute();
    expect(player.snapshot.volume).toBeCloseTo(0.3, 5);
  });

  it("tells the backend about every change", async () => {
    const player = usePlayerStore();
    await player.setVolume(0.5);
    await player.toggleMute();
    await player.toggleMute();
    expect(setVolume.mock.calls.map((c) => c[0])).toEqual([0.5, 0, 0.5]);
  });
});

describe("what is playing", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    playbackState.mockResolvedValue({
      playing: true,
      positionSecs: 12,
      durationSecs: 600,
      volume: 1,
      speed: 1,
      limiterReductionDb: 0,
      deviceName: "",
      deviceSampleRate: 48000,
      stream: null,
    });
    currentTrack.mockResolvedValue(null);
    queueState.mockResolvedValue({
      items: [],
      currentIndex: null,
      upcoming: [],
      shuffle: false,
      repeat: "off",
      context: null,
    });
    masterMixNowPlaying.mockResolvedValue(null);
  });

  /** A mix has no current track, so without this the bar has nothing to show. */
  it("picks up the playlist when a mix is what is playing", async () => {
    masterMixNowPlaying.mockResolvedValue({
      playlistId: "pl_1",
      name: "Evening",
      description: "",
      artwork: "art_1.jpg",
      artworkIds: [],
      trackCount: 3,
      durationSecs: 600,
      laneCount: 3,
      blockCount: 3,
      chapters: [{ startSecs: 0, title: "One", artist: "A" }],
    });

    const player = usePlayerStore();
    await player.refresh();

    expect(player.track).toBeNull();
    expect(player.masterMix?.name).toBe("Evening");
    // The transport is live even though no song is loaded.
    expect(player.hasPlayback).toBe(true);
    expect(player.duration).toBe(600);
  });

  it("has nothing to show when neither a track nor a mix is loaded", async () => {
    const player = usePlayerStore();
    await player.refresh();
    expect(player.hasPlayback).toBe(false);
  });
});
