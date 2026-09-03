<script setup lang="ts">
/**
 * The one menu look: elevated surface, hover rows, check ticks, danger rows,
 * separators. Every menu in the app — the context menu, select menus, preset
 * selects — renders its items through this, so a menu can never drift into a
 * second visual language.
 *
 * Custom rows (an inline "save as" form, for example) go in the default slot,
 * which renders after the grouped items.
 */
import PnmIcon from "../icons/PnmIcon.vue";
import type { IconName } from "../icons/paths";

export interface MenuItem {
  id: string;
  label: string;
  icon?: IconName;
  /** Shows the accent tick on the trailing edge. */
  checked?: boolean;
  /** Destructive action; rendered in the standard danger colour. */
  danger?: boolean;
  /** Cautionary action; the icon is drawn in the warning colour. */
  warning?: boolean;
  disabled?: boolean;
}

export interface MenuGroup {
  /** Optional small-caps group heading. */
  label?: string;
  items: MenuItem[];
}

defineProps<{
  /** Flat list, or grouped when order/heading matters. */
  items?: MenuItem[];
  groups?: MenuGroup[];
}>();

const emit = defineEmits<{ select: [id: string] }>();
</script>

<template>
  <div class="menu" role="menu">
    <template v-for="(group, gi) in groups ?? []" :key="gi">
      <div v-if="gi > 0" class="menu__separator" role="separator" />
      <div v-if="group.label" class="menu__group-label">{{ group.label }}</div>
      <button
        v-for="item in group.items"
        :key="item.id"
        class="menu__item"
        :class="{ 'is-danger': item.danger }"
        role="menuitem"
        :aria-checked="item.checked"
        :disabled="item.disabled"
        @click="emit('select', item.id)"
      >
        <PnmIcon
          v-if="item.icon"
          :name="item.icon"
          :size="15"
          class="menu__icon"
          :class="{ 'is-warning': item.warning }"
        />
        <span class="menu__label truncate">{{ item.label }}</span>
        <PnmIcon v-if="item.checked" name="check" :size="14" class="menu__tick" />
      </button>
    </template>

    <template v-if="items">
      <div v-if="(groups?.length ?? 0) > 0" class="menu__separator" role="separator" />
      <button
        v-for="item in items"
        :key="item.id"
        class="menu__item"
        :class="{ 'is-danger': item.danger }"
        role="menuitem"
        :aria-checked="item.checked"
        :disabled="item.disabled"
        @click="emit('select', item.id)"
      >
        <PnmIcon
          v-if="item.icon"
          :name="item.icon"
          :size="15"
          class="menu__icon"
          :class="{ 'is-warning': item.warning }"
        />
        <span class="menu__label truncate">{{ item.label }}</span>
        <PnmIcon v-if="item.checked" name="check" :size="14" class="menu__tick" />
      </button>
    </template>

    <slot />
  </div>
</template>

<style scoped>
.menu {
  min-width: 200px;
  padding: 5px;
  border-radius: var(--radius);
  background: var(--bg-elevated);
  border: 0.5px solid var(--separator);
  box-shadow: var(--shadow-popover);
}

.menu__group-label {
  padding: 6px 9px 3px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: var(--text-tertiary);
}

.menu__item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 7px 9px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  text-align: left;
  color: var(--text);
}

.menu__item:hover:not(:disabled) {
  background: var(--accent-tint);
  color: var(--accent);
}

.menu__item:disabled {
  opacity: 0.4;
  pointer-events: none;
}

.menu__item.is-danger {
  color: #e0383e;
}

.menu__item.is-danger:hover:not(:disabled) {
  background: rgba(224, 56, 62, 0.1);
  color: #e0383e;
}

.menu__icon {
  flex: none;
  color: var(--text-secondary);
}

.menu__icon.is-warning {
  color: #d69b16;
}

.menu__item:hover:not(:disabled) .menu__icon {
  color: inherit;
}

.menu__label {
  flex: 1;
  min-width: 0;
}

.menu__tick {
  flex: none;
  color: var(--accent);
}

.menu__separator {
  height: 1px;
  margin: 5px 8px;
  background: var(--separator);
}
</style>
