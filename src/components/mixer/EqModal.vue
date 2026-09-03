<script setup lang="ts">
/**
 * The expanded EQ: a Logic-style response graph with a draggable node per
 * band, over a live spectrum of the processed output.
 *
 * Curves are drawn in a stretched `viewBox` so a path can be expressed in
 * simple 0..100 space, but the nodes are absolutely positioned HTML rather
 * than SVG circles — under `preserveAspectRatio="none"` an SVG circle would
 * come out as an ellipse, and working around that means measuring the element
 * and converting radii on every resize.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import AppSlider from "../ui/AppSlider.vue";
import BaseModal from "../ui/BaseModal.vue";
import IconButton from "../ui/IconButton.vue";
import EqPresetSelect from "./EqPresetSelect.vue";
import { bandResponse, logFrequencies, summedResponse } from "@/lib/eqCurve";
import { defaultBands, hasGain } from "@/lib/mixer";
import * as api from "@/lib/api";
import type { BandKind, Eq, EqBand } from "@/lib/types";

const props = defineProps<{ eq: Eq; targetLabel: string; sampleRate: number }>();
const emit = defineEmits<{ change: [eq: Eq]; close: [] }>();

/** Matches `MAX_BANDS` in `audio/dsp.rs`; more would be silently ignored. */
const MAX_BANDS = 12;
/** Matches the fader range in `EqSliders`, so the two editors agree. */
const GAIN_LIMIT = 12;
/** A little taller than the drag limit, so a curve at full boost has room. */
const DISPLAY_DB = 16;
const MIN_HZ = 20;
const MAX_HZ = 20000;
const MIN_Q = 0.1;
const MAX_Q = 12;

/** Resolution of the drawn curves. Enough to look smooth at any width. */
const CURVE_POINTS = 220;

const BAND_COLOURS = [
  "#ff6b4a", "#ff9f2e", "#ffd23f", "#7ed957",
  "#34c9a3", "#3e9bff", "#9d7bff", "#ff6bcb",
  "#c9a227", "#57c7d9", "#b06bff", "#ff5470",
] as const;

const BAND_KINDS: { value: BandKind; label: string; short: string }[] = [
  { value: "highPass", label: "High Pass", short: "HP" },
  { value: "lowShelf", label: "Low Shelf", short: "LS" },
  { value: "peak", label: "Peak", short: "PK" },
  { value: "highShelf", label: "High Shelf", short: "HS" },
  { value: "lowPass", label: "Low Pass", short: "LP" },
];

const GRID_HZ = [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000];
const GRID_DB = [-12, -6, 0, 6, 12];

const plot = ref<HTMLElement | null>(null);
const selected = ref(0);
const dragging = ref<number | null>(null);

function colourFor(index: number) {
  return BAND_COLOURS[index % BAND_COLOURS.length];
}

// -- axes --------------------------------------------------------------------

const LOG_MIN = Math.log10(MIN_HZ);
const LOG_SPAN = Math.log10(MAX_HZ) - LOG_MIN;

function xFor(freq: number): number {
  return ((Math.log10(freq) - LOG_MIN) / LOG_SPAN) * 100;
}

function freqAtX(percent: number): number {
  return Math.pow(10, LOG_MIN + (percent / 100) * LOG_SPAN);
}

function yFor(db: number): number {
  return 50 - (db / DISPLAY_DB) * 50;
}

function dbAtY(percent: number): number {
  return ((50 - percent) / 50) * DISPLAY_DB;
}

function clamp(v: number, min: number, max: number) {
  return Math.min(max, Math.max(min, v));
}

// -- curves ------------------------------------------------------------------

const freqs = computed(() => logFrequencies(MIN_HZ, MAX_HZ, CURVE_POINTS));

function pathFrom(values: readonly number[]): string {
  let d = "";
  for (let i = 0; i < values.length; i += 1) {
    const x = (i / (values.length - 1)) * 100;
    const y = clamp(yFor(values[i]), -20, 120);
    d += `${i === 0 ? "M" : "L"}${x.toFixed(3)},${y.toFixed(3)}`;
  }
  return d;
}

