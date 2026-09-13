<script setup lang="ts">
/**
 * A compact disclosure for explaining one effects area without letting its
 * guidance blur into the controls it describes. It owns only local visibility
 * state so each mixer instance starts expanded and does not share UI state.
 */
import { ref } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";

const props = withDefaults(
  defineProps<{
    title: string;
    note?: string;
    overridden?: boolean;
    canOverride?: boolean;
  }>(),
  { note: "", overridden: false, canOverride: false },
);

const emit = defineEmits<{ clear: [] }>();
const expanded = ref(true);
</script>

<template>
  <section class="effects-explanation">
    <button
      class="effects-explanation__toggle"
      type="button"
      :aria-expanded="expanded"
      :title="expanded ? `Collapse ${props.title} explanation` : `Show ${props.title} explanation`"
      @click="expanded = !expanded"
    >
      <span class="effects-explanation__title">{{ props.title }}</span>
      <span v-if="props.note" class="effects-explanation__note">{{ props.note }}</span>
      <PnmIcon :name="expanded ? 'chevronDown' : 'chevronRight'" :size="14" />
    </button>
    <div v-if="expanded" class="effects-explanation__content">
      <p class="effects-explanation__body">
        <slot />
      </p>
      <button
        v-if="props.canOverride && props.overridden"
        class="effects-explanation__clear"
        type="button"
        title="Remove this override and inherit again"
        @click="emit('clear')"
      >
        <PnmIcon name="close" :size="11" />
        <span>Overridden</span>
      </button>
    </div>
  </section>
</template>

<style scoped>
.effects-explanation {
  border-bottom: 1px solid var(--separator);
  border-radius: var(--radius-sm);
  background: var(--bg-sunken);
  overflow: hidden;
}

.effects-explanation__toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
  min-height: 30px;
  padding: 8px 9px;
  color: var(--text-secondary);
  font-size: 11.5px;
  font-weight: 600;
  line-height: 1.25;
  text-align: left;
}

.effects-explanation__title {
  flex: none;
}

.effects-explanation__note {
  min-width: 0;
  margin-left: auto;
  color: var(--text-tertiary);
  font-size: 10px;
  font-weight: 400;
  white-space: nowrap;
}

.effects-explanation__toggle:hover {
  background: var(--bg-hover);
  color: var(--text);
}

.effects-explanation__toggle:focus-visible {
  outline: 1px solid var(--accent);
  outline-offset: -1px;
}

.effects-explanation__content {
  padding: 0 9px 8px;
}

.effects-explanation__body {
  margin: 0;
  color: var(--text-tertiary);
  font-size: 10.5px;
  line-height: 1.5;
}

.effects-explanation__clear {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  height: 18px;
  margin-top: 7px;
  padding: 0 7px;
  border-radius: 999px;
  background: var(--accent-tint);
  color: var(--accent);
  font-size: 10px;
  font-weight: 600;
}

.effects-explanation__clear:hover {
  background: var(--accent-tint-strong);
}
</style>
