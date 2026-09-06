<script setup lang="ts">
/**
 * "Are you sure?" — the one confirmation dialog.
 *
 * For actions that cannot be undone. Anything reversible should just happen,
 * with a toast saying what it did, rather than asking first.
 */
import BaseModal from "./BaseModal.vue";

withDefaults(
  defineProps<{
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    /** Draws the confirming button as destructive. */
    danger?: boolean;
  }>(),
  { confirmLabel: "Confirm", cancelLabel: "Cancel", danger: false },
);

const emit = defineEmits<{ confirm: []; close: [] }>();
</script>

<template>
  <BaseModal :open="true" :title="title" :width="360" @close="emit('close')">
    <p class="confirm__message">{{ message }}</p>

    <template #footer>
      <button class="pill-button is-plain" type="button" @click="emit('close')">
        {{ cancelLabel }}
      </button>
      <button
        class="pill-button"
        :class="{ 'is-danger': danger }"
        type="button"
        @click="emit('confirm')"
      >
        {{ confirmLabel }}
      </button>
    </template>
  </BaseModal>
</template>

<style scoped>
.confirm__message {
  margin: 0;
  font-size: 12.5px;
  line-height: 1.5;
  color: var(--text-secondary);
}
</style>