const summedPath = computed(() =>
  pathFrom(summedResponse(props.eq, freqs.value, props.sampleRate)),
);

/** Filled to the 0 dB line, so boost and cut read at a glance. */
const summedFill = computed(() => `${summedPath.value} L100,${yFor(0)} L0,${yFor(0)} Z`);

const bandPaths = computed(() =>
  props.eq.bands.map((band, index) => ({
    index,
    colour: colourFor(index),
    enabled: band.enabled && props.eq.enabled,
    d: pathFrom(bandResponse(band, freqs.value, props.sampleRate)),
  })),
);

const nodes = computed(() =>
  props.eq.bands.map((band, index) => ({
    index,
    band,
    colour: colourFor(index),
    x: clamp(xFor(band.freq), 0, 100),
    // A pass filter has no gain, so its node rides the 0 dB line.
    y: clamp(yFor(hasGain(band.kind) ? band.gainDb : 0), 0, 100),
  })),
);

// -- analyser ----------------------------------------------------------------

const spectrum = ref<number[]>([]);
const floorDb = ref(-90);
let frame: number | null = null;
let stopped = false;

/**
 * Mapped onto the same axis as the curves, but on its own dB scale: the
 * spectrum is an absolute level in dBFS while the curves are relative gain,
 * so they share only the frequency axis.
 */
const spectrumPath = computed(() => {
  const bins = spectrum.value;
  if (bins.length === 0) return "";
  const span = Math.abs(floorDb.value);
  let d = "M0,100";
  for (let i = 0; i < bins.length; i += 1) {
    const x = (i / (bins.length - 1)) * 100;
    const level = clamp((bins[i] - floorDb.value) / span, 0, 1);
    d += ` L${x.toFixed(3)},${(100 - level * 100).toFixed(3)}`;
  }
  return `${d} L100,100 Z`;
});

async function poll() {
  if (stopped) return;
  try {
    const next = await api.analyserFrame();
    spectrum.value = next.bins;
    floorDb.value = next.floorDb;
  } catch {
    // The engine may not be up yet; the next frame will pick it up.
  }
  if (!stopped) frame = requestAnimationFrame(() => void poll());
}

onMounted(async () => {
  try {
    await api.setAnalyserEnabled(true);
  } catch {
    // Without it the graph simply has no spectrum behind it.
  }
  void poll();
});

onBeforeUnmount(() => {
  stopped = true;
  if (frame !== null) cancelAnimationFrame(frame);
  void api.setAnalyserEnabled(false).catch(() => {});
});

// -- editing -----------------------------------------------------------------

function patchBand(index: number, patch: Partial<EqBand>) {
  const bands = props.eq.bands.map((b, i) => (i === index ? { ...b, ...patch } : b));
  emit("change", { ...props.eq, bands });
}

function pointerToGraph(event: PointerEvent) {
  const rect = plot.value?.getBoundingClientRect();
  if (!rect || rect.width === 0 || rect.height === 0) return null;
  return {
    x: clamp(((event.clientX - rect.left) / rect.width) * 100, 0, 100),
    y: clamp(((event.clientY - rect.top) / rect.height) * 100, 0, 100),
  };
}

function onNodeDown(event: PointerEvent, index: number) {
  event.preventDefault();
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  dragging.value = index;
  selected.value = index;
}

function onNodeMove(event: PointerEvent, index: number) {
  if (dragging.value !== index) return;
  const point = pointerToGraph(event);
  if (!point) return;

  const band = props.eq.bands[index];
  const patch: Partial<EqBand> = { freq: Math.round(freqAtX(point.x)) };
  // Only a band whose gain does something follows the pointer vertically.
  if (hasGain(band.kind)) {
    patch.gainDb = Number(clamp(dbAtY(point.y), -GAIN_LIMIT, GAIN_LIMIT).toFixed(1));
  }
  patchBand(index, patch);
}

