<script setup lang="ts">
/**
 * A small dropdown in the app's own styling.
 *
 * A native `<select>` renders with the platform widget, which sits badly
 * against the rest of the interface, so this borrows the context menu's look:
 * same surface, radius, shadow and tick.
 */
import { computed, nextTick, ref } from "vue";
import PnmIcon from "./icons/PnmIcon.vue";
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
  listEl.value?.querySelector<HTMLElement>("[data-selected]")?.focus();
}

function choose(id: string) {
  emit("update:modelValue", id);
  open.value = false;
}

/** Roving focus, so the list behaves like a menu rather than a set of buttons. */
function onListKeydown(event: KeyboardEvent) {
  const items = Array.from(listEl.value?.querySelectorAll<HTMLElement>("[data-option]") ?? []);
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
      aria-haspopup="listbox"
      @click="toggle"
    >
      <span class="select__label">{{ label }}</span>
      <span class="select__value">{{ selected?.label }}</span>
      <PnmIcon name="chevronDown" :size="13" class="select__caret" />
    </button>

    <Transition name="pop">
      <div
        v-if="open"
        ref="listEl"
        class="select__menu"
        role="listbox"
        :aria-label="label"
        @keydown="onListKeydown"
      >
        <button
          v-for="option in options"
          :key="option.id"
          data-option
          :data-selected="option.id === modelValue ? '' : undefined"
          class="select__option"
          type="button"
          role="option"
          :aria-selected="option.id === modelValue"
          @click="choose(option.id)"
        >
          <span>{{ option.label }}</span>
          <PnmIcon
            v-if="option.id === modelValue"
            name="check"
            :size="14"
            class="select__tick"
          />
        </button>
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

.select__caret {
  color: var(--text-tertiary);
}

.select__menu {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  z-index: 300;
  min-width: 168px;
  padding: 5px;
  border-radius: var(--radius);
  background: var(--bg-elevated);
  box-shadow: var(--shadow-popover);
  border: 0.5px solid var(--separator);
  transform-origin: top right;
}

.select__option {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 7px 9px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  color: var(--text);
  text-align: left;
}

.select__option:hover,
.select__option:focus-visible {
  background: var(--accent);
  color: var(--accent-contrast);
}

.select__tick {
  margin-left: auto;
  flex: none;
}
</style>
