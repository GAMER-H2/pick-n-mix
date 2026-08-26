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
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  clampCurve,
  gainIn,
  gainOut,
  MAX_FADE_SHAPE,
  MIN_FADE_SHAPE,
  symmetricCurve,
} from "@/lib/crossfadeCurve";
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

type PointHandle = "fadeOutStart" | "fadeOutEnd" | "fadeInStart" | "fadeInEnd";
type ShapeHandle = "fadeOutShape" | "fadeInShape";
type DragTarget = PointHandle | ShapeHandle;

const svgEl = ref<SVGSVGElement | null>(null);
const dragging = ref<DragTarget | null>(null);

// The SVG intentionally stretches to the available width. Store its rendered
// dimensions so ellipse radii can be expressed in SVG units while remaining
// circular in screen pixels.
const plotSize = ref({ width: 0, height: 0 });
const HIT_RADIUS_PX = 10;
const HANDLE_RADIUS_PX = 4;
const SHAPE_HANDLE_RADIUS_PX = 3;
const PLOT_INSET_PX = HIT_RADIUS_PX + 2;

let resizeObserver: ResizeObserver | undefined;

function updatePlotSize() {
  const rect = svgEl.value?.getBoundingClientRect();
  if (rect) plotSize.value = { width: rect.width, height: rect.height };
}

onMounted(() => {
  updatePlotSize();
  resizeObserver = new ResizeObserver(updatePlotSize);
  if (svgEl.value) resizeObserver.observe(svgEl.value);
});

onBeforeUnmount(() => resizeObserver?.disconnect());

function xSvgUnits(pixels: number): number {
  return (pixels / Math.max(plotSize.value.width, 1)) * 100;
}

function ySvgUnits(pixels: number): number {
  return (pixels / Math.max(plotSize.value.height, 1)) * 100;
}

const insetX = computed(() => Math.min(xSvgUnits(PLOT_INSET_PX), 45));
const insetY = computed(() => Math.min(ySvgUnits(PLOT_INSET_PX), 45));
const hitRadiusX = computed(() => xSvgUnits(HIT_RADIUS_PX));
const hitRadiusY = computed(() => ySvgUnits(HIT_RADIUS_PX));
const handleRadiusX = computed(() => xSvgUnits(HANDLE_RADIUS_PX));
const handleRadiusY = computed(() => ySvgUnits(HANDLE_RADIUS_PX));

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
  const ratio = (clamp01(x, min, max) - min) / (max - min);
  return insetX.value + ratio * (100 - 2 * insetX.value);
}

function yPercent(gain01: number): number {
  return insetY.value + (1 - gain01) * (100 - 2 * insetY.value);
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
  { id: "fadeOutStart" as PointHandle, x: draft.value.fadeOutStart, y: 1, colour: "out" as const },
  { id: "fadeOutEnd" as PointHandle, x: draft.value.fadeOutEnd, y: 0, colour: "out" as const },
  { id: "fadeInStart" as PointHandle, x: draft.value.fadeInStart, y: 0, colour: "in" as const },
  { id: "fadeInEnd" as PointHandle, x: draft.value.fadeInEnd, y: 1, colour: "in" as const },
]);

// Position the two shape controls on different central sections. At the
// default equal-power shape both curves cross at their exact midpoint, so
// placing both controls there would make one impossible to select.
const shapeControls = computed(() => {
  const outProgress = 0.4;
  const inProgress = 0.6;
  const outX = draft.value.fadeOutStart + (draft.value.fadeOutEnd - draft.value.fadeOutStart) * outProgress;
  const inX = draft.value.fadeInStart + (draft.value.fadeInEnd - draft.value.fadeInStart) * inProgress;

  return [
    {
      id: "fadeOutShape" as ShapeHandle,
      x: outX,
      y: gainOut(draft.value, outX),
      progress: outProgress,
      colour: "out" as const,
    },
    {
      id: "fadeInShape" as ShapeHandle,
      x: inX,
      y: gainIn(draft.value, inX),
      progress: inProgress,
      colour: "in" as const,
    },
  ];
});

function xAtPointer(clientX: number): number {
  const rect = svgEl.value?.getBoundingClientRect();
  if (!rect || rect.width === 0) return 0;
  const inset = Math.min(PLOT_INSET_PX, rect.width / 2);
  const usableWidth = Math.max(rect.width - 2 * inset, 1);
  const ratio = clamp01((clientX - rect.left - inset) / usableWidth, 0, 1);
  const min = -displayLength.value;
  const max = displayLength.value;
  return min + ratio * (max - min);
}

function updateHandle(handle: PointHandle, x: number) {
  // Snapped to a tenth of a second: a graph this size cannot usefully be
  // dragged more precisely than that by pointer alone.
  const snapped = Math.round(x * 10) / 10;
  draft.value = clampCurve({ ...draft.value, [handle]: snapped }, props.lengthSecs);
}

function gainAtPointer(clientY: number): number {
  const rect = svgEl.value?.getBoundingClientRect();
  if (!rect || rect.height === 0) return 0.5;
  const inset = Math.min(PLOT_INSET_PX, rect.height / 2);
  const usableHeight = Math.max(rect.height - 2 * inset, 1);
  const ratio = clamp01((clientY - rect.top - inset) / usableHeight, 0, 1);
  // Keeping the control off the rails avoids an abrupt, effectively instant
  // segment while still allowing a very early or very late volume move.
  return clamp01(1 - ratio, 0.01, 0.99);
}

