<script setup lang="ts">
/**
 * The Playlist Master Mixer: the drawing, built.
 *
 * A playlist normally plays as a list. Here it is an arrangement — lanes down
 * the side, blocks along the time axis — so two songs can be made to overlap
 * and the join between them shaped by hand rather than by one global curve.
 *
 * Two things about how this is put together:
 *
 * * **The arrangement lives in the store, edits are pure.** Every gesture ends
 *   in one `commit`, so undo is a stack of whole arrangements rather than a
 *   stack of inverse operations, and dragging four blocks at once is no harder
 *   than dragging one.
 * * **A drag previews against a snapshot.** `dragOrigin` is taken on
 *   pointerdown and every move recomputes from it, so a gesture never
 *   accumulates rounding, and cancelling it is just putting the snapshot back.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import MixBlockView from "./MixBlockView.vue";
import {
  SNAP_PIXELS,
  addLane,
  cloneMix,
  deleteBlocks,
  locate,
  mixDuration,
  moveBlocks,
  removeLane,
  rulerStep,
  snapCandidates,
  snapDrag,
  snapTime,
  sourceDuration,
  splitBlock,
  timecode,
  trimBlock,
  updateLane,
} from "@/lib/masterMix";
import { formatDuration } from "@/lib/format";
import { useMasterMixStore, type Tool } from "@/stores/masterMix";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";
import type { MasterMix, MixBlock } from "@/lib/types";

const store = useMasterMixStore();
const player = usePlayerStore();
const ui = useUiStore();

const LANE_HEIGHT = 74;
const HEADER_WIDTH = 190;
/** Blank timeline kept past the end so there is always somewhere to drag to. */
const TAIL_SECS = 60;

const scroller = ref<HTMLElement | null>(null);
const dialog = ref<HTMLElement | null>(null);

const tools: { id: Tool; icon: "automation" | "blade" | "pointer"; label: string; hint: string }[] = [
  {
    id: "automation",
    icon: "automation",
    label: "Volume automation",
    hint: "Volume keyframes over a block — arrives with the next stage",
  },
  { id: "blade", icon: "blade", label: "Blade", hint: "Click a block to split it in two" },
  { id: "select", icon: "pointer", label: "Pointer", hint: "Select, move and trim blocks" },
];

const mix = computed(() => store.mix);
const pps = computed(() => store.pixelsPerSecond);
const contentSecs = computed(() => Math.max(store.duration, 30) + TAIL_SECS);
const contentWidth = computed(() => contentSecs.value * pps.value);
const step = computed(() => rulerStep(pps.value));

const ticks = computed(() => {
  const out: { secs: number; label: string }[] = [];
  for (let t = 0; t <= contentSecs.value; t += step.value) {
    out.push({ secs: t, label: formatDuration(t) });
  }
  return out;
});

/** A hue per lane, evenly spread, so neighbouring lanes never look alike. */
function hueFor(laneIndex: number): number {
  return (laneIndex * 47 + 18) % 360;
}

function entryFor(block: MixBlock) {
  if (block.source.kind !== "entry") return null;
  const index = block.source.index;
  return store.entries.find((e) => e.index === index) ?? null;
}

function waveformFor(block: MixBlock) {
  if (block.source.kind !== "entry") return null;
  return store.waveforms[block.source.index] ?? null;
}

/** Pull in the peaks for every song on the timeline, once the modal is up. */
function loadVisibleWaveforms() {
  const wanted = new Set<number>();
  for (const lane of mix.value.lanes) {
    for (const block of lane.blocks) {
      if (block.source.kind === "entry") wanted.add(block.source.index);
    }
  }
  for (const index of wanted) void store.loadWaveform(index);
}

watch(() => store.mix.lanes.length, loadVisibleWaveforms);
watch(() => store.playlistId, loadVisibleWaveforms);

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/** Timeline seconds under a pointer event. */
function timeAt(event: PointerEvent | MouseEvent): number {
  const element = scroller.value;
  if (!element) return 0;
  const box = element.getBoundingClientRect();
  const x = event.clientX - box.left + element.scrollLeft - HEADER_WIDTH;
  return Math.max(0, x / pps.value);
}

