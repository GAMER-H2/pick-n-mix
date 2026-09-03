<script setup lang="ts">
/**
 * EQ preset picker for the expanded EQ modal: built-in curves, the user's own,
 * and save/delete — all rendered by the shared `ui/PresetSelect`, with the
 * mixer store wiring kept here.
 */
import { computed, ref } from "vue";
import PresetSelect from "../ui/PresetSelect.vue";
import type { MenuItem } from "../ui/MenuSurface.vue";
import { EQ_PRESETS, eqPresetById, eqValuesEqual } from "@/lib/eqPresets";
import { clone } from "@/lib/mixer";
import { useMixerStore } from "@/stores/mixer";
import { useSettingsStore } from "@/stores/settings";
import { useUiStore } from "@/stores/ui";
import type { Eq } from "@/lib/types";

const props = defineProps<{ eq: Eq }>();
const emit = defineEmits<{ change: [eq: Eq] }>();

const mixer = useMixerStore();
const settings = useSettingsStore();
const ui = useUiStore();
const select = ref<InstanceType<typeof PresetSelect> | null>(null);

const currentId = computed(() => {
  const builtIn = EQ_PRESETS.find((preset) => eqValuesEqual(preset.eq, props.eq));
  if (builtIn) return builtIn.id;
  return customPresets.value.find((preset) =>
    preset.settings.eq && eqValuesEqual(preset.settings.eq, props.eq),
  )?.id ?? "";
});

const builtIns = computed<MenuItem[]>(() =>
  EQ_PRESETS.filter((preset) =>
    !settings.preferences.hiddenBuiltInPresetIds.includes(preset.id),
  ).map((preset) => ({
    id: preset.id,
    label: preset.name,
    checked: preset.id === currentId.value,
  })),
);

const customPresets = computed(() =>
  mixer.presets.filter((preset) => preset.kind === "eq" && !preset.builtIn && preset.settings.eq),
);

const custom = computed<MenuItem[]>(() =>
  customPresets.value.map((preset) => ({
    id: preset.id,
    label: preset.name,
    checked: preset.id === currentId.value,
  })),
);

const currentName = computed(() => {
  const id = currentId.value;
  return EQ_PRESETS.find((preset) => preset.id === id)?.name
    ?? customPresets.value.find((preset) => preset.id === id)?.name
    ?? "EQ Preset Select";
});

function onSelect(id: string) {
  const customPreset = customPresets.value.find((preset) => preset.id === id);
  if (customPreset?.settings.eq) {
    emit("change", clone(customPreset.settings.eq));
    return;
  }
  const builtIn = eqPresetById(id);
  if (builtIn) emit("change", builtIn);
}

async function save(name: string) {
  try {
    await mixer.saveEqPreset(name, props.eq);
    select.value?.closeSaveRow();
    ui.notify(`Saved EQ preset "${name}"`);
  } catch (error) {
    ui.notify(`Could not save EQ preset: ${error}`, "error");
  }
}

async function remove(id: string) {
  try {
    await mixer.removePreset(id);
  } catch (error) {
    ui.notify(`Could not delete EQ preset: ${error}`, "error");
  }
}
</script>

<template>
  <PresetSelect
    ref="select"
    :label="currentName"
    :groups="[{ label: 'Built In', items: builtIns }]"
    :custom="custom"
    save-placeholder="EQ preset name"
    save-action-label="Save current EQ…"
    delete-label="Delete EQ preset"
    @select="onSelect"
    @delete="remove"
    @save="save"
  />
</template>
