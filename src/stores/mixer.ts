import { defineStore } from "pinia";
import { computed, ref } from "vue";
import * as api from "@/lib/api";
import { DEFAULTS, clone, resolve, type Section } from "@/lib/mixer";
import type { Eq, FilterInfo, MixerSettings, Preset, ResolvedMixer } from "@/lib/types";

/**
 * Which layer of the cascade the mixer UI is editing.
 *
 * The bottom bar's mixer edits `global`; the button on a playlist header edits
 * that playlist's layer; the button on a track row edits that track's layer.
 */
export type Target =
  | { kind: "global" }
  | { kind: "playlist"; id: string; name: string }
  /** One entry of one playlist. `index` addresses it inside the file. */
  | { kind: "entry"; playlistId: string; index: number; name: string }
  /** One audio block on a playlist's master mix timeline. */
  | { kind: "block"; playlistId: string; blockId: string; name: string };

export const useMixerStore = defineStore("mixer", () => {
  const global = ref<MixerSettings>({});
  const presets = ref<Preset[]>([]);
  const filters = ref<FilterInfo[]>([]);
  const filtersDir = ref("");

  /** The layer currently being edited, and the layers beneath it. */
  const target = ref<Target>({ kind: "global" });
  const targetLayer = ref<MixerSettings>({});
  const underlyingLayers = ref<MixerSettings[]>([]);

  const panelOpen = ref(false);
  const popoverOpen = ref(false);

  /** What the user hears with the current edits applied. */
  const effective = computed<ResolvedMixer>(() =>
    resolve([...underlyingLayers.value, targetLayer.value]),
  );

  const targetLabel = computed(() => {
    switch (target.value.kind) {
      case "global":
        return "All Playback";
      case "playlist":
        return target.value.name;
      case "entry":
        return target.value.name;
      case "block":
        return target.value.name;
    }
  });

  const overriddenSections = computed<Section[]>(() =>
    (Object.keys(DEFAULTS) as Section[]).filter(
      (s) => targetLayer.value[s] !== null && targetLayer.value[s] !== undefined,
    ),
  );

  /**
   * Re-read only the cascade. Presets and the filter catalogue come from disk
   * and cannot have changed just because the track did.
   */
  async function refreshLayers() {
    const layers = await api.mixerLayers();
    global.value = layers.global;
    if (target.value.kind === "global") {
      targetLayer.value = clone(layers.global);
      underlyingLayers.value = [];
    }
  }

  async function refresh() {
    const state = await api.mixerState();
    global.value = state.global;
    presets.value = state.presets;
    filters.value = state.filters;
    if (target.value.kind === "global") {
      targetLayer.value = clone(state.global);
      underlyingLayers.value = [];
    }
    if (!filtersDir.value) filtersDir.value = await api.filtersDirectory();
  }

  /** Point the mixer UI at a different layer. */
  async function editGlobal() {
    const state = await api.mixerState();
    global.value = state.global;
    presets.value = state.presets;
    filters.value = state.filters;
    target.value = { kind: "global" };
    targetLayer.value = clone(state.global);
    underlyingLayers.value = [];
  }

  async function editPlaylist(id: string, name: string, mixer: MixerSettings | null) {
    await refresh();
    target.value = { kind: "playlist", id, name };
    targetLayer.value = mixer ? clone(mixer) : {};
    underlyingLayers.value = [global.value];
  }

  /**
   * Edit one song's override inside one playlist. The override is stored in
   * that playlist's file, so the same song elsewhere is untouched.
   */
  async function editPlaylistEntry(
    playlistId: string,
    index: number,
    name: string,
    mixer: MixerSettings | null,
    playlistMixer: MixerSettings | null = null,
  ) {
    await refresh();
    target.value = { kind: "entry", playlistId, index, name };
    targetLayer.value = mixer ? clone(mixer) : {};
    // Keep an explicit empty playlist layer so the frontend resolver can
    // consistently exclude the entry layer from crossfade resolution.
    underlyingLayers.value = [global.value, playlistMixer ?? {}];
  }

  /** Effects for one block on the master mix, layered over global + playlist. */
  async function editMixBlock(
    playlistId: string,
    blockId: string,
    name: string,
    mixer: MixerSettings | null,
    playlistMixer: MixerSettings | null = null,
  ) {
    await refresh();
    target.value = { kind: "block", playlistId, blockId, name };
    targetLayer.value = mixer ? clone(mixer) : {};
    underlyingLayers.value = [global.value, playlistMixer ?? {}];
  }

  /** Write a whole section into the layer being edited, then persist. */
  async function setSection<K extends Section>(section: K, value: MixerSettings[K]) {
    targetLayer.value = { ...targetLayer.value, [section]: value };
    await persist();
  }

  /** Drop an override so the section falls through to the layer below again. */
  async function clearSection(section: Section) {
    const next = { ...targetLayer.value };
    delete next[section];
    targetLayer.value = next;
    await persist();
  }

  async function setEnabled(enabled: boolean) {
    targetLayer.value = { ...targetLayer.value, enabled };
    await persist();
  }

  /** Clear every override in this layer. */
  async function resetLayer() {
    targetLayer.value = target.value.kind === "global" ? {} : {};
    await persist();
  }

  async function persist() {
    const layer = clone(targetLayer.value);
    switch (target.value.kind) {
      case "global":
        global.value = layer;
        await api.setGlobalMixer(layer);
        break;
      case "playlist":
        await api.setPlaylistMixer(
          target.value.id,
          Object.keys(layer).length ? layer : null,
        );
        break;
      case "entry":
        await api.setPlaylistEntryMixer(
          target.value.playlistId,
          target.value.index,
          Object.keys(layer).length ? layer : null,
        );
        break;
      case "block": {
        const { useMasterMixStore } = await import("./masterMix");
        useMasterMixStore().setBlockMixer(
          target.value.blockId,
          Object.keys(layer).length ? layer : null,
        );
        break;
      }
    }
  }

  /** Layer a preset on top of the current edits, touching only its sections. */
  async function applyPreset(preset: Preset) {
    const settings = clone(preset.settings);
    // A crossfade spans playlist entries, so an entry preset may not introduce
    // one even if that preset carries a crossfade setting.
    if (target.value.kind === "entry" || target.value.kind === "block") delete settings.crossfade;
    // Timeline voices do not currently schedule varispeed or atmosphere beds.
    // Do not persist controls that the selected audio block cannot render.
    if (target.value.kind === "block") {
      delete settings.pitch;
      delete settings.filters;
    }
    targetLayer.value = { ...targetLayer.value, ...settings, preset: preset.name };
    await persist();
  }

  async function saveAsPreset(name: string) {
    presets.value = await api.savePreset(name, clone(targetLayer.value), "mixer");
  }

  async function saveEqPreset(name: string, eq: Eq) {
    presets.value = await api.savePreset(name, { eq: clone(eq) }, "eq");
  }

  async function removePreset(id: string) {
    presets.value = await api.deletePreset(id);
  }

  /** Turn an ambience bed on or off, keeping the rest of the list intact. */
  async function toggleFilter(id: string, enabled: boolean) {
    const current = effective.value.filters.filter((f) => f.id !== id);
    const existing = effective.value.filters.find((f) => f.id === id);
    const next = [
      ...current,
      { id, enabled, volume: existing?.volume ?? 0.4, toneHz: existing?.toneHz ?? 20000 },
    ].filter((f) => f.enabled || f.volume !== 0.4);
    await setSection("filters", next);
  }

  async function setFilterVolume(id: string, volume: number) {
    const next = effective.value.filters.map((f) => (f.id === id ? { ...f, volume } : f));
    if (!next.some((f) => f.id === id)) {
      next.push({ id, enabled: true, volume, toneHz: 20000 });
    }
    await setSection("filters", next);
  }

  return {
    global,
    presets,
    filters,
    filtersDir,
    target,
    targetLayer,
    underlyingLayers,
    panelOpen,
    popoverOpen,
    effective,
    targetLabel,
    overriddenSections,
    refresh,
    refreshLayers,
    editGlobal,
    editPlaylist,
    editPlaylistEntry,
    editMixBlock,
    setSection,
    clearSection,
    setEnabled,
    resetLayer,
    applyPreset,
    saveAsPreset,
    saveEqPreset,
    removePreset,
    toggleFilter,
    setFilterVolume,
  };
});
