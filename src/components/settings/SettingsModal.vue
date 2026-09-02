<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import AppToggle from "../AppToggle.vue";
import QueueList from "../QueueList.vue";
import SelectMenu from "../SelectMenu.vue";
import PnmIcon from "../icons/PnmIcon.vue";
import AdvancedMixer from "../mixer/AdvancedMixer.vue";
import { EQ_PRESETS } from "@/lib/eqPresets";
import * as api from "@/lib/api";
import type { AppPreferences, FadeMode, Preset, ThemePreference } from "@/lib/types";
import { useHomeStore } from "@/stores/home";
import { useLibraryStore } from "@/stores/library";
import { useMixerStore } from "@/stores/mixer";
import { usePlayerStore } from "@/stores/player";
import { usePresetEditorStore } from "@/stores/presetEditor";
import { useSettingsStore } from "@/stores/settings";
import { useUiStore } from "@/stores/ui";

type Pane = "theme" | "playback" | "recommendations" | "mixer" | "library";
type NumberPreference =
  | "mixLength"
  | "replayDays"
  | "replayMinPlays"
  | "archiveDays"
  | "archiveMinPlays"
  | "discoverMaxPlays";

const panes: { id: Pane; label: string; description: string }[] = [
  { id: "theme", label: "Theme", description: "Appearance and accent" },
  { id: "playback", label: "Playback", description: "Audio behaviour" },
  { id: "recommendations", label: "Recommendations", description: "Mixes and history" },
  { id: "mixer", label: "Mixer", description: "Presets and ambience" },
  { id: "library", label: "Library", description: "Sources and scanning" },
];

const numberFields: {
  key: NumberPreference;
  label: string;
  help: string;
  min: number;
  max: number;
}[] = [
  { key: "mixLength", label: "Songs per mix", help: "Target size of generated mixes.", min: 10, max: 200 },
  { key: "replayDays", label: "Replay window", help: "Look back this many days for favourites.", min: 1, max: 3650 },
  { key: "replayMinPlays", label: "Replay minimum plays", help: "Plays required before a song can return.", min: 1, max: 100 },
  { key: "archiveDays", label: "Archive age", help: "Songs untouched for at least this many days.", min: 1, max: 3650 },
  { key: "archiveMinPlays", label: "Archive minimum plays", help: "Past plays required for archive picks.", min: 1, max: 100 },
  { key: "discoverMaxPlays", label: "Discover maximum plays", help: "Only include songs played no more than this.", min: 1, max: 100 },
];

const accents = ["#f56300", "#e23d55", "#a855f7", "#3b82f6", "#14a37f", "#d69b16"];
const dialog = ref<HTMLElement | null>(null);
const activePane = ref<Pane>("theme");
const clearHistoryArmed = ref(false);
const presetName = ref("");
const eqPresetName = ref("");
const busy = ref<string | null>(null);
const pendingFilterDelete = ref<string | null>(null);
const pendingFolderRemove = ref<string | null>(null);

const ui = useUiStore();
const settings = useSettingsStore();
const home = useHomeStore();
const mixer = useMixerStore();
const library = useLibraryStore();
const player = usePlayerStore();
const presetEditor = usePresetEditorStore();

const currentPane = computed(() => panes.find((pane) => pane.id === activePane.value) ?? panes[0]);
const outputDevice = computed(() => player.snapshot.deviceName.trim() || "System default");

/** Stands in for "no override" in the picker, which needs a non-empty id. */
const SYSTEM_DEFAULT = "__default__";
const devices = ref<string[]>([]);

const deviceOptions = computed(() => [
  { id: SYSTEM_DEFAULT, label: "System default" },
  ...devices.value.map((name) => ({ id: name, label: name })),
]);

/**
 * A saved device that is not plugged in right now still shows, so the choice
 * is visible rather than silently reverting to the default in the picker while
 * the preference still holds it.
 */
const selectedDevice = computed(() => {
  const saved = settings.preferences.outputDevice;
  if (!saved) return SYSTEM_DEFAULT;
  return devices.value.includes(saved) ? saved : SYSTEM_DEFAULT;
});

const savedDeviceMissing = computed(
  () =>
    settings.preferences.outputDevice !== "" &&
    devices.value.length > 0 &&
    !devices.value.includes(settings.preferences.outputDevice),
);

