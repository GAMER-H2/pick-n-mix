<script setup lang="ts">
/**
 * A mixer section's heading. When the mixer is pointed at a playlist or track
 * layer, an "Overridden" pill appears so it is obvious which settings belong
 * to that layer and which are inherited.
 */
import PnmIcon from "../icons/PnmIcon.vue";

defineProps<{
  title: string;
  overridden?: boolean;
  /** False for the global layer, where "override" has no meaning. */
  canOverride?: boolean;
}>();

const emit = defineEmits<{ clear: [] }>();
</script>

<template>
  <div class="header">
    <span class="header__title">{{ title }}</span>
    <button
      v-if="canOverride && overridden"
      class="header__pill"
      title="Remove this override and inherit again"
      @click="emit('clear')"
    >
      <PnmIcon name="close" :size="11" />
      <span>Overridden</span>
    </button>
    <slot />
  </div>
</template>

<style scoped>
.header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.header__title {
  font-size: 13px;
  font-weight: 600;
}

.header__pill {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  height: 18px;
  padding: 0 7px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 600;
  background: var(--accent-tint);
  color: var(--accent);
}

.header__pill:hover {
  background: var(--accent-tint-strong);
}
</style>
