import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import NowPlayingScreen from "../layout/NowPlayingScreen.vue";
import { usePlayerStore } from "@/stores/player";
import { useSettingsStore } from "@/stores/settings";
import type { Track } from "@/lib/types";

/** The handlers the component registers for the engine's crossfade events. */
const handlers = new Map<string, (e: { payload: unknown }) => void>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, handler: (e: { payload: unknown }) => void) => {
    handlers.set(name, handler);
    return Promise.resolve(() => {
      handlers.delete(name);
    });
  }),
}));

function startCrossfade(trackId: string, leadSecs: number) {
  handlers.get("crossfade-started")?.({ payload: { trackId, leadSecs } });
}

function cancelCrossfade() {
  handlers.get("crossfade-cancelled")?.({ payload: undefined });
}

vi.mock("vue-router", () => ({
  useRoute: () => ({ name: "now-playing" }),
  useRouter: () => ({ push: vi.fn(), back: vi.fn() }),
  onBeforeRouteLeave: () => undefined,
}));

vi.mock("@/lib/api", () => ({}));

function track(id: string): Track {
  return {
    id,
    fileCount: 1,
    missingFileCount: 0,
    effectiveFileId: "f1",
    preferredFileId: null,
    sourceId: "src",
    location: "/music/x.flac",
    title: `Song ${id}`,
    artist: "A",
    albumArtist: "A",
    album: "Album",
    trackNumber: 1,
    discNumber: null,
    year: null,
    genre: null,
    durationSecs: 200,
    sampleRate: null,
    channels: null,
    bitsPerSample: null,
    bitrateKbps: null,
    fileSize: null,
    format: null,
    artworkId: `art_${id}`,
    musicbrainzRecordingId: null,
    musicbrainzReleaseId: null,
    gainDb: null,
    addedAt: 0,
  };
}

function mountScreen() {
  return mount(NowPlayingScreen, {
    global: {
      stubs: {
        PnmIcon: true,
        Artwork: true,
        PlaylistArtwork: true,
        QueueList: true,
        IconButton: true,
      },
    },
  });
}

describe("NowPlayingScreen queue skeleton", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("shows skeleton rows while the queue is deferred, then the real list", async () => {
    const player = usePlayerStore();
    player.track = track("a");
    player.queue = {
      items: [{ kind: "track", track: track("b") }],
      currentIndex: 0,
      upcoming: [],
      shuffle: false,
      repeat: "off",
      context: null,
    };

    const wrapper = mountScreen();
    await flushPromises();

    expect(wrapper.find(".screen__skeleton").exists()).toBe(true);
    expect(wrapper.findAll(".screen__skeleton-row")).toHaveLength(5);
    expect(wrapper.get("aside.screen__queue").attributes("aria-busy")).toBe("true");

    await vi.advanceTimersByTimeAsync(200);

    expect(wrapper.find(".screen__skeleton").exists()).toBe(false);
    expect(wrapper.find(".screen__list").exists()).toBe(true);
    expect(wrapper.get("aside.screen__queue").attributes("aria-busy")).toBe("false");
  });

  it("shows the empty message once ready with nothing queued", async () => {
    const wrapper = mountScreen();
    await flushPromises();

    expect(wrapper.find(".screen__skeleton").exists()).toBe(true);
    await vi.advanceTimersByTimeAsync(200);
    expect(wrapper.find(".screen__empty").exists()).toBe(true);
  });
});

