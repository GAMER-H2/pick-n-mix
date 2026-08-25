<script setup lang="ts">
/**
 * The boxed row of vertical faders from the simple mixer drawing.
 * Each fader is one EQ band's gain; frequency and Q are edited in the
 * advanced panel.
 */
import { computed, ref } from "vue";
import type { Eq } from "@/lib/types";

const props = defineProps<{ eq: Eq; height?: number }>();
const emit = defineEmits<{ change: [eq: Eq] }>();

const RANGE = 12;
const height = computed(() => props.height ?? 108);
const dragging = ref<number | null>(null);

function fraction(gainDb: number) {
  return (gainDb + RANGE) / (RANGE * 2);
}

/** Signed dB, with a decimal only when one is needed, so a small nudge
 * reads as "+0.2" rather than the misleading "+0". */
function gainLabel(gainDb: number) {
  if (Math.abs(gainDb) < 0.05) return "0";
  const rounded = Math.abs(gainDb) < 1 ? gainDb.toFixed(1) : gainDb.toFixed(0);
  return gainDb > 0 ? `+${rounded}` : rounded;
}

function labelFor(freq: number) {
  return freq >= 1000 ? `${Math.round(freq / 100) / 10}k` : `${Math.round(freq)}`;
}

function setBand(index: number, gainDb: number) {
  const bands = props.eq.bands.map((b, i) =>
    i === index ? { ...b, gainDb: Math.max(-RANGE, Math.min(RANGE, gainDb)) } : b,
  );
  emit("change", { ...props.eq, bands });
}

function gainAt(event: PointerEvent, el: HTMLElement) {
  const rect = el.getBoundingClientRect();
  const ratio = 1 - Math.min(1, Math.max(0, (event.clientY - rect.top) / rect.height));
  return Number((ratio * RANGE * 2 - RANGE).toFixed(1));
}

function onPointerDown(event: PointerEvent, index: number) {
  const el = event.currentTarget as HTMLElement;
  el.setPointerCapture(event.pointerId);
  dragging.value = index;
  setBand(index, gainAt(event, el));
}

function onPointerMove(event: PointerEvent, index: number) {
  if (dragging.value !== index) return;
  setBand(index, gainAt(event, event.currentTarget as HTMLElement));
}

function onPointerUp(event: PointerEvent) {
  dragging.value = null;
  (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
}

/** Double-click a fader to return that band to flat. */
function onDoubleClick(index: number) {
  setBand(index, 0);
}
</script>

<template>
  <div class="eq">
    <div
      v-for="(band, index) in eq.bands"
      :key="index"
      class="eq__band"
      :style="{ height: `${height}px` }"
      @pointerdown="onPointerDown($event, index)"
      @pointermove="onPointerMove($event, index)"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
      @dblclick="onDoubleClick(index)"
    >
      <div class="eq__track">
        <div class="eq__zero" />
        <div
          class="eq__fill"
          :style="{
            top: band.gainDb >= 0 ? `${(1 - fraction(band.gainDb)) * 100}%` : '50%',
            height: `${(Math.abs(band.gainDb) / (RANGE * 2)) * 100}%`,
          }"
        />
        <div class="eq__thumb" :style="{ top: `${(1 - fraction(band.gainDb)) * 100}%` }" />
      </div>
      <div class="eq__label">{{ labelFor(band.freq) }}</div>
      <div class="eq__value">{{ gainLabel(band.gainDb) }}</div>
    </div>
  </div>
</template>

<style scoped>
.eq {
  display: flex;
  justify-content: space-between;
  gap: 4px;
  padding: 12px 10px 8px;
  border-radius: var(--radius);
  background: var(--bg-sunken);
  border: 0.5px solid var(--separator);
}

.eq__band {
  display: flex;
  flex-direction: column;
  align-items: center;
  flex: 1;
  cursor: ns-resize;
  touch-action: none;
}

.eq__track {
  position: relative;
  flex: 1;
  width: 4px;
  border-radius: 999px;
  background: var(--control-track);
}

/* The flat line, so a boost and a cut are visually distinguishable. */
.eq__zero {
  position: absolute;
  left: -3px;
  right: -3px;
  top: 50%;
  height: 1px;
  background: var(--separator-strong);
}

.eq__fill {
  position: absolute;
  left: 0;
  right: 0;
  background: var(--accent);
  border-radius: 999px;
}

.eq__thumb {
  position: absolute;
  left: 50%;
  width: 11px;
  height: 11px;
  margin: -5.5px 0 0 -5.5px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.32);
}

.eq__label {
  margin-top: 6px;
  font-size: 9.5px;
  color: var(--text-tertiary);
}

.eq__value {
  font-size: 9.5px;
  color: var(--text-secondary);
  font-variant-numeric: tabular-nums;
}
</style>
