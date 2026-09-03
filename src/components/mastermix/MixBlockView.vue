<script setup lang="ts">
/**
 * One audio region on the timeline.
 *
 * Draws the slice of its song's waveform that the block actually covers, so
 * trimming an edge or splitting a block shows the audio moving under the cut
 * rather than a generic shape being squashed.
 *
 * Dragging is not handled here: the modal owns it, because a move can cross
 * lanes and a block cannot see its neighbours. This component only reports
 * *what* was grabbed — including volume keyframes when the automation tool
 * is armed.
 */
import { computed, onMounted, ref, watch } from "vue";
import { MAX_GAIN_DB, MIN_GAIN_DB, automationGainAt } from "@/lib/masterMix";
import type { MixBlock, MixEntry, Waveform } from "@/lib/types";

const props = defineProps<{
  block: MixBlock;
  entry: MixEntry | null;
  waveform: Waveform | null;
  /** Varispeed, as source seconds per timeline second. */
  speed: number;
  pixelsPerSecond: number;
  height: number;
  selected: boolean;
  /** Hue for this lane, so a song is recognisable at a glance. */
  hue: number;
  tool: "select" | "blade" | "automation";
}>();

const emit = defineEmits<{
  (e: "grab", payload: { event: PointerEvent; mode: "move" | "trim-start" | "trim-end" }): void;
  (
    e: "automation",
    payload: {
      event: PointerEvent;
      mode: "add" | "move-point" | "curve" | "remove";
      index?: number;
      atSecs: number;
      gainDb: number;
    },
  ): void;
  /** Double-click, which every editor uses to open what was clicked. */
  (e: "openMixer"): void;
}>();

const canvas = ref<HTMLCanvasElement | null>(null);
const overlay = ref<SVGSVGElement | null>(null);

const width = computed(() => Math.max(2, props.block.durationSecs * props.pixelsPerSecond));
const left = computed(() => props.block.startSecs * props.pixelsPerSecond);
const sourceLabel = computed(() =>
  props.block.source.kind === "asset" ? props.block.source.file : "Missing song",
);
const label = computed(() => props.entry?.title ?? sourceLabel.value);
const missing = computed(() => props.entry !== null && !props.entry.available);

/** Trim handles get in the way of a blade cut, so they only exist for the
 *  pointer tool. */
const trimmable = computed(() => props.tool === "select" && width.value > 22);
const automating = computed(() => props.tool === "automation");
const showEnvelope = computed(
  () => automating.value || props.block.automation.length > 0,
);

function gainToY(gainDb: number, height: number): number {
  const t = (gainDb - MIN_GAIN_DB) / (MAX_GAIN_DB - MIN_GAIN_DB);
  return (1 - t) * height;
}

function pointAt(event: PointerEvent): { atSecs: number; gainDb: number } {
  const element = overlay.value;
  if (!element) return { atSecs: 0, gainDb: 0 };
  const box = element.getBoundingClientRect();
  const x = event.clientX - box.left;
  const y = event.clientY - box.top;
  const atSecs = (x / Math.max(1, box.width)) * props.block.durationSecs;
  const t = 1 - y / Math.max(1, box.height);
  const gainDb = MIN_GAIN_DB + t * (MAX_GAIN_DB - MIN_GAIN_DB);
  return {
    atSecs: Math.min(props.block.durationSecs, Math.max(0, atSecs)),
    gainDb: Math.min(MAX_GAIN_DB, Math.max(MIN_GAIN_DB, gainDb)),
  };
}

const envelopePoints = computed(() => {
  const w = width.value;
  const h = Math.max(1, props.height - 6);
  const n = Math.max(2, Math.ceil(w / 3));
  const parts: string[] = [];
  for (let i = 0; i <= n; i += 1) {
    const t = i / n;
    const db = automationGainAt(props.block.automation, t * props.block.durationSecs);
    parts.push(`${t * w},${gainToY(db, h)}`);
  }
  return parts.join(" ");
});

const zeroY = computed(() => gainToY(0, Math.max(1, props.height - 6)));

const keyframes = computed(() => {
  const h = Math.max(1, props.height - 6);
  const w = width.value;
  const dur = Math.max(props.block.durationSecs, 0.0001);
  return props.block.automation.map((point, index) => ({
    index,
    cx: (point.atSecs / dur) * w,
    cy: gainToY(point.gainDb, h),
    point,
  }));
});

const curveHandles = computed(() => {
  const h = Math.max(1, props.height - 6);
  const w = width.value;
  const dur = Math.max(props.block.durationSecs, 0.0001);
  const points = props.block.automation;
  const handles: { index: number; cx: number; cy: number }[] = [];
  for (let i = 0; i < points.length - 1; i += 1) {
    const a = points[i];
    const b = points[i + 1];
    const midSecs = (a.atSecs + b.atSecs) / 2;
    handles.push({
      index: i,
      cx: (midSecs / dur) * w,
      cy: gainToY(automationGainAt(points, midSecs), h),
    });
  }
  return handles;
});

