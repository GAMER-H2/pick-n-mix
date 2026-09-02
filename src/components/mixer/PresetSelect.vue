<script setup lang="ts">
/** The "Preset Select" control at the top of both mixer views. */
import { computed, ref } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import { presetSections } from "@/lib/mixer";
import { useMixerStore } from "@/stores/mixer";
import { useSettingsStore } from "@/stores/settings";
import { useUiStore } from "@/stores/ui";

const mixer = useMixerStore();
const settings = useSettingsStore();
const ui = useUiStore();
const open = ref(false);
const naming = ref(false);
const draftName = ref("");

const current = computed(() => mixer.targetLayer.preset as string | undefined);
const visiblePresets = computed(() => mixer.presets.filter((preset) =>
  preset.kind === "mixer"
  && (!preset.builtIn || !settings.preferences.hiddenBuiltInPresetIds.includes(preset.id)),
));

async function choose(id: string) {
  const preset = mixer.presets.find((p) => p.id === id);
  if (!preset) return;
  open.value = false;
  await mixer.applyPreset(preset);
  const touched = presetSections(preset.settings);
  ui.notify(`Applied "${preset.name}" (${touched.join(", ") || "no changes"})`);
}

async function save() {
  const name = draftName.value.trim();
  if (!name) return;
  await mixer.saveAsPreset(name);
  naming.value = false;
  draftName.value = "";
  ui.notify(`Saved preset "${name}"`);
}

async function remove(id: string, event: Event) {
  event.stopPropagation();
  await mixer.removePreset(id);
}
</script>

<template>
  <div class="preset">
    <button class="preset__button" @click="open = !open">
      <span class="truncate">{{ current || "Preset Select" }}</span>
      <PnmIcon name="chevronDown" :size="14" />
    </button>

    <Transition name="pop">
      <div v-if="open" class="preset__menu">
        <div class="preset__group">Built In</div>
        <button
          v-for="preset in visiblePresets.filter((p) => p.builtIn)"
          :key="preset.id"
          class="preset__item"
          @click="choose(preset.id)"
        >
          <span class="truncate">{{ preset.name }}</span>
          <PnmIcon v-if="current === preset.name" name="check" :size="14" />
        </button>

        <template v-if="visiblePresets.some((p) => !p.builtIn)">
          <div class="preset__group">Yours</div>
          <button
            v-for="preset in visiblePresets.filter((p) => !p.builtIn)"
            :key="preset.id"
            class="preset__item"
            @click="choose(preset.id)"
          >
            <span class="truncate">{{ preset.name }}</span>
            <span class="preset__delete" title="Delete preset" @click="remove(preset.id, $event)">
              <PnmIcon name="trash" :size="13" />
            </span>
          </button>
        </template>

        <div class="preset__separator" />
        <div v-if="naming" class="preset__save">
          <input
            v-model="draftName"
            class="text-field"
            placeholder="Preset name"
            autofocus
            @keydown.enter="save"
            @keydown.esc="naming = false"
          />
          <button class="pill-button" @click="save">Save</button>
        </div>
        <button v-else class="preset__item" @click="naming = true">
          <PnmIcon name="plus" :size="14" />
          <span>Save current settings…</span>
        </button>
      </div>
    </Transition>

    <div v-if="open" class="preset__scrim" @click="open = false" />
  </div>
</template>

<style scoped>
.preset {
  position: relative;
}

.preset__button {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
  height: 30px;
  padding: 0 10px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--separator);
  background: var(--bg-elevated);
  font-size: 12.5px;
  color: var(--text);
}

.preset__button:hover {
  border-color: var(--separator-strong);
}

.preset__menu {
  position: absolute;
  z-index: 20;
  top: calc(100% + 5px);
  left: 0;
  right: 0;
  max-height: 320px;
  overflow-y: auto;
  padding: 5px;
  border-radius: var(--radius);
  background: var(--bg-elevated);
  border: 0.5px solid var(--separator);
  box-shadow: var(--shadow-popover);
}

.preset__scrim {
  position: fixed;
  inset: 0;
  z-index: 10;
}

.preset__group {
  padding: 6px 9px 3px;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--text-tertiary);
}

.preset__item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
  padding: 6px 9px;
  border-radius: var(--radius-sm);
  font-size: 12.5px;
  text-align: left;
}

.preset__item:hover {
  background: var(--bg-hover);
}

.preset__delete {
  display: inline-flex;
  color: var(--text-tertiary);
  opacity: 0;
}

.preset__item:hover .preset__delete {
  opacity: 1;
}

.preset__delete:hover {
  color: #d7373f;
}

.preset__separator {
  height: 1px;
  margin: 5px 8px;
  background: var(--separator);
}

.preset__save {
  display: flex;
  gap: 6px;
  padding: 4px;
}
</style>
