<script setup lang="ts">
/**
 * The standard underlined tab strip. Tab state belongs in the route query, not
 * local state, so Back and Forward step through tabs (see `LibraryView`).
 */
defineProps<{
  tabs: ReadonlyArray<{ id: string; label: string }>;
  modelValue: string;
}>();

const emit = defineEmits<{ "update:modelValue": [id: string] }>();
</script>

<template>
  <nav class="tabs">
    <button
      v-for="option in tabs"
      :key="option.id"
      class="tabs__tab"
      :class="{ 'is-active': modelValue === option.id }"
      @click="emit('update:modelValue', option.id)"
    >
      {{ option.label }}
    </button>
  </nav>
</template>

<style scoped>
.tabs {
  display: flex;
  gap: 4px;
  margin-bottom: 10px;
  border-bottom: 1px solid var(--separator);
}

.tabs__tab {
  position: relative;
  padding: 8px 12px;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
}

.tabs__tab.is-active {
  color: var(--text);
}

.tabs__tab.is-active::after {
  content: "";
  position: absolute;
  left: 12px;
  right: 12px;
  bottom: -1px;
  height: 2px;
  border-radius: 2px;
  background: var(--accent);
}
</style>