function updateShape(handle: ShapeHandle, clientY: number) {
  const control = shapeControls.value.find((candidate) => candidate.id === handle);
  if (!control) return;

  const gain = gainAtPointer(clientY);
  const easedTime = handle === "fadeOutShape"
    ? Math.acos(gain) / (Math.PI / 2)
    : Math.asin(gain) / (Math.PI / 2);
  const shape = clamp01(
    Math.log(easedTime) / Math.log(control.progress),
    MIN_FADE_SHAPE,
    MAX_FADE_SHAPE,
  );
  draft.value = clampCurve({ ...draft.value, [handle]: shape }, props.lengthSecs);
}

function isShapeHandle(handle: DragTarget): handle is ShapeHandle {
  return handle === "fadeOutShape" || handle === "fadeInShape";
}

function onPointerDown(handle: DragTarget, event: PointerEvent) {
  if (props.disabled) return;
  (event.currentTarget as Element).setPointerCapture(event.pointerId);
  dragging.value = handle;
  if (isShapeHandle(handle)) updateShape(handle, event.clientY);
  else updateHandle(handle, xAtPointer(event.clientX));
}

function onPointerMove(event: PointerEvent) {
  if (dragging.value === null) return;
  if (isShapeHandle(dragging.value)) updateShape(dragging.value, event.clientY);
  else updateHandle(dragging.value, xAtPointer(event.clientX));
}

function onPointerUp(event: PointerEvent) {
  if (dragging.value === null) return;
  (event.currentTarget as Element).releasePointerCapture(event.pointerId);
  dragging.value = null;
  emit("change", draft.value);
}

/** Double-click a handle to send it back to the symmetric default — the
 * escape hatch for when a drag has produced a shape that is hard to undo. */
function onDoubleClick(handle: DragTarget) {
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
      <line
        :x1="insetX"
        :y1="insetY"
        :x2="100 - insetX"
        :y2="insetY"
        class="graph__rail"
        vector-effect="non-scaling-stroke"
      />
      <line
        :x1="insetX"
        :y1="100 - insetY"
        :x2="100 - insetX"
        :y2="100 - insetY"
        class="graph__rail"
        vector-effect="non-scaling-stroke"
      />
      <line
        :x1="zeroPercent"
        :y1="insetY"
        :x2="zeroPercent"
        :y2="100 - insetY"
        class="graph__zero"
        vector-effect="non-scaling-stroke"
      />

      <path :d="outPath" class="graph__curve graph__curve--out" vector-effect="non-scaling-stroke" />
      <path :d="inPath" class="graph__curve graph__curve--in" vector-effect="non-scaling-stroke" />

      <!-- Drag these central controls vertically to change each envelope's timing shape. -->
      <g v-for="control in shapeControls" :key="control.id">
        <ellipse
          :cx="xPercent(control.x)"
          :cy="yPercent(control.y)"
          :rx="hitRadiusX"
          :ry="hitRadiusY"
          class="graph__hit graph__hit--shape"
          @pointerdown="onPointerDown(control.id, $event)"
          @dblclick="onDoubleClick(control.id)"
        />
        <ellipse
          :cx="xPercent(control.x)"
          :cy="yPercent(control.y)"
          :rx="dragging === control.id ? xSvgUnits(SHAPE_HANDLE_RADIUS_PX + 1) : xSvgUnits(SHAPE_HANDLE_RADIUS_PX)"
          :ry="dragging === control.id ? ySvgUnits(SHAPE_HANDLE_RADIUS_PX + 1) : ySvgUnits(SHAPE_HANDLE_RADIUS_PX)"
          class="graph__handle graph__shape-handle"
          :class="[`graph__handle--${control.colour}`, { 'is-dragging': dragging === control.id }]"
          vector-effect="non-scaling-stroke"
        />
      </g>

      <g v-for="h in handles" :key="h.id">
        <!-- Generous invisible hit target around a small visible dot. -->
        <ellipse
          :cx="xPercent(h.x)"
          :cy="yPercent(h.y)"
          :rx="hitRadiusX"
          :ry="hitRadiusY"
          class="graph__hit"
          :class="`graph__hit--${h.colour}`"
          @pointerdown="onPointerDown(h.id, $event)"
          @dblclick="onDoubleClick(h.id)"
        />
        <ellipse
          :cx="xPercent(h.x)"
          :cy="yPercent(h.y)"
          :rx="dragging === h.id ? xSvgUnits(HANDLE_RADIUS_PX + 1) : handleRadiusX"
          :ry="dragging === h.id ? ySvgUnits(HANDLE_RADIUS_PX + 1) : handleRadiusY"
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
    <p class="graph__shape-hint">Drag the middle dots up or down to shape each fade.</p>
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
  overflow: hidden;
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

.graph__hit--shape {
  cursor: ns-resize;
}

.graph__handle {
  fill: #fff;
  stroke-width: 2;
  pointer-events: none;
}

.graph__handle--out {
  stroke: var(--accent);
}

.graph__handle--in {
  stroke: var(--accent-secondary);
}

.graph__shape-handle {
  stroke: none;
}

.graph__shape-handle.graph__handle--out {
  fill: var(--accent);
}

.graph__shape-handle.graph__handle--in {
  fill: var(--accent-secondary);
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

.graph__shape-hint {
  margin: 1px 0 0;
  font-size: 9.5px;
  color: var(--text-tertiary);
}
</style>