// ---------------------------------------------------------------------------
// Dragging
// ---------------------------------------------------------------------------

interface Drag {
  mode: "move" | "trim-start" | "trim-end";
  blockId: string;
  originX: number;
  originY: number;
  /** The arrangement as it was before this gesture; every frame recomputes
   *  from here, so a long drag cannot drift. */
  origin: MasterMix;
  ids: string[];
  moved: boolean;
}

const drag = ref<Drag | null>(null);

function onGrab(
  payload: { event: PointerEvent; mode: "move" | "trim-start" | "trim-end" },
  blockId: string,
) {
  const { event, mode } = payload;
  if (event.button !== 0) return;

  if (store.tool === "blade" && mode === "move") {
    const at = timeAt(event);
    const next = splitBlock(mix.value, blockId, at);
    if (next !== mix.value) store.commit(next);
    return;
  }
  if (store.tool !== "select") return;

  // Shift or the platform modifier extends the selection; a plain click on
  // something already selected keeps the group, so a multi-block drag works.
  const additive = event.shiftKey || event.metaKey || event.ctrlKey;
  if (additive) {
    store.toggleSelected(blockId);
  } else if (!store.selection.includes(blockId)) {
    store.select([blockId]);
  }

  const ids = mode === "move" ? [...store.selection] : [blockId];
  drag.value = {
    mode,
    blockId,
    originX: event.clientX,
    originY: event.clientY,
    origin: cloneMix(mix.value),
    ids: ids.length > 0 ? ids : [blockId],
    moved: false,
  };
  (event.target as Element).setPointerCapture?.(event.pointerId);
  window.addEventListener("pointermove", onDragMove);
  window.addEventListener("pointerup", onDragEnd);
  window.addEventListener("pointercancel", onDragCancel);
}

function onDragMove(event: PointerEvent) {
  const current = drag.value;
  if (!current) return;
  const dx = event.clientX - current.originX;
  const dy = event.clientY - current.originY;
  if (!current.moved && Math.abs(dx) < 2 && Math.abs(dy) < 2) return;
  current.moved = true;

  // Alt is the universal "ignore snapping" modifier in timeline editors.
  const tolerance = event.altKey ? 0 : SNAP_PIXELS / pps.value;
  const found = locate(current.origin, current.blockId);
  if (!found) return;
  const candidates = snapCandidates(current.origin, new Set(current.ids));

  if (current.mode === "move") {
    const wanted = found.block.startSecs + dx / pps.value;
    const snapped = snapDrag(wanted, found.block.durationSecs, candidates, tolerance);
    const laneDelta = Math.round(dy / LANE_HEIGHT);
    store.mix = moveBlocks(current.origin, current.ids, snapped - found.block.startSecs, laneDelta);
    return;
  }

  const edge = current.mode === "trim-start" ? "start" : "end";
  const anchor = edge === "start" ? found.block.startSecs : found.block.startSecs + found.block.durationSecs;
  const wanted = snapTime(anchor + dx / pps.value, candidates, tolerance);
  store.mix = trimBlock(
    current.origin,
    current.blockId,
    edge,
    wanted,
    sourceDuration(found.block, store.entries),
  );
}

function onDragEnd() {
  const current = drag.value;
  stopDragListening();
  if (!current) return;
  if (!current.moved) return;
  // Put the pre-drag arrangement back and re-apply the result through
  // `commit`, so undo lands on where the block was rather than on some
  // half-way frame of the drag.
  const result = mix.value;
  store.mix = current.origin;
  store.commit(result);
}

function onDragCancel() {
  const current = drag.value;
  stopDragListening();
  if (current) store.mix = current.origin;
}

function stopDragListening() {
  drag.value = null;
  window.removeEventListener("pointermove", onDragMove);
  window.removeEventListener("pointerup", onDragEnd);
  window.removeEventListener("pointercancel", onDragCancel);
}

