<script setup lang="ts">
/**
 * The standard "nothing here" block: icon, heading, explanation, optional
 * actions. Full-page variant for bare screens (fresh library, empty home);
 * `compact` for inline "nothing matches" messages inside lists and grids.
 */
import PnmIcon from "../icons/PnmIcon.vue";
import type { IconName } from "../icons/paths";

withDefaults(
  defineProps<{
    icon?: IconName;
    /** Heading; omit for the compact variant, which shows the message only. */
    title?: string;
    message?: string;
    /** Inline variant: smaller, no forced height, for use inside a list. */
    compact?: boolean;
  }>(),
  { icon: undefined, title: undefined, message: undefined, compact: false },
);
</script>

<template>
  <div v-if="compact" class="empty empty--compact">
    <p>{{ message ?? title }}</p>
    <slot />
  </div>

  <div v-else class="empty">
    <PnmIcon v-if="icon" :name="icon" :size="44" class="empty__icon" />
    <h1 v-if="title">{{ title }}</h1>
    <p v-if="message">{{ message }}</p>
    <slot />
  </div>
</template>

<style scoped>
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  min-height: 60vh;
  text-align: center;
  color: var(--text-secondary);
}

.empty__icon {
  color: var(--text-tertiary);
}

.empty h1 {
  margin: 4px 0 0;
  font-size: 22px;
  font-weight: 600;
  color: var(--text);
}

.empty p {
  margin: 0;
  max-width: 420px;
  font-size: 13px;
  line-height: 1.55;
}

.empty--compact {
  min-height: 0;
  padding: 40px 0;
  gap: 6px;
}

.empty--compact p {
  font-size: 13px;
  color: var(--text-tertiary);
}
</style>