const fadeModeOptions = [
  { id: "play", label: "Starting playback" },
  { id: "pause", label: "Pausing playback" },
  { id: "both", label: "Starting and pausing" },
];
const visiblePresets = computed(() => mixer.presets.filter((preset) =>
  preset.kind === "mixer"
  && (!preset.builtIn || !settings.preferences.hiddenBuiltInPresetIds.includes(preset.id)),
));
const hiddenPresets = computed(() => mixer.presets.filter((preset) =>
  preset.kind === "mixer"
  && preset.builtIn
  && settings.preferences.hiddenBuiltInPresetIds.includes(preset.id),
));
const builtInEqPresets: Preset[] = EQ_PRESETS.map((preset) => ({
  id: preset.id,
  name: preset.name,
  builtIn: true,
  kind: "eq",
  settings: { eq: preset.eq },
}));
const eqPresets = computed(() => [
  ...builtInEqPresets,
  ...mixer.presets.filter((preset) => preset.kind === "eq" && !preset.builtIn),
]);
const visibleEqPresets = computed(() => eqPresets.value.filter((preset) =>
  !preset.builtIn || !settings.preferences.hiddenBuiltInPresetIds.includes(preset.id),
));
const hiddenEqPresets = computed(() => eqPresets.value.filter((preset) =>
  preset.builtIn && settings.preferences.hiddenBuiltInPresetIds.includes(preset.id),
));
const visibleFilters = computed(() => mixer.filters.filter((filter) =>
  !filter.builtIn || !settings.preferences.hiddenBuiltInFilterIds.includes(filter.id),
));
const hiddenFilters = computed(() => mixer.filters.filter((filter) =>
  filter.builtIn && settings.preferences.hiddenBuiltInFilterIds.includes(filter.id),
));

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function close() {
  presetEditor.close();
  ui.settingsOpen = false;
}

function onKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  event.preventDefault();
  event.stopPropagation();
  if (presetEditor.session) presetEditor.close();
  else close();
}

async function reportFailure(label: string, action: () => Promise<unknown>) {
  try {
    await action();
  } catch (error) {
    ui.notify(`${label}: ${errorMessage(error)}`, "error");
  }
}

async function selectPane(pane: Pane) {
  activePane.value = pane;
  clearHistoryArmed.value = false;
  pendingFilterDelete.value = null;
  pendingFolderRemove.value = null;
  if (pane === "recommendations") {
    await reportFailure("Could not load listening history", () => settings.loadHistory());
  } else if (pane === "mixer") {
    await reportFailure("Could not load mixer settings", () => mixer.refresh());
  } else if (pane === "playback") {
    // Read on open rather than once at mount: devices come and go while the
    // app is running, and a list from ten minutes ago is often wrong.
    await reportFailure("Could not list output devices", async () => {
      devices.value = (await api.outputDevices()) ?? [];
    });
  }
}

function updatePreference(patch: Partial<AppPreferences>) {
  const recommendationKeys: ReadonlyArray<keyof AppPreferences> = [
    "mixLength",
    "replayDays",
    "replayMinPlays",
    "archiveDays",
    "archiveMinPlays",
    "discoverMaxPlays",
  ];
  void reportFailure("Could not save settings", async () => {
    await settings.update(patch);
    if (recommendationKeys.some((key) => key in patch)) await home.refresh();
  });
}

function updateTheme(theme: ThemePreference) {
  updatePreference({ theme });
}

function setFadeEnabled(enabled: boolean) {
  updatePreference({ fadeMode: enabled ? "both" : "off" });
}

function updateFadeMode(mode: string) {
  updatePreference({ fadeMode: mode as FadeMode });
}

/**
 * Switching device reopens the stream at a new sample rate and reloads the
 * current track at its position, so this can take a moment and is reported
 * rather than being fired off silently.
 */
function updateOutputDevice(device: string) {
  void reportFailure("Could not switch output device", async () => {
    await settings.update({ outputDevice: device === SYSTEM_DEFAULT ? "" : device });
    await player.refresh();
  });
}

function updateAccent(event: Event) {
  updatePreference({ accent: (event.target as HTMLInputElement).value });
}

function updateNumber(key: NumberPreference, raw: string, min: number, max: number) {
  const parsed = Number(raw);
  if (!Number.isFinite(parsed)) return;
  updatePreference({ [key]: Math.min(max, Math.max(min, Math.round(parsed))) });
}

async function clearAllHistory() {
  if (!clearHistoryArmed.value) {
    clearHistoryArmed.value = true;
    return;
  }
  busy.value = "history-all";
  try {
    await reportFailure("Could not clear listening history", async () => {
      await settings.clearAllHistory();
      await home.refresh();
    });
    clearHistoryArmed.value = false;
  } finally {
    busy.value = null;
  }
}

async function playHistory(index: number) {
  const track = settings.history[index]?.track;
  if (!track) return;
  close();
  if (player.track?.id === track.id) await player.toggle();
  else await player.playTracks([track]);
}

function openHistoryMenu(index: number, event: MouseEvent) {
  const track = settings.history[index]?.track;
  if (!track) return;
  ui.openContextMenu({
    x: event.clientX,
    y: event.clientY,
    tracks: [track],
    onSelect: close,
  });
}

async function clearTrackHistory(songId: string) {
  busy.value = `history:${songId}`;
  try {
    await reportFailure("Could not clear song history", async () => {
      await settings.clearSongHistory(songId);
      await home.refresh();
    });
  } finally {
    busy.value = null;
  }
}

async function savePreset() {
  const name = presetName.value.trim();
  if (!name) return;
  busy.value = "preset-save";
  try {
    await reportFailure("Could not save preset", () => mixer.saveAsPreset(name));
    presetName.value = "";
  } finally {
    busy.value = null;
  }
}

