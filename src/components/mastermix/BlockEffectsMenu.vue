<script setup lang="ts">
/**
 * The Master Mixer rack's add-effect dropdown: everything the app can put on a
 * region, grouped in one menu.
 *
 * A mix is a mini-DAW, so effects are picked from the rack title bar and laid
 * out as devices along the bottom instead of sending the user to the DJ
 * sidebar — see `BlockEffectsRack`.
 *
 * Effects already on the block are ticked and cannot be added twice: a second
 * reverb on one region would be a second set of controls for the same effect.
 */
import { computed, nextTick, onBeforeUnmount, ref } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import MenuSurface, { type MenuGroup } from "../ui/MenuSurface.vue";
import { useDismiss } from "@/lib/dismiss";
import { visibleBounds } from "@/lib/frame";
import { DEFAULT_CHAIN_ORDER, PINNED_DEVICES, SECTION_LABELS } from "@/lib/mixer";
import type { DeviceSection } from "@/lib/mixer";

const props = withDefaults(
  defineProps<{
    /** Devices the selected block already has. */
    present: DeviceSection[];
    /** No single block is selected, so there is nothing to add an effect to. */
    disabled?: boolean;
    label?: string;
  }>(),
  { disabled: false, label: "+ Add Effect" },
);

const emit = defineEmits<{ add: [section: DeviceSection] }>();

const open = ref(false);
const menuEl = ref<HTMLElement | null>(null);
const buttonEl = ref<HTMLElement | null>(null);
const menuStyle = ref<Record<string, string>>({});

const GAP = 6;
const MIN_ROOM = 140;
const MAX_HEIGHT = 300;

// The trigger is ignored so its own click still toggles the menu shut.
useDismiss(
  () => open.value,
  () => {
    open.value = false;
    stopTracking();
  },
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

const groups = computed<MenuGroup[]>(() => [
  group("Chain", DEFAULT_CHAIN_ORDER),
  group("Region", PINNED_DEVICES),
]);

function place() {
  const trigger = buttonEl.value?.getBoundingClientRect();
  if (!trigger) return;
  const frame = visibleBounds();
  const below = frame.bottom - trigger.bottom - GAP;
  const above = trigger.top - frame.top - GAP;
  const flip = below < MIN_ROOM && above > below;
  const room = Math.max(flip ? above : below, 0);

  menuStyle.value = {
    right: `${Math.max(frame.left, window.innerWidth - trigger.right)}px`,
    minWidth: `${Math.max(176, trigger.width)}px`,
    maxWidth: `${frame.width - 16}px`,
    maxHeight: `${Math.min(MAX_HEIGHT, room)}px`,
    transformOrigin: flip ? "bottom right" : "top right",
    ...(flip
      ? { bottom: `${window.innerHeight - trigger.top + GAP}px` }
      : { top: `${trigger.bottom + GAP}px` }),
  };
}

function stopTracking() {
  window.removeEventListener("scroll", place, true);
  window.removeEventListener("resize", place);
}

async function toggle() {
  if (props.disabled) return;
  open.value = !open.value;
  if (!open.value) {
    stopTracking();
    return;
  }
  place();
  window.addEventListener("scroll", place, true);
  window.addEventListener("resize", place);
  await nextTick();
  menuEl.value?.querySelector<HTMLElement>("[role='menuitem']:not([disabled])")?.focus();
}

function onSelect(id: string) {
  open.value = false;
  stopTracking();
  emit("add", id as DeviceSection);
}

onBeforeUnmount(stopTracking);
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
      <span>{{ props.label }}</span>
      <PnmIcon name="chevronDown" :size="14" />
    </button>

    <Teleport to="body">
      <Transition name="pop">
        <div
          v-if="open"
          ref="menuEl"
          class="effects-menu__menu"
          :style="menuStyle"
        >
          <MenuSurface :groups="groups" @select="onSelect" />
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<style scoped>
.effects-menu {
  position: relative;
}

.effects-menu__trigger {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-width: 132px;
  height: 30px;
  padding: 0 10px;
  border: 1px solid var(--separator);
  border-radius: var(--radius-sm);
  background: var(--bg-elevated);
  font-size: 12.5px;
  color: var(--text);
}

.effects-menu__trigger:hover:not(:disabled) {
  border-color: var(--separator-strong);
}

.effects-menu__trigger:disabled {
  opacity: 0.42;
  cursor: default;
}

.effects-menu__menu {
  position: fixed;
  z-index: var(--z-popover);
  min-width: 176px;
  border-radius: var(--radius);
  background: var(--bg-elevated);
  border: 0.5px solid var(--separator);
  box-shadow: var(--shadow-popover);
  overflow-y: auto;
}
</style>
