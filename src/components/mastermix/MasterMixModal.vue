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
import AppSlider from "../AppSlider.vue";
import AdvancedMixer from "../mixer/AdvancedMixer.vue";
import MixBlockView from "./MixBlockView.vue";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  MAX_GAIN_DB,
  MIN_GAIN_DB,
  SNAP_PIXELS,
  addAutomationPoint,
  addLane,
  cloneMix,
  curveFromMidGain,
  deleteBlocks,
  duplicateBlocks,
  gridCandidates,
  locate,
  mixDuration,
  moveAutomationPoint,
  moveBlocks,
  placeAsset,
  removeAutomationPoint,
  removeLane,
  rulerStep,
  setAutomationCurve,
  snapCandidates,
  snapDrag,
  snapTime,
  splitBlock,
  timecode,
  trimBlock,
  updateLane,
} from "@/lib/masterMix";
import { formatDuration } from "@/lib/format";
import { pitchRatio, resolve } from "@/lib/mixer";
import { useDismiss } from "@/lib/dismiss";
import { useMasterMixStore, type Tool } from "@/stores/masterMix";
import { useMixerStore } from "@/stores/mixer";
import { usePlayerStore } from "@/stores/player";
import { usePlaylistStore } from "@/stores/playlists";
import { useUiStore } from "@/stores/ui";
import * as api from "@/lib/api";
import type { MasterMix, MixBlock } from "@/lib/types";

const store = useMasterMixStore();
const player = usePlayerStore();
const mixer = useMixerStore();
const playlists = usePlaylistStore();
const ui = useUiStore();
const AUDIO_EXTENSIONS = ["mp3", "flac", "wav"] as const;

const HEADER_WIDTH = 190;
const RULER_HEIGHT = 26;
/** Blank timeline kept past the end so there is always somewhere to drag to. */
const TAIL_SECS = 60;

const scroller = ref<HTMLElement | null>(null);
const dialog = ref<HTMLElement | null>(null);

const tools: { id: Tool; icon: "automation" | "blade" | "pointer"; label: string; hint: string }[] = [
  {
    id: "automation",
    icon: "automation",
    label: "Volume automation",
    hint: "Click to add volume keyframes, drag to shape the fade",
  },
  { id: "blade", icon: "blade", label: "Blade", hint: "Click a block to split it in two" },
  { id: "select", icon: "pointer", label: "Pointer", hint: "Select, move and trim blocks" },
];

const mix = computed(() => store.mix);
const pps = computed(() => store.pixelsPerSecond);
const laneHeight = computed(() => store.laneHeight);
const contentSecs = computed(() => Math.max(store.duration, 30) + TAIL_SECS);
const contentWidth = computed(() => contentSecs.value * pps.value);
const step = computed(() => rulerStep(pps.value));
/**
 * The lane colour picker.
 *
 * Drawn in a fixed layer rather than inside the lane header: the header is a
 * sticky, stacked element inside a scrolling grid, so a menu opened in it is
 * painted underneath every lane below and clipped by the scroller. Anchoring
 * it to the swatch's position on screen puts it above everything and lets it
 * flip up when there is no room below.
 */
const colorLane = ref<{ index: number; x: number; y: number; flip: boolean } | null>(null);
const paletteEl = ref<HTMLElement | null>(null);
const colorAnchor = ref<HTMLElement | null>(null);
const PALETTE_HEIGHT = 108;
const renamingLaneId = ref<string | null>(null);
const renameValue = ref("");
const COLOR_HUES = [8, 32, 55, 105, 165, 205, 245, 285, 325] as const;

function toggleColorPicker(laneIndex: number, event: MouseEvent) {
  if (colorLane.value?.index === laneIndex) {
    colorLane.value = null;
    return;
  }
  const swatch = event.currentTarget as HTMLElement;
  const box = swatch.getBoundingClientRect();
  const flip = box.bottom + PALETTE_HEIGHT > window.innerHeight;
  colorAnchor.value = swatch;
  colorLane.value = {
    index: laneIndex,
    x: box.left,
    y: flip ? box.top - 5 : box.bottom + 5,
    flip,
  };
}

const ticks = computed(() => {
  const out: { secs: number; label: string }[] = [];
  for (let t = 0; t <= contentSecs.value; t += step.value) {
    out.push({ secs: t, label: formatDuration(t) });
  }
  return out;
});

/**
 * Unlabelled divisions between the labelled ones, so a position can be read
 * off the ruler to better than the nearest label. Dropped when they would be
 * closer together than they are tall, which is where they stop being marks and
 * start being a grey band.
 */
const minorTicks = computed(() => {
  const spacing = (step.value / 4) * pps.value;
  if (spacing < 12) return [];
  const out: number[] = [];
  for (let t = step.value / 4; t <= contentSecs.value; t += step.value / 4) {
    if (Math.abs(t / step.value - Math.round(t / step.value)) > 1e-6) out.push(t);
  }
  return out;
});

/**
 * The ruler division a snap lines up with: whatever the finest mark on screen
 * is, so "snap to the second points above the tracks" means the marks the user
 * can actually see rather than a hidden grid of its own.
 */
const gridSecs = computed(() => (minorTicks.value.length > 0 ? step.value / 4 : step.value));
/** "0.25s", "2s", "1:00" — how far apart those marks currently are. */
const gridLabel = computed(() =>
  gridSecs.value < 60 ? `${Number(gridSecs.value.toFixed(2))}s` : formatDuration(gridSecs.value),
);

/**
 * Everything a gesture may snap to.
 *
 * The edges of every block it is not itself moving, the playhead — lining an
 * edit up with where you were just listening is the commonest thing to want —
 * and, when the grid toggle is on, the ruler marks either side of `times`.
 */
function snapTargets(source: MasterMix, exclude: Set<string>, times: number[]): number[] {
  const candidates = snapCandidates(source, exclude);
  candidates.push(store.playhead);
  if (store.gridSnapping) candidates.push(...gridCandidates(times, gridSecs.value));
  return candidates;
}

/**
 * Where the thing being dragged, trimmed or cut has locked on, in timeline
 * seconds, or null when nothing has.
 *
 * Snapping without this is guesswork: a block lands on a neighbour's edge and
 * the only evidence is that it looks about right. The line says which edge,
 * and — with the grid on — that it was a ruler mark rather than a block.
 */
const snapLine = ref<number | null>(null);
/**
 * Whether that line is a snap or just the cursor.
 *
 * The blade draws a line wherever it is, so the cut is never a guess; it only
 * *locks* when something is within reach, and the two have to look different
 * or the line would claim an accuracy it does not have.
 */
const snapLocked = ref(false);

/**
 * Which of `times` actually landed on a candidate, if any.
 *
 * Asked after the snap rather than during it, so one rule covers moving,
 * trimming and the blade: whatever the gesture ended up at, if it coincides
 * with something offered, that is what it snapped to.
 */
function snappedAt(times: number[], candidates: number[]): number | null {
  for (const time of times) {
    if (candidates.some((candidate) => Math.abs(candidate - time) < 1e-6)) return time;
  }
  return null;
}

/** Use a persisted lane colour when set, otherwise spread defaults apart. */
function hueFor(laneIndex: number): number {
  const saved = Number(mix.value.lanes[laneIndex]?.colorHue);
  return Number.isFinite(saved) ? ((saved % 360) + 360) % 360 : (laneIndex * 47 + 18) % 360;
}

