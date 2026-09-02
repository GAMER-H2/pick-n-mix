import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useHomeStore } from "../home";
import type { HomeShelves, MixSummary } from "@/lib/types";

const homeShelves = vi.fn();
const listPinnedMixes = vi.fn();
const setMixPinned = vi.fn();
const refreshMixes = vi.fn();

vi.mock("@/lib/api", () => ({
  homeShelves: (...args: unknown[]) => homeShelves(...args),
  listPinnedMixes: (...args: unknown[]) => listPinnedMixes(...args),
  setMixPinned: (...args: unknown[]) => setMixPinned(...args),
  refreshMixes: (...args: unknown[]) => refreshMixes(...args),
}));

function mix(overrides: Partial<MixSummary> = {}): MixSummary {
  return {
    kind: "replay",
    name: "Replay Mix",
    description: "Songs you keep coming back to lately",
    trackCount: 20,
    artworkIds: [],
    pinned: false,
    ...overrides,
  };
}

function shelves(overrides: Partial<HomeShelves> = {}): HomeShelves {
  return {
    mixes: [mix()],
    picks: [],
    recentPlaylists: [],
    playTotal: 40,
    ...overrides,
  };
}

describe("home store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    homeShelves.mockReset().mockResolvedValue(shelves());
    listPinnedMixes.mockReset().mockResolvedValue([]);
    setMixPinned.mockReset().mockResolvedValue([]);
    refreshMixes.mockReset().mockResolvedValue(undefined);
  });

  it("exposes the shelves once loaded", async () => {
    const home = useHomeStore();
    await home.refresh();

    expect(home.mixes).toHaveLength(1);
    expect(home.isEmpty).toBe(false);
    expect(home.loading).toBe(false);
  });

  /**
   * A shelf can be empty for two different reasons, and they need different
   * wording: nothing listened to yet, versus listened plenty but this
   * particular mix found nothing.
   */
  it("treats no counted plays as an empty history", async () => {
    homeShelves.mockResolvedValue(shelves({ playTotal: 0 }));
    const home = useHomeStore();
    await home.refresh();

    expect(home.isEmpty).toBe(true);
  });

  it("only offers a mix once it has enough songs", () => {
    const home = useHomeStore();
    expect(home.isReady(mix({ trackCount: 20 }))).toBe(true);
    expect(home.isReady(mix({ trackCount: 5 }))).toBe(true);
    expect(home.isReady(mix({ trackCount: 4 }))).toBe(false);
    expect(home.isReady(mix({ trackCount: 0 }))).toBe(false);
  });

  it("reloads the shelves after pinning so the badge follows", async () => {
    const home = useHomeStore();
    await home.refresh();
    homeShelves.mockResolvedValue(shelves({ mixes: [mix({ pinned: true })] }));

    await home.setPinned("replay", true);

    expect(setMixPinned).toHaveBeenCalledWith("replay", true);
    expect(home.mixes[0].pinned).toBe(true);
  });

  it("finds a mix whether it came from the shelves or the pinned list", async () => {
    listPinnedMixes.mockResolvedValue([mix({ kind: "archive", name: "Archive Mix" })]);
    const home = useHomeStore();
    await home.refresh();

    expect(home.mix("replay")?.name).toBe("Replay Mix");
    expect(home.mix("archive")?.name).toBe("Archive Mix");
    expect(home.mix("discover")).toBeNull();
  });

  it("regenerating discards the held mixes before reloading", async () => {
    const home = useHomeStore();
    await home.regenerate();

    expect(refreshMixes).toHaveBeenCalled();
    expect(homeShelves).toHaveBeenCalled();
  });

  it("refreshing only the pinned list leaves the shelves alone", async () => {
    const home = useHomeStore();
    await home.refresh();
    homeShelves.mockClear();

    listPinnedMixes.mockResolvedValue([mix({ pinned: true })]);
    await home.refreshPinned();

    expect(homeShelves).not.toHaveBeenCalled();
    expect(home.pinned).toHaveLength(1);
  });
});
