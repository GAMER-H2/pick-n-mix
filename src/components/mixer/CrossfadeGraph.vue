<script setup lang="ts">
/**
 * The keyframe graph from the advanced mixer sketch: four points on a shared
 * timeline, drawn as two curves rather than one.
 *
 * The sketch draws a single dipping line, which reads naturally as "one
 * curve" — but at the default (symmetric) shape, the outgoing song's fade and
 * the incoming song's rise share both of their x positions (they start and
 * end the window together), and with only x editable per point, two curves
 * drawn in one colour would put two independently-draggable handles on top of
 * each other. Splitting the two songs into two colours keeps every handle
 * reachable regardless of the shape, and is a closer match to what the DSP
 * actually computes: two independent envelopes, not one.
 */
import { computed, ref, watch } from "vue";
import { clampCurve, gainIn, gainOut, symmetricCurve } from "@/lib/crossfadeCurve";
import type { CrossfadeCurve } from "@/lib/types";

const props = withDefaults(
  defineProps<{
    curve: CrossfadeCurve;
    lengthSecs: number;
    disabled?: boolean;
  }>(),
  { disabled: false },
);

const emit = defineEmits<{ change: [curve: CrossfadeCurve] }>();

type Handle = "fadeOutStart" | "fadeOutEnd" | "fadeInStart" | "fadeInEnd";

const svgEl = ref<SVGSVGElement | null>(null);
const dragging = ref<Handle | null>(null);

/** A short visual domain even when crossfading is off, so the graph never
 * collapses to a single point and stays legible while disabled. */
const displayLength = computed(() => Math.max(props.lengthSecs, 0.5));

/** Live working copy: the source of truth while a drag is in progress, so
 * dragging one point sees the other point of the same curve move too rather
 * than clamping against a stale prop. Committed on pointerup only. */
const draft = ref<CrossfadeCurve>(clampCurve(props.curve, props.lengthSecs));

watch(
  () => [props.curve, props.lengthSecs] as const,
  ([curve, length]) => {
    if (dragging.value === null) draft.value = clampCurve(curve, length);
  },
  { deep: true },
);

function clamp01(v: number, min: number, max: number) {
  return Math.min(max, Math.max(min, v));
}

function xPercent(x: number): number {
  const min = -displayLength.value;
  const max = displayLength.value;
  return ((clamp01(x, min, max) - min) / (max - min)) * 100;
}

function yPercent(gain01: number): number {
  return (1 - gain01) * 100;
}

const zeroPercent = computed(() => xPercent(0));

const SAMPLES = 48;
function pathFor(gainFn: (curve: CrossfadeCurve, x: number) => number): string {
  const min = -displayLength.value;
  const max = displayLength.value;
  const points: string[] = [];
  for (let i = 0; i <= SAMPLES; i += 1) {
    const x = min + (i / SAMPLES) * (max - min);
    const px = xPercent(x);
    const py = yPercent(gainFn(draft.value, x));
    points.push(`${i === 0 ? "M" : "L"}${px.toFixed(2)},${py.toFixed(2)}`);
  }
  return points.join(" ");
}

const outPath = computed(() => pathFor(gainOut));
const inPath = computed(() => pathFor(gainIn));

const handles = computed(() => [
  { id: "fadeOutStart" as Handle, x: draft.value.fadeOutStart, y: 1, colour: "out" as const },
  { id: "fadeOutEnd" as Handle, x: draft.value.fadeOutEnd, y: 0, colour: "out" as const },
  { id: "fadeInStart" as Handle, x: draft.value.fadeInStart, y: 0, colour: "in" as const },
  { id: "fadeInEnd" as Handle, x: draft.value.fadeInEnd, y: 1, colour: "in" as const },
]);

function xAtPointer(clientX: number): number {
  const rect = svgEl.value?.getBoundingClientRect();
  if (!rect || rect.width === 0) return 0;
  const ratio = clamp01((clientX - rect.left) / rect.width, 0, 1);
  const min = -displayLength.value;
  const max = displayLength.value;
  return min + ratio * (max - min);
}

function updateHandle(handle: Handle, x: number) {
  // Snapped to a tenth of a second: a graph this size cannot usefully be
  // dragged more precisely than that by pointer alone.
  const snapped = Math.round(x * 10) / 10;
  draft.value = clampCurve({ ...draft.value, [handle]: snapped }, props.lengthSecs);
}

function onPointerDown(handle: Handle, event: PointerEvent) {
  if (props.disabled) return;
  (event.currentTarget as Element).setPointerCapture(event.pointerId);
  dragging.value = handle;
  updateHandle(handle, xAtPointer(event.clientX));
}

function onPointerMove(event: PointerEvent) {
  if (dragging.value === null) return;
  updateHandle(dragging.value, xAtPointer(event.clientX));
}

function onPointerUp(event: PointerEvent) {
  if (dragging.value === null) return;
  (event.currentTarget as Element).releasePointerCapture(event.pointerId);
  dragging.value = null;
  emit("change", draft.value);
}

/** Double-click a handle to send it back to the symmetric default — the
 * escape hatch for when a drag has produced a shape that is hard to undo. */
