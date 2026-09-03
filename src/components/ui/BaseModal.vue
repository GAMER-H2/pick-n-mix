<script setup lang="ts">
/**
 * The one modal implementation: scrim, dialog shell, Escape (capture-phase so
 * it wins over page shortcuts), scrim click, initial focus, focus restore on
 * close, and the fade transition. Every dialog in the app renders inside one
 * of these; nobody writes a scrim or an Escape handler by hand anymore.
 *
 * A modal that owns global key handling of its own (the Master Mixer) sets
 * `closeOnEsc` to false and keeps its handler; everything else defers here.
 */
import { nextTick, onBeforeUnmount, ref, watch } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";

const props = withDefaults(
  defineProps<{
    open: boolean;
    /** Plain-text title; omit when the `header` slot renders its own. */
    title?: string;
    subtitle?: string;
    /** Dialog width in px. */
    width?: number;
    /** Selector for the accessible name when the title lives in a slot. */
    labelledby?: string;
    closeOnScrim?: boolean;
    closeOnEsc?: boolean;
    /** `modal-top` sits above another open modal (Master Mix over EQ). */
    layer?: "modal" | "modal-top";
    /**
     * Remove the shell's own padding for workspace-style content (the EQ
     * modal) that manages its edges like the settings and master modals do.
     */
    flush?: boolean;
  }>(),
  {
    title: undefined,
    subtitle: undefined,
    width: 380,
    labelledby: undefined,
    closeOnScrim: true,
    closeOnEsc: true,
    layer: "modal",
    flush: false,
  },
);

const emit = defineEmits<{ close: [] }>();

const dialogRef = ref<HTMLElement | null>(null);
/** Element to hand focus back to when the modal closes. */
let opener: HTMLElement | null = null;

function close() {
  emit("close");
}

function onScrimClick() {
  if (props.closeOnScrim) close();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  event.preventDefault();
  event.stopPropagation();
  close();
}

watch(
  () => dialogRef.value,
  (el) => {
    if (el) el.focus();
  },
);

watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) {
      opener = document.activeElement as HTMLElement | null;
      void nextTick(() => dialogRef.value?.focus());
      window.addEventListener("keydown", onKeydown, true);
    } else {
      window.removeEventListener("keydown", onKeydown, true);
      opener?.focus();
      opener = null;
    }
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown, true);
  opener?.focus();
});

defineExpose({ dialogRef });
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div
        v-if="open"
        class="modal__scrim"
        :class="{ 'modal__scrim--top': layer === 'modal-top' }"
        @click.self="onScrimClick"
      >
        <div
          ref="dialogRef"
          class="modal__dialog"
          :class="{ 'modal__dialog--top': layer === 'modal-top', 'modal__dialog--flush': flush }"
          :style="{ width: `${width}px` }"
          role="dialog"
          aria-modal="true"
          :aria-labelledby="labelledby"
          :aria-label="labelledby ? undefined : title"
          tabindex="-1"
        >
          <header v-if="title || $slots.header" class="modal__head">
            <slot name="header">
              <div>
                <h2 class="modal__title">{{ title }}</h2>
                <p v-if="subtitle" class="modal__subtitle">{{ subtitle }}</p>
              </div>
            </slot>
            <slot name="close">
              <button
                class="icon-button"
                title="Close"
                aria-label="Close"
                @click="close"
              >
                <PnmIcon name="close" :size="17" />
              </button>
            </slot>
          </header>

          <div class="modal__body">
            <slot />
          </div>

          <footer v-if="$slots.footer" class="modal__foot">
            <slot name="footer" />
          </footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.modal__scrim {
  position: fixed;
  inset: 0;
  z-index: var(--z-modal);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 22px;
  background: rgba(0, 0, 0, 0.34);
  backdrop-filter: blur(4px);
}

.modal__scrim--top {
  z-index: var(--z-modal-top);
}

.modal__dialog {
  display: flex;
  flex-direction: column;
  max-width: calc(100vw - 44px);
  max-height: calc(100vh - 44px);
  padding: 16px;
  border-radius: var(--radius-lg);
  background: var(--bg-elevated);
  border: 0.5px solid var(--separator);
  box-shadow: var(--shadow-popover);
  outline: none;
}

.modal__dialog--top {
  z-index: var(--z-modal-top);
}

.modal__dialog--flush {
  padding: 0;
}

.modal__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}

.modal__title {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.modal__subtitle {
  margin: 3px 0 0;
  font-size: 12px;
  color: var(--text-tertiary);
}

.modal__body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.modal__foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 14px;
}
</style>
