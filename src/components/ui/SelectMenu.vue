<script setup lang="ts">
/**
 * A small dropdown in the app's own styling.
 *
 * A native `<select>` renders with the platform widget, which sits badly
 * against the rest of the interface, so this opens a `MenuSurface` — the one
 * menu look — under a pill-shaped trigger.
 */
import { computed, nextTick, ref } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import MenuSurface, { type MenuItem } from "./MenuSurface.vue";
import { useDismiss } from "@/lib/dismiss";

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
  () => (open.value = false),
  listEl,
  { ignore: [root] },
);

async function toggle() {
  open.value = !open.value;
  if (!open.value) return;
  // Focus the current option so the arrow keys have somewhere to start.
  await nextTick();
  listEl.value?.querySelector<HTMLElement>("[aria-checked='true']")?.focus();
}

function choose(id: string) {
  emit("update:modelValue", id);
  open.value = false;
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

    <Transition name="pop">
      <div v-if="open" ref="listEl" class="select__menu" @keydown="onListKeydown">
        <MenuSurface :items="menuItems" @select="choose" />
      </div>
    </Transition>
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

.select__menu {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  z-index: var(--z-popover);
  /* Never narrower than the trigger, so the two line up on both edges rather
     than only on the right. */
  min-width: max(168px, 100%);
  transform-origin: top right;
}
</style>
