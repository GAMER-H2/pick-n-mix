<script setup lang="ts">
/**
 * Rotary knob, as drawn in the advanced mixer's reverb and delay rows.
 *
 * Dragging is vertical rather than rotational: rotational drag is fiddly and
 * every DAW settled on vertical for the same reason. Holding shift gives fine
 * control.
 */
import { computed, ref } from "vue";

const props = withDefaults(
  defineProps<{
    modelValue: number;
    min?: number;
    max?: number;
    label: string;
    /** Accessible name when the visible label is intentionally shorter. */
    ariaLabel?: string;
    /** Formatted readout under the knob. */
    display?: string;
    size?: number;
    disabled?: boolean;
  }>(),
  { min: 0, max: 1, size: 46, disabled: false },
);

const emit = defineEmits<{ "update:modelValue": [value: number] }>();

/** Total sweep of the indicator, leaving a gap at the bottom. */
const SWEEP = 270;
const START = 135;
/** Pixels of vertical drag for the full range. */
const TRAVEL = 160;

const dragging = ref(false);
let startY = 0;
let startValue = 0;

const range = computed(() => props.max - props.min || 1);
const fraction = computed(() =>
  Math.min(1, Math.max(0, (props.modelValue - props.min) / range.value)),
);
const angle = computed(() => START + fraction.value * SWEEP);

const radius = computed(() => props.size / 2 - 3);
const circumference = computed(() => 2 * Math.PI * radius.value);
/** Length of the arc that represents the full sweep. */
const arcLength = computed(() => (circumference.value * SWEEP) / 360);

function onPointerDown(event: PointerEvent) {
  if (props.disabled) return;
  dragging.value = true;
  startY = event.clientY;
  startValue = props.modelValue;
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
}

function onPointerMove(event: PointerEvent) {
  if (!dragging.value) return;
  const scale = event.shiftKey ? 4 : 1;
  const delta = ((startY - event.clientY) / (TRAVEL * scale)) * range.value;
  emit("update:modelValue", clamp(startValue + delta));
}

function onPointerUp(event: PointerEvent) {
  if (!dragging.value) return;
  dragging.value = false;
  (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
}

function onWheel(event: WheelEvent) {
  if (props.disabled) return;
  event.preventDefault();
  const scale = event.shiftKey ? 0.002 : 0.01;
  emit("update:modelValue", clamp(props.modelValue - event.deltaY * scale * range.value));
}

/** Double-click resets to the middle of the range, the usual DAW gesture. */
function onDoubleClick() {
  if (props.disabled) return;
  emit("update:modelValue", props.min + range.value / 2);
}

function clamp(value: number) {
  return Math.min(props.max, Math.max(props.min, value));
}
</script>

<template>
  <div class="knob" :class="{ 'is-disabled': disabled }">
    <div
      class="knob__dial"
      :style="{ width: `${size}px`, height: `${size}px` }"
      role="slider"
      :aria-label="ariaLabel ?? label"
      :aria-valuemin="min"
      :aria-valuemax="max"
      :aria-valuenow="modelValue"
      :tabindex="disabled ? -1 : 0"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
      @wheel="onWheel"
      @dblclick="onDoubleClick"
    >
      <svg :width="size" :height="size" :viewBox="`0 0 ${size} ${size}`">
        <circle
          :cx="size / 2"
          :cy="size / 2"
          :r="radius"
          fill="none"
          stroke="var(--control-track)"
          stroke-width="3"
          stroke-linecap="round"
          :stroke-dasharray="`${arcLength} ${circumference}`"
          :transform="`rotate(${START} ${size / 2} ${size / 2})`"
        />
        <circle
          :cx="size / 2"
          :cy="size / 2"
          :r="radius"
          fill="none"
          stroke="var(--accent)"
          stroke-width="3"
          stroke-linecap="round"
          :stroke-dasharray="`${arcLength * fraction} ${circumference}`"
          :transform="`rotate(${START} ${size / 2} ${size / 2})`"
        />
        <line
          :x1="size / 2"
          :y1="size / 2 - radius + 6"
          :x2="size / 2"
          :y2="size / 2 - radius + 13"
          stroke="var(--text)"
          stroke-width="2"
          stroke-linecap="round"
          :transform="`rotate(${angle + 90} ${size / 2} ${size / 2})`"
        />
      </svg>
    </div>
    <div class="knob__label">{{ label }}</div>
    <div v-if="display" class="knob__value">{{ display }}</div>
  </div>
</template>

<style scoped>
.knob {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
}

.knob.is-disabled {
  opacity: 0.4;
  pointer-events: none;
}

.knob__dial {
  cursor: ns-resize;
  outline: none;
  border-radius: 50%;
  touch-action: none;
}

.knob__dial:focus-visible {
  box-shadow: 0 0 0 3px var(--accent-tint);
}

.knob__label {
  font-size: 10.5px;
  color: var(--text-secondary);
}

.knob__value {
  font-size: 10.5px;
  font-variant-numeric: tabular-nums;
  color: var(--text-tertiary);
}
</style>