function onNodeUp(event: PointerEvent) {
  (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
  dragging.value = null;
}

/** The wheel adjusts Q, which is the one parameter a node cannot express. */
function onNodeWheel(event: WheelEvent, index: number) {
  const band = props.eq.bands[index];
  const step = event.deltaY > 0 ? -0.08 : 0.08;
  const next = clamp(band.q * (1 + step), MIN_Q, MAX_Q);
  selected.value = index;
  patchBand(index, { q: Number(next.toFixed(2)) });
}

/** Double-click returns a band to flat, or a pass filter to its default slope. */
function onNodeDoubleClick(index: number) {
  const band = props.eq.bands[index];
  patchBand(index, hasGain(band.kind) ? { gainDb: 0 } : { q: 0.71 });
}

function addBand() {
  if (props.eq.bands.length >= MAX_BANDS) return;
  const bands = [
    ...props.eq.bands,
    { kind: "peak" as BandKind, freq: 1000, gainDb: 0, q: 0.71, enabled: true },
  ];
  emit("change", { ...props.eq, bands });
  selected.value = bands.length - 1;
}

function removeBand(index: number) {
  if (props.eq.bands.length <= 1) return;
  const bands = props.eq.bands.filter((_, i) => i !== index);
  emit("change", { ...props.eq, bands });
  selected.value = Math.min(selected.value, bands.length - 1);
}

function reset() {
  emit("change", { enabled: true, preampDb: 0, bands: defaultBands() });
  selected.value = 0;
}

// Keep the selection pointing at a band that still exists.
watch(
  () => props.eq.bands.length,
  (length) => {
    if (selected.value >= length) selected.value = Math.max(0, length - 1);
  },
);

// -- formatting --------------------------------------------------------------

function hzLabel(hz: number): string {
  if (hz >= 1000) {
    const k = hz / 1000;
    return `${Number.isInteger(k) ? k : k.toFixed(1)}k`;
  }
  return `${Math.round(hz)}`;
}

function gainLabel(db: number): string {
  return `${db > 0 ? "+" : ""}${db.toFixed(1)}`;
}

function commitNumber(index: number, key: "freq" | "gainDb" | "q", raw: string) {
  const value = Number(raw);
  if (!Number.isFinite(value)) return;
  const limits = {
    freq: [MIN_HZ, MAX_HZ],
    gainDb: [-GAIN_LIMIT, GAIN_LIMIT],
    q: [MIN_Q, MAX_Q],
  }[key];
  patchBand(index, { [key]: clamp(value, limits[0], limits[1]) });
}
</script>

<template>
  <BaseModal :open="true" labelledby="eq-modal-title" :width="1040" flush @close="emit('close')">
    <div class="eq-modal">
      <header class="eq-modal__head">
        <div class="eq-modal__heading">
          <p class="eyebrow">Equaliser</p>
          <h2 id="eq-modal-title">{{ targetLabel }}</h2>
        </div>

        <EqPresetSelect :eq="eq" @change="emit('change', $event)" />

        <label class="eq-modal__power">
          <input
            type="checkbox"
            :checked="eq.enabled"
            @change="emit('change', { ...eq, enabled: ($event.target as HTMLInputElement).checked })"
          />
          <span>{{ eq.enabled ? "On" : "Bypassed" }}</span>
        </label>

        <button class="eq-modal__link" @click="reset">Reset</button>
        <IconButton icon="close" label="Close" :size="18" @click="emit('close')" />
      </header>

      <!-- Graph -------------------------------------------------------------->
      <div ref="plot" class="plot" :class="{ 'is-bypassed': !eq.enabled }">
        <svg class="plot__svg" viewBox="0 0 100 100" preserveAspectRatio="none">
          <!-- Live spectrum of what is actually leaving the app. -->
          <path v-if="spectrumPath" :d="spectrumPath" class="plot__spectrum" />

          <line
            v-for="hz in GRID_HZ"
            :key="`v${hz}`"
            :x1="xFor(hz)"
            :x2="xFor(hz)"
            y1="0"
            y2="100"
            class="plot__grid"
            vector-effect="non-scaling-stroke"
          />
          <line
            v-for="db in GRID_DB"
            :key="`h${db}`"
            x1="0"
            x2="100"
            :y1="yFor(db)"
            :y2="yFor(db)"
            class="plot__grid"
            :class="{ 'plot__grid--zero': db === 0 }"
            vector-effect="non-scaling-stroke"
          />

          <!-- Each band's own contribution, faint, so it is clear which node
               is responsible for which part of the summed curve. -->
          <path
            v-for="band in bandPaths"
            v-show="band.enabled"
            :key="band.index"
            :d="band.d"
            class="plot__band"
            :class="{ 'is-selected': band.index === selected }"
            :stroke="band.colour"
            vector-effect="non-scaling-stroke"
          />

          <path :d="summedFill" class="plot__fill" />
          <path :d="summedPath" class="plot__curve" vector-effect="non-scaling-stroke" />
        </svg>

        <!-- Nodes are HTML so they stay round under the stretched viewBox. -->
        <button
          v-for="node in nodes"
          :key="node.index"
          class="node"
          :class="{
            'is-selected': node.index === selected,
            'is-off': !node.band.enabled,
            'is-dragging': dragging === node.index,
          }"
          :style="{
            left: `${node.x}%`,
            top: `${node.y}%`,
            '--band-colour': node.colour,
          }"
          :title="`Band ${node.index + 1} — drag to move, scroll for Q, double-click to flatten`"
          :aria-label="`Band ${node.index + 1} at ${hzLabel(node.band.freq)} hertz`"
          @pointerdown="onNodeDown($event, node.index)"
          @pointermove="onNodeMove($event, node.index)"
          @pointerup="onNodeUp"
          @pointercancel="onNodeUp"
          @wheel.prevent="onNodeWheel($event, node.index)"
          @dblclick="onNodeDoubleClick(node.index)"
        >
          <span class="node__dot">{{ node.index + 1 }}</span>
        </button>

        <div class="plot__axis plot__axis--x">
          <span v-for="hz in GRID_HZ" :key="hz" :style="{ left: `${xFor(hz)}%` }">
            {{ hzLabel(hz) }}
          </span>
        </div>
        <div class="plot__axis plot__axis--y">
          <span v-for="db in GRID_DB" :key="db" :style="{ top: `${yFor(db)}%` }">
            {{ db > 0 ? `+${db}` : db }}
          </span>
        </div>
      </div>

      <!-- Per-band numeric strip, in Logic's column layout ------------------->
      <div class="bands">
        <div
          v-for="(band, index) in eq.bands"
          :key="index"
          class="band"
          :class="{ 'is-selected': index === selected, 'is-off': !band.enabled }"
          :style="{ '--band-colour': colourFor(index) }"
          @click="selected = index"
        >
          <div class="band__top">
            <button
              class="band__power"
              :aria-label="band.enabled ? `Disable band ${index + 1}` : `Enable band ${index + 1}`"
              :title="band.enabled ? 'Disable this band' : 'Enable this band'"
              @click.stop="patchBand(index, { enabled: !band.enabled })"
            >
              <span class="band__swatch" />
            </button>
            <button
              v-if="eq.bands.length > 1"
              class="band__remove"
              :aria-label="`Remove band ${index + 1}`"
              title="Remove this band"
              @click.stop="removeBand(index)"
            >
              <PnmIcon name="close" :size="11" />
            </button>
          </div>

          <select
            class="band__select"
            :value="band.kind"
            :aria-label="`Band ${index + 1} type`"
            @change="patchBand(index, { kind: ($event.target as HTMLSelectElement).value as BandKind })"
          >
            <option v-for="k in BAND_KINDS" :key="k.value" :value="k.value" :title="k.label">
              {{ k.short }}
            </option>
          </select>

          <input
            class="band__number"
            type="number"
            :value="Math.round(band.freq)"
            :min="MIN_HZ"
            :max="MAX_HZ"
            :aria-label="`Band ${index + 1} frequency`"
            @change="commitNumber(index, 'freq', ($event.target as HTMLInputElement).value)"
          />
          <input
            class="band__number"
            type="number"
            :value="hasGain(band.kind) ? band.gainDb.toFixed(1) : ''"
            :disabled="!hasGain(band.kind)"
            :min="-GAIN_LIMIT"
            :max="GAIN_LIMIT"
            step="0.5"
            :placeholder="hasGain(band.kind) ? '' : '—'"
            :aria-label="`Band ${index + 1} gain`"
            @change="commitNumber(index, 'gainDb', ($event.target as HTMLInputElement).value)"
          />
          <input
            class="band__number"
            type="number"
            :value="band.q.toFixed(2)"
            :min="MIN_Q"
            :max="MAX_Q"
            step="0.1"
            :aria-label="`Band ${index + 1} Q`"
            @change="commitNumber(index, 'q', ($event.target as HTMLInputElement).value)"
          />
        </div>

        <button
          v-if="eq.bands.length < MAX_BANDS"
          class="bands__add"
          title="Add a band"
          aria-label="Add a band"
          @click="addBand"
        >
          <PnmIcon name="plus" :size="14" />
        </button>
      </div>

      <footer class="eq-modal__foot">
        <p class="eq-modal__hint">
          Drag a node to move it, scroll over it for Q, double-click to flatten it.
        </p>
        <div class="eq-modal__preamp">
          <label>Preamp</label>
          <AppSlider
            :model-value="eq.preampDb"
            :min="-12"
            :max="12"
            :step="0.5"
            :origin="0"
            @update:model-value="emit('change', { ...eq, preampDb: $event })"
          />
          <span class="eq-modal__value">{{ gainLabel(eq.preampDb) }} dB</span>
        </div>
      </footer>
    </div>
  </BaseModal>