// ---------------------------------------------------------------------------
// Playhead
// ---------------------------------------------------------------------------

const scrubbing = ref(false);

function onRulerDown(event: PointerEvent) {
  scrubbing.value = true;
  (event.currentTarget as Element).setPointerCapture?.(event.pointerId);
  movePlayhead(event);
}

function onRulerMove(event: PointerEvent) {
  if (scrubbing.value) movePlayhead(event);
}

async function onRulerUp() {
  if (!scrubbing.value) return;
  scrubbing.value = false;
  // Auditioning follows the playhead, exactly as pressing play here would.
  if (store.previewing) await store.play(store.playhead);
}

function movePlayhead(event: PointerEvent) {
  store.playhead = Math.min(timeAt(event), Math.max(store.duration, 0));
}

/** While the engine is playing the mix, the playhead is its position. */
watch(
  () => player.snapshot.positionSecs,
  (position) => {
    if (store.previewing && !scrubbing.value) store.playhead = position;
  },
);

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

async function togglePlay() {
  if (store.previewing && player.playing) {
    await player.toggle();
    return;
  }
  await store.play(store.playhead);
}

async function stop() {
  await store.stop();
  store.playhead = 0;
}

function deleteSelection() {
  if (store.selection.length === 0) return;
  store.commit(deleteBlocks(mix.value, store.selection));
  store.select([]);
}

function addCustomLane() {
  store.commit(addLane(mix.value, `Track ${mix.value.lanes.length + 1}`));
}

function dropLane(laneIndex: number) {
  store.commit(removeLane(mix.value, laneIndex));
}

function toggleMute(laneIndex: number) {
  const lane = mix.value.lanes[laneIndex];
  store.commit(updateLane(mix.value, laneIndex, { muted: !lane.muted }));
}

function toggleSolo(laneIndex: number) {
  const lane = mix.value.lanes[laneIndex];
  store.commit(updateLane(mix.value, laneIndex, { soloed: !lane.soloed }));
}

async function close() {
  await store.close();
}

async function resetArrangement() {
  await store.reset();
  loadVisibleWaveforms();
  ui.notify("Arrangement reset to the playlist order");
}

/** The prototype's "Open Mixer": per-block effects, which is the next stage. */
function openBlockMixer() {
  ui.notify("Per-block mixer effects arrive with the next stage of this feature");
}

function onKeydown(event: KeyboardEvent) {
  if (!store.open) return;
  const target = event.target as HTMLElement | null;
  if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;

  const modifier = event.metaKey || event.ctrlKey;
  if (modifier && event.key.toLowerCase() === "z") {
    event.preventDefault();
    if (event.shiftKey) store.redo();
    else store.undo();
    return;
  }
  switch (event.key) {
    case "Escape":
      event.preventDefault();
      void close();
      break;
    case " ":
      event.preventDefault();
      void togglePlay();
      break;
    case "Backspace":
    case "Delete":
      event.preventDefault();
      deleteSelection();
      break;
    case "v":
      store.tool = "select";
      break;
    case "b":
      store.tool = "blade";
      break;
    default:
      break;
  }
}

/** Zoom around the pointer, so the thing under the cursor stays under it. */
function onWheel(event: WheelEvent) {
  if (!(event.metaKey || event.ctrlKey)) return;
  event.preventDefault();
  const element = scroller.value;
  if (!element) return;
  const before = timeAt(event as unknown as PointerEvent);
  store.zoom(event.deltaY < 0 ? 1.12 : 1 / 1.12);
  void nextTick(() => {
    const box = element.getBoundingClientRect();
    const wanted = event.clientX - box.left - HEADER_WIDTH;
    element.scrollLeft = before * store.pixelsPerSecond - wanted;
  });
}

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  loadVisibleWaveforms();
  dialog.value?.focus();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  stopDragListening();
});

const summary = computed(() => {
  const blocks = mix.value.lanes.reduce((sum, lane) => sum + lane.blocks.length, 0);
  return `${mix.value.lanes.length} tracks · ${blocks} blocks · ${formatDuration(mixDuration(mix.value))}`;
});
</script>