async function saveEqPreset() {
  const name = eqPresetName.value.trim();
  if (!name) return;
  busy.value = "eq-preset-save";
  try {
    await reportFailure("Could not save EQ preset", () => mixer.saveEqPreset(name, mixer.effective.eq));
    eqPresetName.value = "";
  } finally {
    busy.value = null;
  }
}

function editPreset(preset: Preset) {
  presetEditor.open(preset);
}

function setBuiltInHidden(kind: "preset" | "filter", id: string, hidden: boolean) {
  const key = kind === "preset" ? "hiddenBuiltInPresetIds" : "hiddenBuiltInFilterIds";
  const current = settings.preferences[key].filter((candidate) => candidate !== id);
  updatePreference({ [key]: hidden ? [...current, id] : current });
}

async function removePreset(id: string) {
  busy.value = `preset:${id}`;
  try {
    await reportFailure("Could not delete preset", () => mixer.removePreset(id));
  } finally {
    busy.value = null;
  }
}

async function chooseFilter() {
  busy.value = "filter-import";
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      title: "Choose ambience audio",
      filters: [{
        name: "Audio files",
        extensions: ["aac", "aif", "aiff", "alac", "flac", "m4a", "mp3", "ogg", "opus", "wav"],
      }],
    });
    if (typeof selected !== "string") return;
    await api.importFilter(selected);
    await mixer.refresh();
    ui.notify("Atmosphere imported");
  } catch (error) {
    ui.notify(`Could not import atmosphere: ${errorMessage(error)}`, "error");
  } finally {
    busy.value = null;
  }
}

async function removeFilter(id: string) {
  if (pendingFilterDelete.value !== id) {
    pendingFilterDelete.value = id;
    return;
  }
  busy.value = `filter:${id}`;
  try {
    await api.deleteFilter(id);
    await mixer.refresh();
    pendingFilterDelete.value = null;
  } catch (error) {
    ui.notify(`Could not delete atmosphere: ${errorMessage(error)}`, "error");
  } finally {
    busy.value = null;
  }
}

async function chooseFolder() {
  busy.value = "folder-add";
  try {
    const selected = await open({ directory: true, multiple: false, title: "Choose a music folder" });
    if (typeof selected !== "string") return;
    await library.addFolder(selected);
    ui.notify(library.lastReport
      ? `Added ${library.lastReport.added} tracks, updated ${library.lastReport.updated}`
      : "Music folder added");
  } catch (error) {
    ui.notify(`Could not add music folder: ${errorMessage(error)}`, "error");
  } finally {
    busy.value = null;
  }
}

async function removeFolder(path: string) {
  if (pendingFolderRemove.value !== path) {
    pendingFolderRemove.value = path;
    return;
  }
  busy.value = `folder:${path}`;
  try {
    await library.removeFolder(path);
    pendingFolderRemove.value = null;
  } catch (error) {
    ui.notify(`Could not remove music folder: ${errorMessage(error)}`, "error");
  } finally {
    busy.value = null;
  }
}

async function rescan() {
  busy.value = "scan";
  try {
    await library.scan();
    ui.notify(library.lastReport
      ? `Scan complete: ${library.lastReport.added} added, ${library.lastReport.updated} updated`
      : "Library scan complete");
  } catch (error) {
    ui.notify(`Could not scan library: ${errorMessage(error)}`, "error");
  } finally {
    busy.value = null;
  }
}

function historyDate(timestamp: number): string {
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" })
    .format(new Date(timestamp * 1000));
}

function duration(seconds: number): string {
  const rounded = Math.max(0, Math.round(seconds));
  const minutes = Math.floor(rounded / 60);
  return `${minutes}:${String(rounded % 60).padStart(2, "0")}`;
}

onMounted(async () => {
  window.addEventListener("keydown", onKeydown, true);
  await nextTick();
  dialog.value?.focus();
  if (activePane.value === "recommendations") {
    await reportFailure("Could not load listening history", () => settings.loadHistory());
  }
});

onBeforeUnmount(() => window.removeEventListener("keydown", onKeydown, true));
</script>

