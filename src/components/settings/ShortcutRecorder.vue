<script setup lang="ts">
/**
 * The control half of a shortcut row: what the action is bound to now, and the
 * button that records a replacement.
 *
 * Recording listens in the capture phase so the key being pressed is caught
 * before the app's own shortcuts see it — otherwise binding a key would run
 * whatever it is currently bound to on the way past. Escape backs out without
 * recording, which is also why Escape itself is not rebindable; the settings
 * modal's own Escape handler was installed first, so it is told to stand down
 * through `recording` rather than being out-listened.
 *
 * Presentational: it reports the binding it captured and lets the settings
 * pane decide whether to keep it.
 */
import { onBeforeUnmount, ref } from "vue";
import { bindingFor, bindingLabel } from "@/lib/shortcuts";

const props = defineProps<{
  /** Bindings in force, already resolved from defaults where unset. */
  bindings: string[];
  /** Action name, for the accessible names of the buttons. */
  action: string;
  /** Whether these are the user's own bindings rather than the defaults. */
  customised: boolean;
}>();

const emit = defineEmits<{
  record: [binding: string];
  reset: [];
  /** So the surface around this can stand down while a key is being caught. */
  recording: [active: boolean];
}>();

const recording = ref(false);

function onKeydown(event: KeyboardEvent) {
  event.preventDefault();
  event.stopPropagation();
  if (event.key === "Escape") {
    stop();
    return;
  }
  const binding = bindingFor(event);
  // Only modifiers are down so far; keep waiting for the key they qualify.
  if (!binding) return;
  stop();
  emit("record", binding);
}

function stop() {
  if (!recording.value) return;
  recording.value = false;
  window.removeEventListener("keydown", onKeydown, true);
  emit("recording", false);
}

function toggle() {
  if (recording.value) {
    stop();
    return;
  }
  recording.value = true;
  window.addEventListener("keydown", onKeydown, true);
  emit("recording", true);
}

onBeforeUnmount(stop);
</script>

<template>
  <div class="shortcut">
    <p v-if="recording" class="shortcut__prompt" role="status">
      Press a key… <span>Esc to cancel</span>
    </p>
    <ul v-else class="shortcut__keys">
      <li v-for="binding in props.bindings" :key="binding" class="shortcut__key">
        {{ bindingLabel(binding) }}
      </li>
      <li v-if="props.bindings.length === 0" class="shortcut__key is-empty">Not bound</li>
    </ul>

    <button
      class="pill-button is-plain shortcut__record"
      type="button"
      :aria-pressed="recording"
      :aria-label="recording ? `Stop recording ${props.action}` : `Record a new key for ${props.action}`"
      @click="toggle"
    >
      {{ recording ? "Cancel" : "Record" }}
    </button>
    <button
      v-if="props.customised"
      class="pill-button is-plain shortcut__reset"
      type="button"
      :aria-label="`Reset ${props.action} to its default key`"
      @click="emit('reset')"
    >
      Reset
    </button>
  </div>
</template>

<style scoped>
.shortcut {
  display: flex;
  align-items: center;
  gap: 6px;
}

.shortcut__keys {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 5px;
  flex: 1;
  margin: 0;
  padding: 0;
  list-style: none;
}

/* The keycap look: a sunken pill, like the search field and select triggers. */
.shortcut__key {
  padding: 3px 9px;
  border-radius: var(--radius-sm);
  background: var(--bg-sunken);
  border: 0.5px solid var(--separator);
  font-size: 11.5px;
  color: var(--text);
}

.shortcut__key.is-empty {
  background: none;
  border-style: dashed;
  color: var(--text-tertiary);
}

.shortcut__prompt {
  flex: 1;
  margin: 0;
  font-size: 12px;
  color: var(--accent);
}

.shortcut__prompt span {
  color: var(--text-tertiary);
}

.shortcut__record,
.shortcut__reset {
  flex: none;
}
</style>