describe("NowPlayingScreen crossfade artwork handoff", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    handlers.clear();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function queueOf(player: ReturnType<typeof usePlayerStore>) {
    player.queue = {
      items: [
        { kind: "track", track: track("a") },
        { kind: "track", track: track("b") },
      ],
      currentIndex: 0,
      upcoming: [{ kind: "track", track: track("b") }],
      shuffle: false,
      repeat: "off",
      context: null,
    };
  }

  it("fades the incoming art and backdrop in when the engine starts a crossfade", async () => {
    const player = usePlayerStore();
    player.track = track("a");
    queueOf(player);
    player.snapshot = { ...player.snapshot, playing: true, durationSecs: 200, positionSecs: 196 };

    const wrapper = mountScreen();
    await flushPromises();
    expect(handlers.has("crossfade-started")).toBe(true);
    expect(wrapper.find(".screen__art-fade").exists()).toBe(false);

    startCrossfade("b", 4);
    await wrapper.vm.$nextTick();

    // The incoming art and backdrop are layered on top and fading in. Both
    // backdrops are the same wrapper around identically styled imgs, so the
    // two render the same and dropping the top one cannot snap.
    expect(wrapper.find(".screen__art-fade").exists()).toBe(true);
    const layers = wrapper.findAll(".screen__backdrop-layer");
    expect(layers).toHaveLength(2);
    expect(layers[1].classes()).toContain("screen__backdrop-fade");
    expect(layers[1].find("img").exists()).toBe(true);
    expect(wrapper.get("section.screen").attributes("style")).toContain("--screen-fade");
    expect(wrapper.get("section.screen").attributes("style")).toContain("4s");

    // The boundary arrives mid-fade: the overlay stays until it has run, and
    // the art underneath stays pinned to the outgoing cover rather than
    // snapping to the incoming one halfway through the blend.
    player.track = track("b");
    await flushPromises();
    expect(wrapper.find(".screen__art-fade").exists()).toBe(true);
    expect(wrapper.get(".screen__backdrop-layer img").attributes("src")).toContain("art_a");

    await vi.advanceTimersByTimeAsync(4200);
    expect(wrapper.find(".screen__art-fade").exists()).toBe(false);
    expect(wrapper.find(".screen__backdrop-fade").exists()).toBe(false);
    // Only now does the base layer follow the store again.
    expect(wrapper.get(".screen__backdrop-layer img").attributes("src")).toContain("art_b");
  });

  it("unwinds the fade when the engine cancels the blend", async () => {
    const player = usePlayerStore();
    player.track = track("a");
    queueOf(player);
    player.snapshot = { ...player.snapshot, playing: true, durationSecs: 200, positionSecs: 196 };

    const wrapper = mountScreen();
    await flushPromises();

    startCrossfade("b", 6);
    await wrapper.vm.$nextTick();
    expect(wrapper.find(".screen__art-fade").exists()).toBe(true);

    // The listener seeks: the engine abandons the blend and the outgoing
    // track plays on, so the incoming art has to come back off.
    cancelCrossfade();
    await vi.advanceTimersByTimeAsync(400);
    await flushPromises();

    expect(wrapper.find(".screen__art-fade").exists()).toBe(false);
    expect(wrapper.find(".screen__backdrop-fade").exists()).toBe(false);
    expect(wrapper.get(".screen__backdrop-layer img").attributes("src")).toContain("art_a");

    // ...and the fade is not left half-torn-down: approaching the boundary
    // again starts a fresh one.
    startCrossfade("b", 6);
    await wrapper.vm.$nextTick();
    expect(wrapper.find(".screen__art-fade").exists()).toBe(true);
  });

  it("drops the fade at once when a different track is started mid-blend", async () => {
    const player = usePlayerStore();
    player.track = track("a");
    queueOf(player);
    player.snapshot = { ...player.snapshot, playing: true, durationSecs: 200, positionSecs: 196 };

    const wrapper = mountScreen();
    await flushPromises();

    startCrossfade("b", 6);
    await wrapper.vm.$nextTick();

    // Not the track the engine predicted: the listener skipped somewhere
    // else, and what they asked for wins over the blend.
    player.track = track("c");
    await flushPromises();

    expect(wrapper.find(".screen__art-fade").exists()).toBe(false);
    expect(wrapper.get(".screen__backdrop-layer img").attributes("src")).toContain("art_c");
  });

  it("fades for the full lead even when the crossfade is longer than six seconds", async () => {
    const player = usePlayerStore();
    player.track = track("a");
    queueOf(player);
    player.snapshot = { ...player.snapshot, playing: true, durationSecs: 200, positionSecs: 188 };

    const wrapper = mountScreen();
    await flushPromises();

    startCrossfade("b", 12);
    await wrapper.vm.$nextTick();

    expect(wrapper.get("section.screen").attributes("style")).toContain("12s");

    // Halfway: the overlay must still be there, not snapped back early.
    await vi.advanceTimersByTimeAsync(6000);
    expect(wrapper.find(".screen__art-fade").exists()).toBe(true);
    expect(wrapper.find(".screen__backdrop-fade").exists()).toBe(true);

    // It runs to the boundary and is then held, opaque, until the engine
    // confirms the handoff: dropping it first would put the outgoing art back
    // on screen over audio that has already moved on.
    await vi.advanceTimersByTimeAsync(6200);
    expect(wrapper.find(".screen__art-fade").exists()).toBe(true);

    player.track = track("b");
    await flushPromises();
    expect(wrapper.find(".screen__art-fade").exists()).toBe(false);
    expect(wrapper.find(".screen__backdrop-fade").exists()).toBe(false);
  });

  it("gives up holding the overlay if the handoff is never confirmed", async () => {
    const player = usePlayerStore();
    player.track = track("a");
    queueOf(player);
    player.snapshot = { ...player.snapshot, playing: true, durationSecs: 200, positionSecs: 196 };

    const wrapper = mountScreen();
    await flushPromises();

    startCrossfade("b", 4);
    await wrapper.vm.$nextTick();

    // `track-changed` never arrives. The overlay is fully opaque by now, so
    // the grace period passing is invisible — but it must not be held for the
    // rest of the session.
    await vi.advanceTimersByTimeAsync(4000 + 3100);
    expect(wrapper.find(".screen__art-fade").exists()).toBe(false);
    expect(wrapper.find(".screen__backdrop-fade").exists()).toBe(false);
  });

  it("never fades when the crossfadeArt preference is off", async () => {
    const player = usePlayerStore();
    player.track = track("a");
    queueOf(player);
    useSettingsStore().preferences.crossfadeArt = false;

    const wrapper = mountScreen();
    await flushPromises();

    startCrossfade("b", 4);
    await wrapper.vm.$nextTick();

    expect(wrapper.find(".screen__art-fade").exists()).toBe(false);
    expect(wrapper.find(".screen__backdrop-fade").exists()).toBe(false);

    // Turning it back on is live: the next handoff fades again.
    useSettingsStore().preferences.crossfadeArt = true;
    startCrossfade("b", 4);
    await wrapper.vm.$nextTick();
    expect(wrapper.find(".screen__art-fade").exists()).toBe(true);
  });

  it("swaps instantly on a manual track start, with no crossfade event", async () => {
    const player = usePlayerStore();
    player.track = track("a");
    queueOf(player);
    player.snapshot = { ...player.snapshot, playing: true, durationSecs: 200, positionSecs: 196 };

    const wrapper = mountScreen();
    await flushPromises();

    player.track = track("b");
    await flushPromises();

    expect(wrapper.find(".screen__art-fade").exists()).toBe(false);
    expect(wrapper.find(".screen__backdrop-fade").exists()).toBe(false);
  });

  it("ignores a crossfade event for a track it cannot find in the queue", async () => {
    const player = usePlayerStore();
    player.track = track("a");
    queueOf(player);

    const wrapper = mountScreen();
    await flushPromises();

    startCrossfade("unknown", 4);
    await wrapper.vm.$nextTick();

    expect(wrapper.find(".screen__art-fade").exists()).toBe(false);
    expect(wrapper.find(".screen__backdrop-fade").exists()).toBe(false);
  });
});
