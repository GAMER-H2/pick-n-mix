<script setup lang="ts">
/**
 * The "Preset Select" control at the top of both mixer views: applies mixer
 * presets, saves the current settings as one, and deletes custom presets —
 * all rendered by the shared `ui/PresetSelect`.
 */
import { computed, ref } from "vue";
import PresetSelect from "../ui/PresetSelect.vue";
import type { MenuItem } from "../ui/MenuSurface.vue";
import { presetSections } from "@/lib/mixer";
import { useMixerStore } from "@/stores/mixer";
import { useSettingsStore } from "@/stores/settings";
import { useUiStore } from "@/stores/ui";

const mixer = useMixerStore();
const settings = useSettingsStore();
const ui = useUiStore();
const select = ref<InstanceType<typeof PresetSelect> | null>(null);

const current = computed(() => mixer.targetLayer.preset as string | undefined);
const visiblePresets = computed(() => mixer.presets.filter((preset) =>
  preset.kind === "mixer"
  && (!preset.builtIn || !settings.preferences.hiddenBuiltInPresetIds.includes(preset.id)),
));

const builtIns = computed<MenuItem[]>(() =>
  visiblePresets.value.filter((preset) => preset.builtIn).map((preset) => ({
    id: preset.id,
    label: preset.name,
    checked: current.value === preset.name,
  })),
);

const custom = computed<MenuItem[]>(() =>
  visiblePresets.value.filter((preset) => !preset.builtIn).map((preset) => ({
    id: preset.id,
    label: preset.name,
    checked: current.value === preset.name,
  })),
);

async function choose(id: string) {
  const preset = mixer.presets.find((p) => p.id === id);
  if (!preset) return;
  await mixer.applyPreset(preset);
  const touched = presetSections(preset.settings);
  ui.notify(`Applied "${preset.name}" (${touched.join(", ") || "no changes"})`);
}

async function save(name: string) {
  await mixer.saveAsPreset(name);
  select.value?.closeSaveRow();
  ui.notify(`Saved preset "${name}"`);
}

async function remove(id: string) {
  await mixer.removePreset(id);
}
</script>

<template>
  <PresetSelect
    ref="select"
    :label="current || 'Preset Select'"
    :groups="[{ label: 'Built In', items: builtIns }]"
    :custom="custom"
    save-placeholder="Preset name"
    save-action-label="Save current settings…"
    delete-label="Delete preset"
    stretch
    @select="choose"
    @delete="remove"
    @save="save"
  />
</template>