<template>
  <div class="settings-scrim" @click.self="close">
    <div class="settings-workspace" :class="{ 'is-editing': presetEditor.session }">
    <section
      ref="dialog"
      class="settings-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="settings-title"
      tabindex="-1"
    >
      <header class="settings-header">
        <div>
          <p class="eyebrow">Pick n Mix</p>
          <h2 id="settings-title">Settings</h2>
        </div>
        <span v-if="settings.saving" class="saving" role="status">Saving…</span>
        <button class="icon-button" type="button" aria-label="Close settings" @click="close">
          <PnmIcon name="close" :size="17" />
        </button>
      </header>

      <div class="settings-layout">
        <nav class="settings-nav" aria-label="Settings sections">
          <button
            v-for="pane in panes"
            :key="pane.id"
            type="button"
            :class="{ active: activePane === pane.id }"
            :aria-current="activePane === pane.id ? 'page' : undefined"
            @click="selectPane(pane.id)"
          >
            <strong>{{ pane.label }}</strong>
            <span>{{ pane.description }}</span>
          </button>
        </nav>

        <main class="settings-content scroll-area">
          <div class="pane-heading">
            <p class="eyebrow">{{ currentPane.description }}</p>
            <h3>{{ currentPane.label }}</h3>
          </div>

          <section v-if="activePane === 'theme'" class="pane" aria-labelledby="theme-heading">
            <h4 id="theme-heading">Colour scheme</h4>
            <div class="segmented" aria-label="Colour scheme">
              <button
                v-for="option in (['system', 'light', 'dark'] as ThemePreference[])"
                :key="option"
                type="button"
                :class="{ active: settings.preferences.theme === option }"
                :aria-pressed="settings.preferences.theme === option"
                @click="updateTheme(option)"
              >
                {{ option[0].toUpperCase() + option.slice(1) }}
              </button>
            </div>

            <div class="section-heading">
              <div>
                <h4>Accent colour</h4>
                <p>Applied throughout the app and saved immediately.</p>
              </div>
            </div>
            <div class="accent-row">
              <button
                v-for="accent in accents"
                :key="accent"
                type="button"
                class="swatch"
                :class="{ active: settings.preferences.accent.toLowerCase() === accent }"
                :style="{ backgroundColor: accent }"
                :aria-label="`Use accent ${accent}`"
                :aria-pressed="settings.preferences.accent.toLowerCase() === accent"
                @click="updatePreference({ accent })"
              />
              <label class="custom-colour">
                <input
                  type="color"
                  :value="settings.preferences.accent"
                  aria-label="Custom accent colour"
                  @input="updateAccent"
                />
                Custom
              </label>
            </div>
          </section>

          <section v-else-if="activePane === 'playback'" class="pane">
            <div class="setting-row">
              <div>
                <h4>Fade on pause and play</h4>
                <p>Gently ramp audio instead of starting or stopping abruptly.</p>
              </div>
              <AppToggle
                :model-value="settings.preferences.fadeMode !== 'off'"
                label="Fade on pause and play"
                @update:model-value="setFadeEnabled"
              />
            </div>

            <div v-if="settings.preferences.fadeMode !== 'off'" class="field fade-direction">
              <SelectMenu
                :model-value="settings.preferences.fadeMode"
                :options="fadeModeOptions"
                label="Apply fading when"
                @update:model-value="updateFadeMode"
              />
            </div>

            <div class="setting-row">
              <div>
                <h4>Keep reverb on pause</h4>
                <p>
                  Let reverb and delay ring out after pausing instead of stopping with the
                  music. Only applies when one of those effects is on.
                </p>
              </div>
              <AppToggle
                :model-value="settings.preferences.keepReverbOnPause"
                label="Keep reverb on pause"
                @update:model-value="updatePreference({ keepReverbOnPause: $event })"
              />
            </div>

            <div class="section-heading output-heading">
              <div>
                <h4>Audio output</h4>
                <p>Currently playing through <strong>{{ outputDevice }}</strong><template v-if="player.snapshot.deviceSampleRate"> at {{ (player.snapshot.deviceSampleRate / 1000).toFixed(1) }} kHz</template>.</p>
              </div>
            </div>
            <div class="field">
              <SelectMenu
                :model-value="selectedDevice"
                :options="deviceOptions"
                label="Output device"
                @update:model-value="updateOutputDevice"
              />
              <small v-if="savedDeviceMissing">
                <strong>{{ settings.preferences.outputDevice }}</strong> is not connected, so
                the system default is being used until it comes back.
              </small>
              <small v-else>
                Switching device briefly reloads the current track, since the new output may
                run at a different sample rate.
              </small>
            </div>
          </section>

          <section v-else-if="activePane === 'recommendations'" class="pane recommendations-pane">
            <div class="number-grid">
              <label v-for="field in numberFields" :key="field.key" class="number-field">
                <span>{{ field.label }}</span>
                <input
                  type="number"
                  :value="settings.preferences[field.key]"
                  :min="field.min"
                  :max="field.max"
                  step="1"
                  @change="updateNumber(field.key, ($event.target as HTMLInputElement).value, field.min, field.max)"
                />
                <small>{{ field.help }} Range {{ field.min }}–{{ field.max }}.</small>
              </label>
            </div>

            <div class="history-heading">
              <div>
                <h4>Listening history</h4>
                <p>Used to build recommendation shelves and distinguish plays from skips.</p>
              </div>
              <button class="danger-button" type="button" :disabled="busy !== null" @click="clearAllHistory">
                {{ clearHistoryArmed ? "Confirm clear all" : "Clear all history" }}
              </button>
            </div>
            <div v-if="clearHistoryArmed" class="warning" role="alert">
              Clearing history cannot be undone and will leave recommendation shelves empty until you listen again.
              <button type="button" @click="clearHistoryArmed = false">Cancel</button>
            </div>
            <p v-if="settings.historyLoading" class="empty-state">Loading listening history…</p>
            <p v-else-if="settings.history.length === 0" class="empty-state">No listening history yet.</p>
            <QueueList
              v-else
              class="history-queue"
              :items="settings.history.map((record) => record.track)"
              :current-index="null"
              :playing="player.playing"
              :reorderable="false"
              remove-label="Clear track history"
              @play="playHistory"
              @remove="clearTrackHistory(settings.history[$event].play.songId)"
              @menu="openHistoryMenu"
            >
              <template #subtitle="{ index }">
                <span>{{ settings.history[index].track?.artist || "No longer in the library" }}</span>
              </template>
              <template #meta="{ index }">
                <span class="play-kind" :class="{ skip: !settings.history[index].play.counted }">
                  {{ settings.history[index].play.counted ? "Played" : "Skipped" }}
                </span>
                <span class="history-meta">
                  {{ historyDate(settings.history[index].play.playedAt) }} ·
                  {{ duration(settings.history[index].play.secondsPlayed) }} listened
                </span>
              </template>
            </QueueList>
          </section>

          <section v-else-if="activePane === 'mixer'" class="pane">
            <div class="section-heading">
              <div>
                <h4>Mixer presets</h4>
                <p>Save the current {{ mixer.targetLabel.toLowerCase() }} mixer layer for reuse.</p>
              </div>
            </div>
            <form class="inline-form" @submit.prevent="savePreset">
              <input v-model="presetName" maxlength="60" placeholder="Preset name" aria-label="Preset name" />
              <button class="primary-button" type="submit" :disabled="!presetName.trim() || busy !== null">Save current</button>
            </form>
            <ul class="item-list preset-list">
              <li v-for="preset in visiblePresets" :key="preset.id">
                <button class="item-main" type="button" @click="editPreset(preset)">
                  <strong>{{ preset.name }}</strong>
                  <span>{{ preset.builtIn ? "Built in · click to edit a custom copy" : "Custom · click to edit" }}</span>
                </button>
                <button
                  v-if="preset.builtIn"
                  class="text-button"
                  type="button"
                  @click.stop="setBuiltInHidden('preset', preset.id, true)"
                >Hide</button>
                <button
                  v-else
                  class="text-button danger-text"
                  type="button"
                  :disabled="busy !== null"
                  @click.stop="removePreset(preset.id)"
                >Delete</button>
              </li>
            </ul>
            <details v-if="hiddenPresets.length" class="hidden-builtins">
              <summary>Hidden built-in presets ({{ hiddenPresets.length }})</summary>
              <div v-for="preset in hiddenPresets" :key="preset.id">
                <span>{{ preset.name }}</span>
                <button class="text-button" @click="setBuiltInHidden('preset', preset.id, false)">Show</button>
              </div>
            </details>

            <div class="section-heading filters-heading">
              <div>
                <h4>EQ presets</h4>
                <p>Save and manage curves used by the expanded equaliser.</p>
              </div>
            </div>
            <form class="inline-form" @submit.prevent="saveEqPreset">
              <input v-model="eqPresetName" maxlength="60" placeholder="EQ preset name" aria-label="EQ preset name" />
              <button class="primary-button" type="submit" :disabled="!eqPresetName.trim() || busy !== null">Save current EQ</button>
            </form>
            <ul class="item-list preset-list">
              <li v-for="preset in visibleEqPresets" :key="preset.id">
                <button class="item-main" type="button" @click="editPreset(preset)">
                  <strong>{{ preset.name }}</strong>
                  <span>{{ preset.builtIn ? "Built in · click to edit a custom copy" : "Custom · click to edit" }}</span>
                </button>
                <button
                  v-if="preset.builtIn"
                  class="text-button"
                  type="button"
                  @click.stop="setBuiltInHidden('preset', preset.id, true)"
                >Hide</button>
                <button
                  v-else
                  class="text-button danger-text"
                  type="button"
                  :disabled="busy !== null"
                  @click.stop="removePreset(preset.id)"
                >Delete</button>
              </li>
            </ul>
            <details v-if="hiddenEqPresets.length" class="hidden-builtins">
              <summary>Hidden built-in EQ presets ({{ hiddenEqPresets.length }})</summary>
              <div v-for="preset in hiddenEqPresets" :key="preset.id">
                <span>{{ preset.name }}</span>
                <button class="text-button" @click="setBuiltInHidden('preset', preset.id, false)">Show</button>
              </div>
            </details>

            <div class="section-heading filters-heading">
              <div>
                <h4>Atmospheres</h4>
                <p>Looping environmental audio used as optional background sound in the mixer.</p>
              </div>
              <button class="secondary-button" type="button" :disabled="busy !== null" @click="chooseFilter">Import audio…</button>
            </div>
            <p v-if="visibleFilters.length === 0" class="empty-state compact">No ambience audio imported.</p>
            <ul v-else class="item-list">
              <li v-for="filter in visibleFilters" :key="filter.id">
                <div><strong>{{ filter.name }}</strong><span>{{ filter.available ? "Available" : "File missing" }}</span></div>
                <button
                  v-if="filter.builtIn"
                  class="text-button"
                  type="button"
                  @click="setBuiltInHidden('filter', filter.id, true)"
                >Hide</button>
                <button
                  v-else
                  class="text-button danger-text"
                  type="button"
                  :disabled="busy !== null"
                  @click="removeFilter(filter.id)"
                >{{ pendingFilterDelete === filter.id ? "Confirm delete" : "Delete" }}</button>
              </li>
            </ul>
            <details v-if="hiddenFilters.length" class="hidden-builtins">
              <summary>Hidden built-in filters ({{ hiddenFilters.length }})</summary>
              <div v-for="filter in hiddenFilters" :key="filter.id">
                <span>{{ filter.name }}</span>
                <button class="text-button" @click="setBuiltInHidden('filter', filter.id, false)">Show</button>
              </div>
            </details>
            <p v-if="mixer.filtersDir" class="path-note" :title="mixer.filtersDir">Stored in {{ mixer.filtersDir }}</p>
          </section>

          <section v-else class="pane">
            <div class="section-heading">
              <div>
                <h4>Local music folders</h4>
                <p>Pick n Mix scans these folders and merges matching song files.</p>
              </div>
              <div class="button-row">
                <button class="secondary-button" type="button" :disabled="busy !== null || library.scanning" @click="chooseFolder">Add folder…</button>
                <button class="primary-button" type="button" :disabled="busy !== null || library.scanning" @click="rescan">{{ library.scanning ? "Scanning…" : "Rescan" }}</button>
              </div>
            </div>
            <p v-if="library.folders.length === 0" class="empty-state compact">No local music folders configured.</p>
            <ul v-else class="item-list folder-list">
              <li v-for="folder in library.folders" :key="folder">
                <strong :title="folder">{{ folder }}</strong>
                <button class="text-button danger-text" type="button" :disabled="busy !== null" @click="removeFolder(folder)">
                  {{ pendingFolderRemove === folder ? "Confirm remove" : "Remove" }}
                </button>
              </li>
            </ul>
            <p v-if="library.lastReport" class="scan-report">
              Last scan: {{ library.lastReport.scanned }} scanned, {{ library.lastReport.added }} added, {{ library.lastReport.updated }} updated<span v-if="library.lastReport.errors.length">, {{ library.lastReport.errors.length }} errors</span>.
            </p>

            <div class="remote-heading">
              <p class="eyebrow">Preview</p>
              <h4>Remote libraries</h4>
              <p>Planned source sync will merge remote songs with local matches. Connections will require credentials stored securely in the system keychain.</p>
            </div>
            <div class="remote-grid">
              <article v-for="service in ['Navidrome', 'Jellyfin']" :key="service" class="remote-card">
                <div><strong>{{ service }}</strong><span class="status-tag">Not available yet</span></div>
                <p>Sync a remote music library without replacing your local collection.</p>
                <button type="button" disabled>Connect</button>
              </article>
            </div>
          </section>
        </main>
      </div>
    </section>
    <Transition name="slide-panel">
      <AdvancedMixer v-if="presetEditor.session" class="preset-editor" mode="preset" />
    </Transition>
    </div>
  </div>