function onDoubleClick(handle: Handle) {
  if (props.disabled) return;
  const sym = symmetricCurve(props.lengthSecs);
  draft.value = clampCurve({ ...draft.value, [handle]: sym[handle] }, props.lengthSecs);
  emit("change", draft.value);
}

function resetAll() {
  if (props.disabled) return;
  draft.value = symmetricCurve(props.lengthSecs);
  emit("change", draft.value);
}

function fmt(secs: number): string {
  return `${secs > 0 ? "+" : ""}${secs.toFixed(1)}s`;
}
</script>

<template>
  <div class="graph" :class="{ 'is-disabled': disabled }">
    <div class="graph__head">
      <div class="graph__legend">
        <span class="graph__swatch graph__swatch--out" />
        <span>Outgoing</span>
        <span class="graph__swatch graph__swatch--in" />
        <span>Incoming</span>
      </div>
      <button class="graph__reset" @click="resetAll">Reset</button>
    </div>

    <svg
      ref="svgEl"
      class="graph__plot"
      viewBox="0 0 100 100"
      preserveAspectRatio="none"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
    >
      <!-- Gridlines -->
      <line x1="0" y1="0" x2="100" y2="0" class="graph__rail" vector-effect="non-scaling-stroke" />
      <line
        x1="0"
        y1="100"
        x2="100"
        y2="100"
        class="graph__rail"
        vector-effect="non-scaling-stroke"
      />
      <line
        :x1="zeroPercent"
        y1="0"
        :x2="zeroPercent"
        y2="100"
        class="graph__zero"
        vector-effect="non-scaling-stroke"
      />

      <path :d="outPath" class="graph__curve graph__curve--out" vector-effect="non-scaling-stroke" />
      <path :d="inPath" class="graph__curve graph__curve--in" vector-effect="non-scaling-stroke" />

      <g v-for="h in handles" :key="h.id">
        <!-- Generous invisible hit target around a small visible dot. -->
        <circle
          :cx="xPercent(h.x)"
          :cy="yPercent(h.y)"
          r="7"
          class="graph__hit"
          :class="`graph__hit--${h.colour}`"
          vector-effect="non-scaling-stroke"
          @pointerdown="onPointerDown(h.id, $event)"
          @dblclick="onDoubleClick(h.id)"
        />
        <circle
          :cx="xPercent(h.x)"
          :cy="yPercent(h.y)"
          r="2.6"
          class="graph__handle"
          :class="[`graph__handle--${h.colour}`, { 'is-dragging': dragging === h.id }]"
          vector-effect="non-scaling-stroke"
        />
      </g>
    </svg>

    <div class="graph__axis">
      <span>{{ fmt(-displayLength) }}</span>
      <span class="graph__axis-zero">Track ends</span>
      <span>{{ fmt(displayLength) }}</span>
    </div>

    <dl class="graph__readout">
      <div>
        <dt><span class="graph__swatch graph__swatch--out" />Outgoing</dt>
        <dd>{{ fmt(draft.fadeOutStart) }} → {{ fmt(draft.fadeOutEnd) }}</dd>
      </div>
      <div>
        <dt><span class="graph__swatch graph__swatch--in" />Incoming</dt>
        <dd>{{ fmt(draft.fadeInStart) }} → {{ fmt(draft.fadeInEnd) }}</dd>
      </div>
    </dl>
  </div>
</template>

<style scoped>
.graph {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.graph.is-disabled {
  opacity: 0.45;
  pointer-events: none;
}

.graph__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.graph__legend {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 10.5px;
  color: var(--text-tertiary);
}

.graph__swatch {
  display: inline-block;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  margin-right: 1px;
}

.graph__swatch--out {
  background: var(--accent);
}

.graph__swatch--in {
  background: var(--accent-secondary);
}

.graph__reset {
  font-size: 11px;
  color: var(--accent);
}

.graph__plot {
  width: 100%;
  height: 130px;
  border-radius: var(--radius-sm);
  background: var(--bg-sunken);
  border: 0.5px solid var(--separator);
  touch-action: none;
  overflow: visible;
}

.graph__rail {
  stroke: var(--separator);
  stroke-width: 1;
}

.graph__zero {
  stroke: var(--separator-strong);
  stroke-width: 1;
  stroke-dasharray: 3 3;
}

.graph__curve {
  fill: none;
  stroke-width: 2;
}

.graph__curve--out {
  stroke: var(--accent);
}

.graph__curve--in {
  stroke: var(--accent-secondary);
}

.graph__hit {
  fill: transparent;
  cursor: ew-resize;
}

.graph__handle {
  fill: #fff;
  stroke-width: 2;
  pointer-events: none;
  transition: r 0.1s var(--ease);
}

.graph__handle--out {
  stroke: var(--accent);
}

.graph__handle--in {
  stroke: var(--accent-secondary);
}

.graph__handle.is-dragging {
  r: 3.6;
}

.graph__axis {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 9.5px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
}

.graph__axis-zero {
  font-size: 9px;
}

.graph__readout {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px;
  margin: 2px 0 0;
}

.graph__readout dt {
  display: flex;
  align-items: center;
  font-size: 10px;
  color: var(--text-tertiary);
}

.graph__readout dd {
  margin: 1px 0 0;
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}
</style>