function onBodyDown(event: PointerEvent) {
  if (automating.value) return;
  emit("grab", { event, mode: "move" });
}

function onOverlayDown(event: PointerEvent) {
  if (!automating.value || event.button !== 0) return;
  const at = pointAt(event);
  emit("automation", { event, mode: "add", atSecs: at.atSecs, gainDb: at.gainDb });
}

function onPointDown(event: PointerEvent, index: number) {
  if (!automating.value || event.button !== 0) return;
  event.stopPropagation();
  const at = pointAt(event);
  emit("automation", {
    event,
    mode: "move-point",
    index,
    atSecs: at.atSecs,
    gainDb: at.gainDb,
  });
}

function onPointRemove(event: MouseEvent, index: number) {
  if (!automating.value) return;
  event.stopPropagation();
  emit("automation", {
    event: event as unknown as PointerEvent,
    mode: "remove",
    index,
    atSecs: 0,
    gainDb: 0,
  });
}

function onCurveDown(event: PointerEvent, index: number) {
  if (!automating.value || event.button !== 0) return;
  event.stopPropagation();
  const at = pointAt(event);
  emit("automation", {
    event,
    mode: "curve",
    index,
    atSecs: at.atSecs,
    gainDb: at.gainDb,
  });
}

function draw() {
  const element = canvas.value;
  if (!element) return;
  const ratio = window.devicePixelRatio || 1;
  const cssWidth = Math.max(1, Math.round(width.value));
  const cssHeight = Math.max(1, props.height - 24);
  element.width = Math.round(cssWidth * ratio);
  element.height = Math.round(cssHeight * ratio);
  element.style.width = `${cssWidth}px`;
  element.style.height = `${cssHeight}px`;

  const ctx = element.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  ctx.clearRect(0, 0, cssWidth, cssHeight);

  const waveform = props.waveform;
  if (!waveform || waveform.peaks.length === 0) return;

  ctx.fillStyle = `hsl(${props.hue} 70% 26% / 0.8)`;
  const middle = cssHeight / 2;
  const fromPeak = Math.floor(props.block.offsetSecs * waveform.peaksPerSec);
  const loopPeaks = Math.max(0, waveform.peaks.length - fromPeak);
  // Source seconds, not timeline seconds: a region playing at double speed
  // covers twice as much of the song in the same width.
  const sourceSecs = props.block.durationSecs * Math.max(props.speed, 0.01);
  const peaksPerPixel = (sourceSecs * waveform.peaksPerSec) / cssWidth;

  // Timeline peaks beyond EOF wrap to the block's offset. This mirrors the
  // audio source, so extending a region shows each repeated pass explicitly.
  for (let x = 0; x < cssWidth; x += 1) {
    const startPeak = Math.floor(x * peaksPerPixel);
    const endPeak = Math.max(startPeak + 1, Math.floor((x + 1) * peaksPerPixel));
    let peak = 0;
    if (loopPeaks > 0) {
      for (let i = startPeak; i < endPeak; i += 1) {
        peak = Math.max(peak, waveform.peaks[fromPeak + (i % loopPeaks)] ?? 0);
      }
    }
    if (peak === 0) continue;
    const half = (peak / 255) * middle;
    ctx.fillRect(x, middle - half, 1, Math.max(1, half * 2));
  }
}

onMounted(draw);
watch(
  () => [
    props.waveform,
    props.pixelsPerSecond,
    props.height,
    props.hue,
    props.block.offsetSecs,
    props.block.durationSecs,
    props.speed,
  ],
  () => draw(),
);
</script>