</template>

<style scoped>
.eq-modal {
  /* Fills the shell's body exactly: the scrim and shell already frame the
     modal, so the workspace does not inset itself a second time — matching
     how the settings and master mixer workspaces sit in their shells. */
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.eq-modal__head {
  display: flex;
  align-items: center;
  gap: 12px;
  /* Same inset as the master mixer's header, so the workspace modals read
     as one family. */
  padding: 12px 14px;
  border-bottom: 1px solid var(--separator);
}

.eq-modal__heading {
  flex: 1;
  min-width: 0;
}

.eq-modal__heading h2 {
  margin: 2px 0 0;
  font-size: 17px;
  font-weight: 650;
}

.eq-modal__power {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
}

.eq-modal__link {
  font-size: 12px;
  color: var(--accent);
}

/* -- graph ---------------------------------------------------------------- */

.plot {
  position: relative;
  height: 300px;
  margin: 14px 18px 0 34px;
  border-radius: var(--radius);
  background: var(--bg-sunken);
  border: 0.5px solid var(--separator);
  overflow: hidden;
  touch-action: none;
}

.plot.is-bypassed {
  opacity: 0.5;
}

.plot__svg {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
}

.plot__grid {
  stroke: var(--separator);
  stroke-width: 1;
}

.plot__grid--zero {
  stroke: var(--separator-strong);
}

.plot__spectrum {
  fill: var(--text-tertiary);
  opacity: 0.24;
}

.plot__band {
  fill: none;
  stroke-width: 1;
  opacity: 0.4;
}

.plot__band.is-selected {
  stroke-width: 1.5;
  opacity: 0.85;
}

.plot__fill {
  fill: var(--accent);
  opacity: 0.12;
}

.plot__curve {
  fill: none;
  stroke: var(--accent);
  stroke-width: 2;
}

.plot__axis span {
  position: absolute;
  font-size: 9.5px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
  pointer-events: none;
}

.plot__axis--x span {
  bottom: 3px;
  transform: translateX(-50%);
}

.plot__axis--y span {
  left: -30px;
  transform: translateY(-50%);
  width: 26px;
  text-align: right;
}

/* -- nodes ---------------------------------------------------------------- */

.node {
  position: absolute;
  width: 26px;
  height: 26px;
  margin: -13px 0 0 -13px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  cursor: grab;
  touch-action: none;
}

.node.is-dragging {
  cursor: grabbing;
}

.node__dot {
  width: 15px;
  height: 15px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  font-size: 8.5px;
  font-weight: 700;
  color: #fff;
  background: var(--band-colour);
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.35);
  transition: width 0.12s var(--ease), height 0.12s var(--ease);
}

