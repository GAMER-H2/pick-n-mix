<script setup lang="ts">
/**
 * The standard search input: pill-shaped, sunken background, search glyph.
 * Debounce and URL sync stay with the caller (see `LibraryView`).
 */
import PnmIcon from "../icons/PnmIcon.vue";

defineProps<{
  modelValue: string;
  placeholder: string;
}>();

const emit = defineEmits<{ "update:modelValue": [value: string] }>();
</script>

<template>
  <div class="search">
    <PnmIcon name="search" :size="15" />
    <input
      class="search__input"
      type="search"
      :value="modelValue"
      :placeholder="placeholder"
      :aria-label="placeholder"
      @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
    />
  </div>
</template>

<style scoped>
.search {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  border-radius: 999px;
  background: var(--bg-sunken);
  color: var(--text-tertiary);
}

.search__input {
  width: 160px;
  border: 0;
  background: none;
  outline: none;
  font-size: 12.5px;
  user-select: text;
}
</style>