</template>

<style scoped>
.settings-scrim {
  position: fixed;
  inset: 0;
  z-index: 520;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 22px;
  background: rgba(0, 0, 0, 0.34);
  backdrop-filter: blur(4px);
}

.settings-workspace {
  position: relative;
  display: flex;
  width: min(900px, 100%);
  height: min(650px, calc(100vh - 44px));
  transition: width 0.2s var(--ease);
}

.settings-workspace.is-editing {
  width: min(calc(900px + var(--mixer-width)), 100%);
}

.settings-modal {
  width: 100%;
  min-width: 0;
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 0.5px solid var(--separator);
  border-radius: var(--radius-lg);
  outline: none;
  background: var(--bg-elevated);
  box-shadow: var(--shadow-popover);
  color: var(--text);
}

.settings-header {
  min-height: 68px;
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 13px 16px 12px 20px;
  border-bottom: 1px solid var(--separator);
}

.settings-header > div { flex: 1; }
.settings-header h2, .pane-heading h3 { margin: 1px 0 0; font-size: 19px; font-weight: 650; }
.eyebrow { margin: 0; color: var(--accent); font-size: 10.5px; font-weight: 650; letter-spacing: 0.06em; text-transform: uppercase; }
.saving { color: var(--text-tertiary); font-size: 11.5px; }