.node.is-selected .node__dot,
.node:hover .node__dot {
  width: 19px;
  height: 19px;
}

/* A disabled band keeps its node — it is how you turn it back on — but reads
   as an outline rather than a filled dot. */
.node.is-off .node__dot {
  background: transparent;
  color: var(--band-colour);
  box-shadow: inset 0 0 0 1.5px var(--band-colour);
}

/* -- band strip ------------------------------------------------------------ */

.bands {
  display: flex;
  align-items: stretch;
  gap: 5px;
  padding: 12px 18px 0 34px;
  overflow-x: auto;
}

.band {
  flex: 1 1 0;
  min-width: 62px;
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 6px 5px;
  border-radius: var(--radius-sm);
  border: 1px solid transparent;
  cursor: pointer;
}

.band.is-selected {
  border-color: var(--band-colour);
  background: var(--bg-sunken);
}

.band.is-off {
  opacity: 0.45;
}

.band__top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 14px;
}

.band__power {
  display: grid;
  place-items: center;
}

.band__swatch {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: var(--band-colour);
}

.band.is-off .band__swatch {
  background: transparent;
  box-shadow: inset 0 0 0 1.5px var(--band-colour);
}

.band__remove {
  color: var(--text-tertiary);
  opacity: 0;
}

.band:hover .band__remove,
.band__remove:focus-visible {
  opacity: 1;
}

