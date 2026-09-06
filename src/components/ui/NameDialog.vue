<script setup lang="ts">
/**
 * A dialog that asks for one piece of text — a playlist's name, and anything
 * else that is a single field and a button.
 *
 * The field takes focus with its contents selected, so renaming is type-over
 * rather than select-then-type, and Enter is the same as pressing the button.
 */
import { nextTick, onMounted, ref } from "vue";
import BaseModal from "./BaseModal.vue";

const props = withDefaults(
  defineProps<{
    title: string;
    subtitle?: string;
    /** Accessible name of the field. */
    label?: string;
    initial?: string;
    placeholder?: string;
    confirmLabel?: string;
  }>(),
  {
    subtitle: undefined,
    label: "Name",
    initial: "",
    placeholder: "",
    confirmLabel: "Save",
  },
);

const emit = defineEmits<{ submit: [value: string]; close: [] }>();

const value = ref(props.initial);
const field = ref<HTMLInputElement | null>(null);

onMounted(async () => {
  // After BaseModal has moved focus to the dialog itself.
  await nextTick();
  field.value?.focus();
  field.value?.select();
});

function submit() {
  const trimmed = value.value.trim();
  if (!trimmed) return;
  emit("submit", trimmed);
}
</script>

<template>
  <BaseModal :open="true" :title="title" :subtitle="subtitle" :width="380" @close="emit('close')">
    <input
      ref="field"
      v-model="value"
      class="text-field"
      type="text"
      :aria-label="label"
      :placeholder="placeholder"
      @keydown.enter.prevent="submit"
    />

    <template #footer>
      <button class="pill-button is-plain" type="button" @click="emit('close')">Cancel</button>
      <button class="pill-button" type="button" :disabled="!value.trim()" @click="submit">
        {{ confirmLabel }}
      </button>
    </template>
  </BaseModal>
</template>
