<script setup lang="ts">
/**
 * A small dropdown in the app's own styling.
 *
 * A native `<select>` renders with the platform widget, which sits badly
 * against the rest of the interface, so this opens a `MenuSurface` — the one
 * menu look — under a pill-shaped trigger.
 *
 * The menu is teleported to `<body>` and placed from the trigger's box rather
 * than being an absolutely positioned child: a select in a modal with its own
 * scrolling pane (Settings) otherwise has its menu clipped by that pane, which
 * hid the longer output device names behind the modal's sidebar.
 */
import { computed, nextTick, onBeforeUnmount, ref } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import MenuSurface, { type MenuItem } from "./MenuSurface.vue";
import { useDismiss } from "@/lib/dismiss";
import { visibleBounds } from "@/lib/frame";

export interface SelectOption {
  id: string;
  label: string;
}

const props = defineProps<{
  modelValue: string;
  options: ReadonlyArray<SelectOption>;
  /** Accessible name, and the quiet prefix drawn before the value. */
  label: string;
}>();

const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const open = ref(false);
const root = ref<HTMLElement | null>(null);
const listEl = ref<HTMLElement | null>(null);
const menuStyle = ref<Record<string, string>>({});

/** Between the trigger and the menu. */
const GAP = 6;
/** Below this, opening downwards is not worth it and the menu flips up. */
const MIN_ROOM = 160;
const MAX_HEIGHT = 320;

const selected = computed(
  () => props.options.find((option) => option.id === props.modelValue) ?? props.options[0],
);

const menuItems = computed<MenuItem[]>(() =>
  props.options.map((option) => ({
    id: option.id,
    label: option.label,
    checked: option.id === props.modelValue,
  })),
);

useDismiss(
  () => open.value,
  () => {
    open.value = false;
    stopTracking();
  },
  listEl,
  { ignore: [root] },
);

/**
 * Pins the menu under (or over) the trigger, right edges aligned, clamped to
 * the window rather than to the viewport — the outer band of which is the
 * window's own shadow.
 */
function place() {
  const trigger = root.value?.getBoundingClientRect();
  if (!trigger) return;
  const frame = visibleBounds();

  const below = frame.bottom - trigger.bottom - GAP;
  const above = trigger.top - frame.top - GAP;
  const flip = below < MIN_ROOM && above > below;
  const room = Math.max(flip ? above : below, MIN_ROOM);

  menuStyle.value = {
    right: `${Math.max(frame.left, window.innerWidth - trigger.right)}px`,
    minWidth: `${Math.max(168, trigger.width)}px`,
    maxWidth: `${frame.width - 16}px`,
    maxHeight: `${Math.min(MAX_HEIGHT, room)}px`,
    transformOrigin: flip ? "bottom right" : "top right",
    ...(flip
      ? { bottom: `${window.innerHeight - trigger.top + GAP}px` }
      : { top: `${trigger.bottom + GAP}px` }),
  };
}

async function toggle() {
  open.value = !open.value;
  if (!open.value) {
    stopTracking();
    return;
  }
  place();
  // The menu no longer moves with the trigger, so anything that shifts it has
  // to be followed: a scrolling pane behind it, a resized window.
  window.addEventListener("scroll", place, true);
  window.addEventListener("resize", place);
  // Focus the current option so the arrow keys have somewhere to start.
  await nextTick();
  listEl.value?.querySelector<HTMLElement>("[aria-checked='true']")?.focus();
}

function stopTracking() {
  window.removeEventListener("scroll", place, true);
  window.removeEventListener("resize", place);
}

onBeforeUnmount(stopTracking);

function choose(id: string) {
  emit("update:modelValue", id);
  open.value = false;
  stopTracking();
}

/** Roving focus, so the list behaves like a menu rather than a set of buttons. */
function onListKeydown(event: KeyboardEvent) {
  const items = Array.from(listEl.value?.querySelectorAll<HTMLElement>(".menu__item") ?? []);
  if (items.length === 0) return;
  const index = items.indexOf(document.activeElement as HTMLElement);

  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const step = event.key === "ArrowDown" ? 1 : -1;
    const next = (index + step + items.length) % items.length;
    items[next].focus();
  }
}
</script>

<template>
  <div ref="root" class="select">
    <button
      class="select__trigger"
      type="button"
      :aria-label="label"
      :aria-expanded="open"
      aria-haspopup="menu"
      @click="toggle"
    >
      <span class="select__label">{{ label }}</span>
      <span class="select__value">{{ selected?.label }}</span>
      <PnmIcon name="chevronDown" :size="13" class="select__caret" />
    </button>

    <Teleport to="body">
      <Transition name="pop">
        <div
          v-if="open"
          ref="listEl"
          class="select__menu"
          :style="menuStyle"
          @keydown="onListKeydown"
        >
          <MenuSurface :items="menuItems" @select="choose" />
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<style scoped>
.select {
  position: relative;
}

.select__trigger {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 8px 0 10px;
  border-radius: 999px;
  background: var(--bg-sunken);
  font-size: 12.5px;
  color: var(--text);
}

.select__trigger:hover {
  background: var(--bg-active);
}

.select__label {
  color: var(--text-tertiary);
}

/*
 * The value and its caret sit at the trigger's right edge, which is where the
 * menu opens from. In a row that hugs its content the two are the same place;
 * in a stretched one — a dialog field, say — they are not, and without this
 * the menu appears under the far right of a button whose text is at the far
 * left, reading as though it belongs to something else.
 */
.select__value {
  margin-left: auto;
}

.select__caret {
  color: var(--text-tertiary);
}

/* Teleported, so everything that positions it — including the "never narrower
   than the trigger" minimum the two need to line up on both edges — is set
   inline by `place`. */
.select__menu {
  position: fixed;
  z-index: var(--z-popover);
  overflow-y: auto;
}
</style>
