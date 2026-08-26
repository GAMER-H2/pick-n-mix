import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { usePlayerStore } from "../player";

const setVolume = vi.fn().mockResolvedValue(undefined);

vi.mock("@/lib/api", () => ({
  setVolume: (...args: unknown[]) => setVolume(...args),
  playbackState: vi.fn(),
  currentTrack: vi.fn(),
  queueState: vi.fn(),
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
