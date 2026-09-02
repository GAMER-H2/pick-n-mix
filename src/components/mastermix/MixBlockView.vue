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
 * *what* was grabbed.
 */
import { computed, onMounted, ref, watch } from "vue";
import type { MixBlock, MixEntry, Waveform } from "@/lib/types";

const props = defineProps<{
  block: MixBlock;
  entry: MixEntry | null;
  waveform: Waveform | null;
  pixelsPerSecond: number;
  height: number;
  selected: boolean;
  /** Hue for this lane, so a song is recognisable at a glance. */
  hue: number;
  tool: "select" | "blade" | "automation";
}>();

const emit = defineEmits<{
  (e: "grab", payload: { event: PointerEvent; mode: "move" | "trim-start" | "trim-end" }): void;
}>();

const canvas = ref<HTMLCanvasElement | null>(null);

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
  const from = props.block.offsetSecs;
  const secondsPerPixel = props.block.durationSecs / cssWidth;

  // One column per pixel, taking the loudest peak the column spans. Zoomed
  // out that is many peaks per column, which is exactly what keeps a
  // transient visible instead of averaging it away.
  for (let x = 0; x < cssWidth; x += 1) {
    const startPeak = Math.floor((from + x * secondsPerPixel) * waveform.peaksPerSec);
    const endPeak = Math.max(
      startPeak + 1,
      Math.floor((from + (x + 1) * secondsPerPixel) * waveform.peaksPerSec),
    );
    let peak = 0;
    for (let i = Math.max(0, startPeak); i < endPeak && i < waveform.peaks.length; i += 1) {
      peak = Math.max(peak, waveform.peaks[i]);
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
  ],
  () => draw(),
);
</script>

<template>
  <div
    class="block"
    :class="{ 'is-selected': selected, 'is-missing': missing, 'is-blade': tool === 'blade' }"
    :style="{ left: `${left}px`, width: `${width}px`, '--lane-hue': hue }"
    :title="label"
    @pointerdown.stop="emit('grab', { event: $event, mode: 'move' })"
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
