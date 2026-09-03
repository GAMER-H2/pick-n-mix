<script setup lang="ts">
/**
 * The standard icon-only button: the global `.icon-button` style plus a
 * `PnmIcon`, with one `label` prop feeding both `title` and `aria-label` so
 * an icon button can never ship without an accessible name.
 */
import PnmIcon from "../icons/PnmIcon.vue";
import type { IconName } from "../icons/paths";

withDefaults(
  defineProps<{
    icon: IconName;
    /** Human-readable action name; becomes `title` and `aria-label`. */
    label: string;
    /** Glyph size in px; the button stays 30px either way. */
    size?: number;
    /** Accent-tinted state for toggles (mixer, queue, shuffle…). */
    active?: boolean;
    disabled?: boolean;
  }>(),
  { size: 17, active: false, disabled: false },
);

const emit = defineEmits<{ click: [event: MouseEvent] }>();
</script>

<template>
  <button
    class="icon-button"
    :class="{ 'is-active': active }"
    :title="label"
    :aria-label="label"
    :aria-pressed="active"
    :disabled="disabled"
    @click="emit('click', $event)"
  >
    <PnmIcon :name="icon" :size="size" />
  </button>
</template>