<template>
  <div
    class="block"
    :class="{
      'is-selected': selected,
      'is-missing': missing,
      'is-blade': tool === 'blade',
      'is-auto': automating,
    }"
    :style="{ left: `${left}px`, width: `${width}px`, '--lane-hue': hue }"
    :title="label"
    @pointerdown.stop="onBodyDown"
    @dblclick.stop="emit('openMixer')"
  >
    <div class="block__label">{{ label }}</div>
    <canvas ref="canvas" class="block__wave" />

    <!-- Fades draw as the wedges a timeline editor uses, so a hand-made
         crossfade is legible without opening anything. -->
    <div
      v-if="block.fadeInSecs > 0"
      class="block__fade block__fade--in"
      :style="{ width: `${block.fadeInSecs * pixelsPerSecond}px` }"
    />
    <div
      v-if="block.fadeOutSecs > 0"
      class="block__fade block__fade--out"
      :style="{ width: `${block.fadeOutSecs * pixelsPerSecond}px` }"
    />

    <svg
      v-if="showEnvelope"
      ref="overlay"
      class="block__auto"
      :class="{ 'is-live': automating }"
      :viewBox="`0 0 ${width} ${Math.max(1, height - 6)}`"
      preserveAspectRatio="none"
      @pointerdown.stop="onOverlayDown"
    >
      <line
        class="block__auto-zero"
        x1="0"
        :y1="zeroY"
        :x2="width"
        :y2="zeroY"
      />
      <polyline class="block__auto-line" :points="envelopePoints" />
      <template v-if="automating">
        <circle
          v-for="handle in curveHandles"
          :key="`c${handle.index}`"
          class="block__auto-curve"
          :cx="handle.cx"
          :cy="handle.cy"
          r="3.5"
          @pointerdown.stop="onCurveDown($event, handle.index)"
        />
        <circle
          v-for="key in keyframes"
          :key="`k${key.index}`"
          class="block__auto-point"
          :cx="key.cx"
          :cy="key.cy"
          r="5"
          @pointerdown.stop="onPointDown($event, key.index)"
          @dblclick.stop="onPointRemove($event, key.index)"
        />
      </template>
    </svg>

    <template v-if="trimmable">
      <div
        class="block__handle block__handle--start"
        @pointerdown.stop="emit('grab', { event: $event, mode: 'trim-start' })"
      />
      <div
        class="block__handle block__handle--end"
        @pointerdown.stop="emit('grab', { event: $event, mode: 'trim-end' })"
      />
    </template>
  </div>
</template>

<style scoped>
.block {
  position: absolute;
  top: 3px;
  bottom: 3px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border-radius: var(--radius-sm);
  border: 1px solid hsl(var(--lane-hue) 55% 40%);
  background: hsl(var(--lane-hue) 60% 62%);
  cursor: grab;
  touch-action: none;
  user-select: none;
}

.block.is-blade {
  cursor: crosshair;
}

.block.is-auto {
  cursor: crosshair;
}

.block.is-selected {
  border-color: var(--text);
  box-shadow: 0 0 0 1.5px var(--text);
}

/* A song this library does not have still shows where it sits, but nothing
   about it is solid, because none of it will be heard. */
.block.is-missing {
  background: repeating-linear-gradient(
    45deg,
    var(--bg-sunken),
    var(--bg-sunken) 6px,
    var(--bg-hover) 6px,
    var(--bg-hover) 12px
  );
  border-style: dashed;
}

.block__label {
  flex: none;
  padding: 2px 6px;
  font-size: 10.5px;
  font-weight: 600;
  line-height: 15px;
  color: hsl(var(--lane-hue) 70% 16%);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  pointer-events: none;
}

.block.is-missing .block__label {
  color: var(--text-tertiary);
}

.block__wave {
  flex: 1;
  min-height: 0;
  pointer-events: none;
}

.block__fade {
  position: absolute;
  top: 0;
  bottom: 0;
  pointer-events: none;
  opacity: 0.55;
  background: linear-gradient(to right, var(--bg) 0%, transparent 100%);
}

.block__fade--in {
  left: 0;
}

.block__fade--out {
  right: 0;
  background: linear-gradient(to left, var(--bg) 0%, transparent 100%);
}

.block__auto {
  position: absolute;
  inset: 0;
  overflow: visible;
  pointer-events: none;
  background: rgba(0, 0, 0, 0.28);
}

.block__auto.is-live {
  pointer-events: auto;
  cursor: crosshair;
}

.block__auto-zero {
  stroke: hsl(var(--lane-hue) 70% 16% / 0.35);
  stroke-width: 1;
  stroke-dasharray: 4 3;
  vector-effect: non-scaling-stroke;
}

.block__auto-line {
  fill: none;
  stroke: var(--accent);
  stroke-width: 2;
  vector-effect: non-scaling-stroke;
}

.block__auto-point {
  fill: var(--bg-elevated);
  stroke: hsl(var(--lane-hue) 80% 16%);
  stroke-width: 1.5;
  cursor: grab;
  vector-effect: non-scaling-stroke;
}

.block__auto-curve {
  fill: hsl(var(--lane-hue) 70% 30%);
  stroke: hsl(var(--lane-hue) 80% 16%);
  stroke-width: 1;
  cursor: ns-resize;
  vector-effect: non-scaling-stroke;
}

.block__handle {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 7px;
  cursor: ew-resize;
}

.block__handle--start {
  left: 0;
}

.block__handle--end {
  right: 0;
}

.block__handle:hover {
  background: hsl(var(--lane-hue) 80% 35% / 0.5);
}
</style>
