<script setup lang="ts">
/**
 * Horizontal slider used for the scrubber, volume and every linear mixer
 * parameter. Pointer capture means a drag keeps working even when the cursor
 * leaves the element, which matters for the thin player-bar scrubber.
 */
import { computed, ref } from "vue";

const props = withDefaults(
  defineProps<{
    modelValue: number;
    min?: number;
    max?: number;
    step?: number;
    /** Draws the fill outward from this value; use 0 for a boost/cut control. */
    origin?: number | null;
    disabled?: boolean;
    /** Slimmer styling for the player bar. */
    subtle?: boolean;
    /**
     * Values the handle sticks to, in the slider's own units. Holding Shift
     * while dragging bypasses them for fine adjustment.
     */
    detents?: number[];
    /** How close, as a fraction of the range, a detent grabs from. */
    detentRadius?: number;
    /**
     * Positions to mark on the track, in the slider's own units.
     *
     * Unlike detents these only draw: they are landmarks in what is being
     * scrubbed — the songs inside a mix — not places the handle should stick
     * to. They stay visible rather than appearing on hover, since the point of
     * them is to be read at a glance.
     */
    markers?: number[];
  }>(),
  {
    min: 0,
    max: 1,
    step: 0.001,
    origin: null,
    disabled: false,
    subtle: false,
    detents: () => [],
    detentRadius: 0.035,
    markers: () => [],
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: number];
  start: [];
  end: [];
}>();

const el = ref<HTMLElement | null>(null);
const dragging = ref(false);

const range = computed(() => props.max - props.min || 1);
const fraction = computed(() =>
  Math.min(1, Math.max(0, (props.modelValue - props.min) / range.value)),
);

// With an origin the fill grows out from it in either direction, which is how
// a +/- EQ gain should read.
const originFraction = computed(() =>
  props.origin === null
    ? 0
    : Math.min(1, Math.max(0, (props.origin - props.min) / range.value)),
);
const fillLeft = computed(() => Math.min(fraction.value, originFraction.value));
const fillWidth = computed(() => Math.abs(fraction.value - originFraction.value));

/** Snap to the nearest detent when one is within reach. */
function snap(value: number, bypass: boolean): number {
  if (bypass || props.detents.length === 0) return value;
  const reach = range.value * props.detentRadius;
  let best: number | null = null;
  for (const detent of props.detents) {
    const distance = Math.abs(detent - value);
    if (distance <= reach && (best === null || distance < Math.abs(best - value))) {
      best = detent;
    }
  }
  return best ?? value;
}

/** Marker positions as fractions of the track, clamped to it. */
const markerFractions = computed(() =>
  props.markers.map((at) => Math.min(1, Math.max(0, (at - props.min) / range.value))),
);

function valueAt(clientX: number, bypassDetents = false): number {
  const rect = el.value?.getBoundingClientRect();
  if (!rect || rect.width === 0) return props.modelValue;
  const ratio = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
  const raw = snap(props.min + ratio * range.value, bypassDetents);
  const stepped = Math.round(raw / props.step) * props.step;
  // Rounding by step can drift outside the range at the extremes.
  return Math.min(props.max, Math.max(props.min, Number(stepped.toFixed(6))));
}

function onPointerDown(event: PointerEvent) {
  if (props.disabled) return;
  dragging.value = true;
  el.value?.setPointerCapture(event.pointerId);
  emit("start");
  emit("update:modelValue", valueAt(event.clientX, event.shiftKey));
}

function onPointerMove(event: PointerEvent) {
  if (!dragging.value) return;
  emit("update:modelValue", valueAt(event.clientX, event.shiftKey));
}

function onPointerUp(event: PointerEvent) {
  if (!dragging.value) return;
  dragging.value = false;
  el.value?.releasePointerCapture(event.pointerId);
  emit("end");
}

