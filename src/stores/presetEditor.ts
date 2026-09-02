import { computed, ref } from "vue";
import { defineStore } from "pinia";
import * as api from "@/lib/api";
import { clone, resolve, type Section } from "@/lib/mixer";
import { useMixerStore } from "@/stores/mixer";
import type { MixerSettings, Preset } from "@/lib/types";

interface PresetSession {
  sourceId: string;
  sourceName: string;
  sourceBuiltIn: boolean;
  name: string;
  original: MixerSettings;
  draft: MixerSettings;
}

function withoutDisplayMetadata(settings: MixerSettings): MixerSettings {
  const clean = clone(settings);
  delete clean.preset;
  return clean;
}

export const usePresetEditorStore = defineStore("presetEditor", () => {
  const session = ref<PresetSession | null>(null);
  const saving = ref(false);

  const effective = computed(() => resolve([session.value?.draft ?? {}]));
  const dirty = computed(() => {
    const current = session.value;
    if (!current) return false;
    return current.name.trim() !== current.sourceName ||
      JSON.stringify(current.draft) !== JSON.stringify(current.original);
  });

  function open(preset: Preset) {
    const settings = withoutDisplayMetadata(preset.settings);
    session.value = {
      sourceId: preset.id,
      sourceName: preset.builtIn ? `${preset.name} Custom` : preset.name,
      sourceBuiltIn: preset.builtIn,
      name: preset.builtIn ? `${preset.name} Custom` : preset.name,
      original: clone(settings),
      draft: clone(settings),
    };
  }

  function close() {
    session.value = null;
  }

  function setEnabled(enabled: boolean) {
    if (!session.value) return;
    session.value.draft = { ...session.value.draft, enabled };
  }

  function setSection<K extends Section>(section: K, value: MixerSettings[K]) {
    if (!session.value) return;
    session.value.draft = { ...session.value.draft, [section]: value };
  }

  function clearSection(section: Section) {
    if (!session.value) return;
    const next = { ...session.value.draft };
    delete next[section];
    session.value.draft = next;
  }

  function toggleFilter(id: string, enabled: boolean) {
    const current = effective.value.filters.filter((filter) => filter.id !== id);
    const existing = effective.value.filters.find((filter) => filter.id === id);
    const next = [
      ...current,
      { id, enabled, volume: existing?.volume ?? 0.4, toneHz: existing?.toneHz ?? 20_000 },
    ].filter((filter) => filter.enabled || filter.volume !== 0.4);
    setSection("filters", next);
  }

  function setFilterVolume(id: string, volume: number) {
    const next = effective.value.filters.map((filter) =>
      filter.id === id ? { ...filter, volume } : filter,
    );
    if (!next.some((filter) => filter.id === id)) {
      next.push({ id, enabled: true, volume, toneHz: 20_000 });
    }
    setSection("filters", next);
  }

  async function save() {
    const current = session.value;
    if (!current) return;
    const name = current.name.trim();
    if (!name) throw new Error("Preset name cannot be empty");
    const settings = withoutDisplayMetadata(current.draft);
    saving.value = true;
    try {
      const presets = current.sourceBuiltIn
        ? await api.savePreset(name, settings)
        : await api.updatePreset(current.sourceId, name, settings);
      const mixer = useMixerStore();
      mixer.presets = presets;
      const saved = presets.find((preset) => !preset.builtIn && preset.name === name);
      if (!saved) throw new Error("The saved preset was not returned by the backend");
      session.value = {
        sourceId: saved.id,
        sourceName: saved.name,
        sourceBuiltIn: false,
        name: saved.name,
        original: clone(settings),
        draft: clone(settings),
      };
    } finally {
      saving.value = false;
    }
  }

  return {
    session,
    saving,
    effective,
    dirty,
    open,
    close,
    setEnabled,
    setSection,
    clearSection,
    toggleFilter,
    setFilterVolume,
    save,
  };
});
