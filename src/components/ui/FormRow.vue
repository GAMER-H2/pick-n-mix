<script setup lang="ts">
/**
 * Label + control + optional hint: the standard form row for dialogs and the
 * settings modal. The control goes in the default slot; pair it with the
 * label via the `for` prop when the control takes an id.
 */
defineProps<{
  label: string;
  hint?: string;
  /** `id` of the slotted control, for a proper `<label for>` pairing. */
  controlId?: string;
}>();
</script>

<template>
  <div class="form-row">
    <label class="form-row__label" :for="controlId">{{ label }}</label>
    <div class="form-row__control">
      <slot />
      <p v-if="hint" class="form-row__hint">{{ hint }}</p>
    </div>
  </div>
</template>

<style scoped>
.form-row {
  display: grid;
  grid-template-columns: 130px 1fr;
  align-items: center;
  gap: 12px;
  padding: 7px 0;
}

.form-row__label {
  font-size: 12.5px;
  color: var(--text-secondary);
}

.form-row__hint {
  margin: 3px 0 0;
  font-size: 11.5px;
  color: var(--text-tertiary);
}
</style>
