<script setup lang="ts">
/**
 * The Master Mixer's "Effects" button: everything the app can put on a region,
 * in one menu.
 *
 * The arrangement used to borrow the DJ sidebar for this, which meant leaving
 * the timeline to reach a region's reverb. A mix is a mini-DAW, so effects are
 * picked from a menu here and laid out as a chain along the bottom instead —
 * see `BlockEffectsRack`.
 *
 * Effects already on the block are ticked and cannot be added twice: a second
 * reverb on one region would be a second set of controls for the same effect.
 */
import { nextTick, ref } from "vue";
import MenuSurface, { type MenuGroup } from "../ui/MenuSurface.vue";
import { useDismiss } from "@/lib/dismiss";
import { DEFAULT_CHAIN_ORDER, PINNED_DEVICES, SECTION_LABELS } from "@/lib/mixer";
import type { DeviceSection } from "@/lib/mixer";

const props = withDefaults(
  defineProps<{
    /** Devices the selected block already has. */
    present: DeviceSection[];
    /** No single block is selected, so there is nothing to add an effect to. */
    disabled?: boolean;
  }>(),
  { disabled: false },
);

const emit = defineEmits<{ add: [section: DeviceSection] }>();

const open = ref(false);
const menuEl = ref<HTMLElement | null>(null);
const buttonEl = ref<HTMLElement | null>(null);

// The trigger is ignored so its own click still toggles the menu shut.
useDismiss(
  () => open.value,
  () => (open.value = false),
  menuEl,
  { ignore: [buttonEl] },
);

function group(label: string, sections: DeviceSection[]): MenuGroup {
  return {
    label,
    items: sections.map((section) => ({
      id: section,
      label: SECTION_LABELS[section],
      checked: props.present.includes(section),
      disabled: props.present.includes(section),
    })),
  };
}

const groups: MenuGroup[] = [
  group("Chain", DEFAULT_CHAIN_ORDER),
  group("Region", PINNED_DEVICES),
];

async function toggle() {
  if (props.disabled) return;
  open.value = !open.value;
  if (!open.value) return;
  await nextTick();
  menuEl.value?.querySelector<HTMLElement>("[role='menuitem']:not([disabled])")?.focus();
}

function onSelect(id: string) {
  open.value = false;
  emit("add", id as DeviceSection);
}
</script>

<template>
  <div class="effects-menu">
    <button
      ref="buttonEl"
      class="effects-menu__trigger"
      type="button"
      :disabled="disabled"
      :aria-expanded="open"
      aria-haspopup="menu"
      :title="
        disabled
          ? 'Select a single block to add an effect to it'
          : 'Add an effect to the selected block'
      "
      @click="toggle"
    >
      Effects
    </button>

    <Transition name="pop">
      <div v-if="open" ref="menuEl" class="effects-menu__menu">
        <MenuSurface :groups="groups" @select="onSelect" />
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.effects-menu {
  position: relative;
}

/* Matches the modal's other header buttons; those styles are not scoped to
   this component, so the trigger carries its own copy of the same tokens. */
.effects-menu__trigger {
  padding: 5px 14px;
  border: 0.5px solid var(--separator-strong);
  border-radius: 999px;
  font-size: 12px;
  color: var(--text);
}

.effects-menu__trigger:hover:not(:disabled) {
  background: var(--bg-hover);
}

.effects-menu__trigger:disabled {
  opacity: 0.42;
  cursor: default;
}

.effects-menu__menu {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  z-index: var(--z-popover);
  min-width: 176px;
  border-radius: var(--radius);
  background: var(--bg-elevated);
  border: 0.5px solid var(--separator);
  box-shadow: var(--shadow-popover);
}
</style>
