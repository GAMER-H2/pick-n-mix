import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount, type VueWrapper } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import SettingsModal from "../settings/SettingsModal.vue";
import { useSettingsStore } from "@/stores/settings";
import { usePresetEditorStore } from "@/stores/presetEditor";
import { useUiStore } from "@/stores/ui";
import type { Track } from "@/lib/types";

const setAppPreferences = vi.fn();
const outputDevices = vi.fn();
const homeShelves = vi.fn();
const listeningHistory = vi.fn();
const clearListeningHistory = vi.fn();
const clearListeningHistoryForSong = vi.fn();
const mixerState = vi.fn();
const filtersDirectory = vi.fn();
const savePreset = vi.fn();
const updatePreset = vi.fn();
const deletePreset = vi.fn();
const playTracks = vi.fn();
const togglePlay = vi.fn();
const importFilter = vi.fn();
const deleteFilter = vi.fn();
const openDialog = vi.fn();

vi.mock("@/lib/api", () => ({
  setAppPreferences: (...args: unknown[]) => setAppPreferences(...args),
  homeShelves: (...args: unknown[]) => homeShelves(...args),
  listeningHistory: (...args: unknown[]) => listeningHistory(...args),
  clearListeningHistory: (...args: unknown[]) => clearListeningHistory(...args),
  clearListeningHistoryForSong: (...args: unknown[]) => clearListeningHistoryForSong(...args),
  mixerState: (...args: unknown[]) => mixerState(...args),
  filtersDirectory: (...args: unknown[]) => filtersDirectory(...args),
  savePreset: (...args: unknown[]) => savePreset(...args),
  updatePreset: (...args: unknown[]) => updatePreset(...args),
  deletePreset: (...args: unknown[]) => deletePreset(...args),
  importFilter: (...args: unknown[]) => importFilter(...args),
  deleteFilter: (...args: unknown[]) => deleteFilter(...args),
  playTracks: (...args: unknown[]) => playTracks(...args),
  togglePlay: (...args: unknown[]) => togglePlay(...args),
  outputDevices: (...args: unknown[]) => outputDevices(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openDialog(...args),
}));

function buttonWithText(wrapper: VueWrapper, text: string) {
  return wrapper.findAll("button").find((button) => button.text().trim().startsWith(text));
}

function track(): Track {
  return {
    id: "song-1", sourceId: "local", location: "/Music/song.flac", title: "History Song",
    artist: "History Artist", albumArtist: "History Artist", album: "History Album",
    trackNumber: 1, discNumber: 1, year: 2024, genre: "Rock", durationSecs: 180,
    sampleRate: 48000, channels: 2, bitsPerSample: 24, bitrateKbps: 1200,
    fileSize: 20_000_000, format: "FLAC", artworkId: null, musicbrainzRecordingId: null,
    musicbrainzReleaseId: null, gainDb: null, addedAt: 1, fileCount: 1,
    missingFileCount: 0, effectiveFileId: "file-1", preferredFileId: null,
  };
}

async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  await new Promise((resolve) => window.setTimeout(resolve, 0));
}

