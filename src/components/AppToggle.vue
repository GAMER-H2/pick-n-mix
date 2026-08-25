<script setup lang="ts">
/** The pill switch drawn beside "Normalisation" in the simple mixer. */
defineProps<{ modelValue: boolean; disabled?: boolean; label?: string }>();
const emit = defineEmits<{ "update:modelValue": [value: boolean] }>();
</script>

<template>
  <button
    type="button"
    class="toggle"
    :class="{ 'is-on': modelValue }"
    :disabled="disabled"
    :aria-label="label"
    :aria-pressed="modelValue"
    @click="emit('update:modelValue', !modelValue)"
  >
    <span class="toggle__knob" />
  </button>
</template>

<style scoped>
.toggle {
  position: relative;
  width: 38px;
  height: 22px;
  border-radius: 999px;
  background: var(--control-track);
  transition: background 0.18s var(--ease);
  flex: none;
}

.toggle.is-on {
  background: var(--accent);
}

.toggle:disabled {
  opacity: 0.4;
  pointer-events: none;
}

.toggle__knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
  transition: transform 0.18s var(--ease);
}

.toggle.is-on .toggle__knob {
  transform: translateX(16px);
}
</style>
