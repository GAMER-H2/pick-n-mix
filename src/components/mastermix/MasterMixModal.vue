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
import AdvancedMixer from "../mixer/AdvancedMixer.vue";
import MixBlockView from "./MixBlockView.vue";
import { open } from "@tauri-apps/plugin-dialog";
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
const colorLaneId = ref<string | null>(null);
const renamingLaneId = ref<string | null>(null);
const renameValue = ref("");
const COLOR_HUES = [8, 32, 55, 105, 165, 205, 245, 285, 325] as const;

const ticks = computed(() => {
  const out: { secs: number; label: string }[] = [];
  for (let t = 0; t <= contentSecs.value; t += step.value) {
    out.push({ secs: t, label: formatDuration(t) });
  }
  return out;
});

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
    const laneDelta = Math.round(dy / laneHeight.value);
    store.mix = moveBlocks(current.origin, current.ids, snapped - found.block.startSecs, laneDelta);
    return;
  }

  const edge = current.mode === "trim-start" ? "start" : "end";
  const anchor = edge === "start" ? found.block.startSecs : found.block.startSecs + found.block.durationSecs;
  const wanted = snapTime(anchor + dx / pps.value, candidates, tolerance);
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
  // Auditioning follows the playhead; a paused transport stays paused.
  if (store.previewing) await store.reloadPreview();
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

async function play() {
  if (store.previewing && store.previewPaused) await store.resume();
  else if (!store.previewing) await store.play(store.playhead);
}

async function pause() {
  await store.pause();
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
  colorLaneId.value = null;
}

function addCustomLane() {
  store.commit(addLane(mix.value, `Track ${mix.value.lanes.length + 1}`));
}

function dropLane(laneIndex: number) {
  store.commit(removeLane(mix.value, laneIndex));
}

async function toggleMute(laneIndex: number) {
  const lane = mix.value.lanes[laneIndex];
  store.commit(updateLane(mix.value, laneIndex, { muted: !lane.muted }));
  await store.reloadPreview();
}

async function toggleSolo(laneIndex: number) {
  const lane = mix.value.lanes[laneIndex];
  store.commit(updateLane(mix.value, laneIndex, { soloed: !lane.soloed }));
  await store.reloadPreview();
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
  await mixer.editMixBlock(store.playlistId, block.id, name, block.mixer, playlistMixer);
  mixer.panelOpen = true;
}

function laneIndexAt(event: PointerEvent | DragEvent): number {
  const element = scroller.value;
  if (!element) return mix.value.lanes.length;
  const box = element.getBoundingClientRect();
  const y = event.clientY - box.top + element.scrollTop - RULER_HEIGHT;
  const index = Math.floor(y / laneHeight.value);
  if (index < 0) return 0;
  if (index >= mix.value.lanes.length) return mix.value.lanes.length;
  return index;
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

function pathsFromDrop(event: DragEvent): string[] {
  const files = event.dataTransfer?.files;
  if (!files) return [];
  return Array.from(files)
    .map((file) => (file as File & { path?: string }).path)
    .filter((path): path is string => typeof path === "string" && isAudioPath(path));
}

const dropping = ref(false);

function onFileDrag(event: DragEvent) {
  if (!event.dataTransfer?.types.includes("Files")) return;
  event.preventDefault();
  dropping.value = true;
}

function onFileDragLeave(event: DragEvent) {
  if (event.currentTarget === event.target) dropping.value = false;
}

async function onFileDrop(event: DragEvent) {
  event.preventDefault();
  dropping.value = false;
  const paths = pathsFromDrop(event);
  if (paths.length === 0) {
    ui.notify("Only MP3, FLAC and WAV files can become blocks");
    return;
  }
  await importPaths(paths, timeAt(event as unknown as PointerEvent), laneIndexAt(event));
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
  switch (event.key) {
    case "Escape":
      event.preventDefault();
      if (blockMixerOpen.value) mixer.panelOpen = false;
      else void close();
      break;
    case " ":
      event.preventDefault();
      if (store.previewing && !store.previewPaused) void pause();
      else void play();
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
    case "a":
      store.tool = "automation";
      break;
    default:
      break;
  }
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
  const before = timeAt(event as unknown as PointerEvent);
  store.zoom(factor);
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
  stopAutomationListening();
});