<template>
  <div class="mm-scrim" @pointerdown.self="close">
    <section
      ref="dialog"
      class="mm"
      role="dialog"
      aria-modal="true"
      aria-label="Playlist master mixer"
      tabindex="-1"
    >
      <header class="mm__header">
        <div class="mm__title">
          <h2>Playlist Master Mixer</h2>
          <p>{{ store.playlistName }} · {{ summary }}</p>
        </div>

        <div class="mm__tools" role="group" aria-label="Tools">
          <button
            v-for="entry in tools"
            :key="entry.id"
            type="button"
            class="mm__tool"
            :class="{ 'is-active': store.tool === entry.id }"
            :disabled="entry.id === 'automation'"
            :title="entry.hint"
            :aria-label="entry.label"
            :aria-pressed="store.tool === entry.id"
            @click="store.tool = entry.id"
          >
            <PnmIcon :name="entry.icon" :size="17" />
          </button>
        </div>

        <button
          class="mm__mixer-button"
          type="button"
          title="Effects for the selected block — arrives with the next stage"
          @click="openBlockMixer"
        >
          Open Mixer
        </button>

        <button class="icon-button" type="button" aria-label="Close master mixer" @click="close">
          <PnmIcon name="close" :size="17" />
        </button>
      </header>

      <div class="mm__transport">
        <button
          class="icon-button"
          type="button"
          :aria-label="store.previewing && player.playing ? 'Pause' : 'Play the mix'"
          @click="togglePlay"
        >
          <PnmIcon :name="store.previewing && player.playing ? 'pause' : 'play'" :size="17" />
        </button>
        <button class="icon-button" type="button" aria-label="Stop" @click="stop">
          <PnmIcon name="stop" :size="15" />
        </button>
        <span class="mm__timecode">{{ timecode(store.playhead) }}</span>

        <span class="mm__divider" />

        <button
          class="icon-button"
          type="button"
          aria-label="Undo"
          :disabled="!store.canUndo"
          @click="store.undo()"
        >
          <PnmIcon name="undo" :size="16" />
        </button>
        <button
          class="icon-button"
          type="button"
          aria-label="Redo"
          :disabled="!store.canRedo"
          @click="store.redo()"
        >
          <PnmIcon name="redo" :size="16" />
        </button>

        <span class="mm__divider" />

        <button class="icon-button" type="button" aria-label="Zoom out" @click="store.zoom(1 / 1.4)">
          <PnmIcon name="minimize" :size="16" />
        </button>
        <button class="icon-button" type="button" aria-label="Zoom in" @click="store.zoom(1.4)">
          <PnmIcon name="plus" :size="16" />
        </button>

        <span class="mm__spacer" />

        <label class="mm__enable">
          <input
            type="checkbox"
            :checked="mix.enabled"
            @change="store.setEnabled(($event.target as HTMLInputElement).checked)"
          />
          <span>Play this playlist as a mix</span>
        </label>
        <button class="mm__text-button" type="button" @click="resetArrangement">Reset</button>
      </div>

      <p v-if="store.error" class="mm__error" role="alert">{{ store.error }}</p>

      <div
        v-if="!store.loading"
        ref="scroller"
        class="mm__body"
        :class="{ 'is-blade': store.tool === 'blade' }"
        @wheel="onWheel"
        @pointerdown.self="store.select([])"
      >
        <div class="mm__grid" :style="{ width: `${HEADER_WIDTH + contentWidth}px` }">
          <div class="mm__ruler-row">
            <div class="mm__corner" :style="{ width: `${HEADER_WIDTH}px` }" />
            <div
              class="mm__ruler"
              :style="{ width: `${contentWidth}px` }"
              @pointerdown="onRulerDown"
              @pointermove="onRulerMove"
              @pointerup="onRulerUp"
              @pointercancel="onRulerUp"
            >
              <span
                v-for="tick in ticks"
                :key="tick.secs"
                class="mm__tick"
                :style="{ left: `${tick.secs * pps}px` }"
                >{{ tick.label }}</span
              >
            </div>
          </div>

          <div
            v-for="(lane, laneIndex) in mix.lanes"
            :key="lane.id"
            class="mm__lane"
            :style="{ height: `${LANE_HEIGHT}px` }"
          >
            <div class="mm__lane-head" :style="{ width: `${HEADER_WIDTH}px` }">
              <span class="mm__lane-swatch" :style="{ background: `hsl(${hueFor(laneIndex)} 60% 62%)` }" />
              <span class="mm__lane-name" :title="lane.name">{{ lane.name || `Track ${laneIndex + 1}` }}</span>
              <div class="mm__lane-buttons">
                <button
                  type="button"
                  class="mm__ms"
                  :class="{ 'is-on': lane.muted }"
                  :aria-pressed="lane.muted"
                  :title="`Mute ${lane.name}`"
                  @click="toggleMute(laneIndex)"
                >
                  M
                </button>
                <button
                  type="button"
                  class="mm__ms mm__ms--solo"
                  :class="{ 'is-on': lane.soloed }"
                  :aria-pressed="lane.soloed"
                  :title="`Solo ${lane.name}`"
                  @click="toggleSolo(laneIndex)"
                >
                  S
                </button>
                <button
                  type="button"
                  class="mm__ms mm__ms--drop"
                  :title="`Delete ${lane.name}`"
                  @click="dropLane(laneIndex)"
                >
                  <PnmIcon name="close" :size="11" />
                </button>
              </div>
            </div>

            <div class="mm__lane-track" :style="{ width: `${contentWidth}px` }">
              <MixBlockView
                v-for="block in lane.blocks"
                :key="block.id"
                :block="block"
                :entry="entryFor(block)"
                :waveform="waveformFor(block)"
                :pixels-per-second="pps"
                :height="LANE_HEIGHT"
                :selected="store.selected.has(block.id)"
                :hue="hueFor(laneIndex)"
                :tool="store.tool"
                @grab="onGrab($event, block.id)"
              />
            </div>
          </div>

          <div class="mm__add-lane" :style="{ width: `${HEADER_WIDTH}px` }">
            <button type="button" @click="addCustomLane">
              <PnmIcon name="plus" :size="13" />
              <span>Add Custom Track</span>
            </button>
          </div>

          <div
            class="mm__playhead"
            :style="{ left: `${HEADER_WIDTH + store.playhead * pps}px` }"
            aria-hidden="true"
          />
        </div>
      </div>

      <p v-else class="mm__loading">Loading the arrangement…</p>

      <footer class="mm__footer">
        <span v-if="store.tool === 'blade'">Click a block to split it. Press V for the pointer.</span>
        <span v-else>
          Drag to move, drag an edge to trim, hold Alt to ignore snapping. Overlap two blocks to
          crossfade them.
        </span>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.mm-scrim {
  position: fixed;
  inset: 0;
  z-index: 530;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 18px;
  background: rgba(0, 0, 0, 0.4);
  backdrop-filter: blur(4px);
}