.band__select,
.band__number {
  width: 100%;
  height: 21px;
  padding: 0 3px;
  border-radius: 4px;
  border: 1px solid var(--separator);
  background: var(--bg);
  color: var(--text);
  font: inherit;
  font-size: 10.5px;
  font-variant-numeric: tabular-nums;
  text-align: center;
}

.band__number:disabled {
  color: var(--text-tertiary);
  background: transparent;
}

.band__number::-webkit-outer-spin-button,
.band__number::-webkit-inner-spin-button {
  -webkit-appearance: none;
  margin: 0;
}

.bands__add {
  flex: none;
  width: 30px;
  align-self: center;
  height: 30px;
  display: grid;
  place-items: center;
  border-radius: var(--radius-sm);
  border: 1px dashed var(--separator-strong);
  color: var(--text-secondary);
}

.bands__add:hover {
  color: var(--accent);
  border-color: var(--accent);
}

/* -- footer ---------------------------------------------------------------- */

.eq-modal__foot {
  padding: 8px 14px 12px;
}

.eq-modal__hint {
  margin: 0;
  font-size: 10.5px;
  color: var(--text-tertiary);
}

.eq-modal__preamp {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 10px;
  padding-top: 12px;
  border-top: 1px solid var(--separator);
  font-size: 12px;
}

.eq-modal__preamp label {
  width: 54px;
  color: var(--text-secondary);
}

.eq-modal__preamp :deep(.slider) {
  flex: 1;
}

.eq-modal__value {
  width: 62px;
  text-align: right;
  font-size: 11.5px;
  color: var(--text-secondary);
  font-variant-numeric: tabular-nums;
}
</style>