function entryFor(block: MixBlock) {
  if (block.source.kind !== "entry") return null;
  const index = block.source.index;
  return store.entries.find((e) => e.index === index) ?? null;
}

/**
 * How fast a block plays, resolved through the layers this editor can see.
 *
 * The drawing needs it as much as the engine does: at double speed a region
 * covers twice as much of the song, so the waveform under it has to be read
 * twice as fast or the picture stops matching the sound.
 *
 * The playlist-entry layer is left out — the timeline has no way to show or
 * edit it — so a per-song pitch override set from the playlist would draw as
 * though it were absent while still being heard. The global layer is left out
 * because a mix ignores it outright; see `build_plan` in `commands.rs`.
 */
function speedFor(block: MixBlock): number {
  const resolved = resolve([playlists.open?.mixer ?? {}, block.mixer ?? {}]);
  return resolved.enabled ? pitchRatio(resolved.pitch) : 1;
}

function waveformFor(block: MixBlock) {
  if (block.source.kind === "asset") return store.assetWaveforms[block.source.file] ?? null;
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
  for (const lane of mix.value.lanes) {
    for (const block of lane.blocks) {
      if (block.source.kind === "asset") void store.loadAssetWaveform(block.source.file);
    }
  }
}

watch(() => store.mix.lanes.length, loadVisibleWaveforms);
watch(() => store.playlistId, loadVisibleWaveforms);
watch(
  () => store.selection.join("\u0000"),
  () => {
    if (
      mixer.target.kind === "block" &&
      mixer.target.playlistId === store.playlistId &&
      (store.selection.length !== 1 || store.selection[0] !== mixer.target.blockId)
    ) {
      mixer.panelOpen = false;
    }
  },
);

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/**
 * Timeline seconds at a window x coordinate.
 *
 * Split from the event form because a file drop does not arrive as a pointer
 * event: Tauri reports it separately, with a position of its own.
 */
function timeAtClientX(clientX: number): number {
  const element = scroller.value;
  if (!element) return 0;
  const box = element.getBoundingClientRect();
  const x = clientX - box.left + element.scrollLeft - HEADER_WIDTH;
  return Math.max(0, x / pps.value);
}

/** Timeline seconds under a pointer event. */
function timeAt(event: PointerEvent | MouseEvent): number {
  return timeAtClientX(event.clientX);
}

/** Which lane a window y coordinate falls on; the lane count means "a new one". */
function laneIndexAtClientY(clientY: number): number {
  const element = scroller.value;
  if (!element) return mix.value.lanes.length;
  const box = element.getBoundingClientRect();
  const y = clientY - box.top + element.scrollTop - RULER_HEIGHT;
  const index = Math.floor(y / laneHeight.value);
  if (index < 0) return 0;
  if (index >= mix.value.lanes.length) return mix.value.lanes.length;
  return index;
}

// ---------------------------------------------------------------------------
// Dragging
// ---------------------------------------------------------------------------

interface Drag {
  mode: "move" | "trim-start" | "trim-end" | "move-point" | "curve";
  blockId: string;
  originX: number;
  originY: number;
  /** The arrangement as it was before this gesture; every frame recomputes
   *  from here, so a long drag cannot drift. */
  origin: MasterMix;
  ids: string[];
  moved: boolean;
  pointIndex?: number;
  /** Overlay box at pointer-down, so a drag can leave the block. */
  overlay?: DOMRect;
  durationSecs?: number;
}

const drag = ref<Drag | null>(null);