/* "Fills most of the window, with little deadspace" — the drawing's proportions. */
.mm {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  overflow: hidden;
  border: 0.5px solid var(--separator);
  border-radius: var(--radius-lg);
  outline: none;
  background: var(--bg-elevated);
  box-shadow: var(--shadow-popover);
  color: var(--text);
}

.mm__header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  border-bottom: 0.5px solid var(--separator);
}

.mm__title {
  flex: 1;
  min-width: 0;
}

.mm__title h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 650;
}

.mm__title p {
  margin: 2px 0 0;
  font-size: 11.5px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.mm__tools {
  display: flex;
  gap: 2px;
  padding: 2px;
  border-radius: var(--radius-sm);
  background: var(--bg-sunken);
}

.mm__tool {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 26px;
  border-radius: 4px;
  color: var(--text-secondary);
}

.mm__tool:hover:not(:disabled) {
  background: var(--bg-hover);
  color: var(--text);
}

.mm__tool.is-active {
  background: var(--bg-elevated);
  color: var(--accent);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12);
}

.mm__tool:disabled {
  opacity: 0.35;
  cursor: default;
}

.mm__mixer-button {
  padding: 5px 14px;
  border: 0.5px solid var(--separator-strong);
  border-radius: 999px;
  font-size: 12px;
  color: var(--text);
}