describe("SettingsModal", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.resetAllMocks();
    setAppPreferences.mockImplementation(async (preferences) => preferences);
    outputDevices.mockResolvedValue([]);
    homeShelves.mockResolvedValue({ mixes: [], picks: [], recentPlaylists: [], playTotal: 0 });
    listeningHistory.mockResolvedValue([]);
    clearListeningHistory.mockResolvedValue(undefined);
    clearListeningHistoryForSong.mockResolvedValue(undefined);
    mixerState.mockResolvedValue({ global: {}, presets: [], filters: [] });
    filtersDirectory.mockResolvedValue("/tmp/filters");
    savePreset.mockResolvedValue([]);
    updatePreset.mockResolvedValue([]);
    deletePreset.mockResolvedValue([]);
    playTracks.mockResolvedValue(undefined);
    togglePlay.mockResolvedValue(true);
    importFilter.mockResolvedValue([]);
    deleteFilter.mockResolvedValue([]);
  });

  it("navigates between all settings panes", async () => {
    const wrapper = mount(SettingsModal);

    for (const pane of ["Playback", "Recommendations", "Mixer", "Library", "Theme"]) {
      const button = buttonWithText(wrapper, pane);
      if (!button) throw new Error(`Missing ${pane} navigation button`);
      await button.trigger("click");
      await settle();
      expect(wrapper.find(".pane-heading h3").text()).toBe(pane);
    }

    expect(listeningHistory).toHaveBeenCalledWith(200);
    expect(mixerState).toHaveBeenCalledTimes(1);
  });

  it("persists a theme selection immediately", async () => {
    const wrapper = mount(SettingsModal);
    const dark = buttonWithText(wrapper, "Dark");
    if (!dark) throw new Error("Missing Dark theme button");

    await dark.trigger("click");
    await settle();

    expect(setAppPreferences).toHaveBeenCalledWith(expect.objectContaining({ theme: "dark" }));
    expect(useSettingsStore().preferences.theme).toBe("dark");
  });

  it("requires a two-step confirmation before clearing all history", async () => {
    const wrapper = mount(SettingsModal);
    const recommendations = buttonWithText(wrapper, "Recommendations");
    if (!recommendations) throw new Error("Missing Recommendations navigation button");
    await recommendations.trigger("click");
    await settle();

    const first = buttonWithText(wrapper, "Clear all history");
    if (!first) throw new Error("Missing clear history button");
    await first.trigger("click");

    expect(clearListeningHistory).not.toHaveBeenCalled();
    expect(wrapper.get("[role='alert']").text()).toContain("recommendation shelves empty");

    const confirm = buttonWithText(wrapper, "Confirm clear all");
    if (!confirm) throw new Error("Missing history confirmation button");
    await confirm.trigger("click");
    await settle();

    expect(clearListeningHistory).toHaveBeenCalledTimes(1);
    expect(wrapper.find("[role='alert']").exists()).toBe(false);
  });

  it("defaults fading off and exposes a direction selector only when enabled", async () => {
    const wrapper = mount(SettingsModal);
    const playback = buttonWithText(wrapper, "Playback");
    if (!playback) throw new Error("Missing Playback navigation button");
    await playback.trigger("click");
    await settle();

    expect(wrapper.find("[aria-label='Apply fading when']").exists()).toBe(false);
    await wrapper.get("[aria-label='Fade on pause and play']").trigger("click");
    await settle();
    expect(setAppPreferences).toHaveBeenCalledWith(expect.objectContaining({ fadeMode: "both" }));

    // The app's own picker, not a native select: opening it lists the modes.
    await wrapper.get("[aria-label='Apply fading when']").trigger("click");
    const pausing = buttonWithText(wrapper, "Pausing playback");
    if (!pausing) throw new Error("Missing the pausing option");
    await pausing.trigger("click");
    await settle();
    expect(setAppPreferences).toHaveBeenLastCalledWith(expect.objectContaining({ fadeMode: "pause" }));
  });

  it("keeping reverb on pause is a real setting", async () => {
    const wrapper = mount(SettingsModal);
    const playback = buttonWithText(wrapper, "Playback");
    if (!playback) throw new Error("Missing Playback navigation button");
    await playback.trigger("click");
    await settle();

    const toggle = wrapper.get("[aria-label='Keep reverb on pause']");
    expect(toggle.attributes()).not.toHaveProperty("disabled");
    await toggle.trigger("click");
    await settle();
    expect(setAppPreferences).toHaveBeenCalledWith(
      expect.objectContaining({ keepReverbOnPause: true }),
    );
  });

  it("lists the machine's output devices and switches between them", async () => {
    outputDevices.mockResolvedValue(["Built-in Output", "Studio Monitors"]);
    const wrapper = mount(SettingsModal);
    const playback = buttonWithText(wrapper, "Playback");
    if (!playback) throw new Error("Missing Playback navigation button");
    await playback.trigger("click");
    await settle();

    await wrapper.get("[aria-label='Output device']").trigger("click");
    const monitors = buttonWithText(wrapper, "Studio Monitors");
    if (!monitors) throw new Error("Missing the second device");
    await monitors.trigger("click");
    await settle();

    expect(setAppPreferences).toHaveBeenCalledWith(
      expect.objectContaining({ outputDevice: "Studio Monitors" }),
    );
  });

  /// Choosing the default again has to clear the override, not store its label.
  it("returning to the system default clears the saved device", async () => {
    outputDevices.mockResolvedValue(["Built-in Output"]);
    const wrapper = mount(SettingsModal);
    const playback = buttonWithText(wrapper, "Playback");
    if (!playback) throw new Error("Missing Playback navigation button");
    await playback.trigger("click");
    await settle();

    await wrapper.get("[aria-label='Output device']").trigger("click");
    const fallback = buttonWithText(wrapper, "System default");
    if (!fallback) throw new Error("Missing the system default option");
    await fallback.trigger("click");
    await settle();

    expect(setAppPreferences).toHaveBeenCalledWith(
      expect.objectContaining({ outputDevice: "" }),
    );
  });

  it("renders history as a fixed queue-style list with playback and context actions", async () => {
    const historyTrack = track();
    listeningHistory.mockResolvedValue([{
      play: {
        songId: historyTrack.id, playedAt: 1_700_000_000, secondsPlayed: 42,
        fraction: 0.23, counted: true, contextKind: "library", contextId: "library",
      },
      track: historyTrack,
    }]);
    const wrapper = mount(SettingsModal);
    useUiStore().settingsOpen = true;
    const recommendations = buttonWithText(wrapper, "Recommendations");
    if (!recommendations) throw new Error("Missing Recommendations navigation button");
    await recommendations.trigger("click");
    await settle();

    expect(wrapper.text()).toContain("History Song");
    expect(wrapper.text()).toContain("Played");
    expect(wrapper.find("[aria-label='Drag to reorder']").exists()).toBe(false);

    await wrapper.get(".history-queue [data-row]").trigger("contextmenu", {
      clientX: 24,
      clientY: 36,
    });
    expect(useUiStore().contextMenu?.tracks).toEqual([historyTrack]);
    expect(useUiStore().contextMenu?.x).toBe(24);

    await wrapper.get(".history-queue [title='Play History Song']").trigger("click");
    await settle();
    expect(playTracks).toHaveBeenCalledWith({
      trackIds: [historyTrack.id],
      startIndex: 0,
      context: null,
    });
    expect(useUiStore().settingsOpen).toBe(false);
  });

  it("opens presets in the isolated advanced mixer and persists built-in hiding", async () => {
    const builtIn = { id: "flat", name: "Flat", builtIn: true, kind: "mixer", settings: {} };
    mixerState.mockResolvedValue({ global: {}, context: null, track: null, effective: {}, presets: [builtIn], filters: [] });
    const wrapper = mount(SettingsModal);
    const mixerButton = buttonWithText(wrapper, "Mixer");
    if (!mixerButton) throw new Error("Missing Mixer navigation button");
    await mixerButton.trigger("click");
    await settle();

    await wrapper.get(".item-main").trigger("click");
    expect(wrapper.text()).toContain("Preset · Flat Custom");
    expect(wrapper.text()).toContain("Playback does not change while you edit it");
    expect(savePreset).not.toHaveBeenCalled();

    const hide = buttonWithText(wrapper, "Hide");
    if (!hide) throw new Error("Missing Hide button");
    await hide.trigger("click");
    await settle();
    expect(setAppPreferences).toHaveBeenLastCalledWith(expect.objectContaining({
      hiddenBuiltInPresetIds: ["flat"],
    }));
  });

  it("manages EQ presets separately and opens an EQ-only editor", async () => {
    const customEq = {
      id: "eq-custom",
      name: "My EQ",
      builtIn: false,
      kind: "eq",
      settings: {
        eq: {
          enabled: true,
          preampDb: 0,
          bands: [{ kind: "peak", freq: 1000, gainDb: 3, q: 0.71, enabled: true }],
        },
      },
    };
    mixerState.mockResolvedValue({
      global: {}, context: null, track: null, effective: {}, presets: [customEq], filters: [],
    });
    savePreset.mockResolvedValue([customEq]);
    const wrapper = mount(SettingsModal);
    const mixerButton = buttonWithText(wrapper, "Mixer");
    if (!mixerButton) throw new Error("Missing Mixer navigation button");
    await mixerButton.trigger("click");
    await settle();

    expect(wrapper.text()).toContain("EQ presets");
    expect(wrapper.text()).toContain("My EQ");
    const customItem = wrapper.findAll(".item-main").find((item) => item.text().includes("My EQ"));
    await customItem!.trigger("click");
    expect(wrapper.text()).toContain("EQ Preset Editor");
    expect(wrapper.text()).not.toContain("Pitch");

    usePresetEditorStore().close();
    await wrapper.vm.$nextTick();
    await wrapper.get("[aria-label='EQ preset name']").setValue("Saved Curve");
    const eqForm = wrapper.findAll("form").find((form) =>
      form.find("[aria-label='EQ preset name']").exists(),
    );
    await eqForm!.trigger("submit");
    await settle();
    expect(savePreset).toHaveBeenCalledWith(
      "Saved Curve",
      expect.objectContaining({ eq: expect.any(Object) }),
      "eq",
    );
  });

  it("labels the remote-library controls honestly", async () => {
    const wrapper = mount(SettingsModal);
    const playback = buttonWithText(wrapper, "Playback");
    if (!playback) throw new Error("Missing Playback navigation button");
    await playback.trigger("click");

    // Both playback controls are implemented now, so nothing here should
    // still be claiming otherwise.
    expect(wrapper.text()).not.toContain("requires engine restart support");
    expect(wrapper.text()).not.toContain("does not yet render effect tails");
    expect(wrapper.text()).not.toContain("Planned");

    const library = buttonWithText(wrapper, "Library");
    if (!library) throw new Error("Missing Library navigation button");
    await library.trigger("click");

    expect(wrapper.text()).toContain("Not available yet");
    expect(wrapper.text()).toContain("system keychain");
    const connectButtons = wrapper.findAll("button").filter((button) => button.text() === "Connect");
    expect(connectButtons).toHaveLength(2);
    expect(connectButtons.every((button) => "disabled" in button.attributes())).toBe(true);
  });
});