function onGrab(
  payload: { event: PointerEvent; mode: "move" | "trim-start" | "trim-end" },
  blockId: string,
) {
  const { event, mode } = payload;
  if (event.button !== 0) return;

  if (store.tool === "blade" && mode === "move") {
    // The blade snaps like a drag does: to where the other songs start and
    // end, and to the playhead. Cutting one song exactly where the next one
    // comes in is the reason to reach for it at all, and by hand that is a
    // few pixels of luck. Alt inverts it, as everywhere else.
    const wanted = timeAt(event);
    const snapping = store.snapping !== event.altKey;
    const at = snapping
      ? snapTime(
          wanted,
          snapTargets(mix.value, new Set([blockId]), [wanted]),
          SNAP_PIXELS / pps.value,
        )
      : wanted;
    const next = splitBlock(mix.value, blockId, at);
    if (next !== mix.value) store.commit(next);
    snapLine.value = null;
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

  // Alt is the universal "ignore snapping" modifier in timeline editors, and
  // inverts the toggle rather than only turning snapping off — so it is also
  // how you snap one drag while snapping is otherwise disabled.
  const snapping = store.snapping !== event.altKey;
  const tolerance = snapping ? SNAP_PIXELS / pps.value : 0;
  const found = locate(current.origin, current.blockId);
  if (!found) return;
  const excluded = new Set(current.ids);

  if (current.mode === "move") {
    const wanted = found.block.startSecs + dx / pps.value;
    const candidates = snapTargets(current.origin, excluded, [
      wanted,
      wanted + found.block.durationSecs,
    ]);
    const snapped = snapDrag(wanted, found.block.durationSecs, candidates, tolerance);
    snapLine.value = snappedAt([snapped, snapped + found.block.durationSecs], candidates);
    snapLocked.value = true;
    const laneDelta = Math.round(dy / laneHeight.value);
    store.mix = moveBlocks(current.origin, current.ids, snapped - found.block.startSecs, laneDelta);
    return;
  }

  const edge = current.mode === "trim-start" ? "start" : "end";
  const anchor = edge === "start" ? found.block.startSecs : found.block.startSecs + found.block.durationSecs;
  const dragged = anchor + dx / pps.value;
  const trimCandidates = snapTargets(current.origin, excluded, [dragged]);
  const wanted = snapTime(dragged, trimCandidates, tolerance);
  snapLine.value = snappedAt([wanted], trimCandidates);
  snapLocked.value = true;
  store.mix = trimBlock(current.origin, current.blockId, edge, wanted);
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
  snapLine.value = null;
  window.removeEventListener("pointermove", onDragMove);
  window.removeEventListener("pointerup", onDragEnd);
  window.removeEventListener("pointercancel", onDragCancel);
}

function onAutomation(
  payload: {
    event: PointerEvent;
    mode: "add" | "move-point" | "curve" | "remove";
    index?: number;
    atSecs: number;
    gainDb: number;
  },
  blockId: string,
) {
  const { event, mode } = payload;
  if (mode === "add") {
    store.commit(addAutomationPoint(mix.value, blockId, payload.atSecs, payload.gainDb));
    if (!store.selection.includes(blockId)) store.select([blockId]);
    return;
  }
  if (mode === "remove" && payload.index !== undefined) {
    store.commit(removeAutomationPoint(mix.value, blockId, payload.index));
    return;
  }
  if (payload.index === undefined) return;
  const found = locate(mix.value, blockId);
  const overlay = (event.currentTarget as Element | null)?.closest("svg")?.getBoundingClientRect();
  drag.value = {
    mode: mode === "curve" ? "curve" : "move-point",
    blockId,
    originX: event.clientX,
    originY: event.clientY,
    origin: cloneMix(mix.value),
    ids: [blockId],
    moved: false,
    pointIndex: payload.index,
    overlay,
    durationSecs: found?.block.durationSecs ?? 0,
  };
  (event.target as Element).setPointerCapture?.(event.pointerId);
  window.addEventListener("pointermove", onAutomationMove);
  window.addEventListener("pointerup", onAutomationEnd);
  window.addEventListener("pointercancel", onAutomationCancel);
}

function gainAtPointer(event: PointerEvent, box: DOMRect): number {
  const t = 1 - (event.clientY - box.top) / Math.max(1, box.height);
  return MIN_GAIN_DB + t * (MAX_GAIN_DB - MIN_GAIN_DB);
}

function onAutomationMove(event: PointerEvent) {
  const current = drag.value;
  if (!current || current.pointIndex === undefined || !current.overlay) return;
  if (current.mode !== "move-point" && current.mode !== "curve") return;
  current.moved = true;
  const found = locate(current.origin, current.blockId);
  if (!found) return;
  const box = current.overlay;
  const atSecs = ((event.clientX - box.left) / Math.max(1, box.width)) * (current.durationSecs ?? 0);
  const gainDb = gainAtPointer(event, box);

  if (current.mode === "move-point") {
    store.mix = moveAutomationPoint(
      current.origin,
      current.blockId,
      current.pointIndex,
      atSecs,
      gainDb,
    );
    return;
  }
  const points = found.block.automation;
  const a = points[current.pointIndex];
  const b = points[current.pointIndex + 1];
  if (!a || !b) return;
  store.mix = setAutomationCurve(
    current.origin,
    current.blockId,
    current.pointIndex,
    curveFromMidGain(a.gainDb, b.gainDb, gainDb),
  );
}

function onAutomationEnd() {
  const current = drag.value;
  stopAutomationListening();
  if (!current || (current.mode !== "move-point" && current.mode !== "curve")) return;
  if (!current.moved) return;
  const result = mix.value;
  store.mix = current.origin;
  store.commit(result);
}

function onAutomationCancel() {
  const current = drag.value;
  stopAutomationListening();
  if (current) store.mix = current.origin;
}

function stopAutomationListening() {
  drag.value = null;
  window.removeEventListener("pointermove", onAutomationMove);
  window.removeEventListener("pointerup", onAutomationEnd);
  window.removeEventListener("pointercancel", onAutomationCancel);
}

/**
 * The blade's line, following the pointer before anything has been cut.
 *
 * A cut is one click with nothing to undo it but undo, so where it will land
 * has to be visible before it happens. Only the snapped position is drawn:
 * with snapping off there is nothing to show that the cursor does not already
 * say.
 */
function onBladeHover(event: PointerEvent) {
  if (store.tool !== "blade" || drag.value) {
    snapLine.value = null;
    return;
  }
  // Over the lane headers there is no time under the pointer to draw at.
  const element = scroller.value;
  const box = element?.getBoundingClientRect();
  if (!box || event.clientX < box.left + HEADER_WIDTH) {
    snapLine.value = null;
    return;
  }

  const wanted = timeAt(event);
  if (!(store.snapping !== event.altKey)) {
    snapLine.value = wanted;
    snapLocked.value = false;
    return;
  }
  const over = blockAt(wanted, laneIndexAtClientY(event.clientY));
  const candidates = snapTargets(mix.value, new Set(over ? [over.id] : []), [wanted]);
  const at = snapTime(wanted, candidates, SNAP_PIXELS / pps.value);
  const locked = snappedAt([at], candidates);
  snapLine.value = locked ?? wanted;
  snapLocked.value = locked !== null;
}

/** The block covering `atSecs` on `laneIndex`, if there is one. */
function blockAt(atSecs: number, laneIndex: number): MixBlock | null {
  const lane = mix.value.lanes[laneIndex];
  if (!lane) return null;
  return (
    lane.blocks.find(
      (block) => atSecs >= block.startSecs && atSecs <= block.startSecs + block.durationSecs,
    ) ?? null
  );
}

// ---------------------------------------------------------------------------
// Marquee selection
// ---------------------------------------------------------------------------

/**
 * Dragging across empty track space selects everything the box touches, which
 * is how every timeline editor selects a passage rather than a region. A plain
 * click with no drag falls out of the same gesture as "select nothing", which
 * is what clicking the background is expected to do.
 */
const marquee = ref<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
let marqueeBase: string[] = [];

const marqueeBox = computed(() => {
  const box = marquee.value;
  if (!box) return null;
  return {
    left: Math.min(box.x0, box.x1),
    top: Math.min(box.y0, box.y1),
    width: Math.abs(box.x1 - box.x0),
    height: Math.abs(box.y1 - box.y0),
  };
});

/** A pointer position in the scrolling grid's own coordinates. */
function gridPoint(event: PointerEvent): { x: number; y: number } {
  const element = scroller.value;
  if (!element) return { x: 0, y: 0 };
  const box = element.getBoundingClientRect();
  return {
    x: event.clientX - box.left + element.scrollLeft,
    y: event.clientY - box.top + element.scrollTop,
  };
}

function onLaneBackgroundDown(event: PointerEvent) {
  if (event.button !== 0 || store.tool !== "select") return;
  const additive = event.shiftKey || event.metaKey || event.ctrlKey;
  marqueeBase = additive ? [...store.selection] : [];
  if (!additive) store.select([]);
  const point = gridPoint(event);
  marquee.value = { x0: point.x, y0: point.y, x1: point.x, y1: point.y };
  window.addEventListener("pointermove", onMarqueeMove);
  window.addEventListener("pointerup", onMarqueeEnd);
  window.addEventListener("pointercancel", onMarqueeEnd);
}

function onMarqueeMove(event: PointerEvent) {
  const box = marquee.value;
  if (!box) return;
  const point = gridPoint(event);
  marquee.value = { ...box, x1: point.x, y1: point.y };

  const fromSecs = (Math.min(box.x0, point.x) - HEADER_WIDTH) / pps.value;
  const toSecs = (Math.max(box.x0, point.x) - HEADER_WIDTH) / pps.value;
  const fromLane = Math.floor((Math.min(box.y0, point.y) - RULER_HEIGHT) / laneHeight.value);
  const toLane = Math.floor((Math.max(box.y0, point.y) - RULER_HEIGHT) / laneHeight.value);

  const hit = new Set(marqueeBase);
  mix.value.lanes.forEach((lane, laneIndex) => {
    if (laneIndex < fromLane || laneIndex > toLane) return;
    for (const block of lane.blocks) {
      // Touching counts, as in Logic: a box has to be dragged around a region
      // to *contain* it, but brushing it is what people mean.
      if (block.startSecs <= toSecs && block.startSecs + block.durationSecs >= fromSecs) {
        hit.add(block.id);
      }
    }
  });
  store.select([...hit]);
}

function onMarqueeEnd() {
  marquee.value = null;
  marqueeBase = [];
  window.removeEventListener("pointermove", onMarqueeMove);
  window.removeEventListener("pointerup", onMarqueeEnd);
  window.removeEventListener("pointercancel", onMarqueeEnd);
}

// ---------------------------------------------------------------------------
// Playhead
// ---------------------------------------------------------------------------

const scrubbing = ref(false);

/**
 * Where the playhead is *drawn*, which is not quite where the engine says it
 * is.
 *
 * The engine is polled five times a second. That is plenty to know where the
 * music has got to and nowhere near enough to draw a line that looks like it
 * is moving — at 200ms a piece the playhead visibly hops. So between reports
 * it is carried forward by the wall clock, and every report that arrives
 * re-anchors it. The audio stays the authority; only the frames in between
 * are invented.
 */
const renderPlayhead = ref(0);
let anchorSecs = 0;
let anchorAt = 0;
let frame = 0;

function anchorPlayhead() {
  anchorSecs = store.playhead;
  anchorAt = performance.now();
  renderPlayhead.value = anchorSecs;
}

/** Re-anchor on every position the store accepts, wherever it came from. */
watch(() => store.playhead, anchorPlayhead);
watch(() => store.previewing && !store.previewPaused, anchorPlayhead);

function onFrame() {
  frame = requestAnimationFrame(onFrame);
  if (!store.previewing || store.previewPaused || scrubbing.value) return;
  const elapsed = (performance.now() - anchorAt) / 1000;
  renderPlayhead.value = Math.min(anchorSecs + elapsed, Math.max(store.duration, 0));
  if (store.followPlayhead) keepPlayheadVisible();
}

/**
 * Scroll so a running playhead stays on screen — Logic's "catch playhead".
 *
 * Only once it has actually left the visible timeline, and then placed a
 * quarter of the way in rather than hard against the edge, so what comes next
 * is on screen instead of what has already been heard.
 */
function keepPlayheadVisible() {
  const element = scroller.value;
  if (!element) return;
  const visible = element.clientWidth - HEADER_WIDTH;
  if (visible <= 0) return;
  const x = renderPlayhead.value * pps.value;
  const from = element.scrollLeft;
  if (x >= from && x <= from + visible - 24) return;
  element.scrollLeft = Math.max(0, x - visible * 0.25);
}

function onRulerDown(event: PointerEvent) {
  if (event.button !== 0) return;
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
  // Auditioning follows the playhead; a paused transport stays paused.
  if (store.previewing) await store.reloadPreview();
}

function movePlayhead(event: PointerEvent) {
  seekTo(timeAt(event));
}

/** Put the playhead somewhere, clamped to the arrangement. */
function seekTo(secs: number) {
  store.setPlayhead(Math.min(Math.max(secs, 0), Math.max(store.duration, 0)));
}

/** Move the playhead and, if something is playing, take the audio with it. */
async function jumpTo(secs: number) {
  seekTo(secs);
  if (store.previewing) await store.reloadPreview();
}

/**
 * While the engine is playing the mix, the playhead is its position — but only
 * once the position being reported belongs to the plan that is now loaded.
 * The store holds that gate; see `applyEnginePosition`.
 */
watch(
  () => player.snapshot.positionSecs,
  (position) => {
    if (!scrubbing.value) store.applyEnginePosition(position);
  },
);

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

async function play() {
  if (store.previewing && store.previewPaused) await store.resume();
  else if (!store.previewing) await store.play(store.playhead);
}

async function pause() {
  await store.pause();
}

/**
 * Stop, and put the playhead back where playing began — not at zero.
 *
 * A timeline editor's playhead is a place you are working, and hearing the
 * same join twice should not mean finding it again. Stopping when already
 * parked at the start position does go to the beginning, so there is still a
 * one-key way back.
 */
async function stop() {
  const start = store.playStartSecs;
  await store.stop();
  seekTo(Math.abs(store.playhead - start) < 0.01 ? 0 : start);
}

function deleteSelection() {
  if (store.selection.length === 0) return;
  store.commit(deleteBlocks(mix.value, store.selection));
  store.select([]);
}

function duplicateSelection() {
  const duplicated = duplicateBlocks(mix.value, store.selection);
  if (duplicated.blockIds.length === 0) return;
  store.commit(duplicated.mix);
  store.select(duplicated.blockIds);
}

function startLaneRename(laneIndex: number) {
  const lane = mix.value.lanes[laneIndex];
  renamingLaneId.value = lane.id;
  renameValue.value = lane.name;
  void nextTick(() => {
    const input = dialog.value?.querySelector<HTMLInputElement>(`.mm__lane-name-input[data-lane-id="${lane.id}"]`);
    input?.focus();
    input?.select();
  });
}

function finishLaneRename(laneIndex: number) {
  const lane = mix.value.lanes[laneIndex];
  if (renamingLaneId.value !== lane.id) return;
  const name = renameValue.value.trim() || `Track ${laneIndex + 1}`;
  renamingLaneId.value = null;
  if (name !== lane.name) store.commit(updateLane(mix.value, laneIndex, { name }));
}

function cancelLaneRename() {
  renamingLaneId.value = null;
}

function setLaneColor(laneIndex: number, colorHue: number) {
  store.commit(updateLane(mix.value, laneIndex, { colorHue }));
  colorLane.value = null;
}

useDismiss(
  () => colorLane.value !== null,
  () => (colorLane.value = null),
  paletteEl,
  { ignore: [colorAnchor] },
);

function addCustomLane() {
  store.commit(addLane(mix.value, `Track ${mix.value.lanes.length + 1}`));
}

function dropLane(laneIndex: number) {
  colorLane.value = null;
  store.commit(removeLane(mix.value, laneIndex));
}

// Mute, solo and everything else audible get their audition rebuilt by
// `commit` itself, so an edit is heard without touching the transport.
function toggleMute(laneIndex: number) {
  const lane = mix.value.lanes[laneIndex];
  store.commit(updateLane(mix.value, laneIndex, { muted: !lane.muted }));
}

function toggleSolo(laneIndex: number) {
  const lane = mix.value.lanes[laneIndex];
  store.commit(updateLane(mix.value, laneIndex, { soloed: !lane.soloed }));
}

/**
 * The lane fader.
 *
 * A drag would otherwise put one undo step on the stack per frame, so the
 * arrangement is written straight through while the pointer is down and the
 * whole gesture is committed once on release — the same bargain the block
 * mixer's knobs make.
 */
let gainBeforeDrag: MasterMix | null = null;

function startLaneGain() {
  gainBeforeDrag = cloneMix(mix.value);
}

function onLaneGain(laneIndex: number, gainDb: number) {
  store.mix = updateLane(mix.value, laneIndex, { gainDb });
}

function endLaneGain() {
  const before = gainBeforeDrag;
  gainBeforeDrag = null;
  if (!before) return;
  const result = mix.value;
  store.mix = before;
  store.commit(result);
}

/**
 * Typing a level into a lane's readout.
 *
 * The field shows the lane's own gain until it is focused, and the draft
 * afterwards: without that, every keystroke would be overwritten by the
 * formatted value and a level could only ever be dragged to. What is typed is
 * read on the way out, so a half-finished number never reaches the mix.
 */
const editingGainLaneId = ref<string | null>(null);
const gainDraft = ref("");

function gainText(lane: { id: string; gainDb: number }): string {
  if (editingGainLaneId.value === lane.id) return gainDraft.value;
  return `${lane.gainDb > 0 ? "+" : ""}${lane.gainDb.toFixed(1)}`;
}

function startGainEdit(laneIndex: number) {
  const lane = mix.value.lanes[laneIndex];
  if (!lane) return;
  editingGainLaneId.value = lane.id;
  gainDraft.value = lane.gainDb.toFixed(1);
}

function finishGainEdit(laneIndex: number) {
  const lane = mix.value.lanes[laneIndex];
  if (!lane || editingGainLaneId.value !== lane.id) return;
  editingGainLaneId.value = null;
  const wanted = Number.parseFloat(gainDraft.value);
  // Nonsense simply puts the fader's own value back, rather than silencing a
  // lane because a stray character was typed into it.
  if (!Number.isFinite(wanted)) return;
  const gainDb =
    Math.round(Math.min(MAX_GAIN_DB, Math.max(MIN_GAIN_DB, wanted)) * 10) / 10;
  if (gainDb !== lane.gainDb) store.commit(updateLane(mix.value, laneIndex, { gainDb }));
}

function cancelGainEdit() {
  editingGainLaneId.value = null;
}

const hasSolo = computed(() => mix.value.lanes.some((lane) => lane.soloed));

function laneAudible(laneIndex: number): boolean {
  const lane = mix.value.lanes[laneIndex];
  return !!lane && !lane.muted && (!hasSolo.value || lane.soloed);
}

async function close() {
  if (mixer.target.kind === "block" && mixer.target.playlistId === store.playlistId) {
    mixer.panelOpen = false;
    await mixer.editGlobal();
  }
  await store.close();
}

async function resetArrangement() {
  await store.reset();
  loadVisibleWaveforms();
  ui.notify("Arrangement reset to the playlist order");
}

const selectedBlock = computed<MixBlock | null>(() => {
  if (store.selection.length !== 1) return null;
  return locate(mix.value, store.selection[0])?.block ?? null;
});
const blockMixerOpen = computed(
  () =>
    mixer.panelOpen &&
    mixer.target.kind === "block" &&
    mixer.target.playlistId === store.playlistId,
);

/** Open effects for exactly one selected audio region. */
async function openBlockMixer() {
  const block = selectedBlock.value;
  if (!block) {
    ui.notify("Select a block first");
    return;
  }
  const name =
    entryFor(block)?.title ??
    (block.source.kind === "asset" ? block.source.file : "Audio block");
  const playlistMixer = playlists.open?.mixer ?? null;
  // Recorded before the panel can write anything, so the first pitch change
  // has a speed to be measured against and the region resizes by the right
  // amount rather than from a standing start.
  store.noteBlockSpeed(block.id, speedFor(block));
  await mixer.editMixBlock(store.playlistId, block.id, name, block.mixer, playlistMixer);
  mixer.panelOpen = true;
}

async function importPaths(paths: string[], startSecs: number, laneIndex: number) {
  if (!store.playlistId || paths.length === 0) return;
  let next = mix.value;
  let lane = laneIndex;
  for (const path of paths) {
    try {
      const asset = await api.importMixAsset(store.playlistId, path);
      const name = asset.file.replace(/\.[^.]+$/, "");
      next = placeAsset(next, asset.file, asset.durationSecs, startSecs, lane, name);
      void store.loadAssetWaveform(asset.file);
      if (lane >= mix.value.lanes.length) lane = next.lanes.length - 1;
      startSecs += asset.durationSecs;
    } catch (e) {
      ui.notify(String(e), "error");
    }
  }
  if (next !== mix.value) store.commit(next);
}

async function chooseAudio() {
  const selected = await open({
    multiple: true,
    title: "Import audio into the mix",
    filters: [{ name: "Audio", extensions: [...AUDIO_EXTENSIONS] }],
  });
  if (selected === null) return;
  const paths = Array.isArray(selected) ? selected : [selected];
  await importPaths(paths, store.playhead, mix.value.lanes.length);
}

function isAudioPath(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase();
  return !!ext && (AUDIO_EXTENSIONS as readonly string[]).includes(ext);
}

const dropping = ref(false);
let unlistenDrop: UnlistenFn | null = null;

/**
 * Files dropped from the desktop.
 *
 * Not through HTML drag and drop: Tauri intercepts the webview's native drop
 * so it can hand over real paths, which means the DOM's `drop` never fires and
 * `File.path` — an Electron-ism — does not exist here anyway. The window
 * reports the drop instead, with a position in *physical* pixels that has to
 * be brought back to CSS pixels before it means anything to the layout.
 */
async function listenForDrops() {
  if (!("__TAURI_INTERNALS__" in window)) return;
  try {
    unlistenDrop = await getCurrentWebview().onDragDropEvent(async ({ payload }) => {
      if (payload.type === "leave") {
        dropping.value = false;
        return;
      }
      if (payload.type === "enter" || payload.type === "over") {
        dropping.value = store.open;
        return;
      }
      dropping.value = false;
      if (!store.open) return;
      const ratio = window.devicePixelRatio || 1;
      const x = payload.position.x / ratio;
      const y = payload.position.y / ratio;
      const paths = payload.paths.filter(isAudioPath);
      if (paths.length === 0) {
        ui.notify("Only MP3, FLAC and WAV files can become blocks");
        return;
      }
      await importPaths(paths, timeAtClientX(x), laneIndexAtClientY(y));
    });
  } catch (error) {
    // Without the drop listener the Import button still works, so this is not
    // worth interrupting the user for.
    console.error("Master mixer: file drops are unavailable:", error);
  }
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
  if (modifier && event.key.toLowerCase() === "d") {
    event.preventDefault();
    duplicateSelection();
    return;
  }
  if (modifier && event.key.toLowerCase() === "a") {
    event.preventDefault();
    selectAll();
    return;
  }
  if (modifier) return;

  switch (event.key) {
    case "Escape":
      event.preventDefault();
      if (blockMixerOpen.value) mixer.panelOpen = false;
      else void close();
      break;
    case " ":
      // Pause, never stop. Stopping is the button, and Logic's own space bar
      // leaves the playhead where the music got to.
      event.preventDefault();
      if (store.previewing && !store.previewPaused) void pause();
      else void play();
      break;
    case "Backspace":
    case "Delete":
      event.preventDefault();
      deleteSelection();
      break;
    // Return to the start, and Logic's own "go to end".
    case "Enter":
    case "Home":
      event.preventDefault();
      void jumpTo(0);
      break;
    case "End":
      event.preventDefault();
      void jumpTo(store.duration);
      break;
    // With something selected the arrows nudge it; with nothing selected they
    // walk the playhead, one ruler division at a time.
    case "ArrowLeft":
    case "ArrowRight": {
      event.preventDefault();
      const direction = event.key === "ArrowRight" ? 1 : -1;
      if (store.selection.length > 0) nudgeSelection(direction * nudgeSecs(event));
      else void jumpTo(store.playhead + direction * nudgeSecs(event));
      break;
    }
    case "ArrowUp":
    case "ArrowDown":
      if (store.selection.length === 0) break;
      event.preventDefault();
      nudgeSelectionLanes(event.key === "ArrowDown" ? 1 : -1);
      break;
    case "v":
      store.tool = "select";
      break;
    case "b":
      store.tool = "blade";
      break;
    case "a":
      store.tool = "automation";
      break;
    default:
      break;
  }
}

/** A whole ruler division normally, a tenth of one with Shift held. */
function nudgeSecs(event: KeyboardEvent): number {
  return event.shiftKey ? step.value / 10 : step.value;
}

function nudgeSelection(deltaSecs: number) {
  store.commit(moveBlocks(mix.value, store.selection, deltaSecs));
}

function nudgeSelectionLanes(delta: number) {
  store.commit(moveBlocks(mix.value, store.selection, 0, delta));
}

function selectAll() {
  store.select(mix.value.lanes.flatMap((lane) => lane.blocks.map((block) => block.id)));
}

/**
 * Zoom the time axis while holding one timeline position still on screen.
 *
 * The buttons hold the playhead, so zooming in on the join you are working on
 * does not send it off the edge; the wheel holds the pointer, which is what a
 * wheel over a timeline is expected to do.
 */
function zoomAround(factor: number, atSecs: number, screenX: number) {
  const element = scroller.value;
  store.zoom(factor);
  if (!element) return;
  void nextTick(() => {
    element.scrollLeft = Math.max(0, atSecs * store.pixelsPerSecond - screenX);
  });
}

function zoomTime(factor: number) {
  const element = scroller.value;
  const visible = element ? element.clientWidth - HEADER_WIDTH : 0;
  const from = element?.scrollLeft ?? 0;
  const held = renderPlayhead.value;
  // A playhead that is not on screen is no use as an anchor; hold the middle
  // of what the user is actually looking at instead.
  const onScreen = held * pps.value >= from && held * pps.value <= from + visible;
  const anchorSeconds = onScreen ? held : (from + visible / 2) / pps.value;
  zoomAround(factor, anchorSeconds, anchorSeconds * pps.value - from);
}

/** Fit the whole arrangement across the timeline, with a little air. */
function zoomToFit() {
  const element = scroller.value;
  if (!element) return;
  const visible = element.clientWidth - HEADER_WIDTH;
  const span = Math.max(store.duration, 1);
  store.zoom(((visible - 24) / span) / pps.value);
  void nextTick(() => (element.scrollLeft = 0));
}

/** Continuous, restrained zoom: Option adjusts lane height, Cmd/Ctrl adjusts time. */
function onWheel(event: WheelEvent) {
  const factor = Math.exp(-event.deltaY * 0.0015);
  if (event.altKey) {
    event.preventDefault();
    store.zoomTracks(factor);
    return;
  }
  if (!(event.metaKey || event.ctrlKey)) return;
  event.preventDefault();
  const element = scroller.value;
  if (!element) return;
  const before = timeAtClientX(event.clientX);
  const box = element.getBoundingClientRect();
  zoomAround(factor, before, event.clientX - box.left - HEADER_WIDTH);
}

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  loadVisibleWaveforms();
  dialog.value?.focus();
  anchorPlayhead();
  frame = requestAnimationFrame(onFrame);
  void listenForDrops();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  cancelAnimationFrame(frame);
  unlistenDrop?.();
  unlistenDrop = null;
  stopDragListening();
  stopAutomationListening();
  onMarqueeEnd();
});