.mm__mixer-button:hover {
  background: var(--bg-hover);
}

.mm__transport {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 7px 14px;
  border-bottom: 0.5px solid var(--separator);
  background: var(--bg-sidebar);
}

.mm__timecode {
  margin-left: 6px;
  font-variant-numeric: tabular-nums;
  font-size: 12.5px;
  color: var(--text-secondary);
}

.mm__divider {
  width: 1px;
  height: 18px;
  margin: 0 6px;
  background: var(--separator);
}

.mm__spacer {
  flex: 1;
}

.mm__enable {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
}

.mm__text-button {
  margin-left: 10px;
  font-size: 12px;
  color: var(--text-secondary);
}

.mm__text-button:hover {
  color: var(--text);
}

.mm__error {
  margin: 0;
  padding: 7px 14px;
  font-size: 12px;
  color: var(--accent);
  background: var(--accent-tint);
}

.mm__body {
  position: relative;
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: var(--bg);
}

.mm__grid {
  position: relative;
  min-height: 100%;
}

.mm__ruler-row {
  position: sticky;
  top: 0;
  z-index: 3;
  display: flex;
  height: 26px;
  background: var(--bg-sunken);
  border-bottom: 0.5px solid var(--separator);
}

.mm__corner {
  position: sticky;
  left: 0;
  z-index: 4;
  flex: none;
  background: var(--bg-sunken);
  border-right: 0.5px solid var(--separator);
}

.mm__ruler {
  position: relative;
  flex: none;
  cursor: text;
  touch-action: none;
}

.mm__tick {
  position: absolute;
  top: 0;
  padding-left: 4px;
  font-size: 10px;
  line-height: 25px;
  color: var(--text-tertiary);
  border-left: 1px solid var(--separator);
  white-space: nowrap;
  pointer-events: none;
}

.mm__lane {
  display: flex;
  border-bottom: 0.5px solid var(--separator);
}

.mm__lane-head {
  position: sticky;
  left: 0;
  z-index: 2;
  flex: none;
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 0 8px;
  background: var(--bg-sidebar);
  border-right: 0.5px solid var(--separator);
}

.mm__lane-swatch {
  flex: none;
  width: 4px;
  height: 32px;
  border-radius: 2px;
}

.mm__lane-name {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  font-weight: 550;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.mm__lane-buttons {
  display: flex;
  gap: 3px;
}

.mm__ms {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 18px;
  border: 0.5px solid var(--separator-strong);
  border-radius: 3px;
  font-size: 10px;
  font-weight: 700;
  color: var(--text-secondary);
}

.mm__ms:hover {
  background: var(--bg-hover);
}

.mm__ms.is-on {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-contrast);
}

.mm__ms--solo.is-on {
  background: var(--accent-secondary);
  border-color: var(--accent-secondary);
}

.mm__ms--drop {
  opacity: 0;
}

.mm__lane:hover .mm__ms--drop {
  opacity: 1;
}

.mm__lane-track {
  position: relative;
  flex: none;
}

.mm__add-lane {
  position: sticky;
  left: 0;
  z-index: 2;
  padding: 8px;
}

.mm__add-lane button {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-secondary);
}

.mm__add-lane button:hover {
  color: var(--accent);
}

.mm__playhead {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 1px;
  z-index: 5;
  background: var(--accent);
  pointer-events: none;
}

.mm__loading {
  flex: 1;
  display: grid;
  place-items: center;
  margin: 0;
  font-size: 13px;
  color: var(--text-tertiary);
}

.mm__footer {
  margin: 0;
  padding: 8px 14px;
  border-top: 0.5px solid var(--separator);
  font-size: 11.5px;
  color: var(--text-tertiary);
  background: var(--bg-sidebar);
}
</style>
