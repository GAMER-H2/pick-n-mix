import { defineStore } from "pinia";
import { computed, ref } from "vue";
import * as api from "@/lib/api";
import type { AppPreferences, PlayRecord } from "@/lib/types";

export const DEFAULT_PREFERENCES: AppPreferences = {
  theme: "system",
  accent: "#f56300",
  fadeMode: "off",
  keepReverbOnPause: false,
  outputDevice: "",
  mixLength: 50,
  replayDays: 30,
  replayMinPlays: 2,
  archiveDays: 60,
  archiveMinPlays: 3,
  discoverMaxPlays: 3,
  hiddenBuiltInPresetIds: [],
  hiddenBuiltInFilterIds: [],
};

const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
let listeningForSystemTheme = false;

function channel(hex: string, offset: number) {
  return Math.max(0, Math.min(255, Number.parseInt(hex, 16) + offset))
    .toString(16)
    .padStart(2, "0");
}

function shiftedColour(hex: string, offset: number) {
  return `#${channel(hex.slice(1, 3), offset)}${channel(hex.slice(3, 5), offset)}${channel(hex.slice(5, 7), offset)}`;
}

function accentRgb(hex: string) {
  return [hex.slice(1, 3), hex.slice(3, 5), hex.slice(5, 7)]
    .map((part) => Number.parseInt(part, 16))
    .join(", ");
}

export const useSettingsStore = defineStore("settings", () => {
  const preferences = ref<AppPreferences>({ ...DEFAULT_PREFERENCES });
  const loaded = ref(false);
  const saving = ref(false);
  const history = ref<PlayRecord[]>([]);
  const historyLoading = ref(false);

  const resolvedTheme = computed<"light" | "dark">(() => {
    if (preferences.value.theme === "system") return systemDark.matches ? "dark" : "light";
    return preferences.value.theme;
  });

  function applyAppearance() {
    const root = document.documentElement;
    const accent = /^#[0-9a-f]{6}$/i.test(preferences.value.accent)
      ? preferences.value.accent
      : DEFAULT_PREFERENCES.accent;
    root.setAttribute("data-theme", resolvedTheme.value);
    root.style.setProperty("--accent", accent);
    root.style.setProperty("--accent-hover", shiftedColour(accent, 20));
    root.style.setProperty("--accent-active", shiftedColour(accent, -28));
    root.style.setProperty("--accent-tint", `rgba(${accentRgb(accent)}, 0.12)`);
    root.style.setProperty("--accent-tint-strong", `rgba(${accentRgb(accent)}, 0.2)`);
  }

  async function initialise() {
    if (!listeningForSystemTheme) {
      systemDark.addEventListener("change", applyAppearance);
      listeningForSystemTheme = true;
    }
    try {
      preferences.value = await api.appPreferences();
    } catch (error) {
      console.error("Unable to load settings:", error);
    } finally {
      loaded.value = true;
      applyAppearance();
    }
  }

  async function update(patch: Partial<AppPreferences>) {
    const previous = preferences.value;
    preferences.value = { ...previous, ...patch };
    applyAppearance();
    saving.value = true;
    try {
      preferences.value = await api.setAppPreferences(preferences.value);
      applyAppearance();
      return preferences.value;
    } catch (error) {
      preferences.value = previous;
      applyAppearance();
      throw error;
    } finally {
      saving.value = false;
    }
  }

  async function loadHistory() {
    historyLoading.value = true;
    try {
      history.value = await api.listeningHistory(200);
    } finally {
      historyLoading.value = false;
    }
  }

  async function clearAllHistory() {
    await api.clearListeningHistory();
    history.value = [];
  }

  async function clearSongHistory(songId: string) {
    await api.clearListeningHistoryForSong(songId);
    history.value = history.value.filter((record) => record.play.songId !== songId);
  }

  return {
    preferences,
    loaded,
    saving,
    history,
    historyLoading,
    resolvedTheme,
    initialise,
    update,
    applyAppearance,
    loadHistory,
    clearAllHistory,
    clearSongHistory,
  };
});