const summary = computed(() => {
  const blocks = mix.value.lanes.reduce((sum, lane) => sum + lane.blocks.length, 0);
  return `${mix.value.lanes.length} tracks · ${blocks} blocks · ${formatDuration(mixDuration(mix.value))}`;
});
</script>

<template>
  <div class="mm-scrim" :class="{ 'is-drop': dropping }" @pointerdown.self="close">
    <div class="mm__workspace" :class="{ 'is-editing': blockMixerOpen }">
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
            :title="entry.hint"
            :aria-label="entry.label"
            :aria-pressed="store.tool === entry.id"
            @click="store.tool = entry.id"
          >
            <PnmIcon :name="entry.icon" :size="17" />
          </button>
        </div>

        <button
          class="mm__duplicate-button"
          type="button"
          :disabled="store.selection.length === 0"
          title="Duplicate selected blocks (Cmd/Ctrl+D)"
          @click="duplicateSelection"
        >
          Duplicate
        </button>
        <button
          class="mm__mixer-button"
          type="button"
          :disabled="!selectedBlock"
          :title="selectedBlock ? 'Effects for the selected block' : 'Select exactly one audio block'"
          @click="openBlockMixer"
        >
          Block Mixer
        </button>

        <button class="icon-button" type="button" aria-label="Close master mixer" @click="close">
          <PnmIcon name="close" :size="17" />
        </button>
      </header>

      <div class="mm__transport">
        <button
          class="icon-button"
          type="button"
          aria-label="Play the mix"
          :disabled="store.previewing && !store.previewPaused"
          @click="play"
        >
          <PnmIcon name="play" :size="17" />
        </button>
        <button
          class="icon-button"
          type="button"
          aria-label="Pause"
          :disabled="!store.previewing || store.previewPaused"
          @click="pause"
        >
          <PnmIcon name="pause" :size="17" />
        </button>
        <button class="icon-button" type="button" aria-label="Stop" :disabled="!store.previewing" @click="stop">
          <PnmIcon name="stop" :size="15" />
        </button>
        <span class="mm__timecode">{{ timecode(renderPlayhead) }}</span>

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

        <button
          class="mm__toggle"
          type="button"
          :class="{ 'is-on': store.snapping }"
          :aria-pressed="store.snapping"
          title="Snap to other blocks' edges and the playhead, when dragging, trimming or cutting. Hold Alt to invert this."
          @click="store.snapping = !store.snapping"
        >
          Snap
        </button>
        <button
          class="mm__toggle"
          type="button"
          :class="{ 'is-on': store.gridSnapping }"
          :aria-pressed="store.gridSnapping"
          :disabled="!store.snapping"
          :title="`Also snap to the ruler's marks above the tracks, currently every ${gridLabel}`"
          @click="store.gridSnapping = !store.gridSnapping"
        >
          Grid
        </button>
        <button
          class="mm__toggle"
          type="button"
          :class="{ 'is-on': store.followPlayhead }"
          :aria-pressed="store.followPlayhead"
          title="Scroll to keep the playhead on screen while the mix plays"
          @click="store.followPlayhead = !store.followPlayhead"
        >
          Follow
        </button>

        <span class="mm__divider" />

        <div class="mm__zoom-group" role="group" aria-label="Timeline zoom">
          <span>Time</span>
          <button class="icon-button" type="button" aria-label="Zoom timeline out" @click="zoomTime(1 / 1.4)">
            <PnmIcon name="minimize" :size="16" />
          </button>
          <button class="icon-button" type="button" aria-label="Zoom timeline in" @click="zoomTime(1.4)">
            <PnmIcon name="plus" :size="16" />
          </button>
          <button class="mm__toggle" type="button" title="Fit the whole mix on screen" @click="zoomToFit">
            Fit
          </button>
        </div>
        <div class="mm__zoom-group" role="group" aria-label="Track height">
          <span>Tracks</span>
          <button class="icon-button" type="button" aria-label="Decrease track height" @click="store.zoomTracks(1 / 1.2)">
            <PnmIcon name="minimize" :size="16" />
          </button>
          <button class="icon-button" type="button" aria-label="Increase track height" @click="store.zoomTracks(1.2)">
            <PnmIcon name="plus" :size="16" />
          </button>
        </div>

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
        @pointermove="onBladeHover"
        @pointerleave="snapLine = null"
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
                v-for="secs in minorTicks"
                :key="`m${secs}`"
                class="mm__tick mm__tick--minor"
                :style="{ left: `${secs * pps}px` }"
              />
              <span
                v-for="tick in ticks"
                :key="tick.secs"
                class="mm__tick"
                :style="{ left: `${tick.secs * pps}px` }"
                >{{ tick.label }}</span
              >
              <span
                class="mm__playhead-head"
                :style="{ left: `${renderPlayhead * pps}px` }"
                aria-hidden="true"
              />
            </div>
          </div>

          <div
            v-for="(lane, laneIndex) in mix.lanes"
            :key="lane.id"
            class="mm__lane"
            :class="{
              'is-muted': lane.muted,
              'is-solo-silenced': !lane.muted && !laneAudible(laneIndex),
            }"
            :style="{ height: `${laneHeight}px` }"
          >
            <div class="mm__lane-head" :style="{ width: `${HEADER_WIDTH}px` }">
              <button
                type="button"
                class="mm__lane-swatch"
                :style="{ background: `hsl(${hueFor(laneIndex)} 60% 62%)` }"
                :aria-label="`Choose colour for ${lane.name}`"
                :aria-expanded="colorLane?.index === laneIndex"
                @click="toggleColorPicker(laneIndex, $event)"
              />
              <div class="mm__lane-body">
                <div class="mm__lane-row">
                  <input
                    v-if="renamingLaneId === lane.id"
                    v-model="renameValue"
                    class="mm__lane-name mm__lane-name-input"
                    :data-lane-id="lane.id"
                    :aria-label="`Rename ${lane.name}`"
                    @keydown.enter.prevent="finishLaneRename(laneIndex); ($event.target as HTMLInputElement).blur()"
                    @keydown.escape.prevent="cancelLaneRename(); ($event.target as HTMLInputElement).blur()"
                    @blur="finishLaneRename(laneIndex)"
                  />
                  <span
                    v-else
                    class="mm__lane-name"
                    :title="`${lane.name} — double-click to rename`"
                    @dblclick="startLaneRename(laneIndex)"
                  >{{ lane.name || `Track ${laneIndex + 1}` }}</span>
                  <div class="mm__lane-buttons">
                    <button
                      type="button"
                      class="mm__ms"
                      :class="{ 'is-on': lane.muted }"
                      :aria-pressed="lane.muted"
                      :title="`${lane.muted ? 'Unmute' : 'Mute'} ${lane.name}`"
                      :aria-label="`${lane.muted ? 'Unmute' : 'Mute'} ${lane.name}`"
                      @click="toggleMute(laneIndex)"
                    >
                      M
                    </button>
                    <button
                      type="button"
                      class="mm__ms mm__ms--solo"
                      :class="{ 'is-on': lane.soloed }"
                      :aria-pressed="lane.soloed"
                      :title="`${lane.soloed ? 'Unsolo' : 'Solo'} ${lane.name}`"
                      :aria-label="`${lane.soloed ? 'Unsolo' : 'Solo'} ${lane.name}`"
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

                <!-- The lane fader, which the arrangement has always stored
                     and nothing could reach. Hidden on short lanes, as Logic
                     hides a track's controls when there is no room. -->
                <div v-if="laneHeight >= 62" class="mm__lane-gain">
                  <AppSlider
                    :model-value="lane.gainDb"
                    :min="MIN_GAIN_DB"
                    :max="MAX_GAIN_DB"
                    :step="0.5"
                    :origin="0"
                    :detents="[0]"
                    subtle
                    @start="startLaneGain"
                    @update:model-value="onLaneGain(laneIndex, $event)"
                    @end="endLaneGain"
                  />
                  <!-- Typed as well as dragged: a fader is for finding a level,
                       and a number is for matching one exactly. -->
                  <input
                    class="mm__lane-db"
                    type="text"
                    inputmode="decimal"
                    spellcheck="false"
                    :value="gainText(lane)"
                    :aria-label="`Level for ${lane.name}, in decibels`"
                    :title="`Level for ${lane.name} in decibels, between ${MIN_GAIN_DB} and +${MAX_GAIN_DB}`"
                    @focus="startGainEdit(laneIndex); ($event.target as HTMLInputElement).select()"
                    @input="gainDraft = ($event.target as HTMLInputElement).value"
                    @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
                    @keydown.escape.prevent="cancelGainEdit(); ($event.target as HTMLInputElement).blur()"
                    @blur="finishGainEdit(laneIndex)"
                  />
                </div>
              </div>
            </div>

            <div
              class="mm__lane-track"
              :style="{ width: `${contentWidth}px` }"
              @pointerdown.self="onLaneBackgroundDown"
            >
              <MixBlockView
                v-for="block in lane.blocks"
                :key="block.id"
                :block="block"
                :entry="entryFor(block)"
                :waveform="waveformFor(block)"
                :speed="speedFor(block)"
                :pixels-per-second="pps"
                :height="laneHeight"
                :selected="store.selected.has(block.id)"
                :hue="hueFor(laneIndex)"
                :tool="store.tool"
                @grab="onGrab($event, block.id)"
                @automation="onAutomation($event, block.id)"
                @open-mixer="store.select([block.id]); openBlockMixer()"
              />
            </div>
          </div>

          <div class="mm__add-lane" :style="{ width: `${HEADER_WIDTH}px` }">
            <button type="button" @click="addCustomLane">
              <PnmIcon name="plus" :size="13" />
              <span>Add Custom Track</span>
            </button>
            <button type="button" @click="chooseAudio">
              <PnmIcon name="importFile" :size="13" />
              <span>Import audio</span>
            </button>
          </div>

          <div
            v-if="marqueeBox"
            class="mm__marquee"
            :style="{
              left: `${marqueeBox.left}px`,
              top: `${marqueeBox.top}px`,
              width: `${marqueeBox.width}px`,
              height: `${marqueeBox.height}px`,
            }"
            aria-hidden="true"
          />

          <div
            v-if="snapLine !== null"
            class="mm__snapline"
            :class="{ 'is-locked': snapLocked }"
            :style="{ left: `${HEADER_WIDTH + snapLine * pps}px` }"
            aria-hidden="true"
          />

          <div
            class="mm__playhead"
            :style="{ left: `${HEADER_WIDTH + renderPlayhead * pps}px` }"
            aria-hidden="true"
          />
        </div>
      </div>

      <p v-else class="mm__loading">Loading the arrangement…</p>

      <footer class="mm__footer">
        <span v-if="store.tool === 'blade'">
          Click a block to split it. With Snap on the cut lands on the nearest block
          edge or the playhead; Alt inverts that. Press V for the pointer.
        </span>
        <span v-else-if="store.tool === 'automation'">
          Click to add a keyframe, drag to move it, drag the midpoint to bend the curve. Double-click
          a point to remove it.
        </span>
        <span v-else>
          Drag to move, drag an edge to trim, drag empty space to select. Alt inverts snapping,
          arrows nudge, Cmd/Ctrl+D duplicates. Option+wheel changes track height. Drop MP3, FLAC
          or WAV anywhere to import.
        </span>
      </footer>
    </section>
    <Transition name="slide-panel">
      <AdvancedMixer v-if="blockMixerOpen" class="mm__block-mixer" />
    </Transition>
    </div>

    <Teleport to="body">
      <div
        v-if="colorLane"
        ref="paletteEl"
        class="mm__palette"
        :class="{ 'is-flipped': colorLane.flip }"
        role="menu"
        aria-label="Track colours"
        :style="{ left: `${colorLane.x}px`, top: `${colorLane.y}px` }"
      >
        <button
          v-for="colorHue in COLOR_HUES"
          :key="colorHue"
          type="button"
          class="mm__palette-color"
          :class="{ 'is-selected': hueFor(colorLane.index) === colorHue }"
          :style="{ background: `hsl(${colorHue} 60% 62%)` }"
          :aria-label="`Use hue ${colorHue}`"
          @click="setLaneColor(colorLane.index, colorHue)"
        />
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.mm-scrim.is-drop .mm {
  outline: 2px dashed var(--accent);
  outline-offset: -8px;
}