const summary = computed(() => {
  const blocks = mix.value.lanes.reduce((sum, lane) => sum + lane.blocks.length, 0);
  return `${mix.value.lanes.length} tracks · ${blocks} blocks · ${formatDuration(mixDuration(mix.value))}`;
});
</script>

<template>
  <div
    class="mm-scrim"
    :class="{ 'is-drop': dropping }"
    @pointerdown.self="close"
    @dragenter="onFileDrag"
    @dragover="onFileDrag"
    @dragleave="onFileDragLeave"
    @drop="onFileDrop"
  >
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

        <div class="mm__zoom-group" role="group" aria-label="Timeline zoom">
          <span>Time</span>
          <button class="icon-button" type="button" aria-label="Zoom timeline out" @click="store.zoom(1 / 1.4)">
            <PnmIcon name="minimize" :size="16" />
          </button>
          <button class="icon-button" type="button" aria-label="Zoom timeline in" @click="store.zoom(1.4)">
            <PnmIcon name="plus" :size="16" />
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
            :class="{
              'is-muted': lane.muted,
              'is-solo-silenced': !lane.muted && !laneAudible(laneIndex),
            }"
            :style="{ height: `${laneHeight}px` }"
          >
            <div class="mm__lane-head" :style="{ width: `${HEADER_WIDTH}px` }">
              <div class="mm__color-wrap">
                <button
                  type="button"
                  class="mm__lane-swatch"
                  :style="{ background: `hsl(${hueFor(laneIndex)} 60% 62%)` }"
                  :aria-label="`Choose colour for ${lane.name}`"
                  :aria-expanded="colorLaneId === lane.id"
                  @click="colorLaneId = colorLaneId === lane.id ? null : lane.id"
                />
                <div v-if="colorLaneId === lane.id" class="mm__palette" role="menu" aria-label="Track colours">
                  <button
                    v-for="colorHue in COLOR_HUES"
                    :key="colorHue"
                    type="button"
                    class="mm__palette-color"
                    :class="{ 'is-selected': hueFor(laneIndex) === colorHue }"
                    :style="{ background: `hsl(${colorHue} 60% 62%)` }"
                    :aria-label="`Use hue ${colorHue}`"
                    @click="setLaneColor(laneIndex, colorHue)"
                  />
                </div>
              </div>
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

            <div class="mm__lane-track" :style="{ width: `${contentWidth}px` }">
              <MixBlockView
                v-for="block in lane.blocks"
                :key="block.id"
                :block="block"
                :entry="entryFor(block)"
                :waveform="waveformFor(block)"
                :pixels-per-second="pps"
                :height="laneHeight"
                :selected="store.selected.has(block.id)"
                :hue="hueFor(laneIndex)"
                :tool="store.tool"
                @grab="onGrab($event, block.id)"
                @automation="onAutomation($event, block.id)"
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
            class="mm__playhead"
            :style="{ left: `${HEADER_WIDTH + store.playhead * pps}px` }"
            aria-hidden="true"
          />
        </div>
      </div>

      <p v-else class="mm__loading">Loading the arrangement…</p>

      <footer class="mm__footer">
        <span v-if="store.tool === 'blade'">Click a block to split it. Press V for the pointer.</span>
        <span v-else-if="store.tool === 'automation'">
          Click to add a keyframe, drag to move it, drag the midpoint to bend the curve. Double-click
          a point to remove it.
        </span>
        <span v-else>
          Drag to move, drag an edge to trim, hold Alt to ignore snapping. Cmd/Ctrl+D duplicates.
          Option+wheel changes track height. Drop MP3, FLAC or WAV to import.
        </span>
      </footer>
    </section>
    <Transition name="slide-panel">
      <AdvancedMixer v-if="blockMixerOpen" class="mm__block-mixer" />
    </Transition>
    </div>
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
  /* Keep interactive chrome clear of macOS's overlay traffic lights. */
  padding: 44px 22px 22px 78px;
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

.mm__color-wrap {
  position: relative;
  flex: none;
}

.mm__lane-swatch {
  display: block;
  width: 7px;
  height: 32px;
  border-radius: 3px;
}

.mm__palette {
  position: absolute;
  z-index: 8;
  top: calc(100% + 5px);
  left: 0;
  display: grid;
  grid-template-columns: repeat(3, 18px);
  gap: 5px;
  padding: 7px;
  border: 0.5px solid var(--separator-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-elevated);
  box-shadow: var(--shadow-popover);
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
  z-index: 2;
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
    padding-right: 10px;
    padding-bottom: 10px;
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