.settings-layout { min-height: 0; flex: 1; display: grid; grid-template-columns: 190px minmax(0, 1fr); }
/* Matches the app's own sidebar rather than inventing a second one: same
   surface, same radius, same accent-tinted active row. */
.settings-nav { padding: 14px 10px; border-right: 1px solid var(--separator); background: var(--bg-sidebar); }
.settings-nav button { width: 100%; display: flex; flex-direction: column; gap: 2px; padding: 8px 10px; border-radius: var(--radius-sm); text-align: left; color: var(--text); }
.settings-nav button:hover { background: var(--bg-hover); }
.settings-nav button.active { color: var(--accent); background: var(--accent-tint); }
.settings-nav button.active span { color: var(--accent); opacity: 0.75; }
.settings-nav strong { font-size: 13.5px; font-weight: 500; }
.settings-nav span { color: var(--text-tertiary); font-size: 10.5px; }

.settings-content { overflow: auto; padding: 22px clamp(22px, 5vw, 52px) 34px; }
.pane-heading { max-width: 610px; margin: 0 auto 22px; }
.pane { max-width: 610px; margin: 0 auto; }
.pane h4 { margin: 0; font-size: 13px; font-weight: 650; }
.pane p { color: var(--text-secondary); }
.segmented { display: grid; grid-template-columns: repeat(3, 1fr); margin-top: 10px; padding: 3px; border-radius: var(--radius); background: var(--control-track); }
.segmented button { padding: 8px; border-radius: var(--radius-sm); color: var(--text-secondary); font-size: 12px; }
.segmented button.active { background: var(--bg-elevated); color: var(--text); box-shadow: 0 1px 4px rgba(0, 0, 0, 0.12); }
.section-heading { display: flex; align-items: center; justify-content: space-between; gap: 16px; margin-top: 27px; }
.section-heading p, .setting-row p, .remote-heading p { margin: 4px 0 0; font-size: 11.5px; line-height: 1.45; }
.accent-row { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; margin-top: 13px; }
.swatch { width: 28px; height: 28px; border-radius: 50%; border: 2px solid transparent; box-shadow: inset 0 0 0 1px rgba(255,255,255,.2); }
.swatch.active { border-color: var(--text); box-shadow: inset 0 0 0 2px var(--bg-elevated); }
.custom-colour { display: flex; align-items: center; gap: 7px; color: var(--text-secondary); font-size: 11.5px; }
.custom-colour input { width: 32px; height: 28px; padding: 0; border: 0; background: transparent; }
.setting-row { min-height: 66px; display: flex; align-items: center; justify-content: space-between; gap: 24px; padding: 13px 0; border-bottom: 1px solid var(--separator); }
.setting-row.is-disabled { opacity: .65; }
.status-tag { display: inline-block; margin-left: 5px; padding: 2px 6px; border-radius: 999px; background: var(--control-track); color: var(--text-tertiary); font-size: 9px; font-weight: 600; text-transform: uppercase; }
.output-heading { margin-top: 25px; }
.field { display: flex; flex-direction: column; align-items: flex-start; gap: 7px; margin-top: 12px; font-size: 11.5px; font-weight: 600; }
.inline-form input, .number-field input {
  height: 30px; padding: 0 10px;
  border: 1px solid var(--separator); border-radius: var(--radius-sm);
  background: var(--bg-elevated); color: var(--text);
  outline: none; user-select: text;
  transition: border-color 0.15s var(--ease), box-shadow 0.15s var(--ease);
}
.inline-form input:focus, .number-field input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-tint); }
.fade-direction { margin-bottom: 8px; }
.field small, .number-field small { color: var(--text-tertiary); font-size: 10.5px; font-weight: 400; line-height: 1.35; }
.number-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 15px 20px; }
.number-field { display: grid; grid-template-columns: 1fr 82px; align-items: center; gap: 5px 10px; font-size: 11.5px; font-weight: 600; }
.number-field input { width: 82px; }
.number-field small { grid-column: 1 / -1; }
.history-heading { display: flex; justify-content: space-between; align-items: center; gap: 18px; margin-top: 28px; }
.history-heading p { margin: 4px 0 0; font-size: 11px; }
.warning { margin-top: 10px; padding: 10px 12px; border: 1px solid rgba(215,55,63,.35); border-radius: var(--radius-sm); background: rgba(215,55,63,.08); color: #d7373f; font-size: 11.5px; line-height: 1.45; }
.warning button { margin-left: 7px; color: inherit; text-decoration: underline; }
.history-list, .item-list { margin: 12px 0 0; padding: 0; list-style: none; border: 1px solid var(--separator); border-radius: var(--radius); overflow: hidden; }
.history-list li { display: grid; grid-template-columns: minmax(120px, 1fr) auto auto auto; align-items: center; gap: 10px; padding: 10px 11px; border-bottom: 1px solid var(--separator); }
.history-list li:last-child, .item-list li:last-child { border-bottom: 0; }
.history-copy { min-width: 0; display: flex; flex-direction: column; }
.history-copy strong, .history-copy span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.history-copy strong { font-size: 11.5px; }
.history-copy span, .history-meta { color: var(--text-tertiary); font-size: 10px; }
.play-kind { padding: 2px 6px; border-radius: 999px; background: var(--accent-tint); color: var(--accent); font-size: 9px; font-weight: 650; text-transform: uppercase; }
.play-kind.skip { background: var(--control-track); color: var(--text-tertiary); }
.empty-state { padding: 36px 12px; text-align: center; color: var(--text-tertiary) !important; font-size: 11.5px; }
.empty-state.compact { padding: 18px 10px; border: 1px dashed var(--separator); border-radius: var(--radius-sm); }
.inline-form { display: flex; gap: 8px; margin-top: 12px; }
.inline-form input { min-width: 0; flex: 1; }
/* The same pill vocabulary the rest of the app uses, rather than a second
   set of square buttons that only exist in this modal. */
.primary-button, .secondary-button, .danger-button, .remote-card button {
  display: inline-flex; align-items: center; justify-content: center; gap: 6px;
  height: 30px; padding: 0 14px; border-radius: 999px;
  font-size: 12.5px; font-weight: 600;
  transition: background 0.15s var(--ease), transform 0.1s var(--ease);
}
.primary-button:active, .secondary-button:active, .danger-button:active, .remote-card button:active { transform: scale(0.97); }
.primary-button { background: var(--accent); color: var(--accent-contrast); }
.primary-button:hover:not(:disabled) { background: var(--accent-hover); }
.secondary-button, .remote-card button { background: var(--bg-hover); color: var(--text); }
.secondary-button:hover:not(:disabled), .remote-card button:hover:not(:disabled) { background: var(--bg-active); }
.danger-button { background: rgba(215,55,63,.12); color: #d7373f; }
.danger-button:hover:not(:disabled) { background: rgba(215,55,63,.2); }
.primary-button:disabled, .secondary-button:disabled, .danger-button:disabled, .remote-card button:disabled { opacity: .45; pointer-events: none; }
.item-list li { min-height: 45px; display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 8px 11px; border-bottom: 1px solid var(--separator); }
.item-list li > div { display: flex; flex-direction: column; gap: 2px; }
.item-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; text-align: left; }
.hidden-builtins { margin-top: 9px; color: var(--text-secondary); font-size: 11px; }
.hidden-builtins summary { cursor: pointer; }
.hidden-builtins > div { display: flex; justify-content: space-between; padding: 7px 4px; border-bottom: 1px solid var(--separator); }
.item-list strong { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11.5px; }
.item-list span { color: var(--text-tertiary); font-size: 10px; }
.text-button { color: var(--accent); font-size: 10.5px; white-space: nowrap; }
.danger-text { color: #d7373f; }
.filters-heading { margin-top: 28px; }
.path-note, .scan-report { overflow: hidden; margin: 8px 2px 0; color: var(--text-tertiary) !important; font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
.button-row { display: flex; gap: 7px; }
.folder-list li strong { flex: 1; }
.remote-heading { margin-top: 31px; padding-top: 24px; border-top: 1px solid var(--separator); }
.remote-heading h4 { margin-top: 3px; }
.remote-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 11px; margin-top: 13px; }
.remote-card { padding: 13px; border: 1px solid var(--separator); border-radius: var(--radius); background: var(--bg-sunken); }
.remote-card > div { display: flex; align-items: center; justify-content: space-between; gap: 7px; }
.remote-card p { min-height: 34px; margin: 8px 0 12px; font-size: 10.5px; line-height: 1.4; }
.remote-card button { width: 100%; }

.history-queue { margin-top: 12px; }
.history-queue :deep(.row) { padding-left: 6px; }
.history-queue :deep(.row__text) { max-width: 190px; }
.history-queue .history-meta { max-width: 170px; color: var(--text-tertiary); font-size: 10px; }
.preset-editor { height: 100%; border: 0.5px solid var(--separator); border-radius: 0 var(--radius-lg) var(--radius-lg) 0; box-shadow: var(--shadow-popover); overflow: hidden; }
.settings-workspace.is-editing .settings-modal { border-radius: var(--radius-lg) 0 0 var(--radius-lg); }

@media (max-width: 1000px) {
  .settings-workspace.is-editing { width: min(900px, 100%); }
  .preset-editor { position: absolute; z-index: 2; top: 0; right: 0; width: min(var(--mixer-width), 92vw); }
}

@media (max-width: 700px) {
  .settings-scrim { padding: 10px; }
  .settings-workspace { height: calc(100vh - 20px); }
  .settings-modal { height: 100%; }
  .settings-layout { grid-template-columns: 1fr; grid-template-rows: auto minmax(0, 1fr); }
  .settings-nav { display: flex; gap: 4px; overflow-x: auto; padding: 7px 8px; border-right: 0; border-bottom: 1px solid var(--separator); }
  .settings-nav button { width: auto; flex: 0 0 auto; padding: 8px 10px; }
  .settings-nav span { display: none; }
  .settings-content { padding: 18px 17px 28px; }
  .number-grid, .remote-grid { grid-template-columns: 1fr; }
  .history-list li { grid-template-columns: minmax(100px, 1fr) auto auto; }
  .history-meta { grid-column: 1 / 3; }
  .section-heading { align-items: flex-start; flex-direction: column; }
}

@media (max-width: 430px) {
  .settings-header { min-height: 58px; }
  .pane-heading { margin-bottom: 17px; }
  .history-heading { align-items: flex-start; flex-direction: column; }
  .history-list li { grid-template-columns: 1fr auto; }
  .history-meta { grid-column: 1; }
  .setting-row { align-items: flex-start; }
  .button-row { width: 100%; }
  .button-row button { flex: 1; }
}
</style>