.mm-scrim {
  position: fixed;
  inset: 0;
  z-index: 530;
  display: flex;
  align-items: center;
  justify-content: center;
  /* Even on every side, and as small as it can be: the modal is meant to fill
     the window. The traffic lights are dealt with in the header, which is the
     only part of this that has anything to click underneath them. */
  padding: 20px;
  background: rgba(0, 0, 0, 0.4);
  backdrop-filter: blur(4px);
}

.mm__workspace {
  position: relative;
  display: flex;
  width: 100%;
  height: 100%;
  min-width: 0;
}

/* "Fills most of the window, with little deadspace" — the drawing's proportions. */
.mm {
  display: flex;
  flex-direction: column;
  flex: 1;
  width: 100%;
  min-width: 0;
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
  /* On macOS the window's traffic lights are drawn over this corner, so the
     title starts after them. `--overlay-controls` is zero everywhere else. */
  padding: 12px 14px 12px calc(14px + var(--overlay-controls));
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

.mm__mixer-button,
.mm__duplicate-button {
  padding: 5px 14px;
  border: 0.5px solid var(--separator-strong);
  border-radius: 999px;
  font-size: 12px;
  color: var(--text);
}

.mm__mixer-button:hover:not(:disabled),
.mm__duplicate-button:hover:not(:disabled) {
  background: var(--bg-hover);
}

.mm__mixer-button:disabled,
.mm__duplicate-button:disabled {
  opacity: 0.42;
  cursor: default;
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

.mm__zoom-group {
  display: flex;
  align-items: center;
  gap: 2px;
}

.mm__zoom-group > span {
  margin: 0 3px 0 2px;
  font-size: 10px;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
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

/*
 * Stacking inside the grid, low to high: blocks, the playhead, the lane
 * headers, the ruler, the corner. The headers sit *above* the playhead
 * deliberately — the playhead is positioned in grid coordinates, so once the
 * timeline is scrolled right it would otherwise be drawn across the track
 * names as if it were somewhere it is not.
 */
.mm__ruler-row {
  position: sticky;
  top: 0;
  z-index: 7;
  display: flex;
  height: 26px;
  background: var(--bg-sunken);
  border-bottom: 0.5px solid var(--separator);
}

.mm__corner {
  position: sticky;
  left: 0;
  z-index: 8;
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

.mm__tick--minor {
  top: auto;
  bottom: 0;
  height: 6px;
  border-left-color: var(--separator-strong);
  opacity: 0.6;
}

/* The grab handle a timeline ruler has, so the playhead is something you can
   see and take hold of rather than a hairline. */
.mm__playhead-head {
  position: absolute;
  bottom: 0;
  width: 9px;
  height: 9px;
  margin-left: -4px;
  background: var(--accent);
  clip-path: polygon(0 0, 100% 0, 50% 100%);
  pointer-events: none;
}

.mm__lane {
  display: flex;
  border-bottom: 0.5px solid var(--separator);
}

.mm__lane-head {
  position: sticky;
  left: 0;
  z-index: 6;
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
  display: block;
  width: 7px;
  height: 32px;
  border-radius: 3px;
}

/* Above the modal's own scrim rather than inside the scrolling grid, which is
   what stops later lanes painting over it. */
.mm__palette {
  position: fixed;
  z-index: 620;
  display: grid;
  grid-template-columns: repeat(3, 18px);
  gap: 5px;
  padding: 7px;
  border: 0.5px solid var(--separator-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-elevated);
  box-shadow: var(--shadow-popover);
}

/* Anchored by its bottom edge when there is no room below the swatch. */
.mm__palette.is-flipped {
  transform: translateY(-100%);
}

.mm__palette-color {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  border: 1px solid transparent;
}

.mm__palette-color.is-selected {
  border-color: var(--text);
  box-shadow: 0 0 0 1px var(--bg-elevated), 0 0 0 2px var(--text);
}

.mm__lane-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.mm__lane-row {
  display: flex;
  align-items: center;
  gap: 7px;
}

.mm__lane-gain {
  display: flex;
  align-items: center;
  gap: 6px;
}

.mm__lane-db {
  flex: none;
  width: 36px;
  padding: 1px 2px;
  border: 1px solid transparent;
  border-radius: 3px;
  outline: none;
  background: transparent;
  text-align: right;
  font: inherit;
  font-size: 9.5px;
  font-variant-numeric: tabular-nums;
  color: var(--text-tertiary);
}

.mm__lane-db:hover {
  border-color: var(--separator-strong);
  color: var(--text-secondary);
}

.mm__lane-db:focus {
  border-color: var(--accent);
  background: var(--bg);
  color: var(--text);
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

.mm__lane-name-input {
  padding: 2px 4px;
  border: 1px solid var(--accent);
  border-radius: 3px;
  outline: none;
  background: var(--bg);
  color: var(--text);
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
  transition: opacity 0.15s var(--ease), filter 0.15s var(--ease);
}

.mm__lane.is-muted .mm__lane-track,
.mm__lane.is-solo-silenced .mm__lane-track {
  opacity: 0.32;
  filter: saturate(0.35);
}

.mm__lane.is-solo-silenced .mm__lane-name {
  color: var(--text-tertiary);
}

.mm__add-lane {
  position: sticky;
  left: 0;
  z-index: 6;
  display: flex;
  flex-direction: column;
  gap: 2px;
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

/* Distinct from the playhead, which is the other vertical line here: the
   second accent, and dashed while it is only following the cursor. Solid means
   it has locked on to an edge, the playhead or a ruler mark. */
.mm__snapline {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 0;
  z-index: 5;
  border-left: 1px dashed var(--accent-secondary);
  opacity: 0.55;
  pointer-events: none;
}

.mm__snapline.is-locked {
  border-left-style: solid;
  opacity: 1;
}

.mm__marquee {
  position: absolute;
  z-index: 4;
  border: 1px solid var(--accent);
  background: var(--accent-tint);
  pointer-events: none;
}

.mm__toggle {
  padding: 3px 9px;
  border: 0.5px solid var(--separator-strong);
  border-radius: 999px;
  font-size: 11px;
  color: var(--text-secondary);
}

.mm__toggle:hover {
  background: var(--bg-hover);
  color: var(--text);
}

.mm__toggle:disabled {
  opacity: 0.4;
  cursor: default;
}

.mm__toggle.is-on {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-contrast);
}

.mm__loading {
  flex: 1;
  display: grid;
  place-items: center;
  margin: 0;
  font-size: 13px;
  color: var(--text-tertiary);
}

.mm__block-mixer {
  height: 100%;
  border: 0.5px solid var(--separator);
  border-radius: 0 var(--radius-lg) var(--radius-lg) 0;
  overflow: hidden;
  box-shadow: var(--shadow-popover);
}

.mm__workspace.is-editing .mm {
  border-radius: var(--radius-lg) 0 0 var(--radius-lg);
}

.mm__footer {
  margin: 0;
  padding: 8px 14px;
  border-top: 0.5px solid var(--separator);
  font-size: 11.5px;
  color: var(--text-tertiary);
  background: var(--bg-sidebar);
}

@media (max-width: 1180px) {
  .mm__block-mixer {
    position: absolute;
    z-index: 8;
    top: 0;
    right: 0;
    width: min(var(--mixer-width), 92vw);
  }

  .mm__workspace.is-editing .mm {
    border-radius: var(--radius-lg);
  }
}

@media (max-width: 760px) {
  .mm-scrim {
    padding: 10px;
  }

  .mm__header {
    flex-wrap: wrap;
  }

  .mm__transport {
    overflow-x: auto;
  }

  .mm__enable span {
    display: none;
  }
}
</style>
