<script setup lang="ts">
/**
 * Label + slider + formatted value: the standard mixer/settings row for any
 * continuous parameter. Everything except `label` and `format` passes through
 * to `AppSlider` (min, max, step, origin, detents…).
 */
import AppSlider from "./AppSlider.vue";

defineProps<{
  label: string;
  modelValue: number;
  /** Formats the trailing value readout, e.g. `(v) => \`${v}s\``. */
  format?: (value: number) => string;
}>();

const emit = defineEmits<{ "update:modelValue": [value: number] }>();

function formatDefault(value: number) {
  return String(Math.round(value * 100) / 100);
}
</script>

<template>
  <div class="slider-row">
    <label class="slider-row__label">{{ label }}</label>
    <AppSlider
      v-bind="$attrs"
      :model-value="modelValue"
      @update:model-value="emit('update:modelValue', $event)"
    />
    <span class="slider-row__value">{{ (format ?? formatDefault)(modelValue) }}</span>
  </div>
</template>

<style scoped>
.slider-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.slider-row__label {
  flex: none;
  width: 92px;
  font-size: 12px;
  color: var(--text-secondary);
}

.slider-row__value {
  flex: none;
  min-width: 44px;
  text-align: right;
  font-size: 11.5px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
}
</style>
