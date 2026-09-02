<script setup lang="ts">
import { computed, ref } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
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
const open = ref(false);
const naming = ref(false);
const draftName = ref("");

const builtIns = computed(() => EQ_PRESETS.filter((preset) =>
  !settings.preferences.hiddenBuiltInPresetIds.includes(preset.id),
));
const custom = computed(() => mixer.presets.filter((preset) =>
  preset.kind === "eq" && !preset.builtIn && preset.settings.eq,
));
const currentId = computed(() => {
  const builtIn = builtIns.value.find((preset) => eqValuesEqual(preset.eq, props.eq));
  if (builtIn) return builtIn.id;
  return custom.value.find((preset) =>
    preset.settings.eq && eqValuesEqual(preset.settings.eq, props.eq),
  )?.id ?? "";
});
const currentName = computed(() => {
  const id = currentId.value;
  return builtIns.value.find((preset) => preset.id === id)?.name
    ?? custom.value.find((preset) => preset.id === id)?.name
    ?? "EQ Preset Select";
});

function chooseBuiltIn(id: string) {
  const eq = eqPresetById(id);
  if (!eq) return;
  open.value = false;
  emit("change", eq);
}

function chooseCustom(id: string) {
  const eq = custom.value.find((preset) => preset.id === id)?.settings.eq;
  if (!eq) return;
  open.value = false;
  emit("change", clone(eq));
}

async function save() {
  const name = draftName.value.trim();
  if (!name) return;
  try {
    await mixer.saveEqPreset(name, props.eq);
    naming.value = false;
    draftName.value = "";
    ui.notify(`Saved EQ preset "${name}"`);
  } catch (error) {
    ui.notify(`Could not save EQ preset: ${error}`, "error");
  }
}

async function remove(id: string, event: Event) {
  event.stopPropagation();
  try {
    await mixer.removePreset(id);
  } catch (error) {
    ui.notify(`Could not delete EQ preset: ${error}`, "error");
  }
}
</script>

<template>
  <div class="preset">
    <button class="preset__button" aria-haspopup="menu" :aria-expanded="open" @click="open = !open">
      <span class="truncate">{{ currentName }}</span>
      <PnmIcon name="chevronDown" :size="14" />
    </button>

    <Transition name="pop">
      <div v-if="open" class="preset__menu" role="menu" aria-label="EQ presets">
        <div class="preset__group">Built In</div>
        <button
          v-for="preset in builtIns"
          :key="preset.id"
          class="preset__item"
          role="menuitem"
          @click="chooseBuiltIn(preset.id)"
        >
          <span class="truncate">{{ preset.name }}</span>
          <PnmIcon v-if="currentId === preset.id" name="check" :size="14" />
        </button>

        <template v-if="custom.length">
          <div class="preset__group">Yours</div>
          <button
            v-for="preset in custom"
            :key="preset.id"
            class="preset__item"
            role="menuitem"
            @click="chooseCustom(preset.id)"
          >
            <span class="truncate">{{ preset.name }}</span>
            <span class="preset__actions">
              <PnmIcon v-if="currentId === preset.id" name="check" :size="14" />
              <span class="preset__delete" title="Delete EQ preset" @click="remove(preset.id, $event)">
                <PnmIcon name="trash" :size="13" />
              </span>
            </span>
          </button>
        </template>

        <div class="preset__separator" />
        <div v-if="naming" class="preset__save">
          <input
            v-model="draftName"
            class="text-field"
            placeholder="EQ preset name"
            aria-label="EQ preset name"
            autofocus
            @keydown.enter="save"
            @keydown.esc="naming = false"
          />
          <button class="pill-button" @click="save">Save</button>
        </div>
        <button v-else class="preset__item" role="menuitem" @click="naming = true">
          <PnmIcon name="plus" :size="14" />
          <span>Save current EQ…</span>
        </button>
      </div>
    </Transition>

    <div v-if="open" class="preset__scrim" @click="open = false" />
  </div>
</template>

<style scoped>
.preset { position: relative; flex: none; }
.preset__button { display: flex; align-items: center; justify-content: space-between; gap: 8px; min-width: 160px; height: 30px; padding: 0 10px; border-radius: var(--radius-sm); border: 1px solid var(--separator); background: var(--bg-elevated); font-size: 12.5px; color: var(--text); }
.preset__button:hover { border-color: var(--separator-strong); }
.preset__menu { position: absolute; z-index: 320; top: calc(100% + 5px); right: 0; width: 230px; max-height: min(360px, 70vh); overflow-y: auto; padding: 5px; border-radius: var(--radius); background: var(--bg-elevated); border: 0.5px solid var(--separator); box-shadow: var(--shadow-popover); }
.preset__scrim { position: fixed; inset: 0; z-index: 310; }
.preset__group { padding: 6px 9px 3px; font-size: 10px; font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; color: var(--text-tertiary); }
.preset__item { display: flex; align-items: center; justify-content: space-between; gap: 8px; width: 100%; padding: 6px 9px; border-radius: var(--radius-sm); font-size: 12.5px; text-align: left; }
.preset__item:hover { background: var(--bg-hover); }
.preset__actions { display: inline-flex; align-items: center; gap: 6px; }
.preset__delete { display: inline-flex; color: var(--text-tertiary); opacity: 0; }
.preset__item:hover .preset__delete, .preset__delete:focus-visible { opacity: 1; }
.preset__delete:hover { color: #d7373f; }
.preset__separator { height: 1px; margin: 5px 8px; background: var(--separator); }
.preset__save { display: flex; gap: 6px; padding: 4px; }
.preset__save input { min-width: 0; }
</style>