function onKeydown(event: KeyboardEvent) {
  if (props.disabled) return;
  const large = range.value / 10;
  const deltas: Record<string, number> = {
    ArrowLeft: -props.step,
    ArrowDown: -props.step,
    ArrowRight: props.step,
    ArrowUp: props.step,
    PageDown: -large,
    PageUp: large,
  };
  const delta = deltas[event.key];
  if (delta === undefined) return;
  event.preventDefault();
  emit("update:modelValue", Math.min(props.max, Math.max(props.min, props.modelValue + delta)));
}
</script>

<template>
  <div
    ref="el"
    class="slider"
    :class="{ 'is-dragging': dragging, 'is-subtle': subtle, 'is-disabled': disabled }"
    role="slider"
    :tabindex="disabled ? -1 : 0"
    :aria-valuemin="min"
    :aria-valuemax="max"
    :aria-valuenow="modelValue"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="onPointerUp"
    @pointercancel="onPointerUp"
    @keydown="onKeydown"
  >
    <div class="slider__track">
      <div
        class="slider__fill"
        :style="{ left: `${fillLeft * 100}%`, width: `${fillWidth * 100}%` }"
      />
    </div>
    <span
      v-for="detent in detents"
      :key="detent"
      class="slider__detent"
      :style="{ left: `${((detent - min) / range) * 100}%` }"
    />
    <span
      v-for="(at, index) in markerFractions"
      :key="`marker-${index}`"
      class="slider__marker"
      :style="{ left: `${at * 100}%` }"
    />
    <div class="slider__thumb" :style="{ left: `${fraction * 100}%` }" />
  </div>
</template>

<style scoped>
.slider {
  position: relative;
  display: flex;
  align-items: center;
  /* Fill the space given, and survive being a flex item: without these the
     track has no width to be 100% of and the slider collapses to its thumb. */
  flex: 1 1 auto;
  width: 100%;
  min-width: 0;
  height: 20px;
  outline: none;
  touch-action: none;
}

.slider.is-disabled {
  opacity: 0.4;
  pointer-events: none;
}

.slider__track {
  position: relative;
  width: 100%;
  height: 4px;
  border-radius: 999px;
  background: var(--control-track);
  overflow: hidden;
}

.is-subtle .slider__track {
  height: 3px;
}

.slider__fill {
  position: absolute;
  top: 0;
  bottom: 0;
  background: var(--accent);
  border-radius: 999px;
}

.slider__detent {
  position: absolute;
  top: 50%;
  width: 2px;
  height: 2px;
  margin-left: -1px;
  border-radius: 50%;
  background: var(--text-tertiary);
  transform: translateY(-50%);
  opacity: 0;
  transition: opacity 0.12s var(--ease);
  pointer-events: none;
}

.slider:hover .slider__detent,
.slider.is-dragging .slider__detent {
  opacity: 0.75;
}

/* Drawn like a detent, but always visible: it is information rather than a
   hint about where the handle will settle. */
.slider__marker {
  position: absolute;
  top: 50%;
  width: 3px;
  height: 3px;
  margin-left: -1.5px;
  border-radius: 50%;
  background: var(--text-secondary);
  transform: translateY(-50%);
  opacity: 0.85;
  pointer-events: none;
}

.slider__thumb {
  position: absolute;
  top: 50%;
  width: 12px;
  height: 12px;
  margin-left: -6px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.32), 0 0 0 0.5px rgba(0, 0, 0, 0.1);
  transform: translateY(-50%);
  transition: transform 0.12s var(--ease), opacity 0.12s var(--ease);
}

/* The scrubber's handle stays out of the way until the row is touched. */
.is-subtle .slider__thumb {
  opacity: 0;
  transform: translateY(-50%) scale(0.7);
}

.is-subtle:hover .slider__thumb,
.is-subtle:focus-visible .slider__thumb,
.is-subtle.is-dragging .slider__thumb {
  opacity: 1;
  transform: translateY(-50%) scale(1);
}

.slider:focus-visible .slider__track {
  box-shadow: 0 0 0 3px var(--accent-tint);
}

.slider.is-dragging .slider__thumb {
  transform: translateY(-50%) scale(1.15);
}
</style>
