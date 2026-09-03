import { defineStore } from "pinia";
import { computed, ref } from "vue";
import * as api from "@/lib/api";
import {
  mixDuration,
  scaleBlockForSpeed,
  setBlockMixer as patchBlockMixer,
  soundSignature,
} from "@/lib/masterMix";
import type { MasterMix, MixEntry, MixerSettings, Waveform } from "@/lib/types";

/** Which of the three tools in the drawing is armed. */
export type Tool = "select" | "blade" | "automation";

const EMPTY: MasterMix = { enabled: false, revision: 0, lanes: [] };
/** How many edits back the user can go. Snapshots are small — a mix is a few
 *  hundred numbers — so this can be generous. */
const UNDO_DEPTH = 100;
/** Edits are batched into one write rather than one per drag frame. */
const SAVE_DELAY_MS = 600;
/** How far short of a requested position still counts as having arrived. */
const STALE_TOLERANCE_SECS = 0.35;

export const useMasterMixStore = defineStore("masterMix", () => {
  const open = ref(false);
  const playlistId = ref("");
  const playlistName = ref("");
  const mix = ref<MasterMix>(EMPTY);
  const entries = ref<MixEntry[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const tool = ref<Tool>("select");
  const selection = ref<string[]>([]);
  const pixelsPerSecond = ref(8);
  /** Session-only vertical zoom. Kept out of the saved arrangement. */
  const laneHeight = ref(74);
  /** Where the playhead sits, in timeline seconds. Driven by the engine while
   *  auditioning and by the user's clicks otherwise. */
  const playhead = ref(0);
  /**
   * Where the last audition was started from.
   *
   * Stop returns here rather than to zero, which is what a timeline editor
   * does: the playhead is a place you are working, and losing it every time
   * the transport stops makes hearing one join twice a chore. Stopping when
   * already parked there goes to the beginning instead.
   */
  const playStartSecs = ref(0);
  /**
   * Set to the position an audition was asked to start from, and cleared by
   * the first engine report that has arrived there.
   *
   * The engine is polled five times a second, so a snapshot emitted just
   * before a seek or a rebuild can land just after it — and if it were
   * believed, the playhead would jump back to where it used to be and then
   * forward again. That flicker is what makes the playhead look like it will
   * not stay where it is put.
   */
  const expectedPosition = ref<number | null>(null);
  const previewing = ref(false);
  const previewPaused = ref(false);
  /**
   * The playback speed each block was last known to have, so a change to one
   * can be told from the first time it is seen. Session-only: it is derived
   * from the mixer cascade, not stored in the arrangement.
   */
  const blockSpeeds = ref<Record<string, number>>({});

  /**
   * Snapping to other blocks' edges, the playhead, and — with `gridSnapping`
   * on — the ruler's own divisions. Alt overrides it for one drag, and it
   * governs where the blade cuts as well as where a block lands.
   */
  const snapping = ref(true);
  /**
   * Add the ruler's marks to what snapping offers.
   *
   * Off by default: edge-to-edge is what butts two songs together, and a grid
   * that is always on gets in the way of that. On, it is how a block is placed
   * on an exact second rather than near one.
   */
  const gridSnapping = ref(false);
  /**
   * Scroll the timeline to keep a moving playhead on screen.
   *
   * Off by default: the timeline moving under the pointer while an edit is
   * being lined up is worse than losing sight of the playhead, which the
   * transport can always be asked for again.
   */
  const followPlayhead = ref(false);

  /** One waveform per playlist entry, fetched lazily and kept for the session. */
  const waveforms = ref<Record<number, Waveform>>({});
  const waveformsLoading = ref<Set<number>>(new Set());
  /** Waveforms for imported files, keyed by the asset's stored name. */
  const assetWaveforms = ref<Record<string, Waveform>>({});
  const assetWaveformsLoading = ref<Set<string>>(new Set());

  const undoStack = ref<MasterMix[]>([]);
  const redoStack = ref<MasterMix[]>([]);
  let saveTimer: number | undefined;
  let previewReloadTimer: number | undefined;
  let sessionToken: string | null = null;
  let lifecycle = 0;

  const duration = computed(() => mixDuration(mix.value));
  const canUndo = computed(() => undoStack.value.length > 0);
  const canRedo = computed(() => redoStack.value.length > 0);
  const selected = computed(() => new Set(selection.value));

  /** Master mixes are JSON documents. Serialising snapshots strips Vue proxies
   * at every depth while retaining forward-compatible fields verbatim. */
  function snapshotMix(value: MasterMix): MasterMix {
    return JSON.parse(JSON.stringify(value));
  }

  async function openFor(id: string) {
    const request = ++lifecycle;
    open.value = true;
    loading.value = true;
    error.value = null;
    playlistId.value = id;
    selection.value = [];
    undoStack.value = [];
    redoStack.value = [];
    waveforms.value = {};
    assetWaveforms.value = {};
    playhead.value = 0;
    playStartSecs.value = 0;
    expectedPosition.value = null;
    blockSpeeds.value = {};
    previewing.value = false;
    previewPaused.value = false;
    try {
      const token = await api.beginMasterMixSession();
      if (request !== lifecycle) {
        // A newer open shares the backend's existing capture and owns it. A
        // close leaves no owner, so it must end a begin that completed late.
        if (!open.value) await api.endMasterMixSession(token);
        return;
      }
      sessionToken = token;
      const loaded = await api.masterMix(id);
      if (request === lifecycle && sessionToken === token) apply(loaded);
    } catch (e) {
      if (request === lifecycle) error.value = String(e);
    } finally {
      if (request === lifecycle) loading.value = false;
    }
  }

  function apply(view: {
    mix: MasterMix;
    entries: MixEntry[];
    playlistName: string;
  }) {
    mix.value = view.mix;
    entries.value = view.entries;
    playlistName.value = view.playlistName;
  }

  async function close() {
    ++lifecycle;
    open.value = false;
    // A pending edit must reach disk before the modal goes away, or the last
    // thing the user did is the one thing that is lost.
    await flush();
    window.clearTimeout(previewReloadTimer);
    previewReloadTimer = undefined;
    const token = sessionToken;
    sessionToken = null;
    previewing.value = false;
    previewPaused.value = false;
    if (token) {
      try {
        await api.endMasterMixSession(token);
      } catch (e) {
        error.value = String(e);
      }
    }
    loading.value = false;
    selection.value = [];
  }

  /**
   * Record an edit: the previous state goes on the undo stack, the new one
   * becomes current, and a save is scheduled.
   *
   * Dragging calls this once per gesture, not once per frame — see the
   * pointer handlers in the modal, which mutate a working copy and commit on
   * release.
   */
  function commit(next: MasterMix) {
    const before = soundSignature(mix.value);
    undoStack.value = [
      ...undoStack.value.slice(-(UNDO_DEPTH - 1)),
      snapshotMix(mix.value),
    ];
    redoStack.value = [];
    mix.value = next;
    scheduleSave();
    // Moving a block should be audible without having to nudge the transport
    // to notice. Renaming or recolouring a lane should not cost a re-seek.
    if (soundSignature(next) !== before) schedulePreviewReload();
  }

  function undo() {
    const previous = undoStack.value.pop();
    if (!previous) return;
    const before = soundSignature(mix.value);
    redoStack.value = [...redoStack.value, snapshotMix(mix.value)];
    mix.value = previous;
    pruneSelection();
    scheduleSave();
    if (soundSignature(previous) !== before) schedulePreviewReload();
  }

  function redo() {
    const next = redoStack.value.pop();
    if (!next) return;
    const before = soundSignature(mix.value);
    undoStack.value = [...undoStack.value, snapshotMix(mix.value)];
    mix.value = next;
    pruneSelection();
    scheduleSave();
    if (soundSignature(next) !== before) schedulePreviewReload();
  }

  /** Undo can remove blocks that were selected; a stale id would break the
   *  next keyboard action. */
  function pruneSelection() {
    const alive = new Set(
      mix.value.lanes.flatMap((lane) => lane.blocks.map((b) => b.id)),
    );
    selection.value = selection.value.filter((id) => alive.has(id));
  }

  function scheduleSave() {
    window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => void save(), SAVE_DELAY_MS);
  }

  /** Write now rather than waiting out the debounce. */
  async function flush() {
    if (saveTimer === undefined) return;
    window.clearTimeout(saveTimer);
    saveTimer = undefined;
    await save();
  }

  async function save() {
    saveTimer = undefined;
    if (!playlistId.value) return;
    try {
      // The backend normalises and hands the result back, so the interface
      // shows what was actually stored rather than what it asked for.
      apply(await api.setMasterMix(playlistId.value, mix.value));
      pruneSelection();
      error.value = null;
    } catch (e) {
      error.value = String(e);
    }
  }

  async function setEnabled(enabled: boolean) {
    await flush();
    try {
      apply(await api.setMasterMixEnabled(playlistId.value, enabled));
    } catch (e) {
      error.value = String(e);
    }
  }

  async function reset() {
    window.clearTimeout(saveTimer);
    saveTimer = undefined;
    try {
      undoStack.value = [
        ...undoStack.value.slice(-(UNDO_DEPTH - 1)),
        snapshotMix(mix.value),
      ];
      redoStack.value = [];
      apply(await api.resetMasterMix(playlistId.value));
      selection.value = [];
    } catch (e) {
      error.value = String(e);
    }
  }

  /** Audition from `fromSecs`, including edits not yet written to disk. */
  async function play(fromSecs = playhead.value) {
    const token = sessionToken;
    if (!playlistId.value || !token) return;
    const from = Math.max(0, fromSecs);
    try {
      await api.playMasterMix(playlistId.value, mix.value, from, token);
      if (sessionToken !== token) return;
      previewing.value = true;
      previewPaused.value = false;
      playStartSecs.value = from;
      playhead.value = from;
      expectedPosition.value = from;
      error.value = null;
    } catch (e) {
      if (sessionToken !== token) return;
      previewing.value = false;
      error.value = String(e);
    }
  }

  async function pause() {
    const token = sessionToken;
    if (!previewing.value || previewPaused.value || !token) return;
    const paused = !(await api.setMasterMixPlaying(false, token));
    if (sessionToken === token && paused) previewPaused.value = true;
  }

  async function resume() {
    const token = sessionToken;
    if (!previewing.value || !previewPaused.value || !token) return;
    const resumed = await api.setMasterMixPlaying(true, token);
    if (sessionToken === token && resumed) previewPaused.value = false;
  }

  /** Rebuild the loaded plan so timeline and mixer edits become audible. */
  async function reloadPreview() {
    if (!previewing.value) return;
    const token = sessionToken;
    const absolutePosition = playhead.value;
    const wasPaused = previewPaused.value;
    const resumeFrom = playStartSecs.value;
    await play(absolutePosition);
    if (sessionToken !== token) return;
    // Polling can briefly report the replacement decoder at zero while it is
    // loading. Reloading is an implementation detail, not a playhead move, so
    // neither the playhead nor where Stop will return to may move with it.
    playhead.value = absolutePosition;
    playStartSecs.value = resumeFrom;
    if (wasPaused && previewing.value) await pause();
  }

  function schedulePreviewReload() {
    if (!previewing.value) return;
    window.clearTimeout(previewReloadTimer);
    previewReloadTimer = window.setTimeout(() => {
      previewReloadTimer = undefined;
      void reloadPreview();
    }, 150);
  }

  async function stop() {
    window.clearTimeout(previewReloadTimer);
    previewReloadTimer = undefined;
    const token = sessionToken;
    if (!previewing.value || !token) return;
    previewing.value = false;
    previewPaused.value = false;
    expectedPosition.value = null;
    try {
      await api.stopMasterMix(token);
    } catch {
      // Nothing useful to do: the engine is already not playing this mix.
    }
  }

  /**
   * Take a position reported by the engine, unless it is a stale one from
   * before the seek or rebuild that is still settling.
   *
   * Reports are only ever *late*, never early, so a position short of what was
   * asked for is the old plan still talking. Once one arrives at or past the
   * target the guard is dropped and the playhead follows the engine again.
   */
  function applyEnginePosition(positionSecs: number) {
    if (!previewing.value) return;
    const expected = expectedPosition.value;
    if (expected !== null) {
      if (positionSecs + STALE_TOLERANCE_SECS < expected) return;
      expectedPosition.value = null;
    }
    playhead.value = positionSecs;
  }

  /** Move the playhead by hand, which abandons any position the engine owes. */
  function setPlayhead(secs: number) {
    expectedPosition.value = null;
    playhead.value = Math.max(0, secs);
  }

  /** Called when the engine reports the arrangement has run out. */
  function previewEnded() {
    previewing.value = false;
    previewPaused.value = false;
    expectedPosition.value = null;
  }

  async function loadWaveform(index: number) {
    if (waveforms.value[index] || waveformsLoading.value.has(index)) return;
    waveformsLoading.value.add(index);
    try {
      const waveform = await api.entryWaveform(playlistId.value, index);
      waveforms.value = { ...waveforms.value, [index]: waveform };
    } catch {
      // A song that cannot be decoded simply draws as an empty block.
    } finally {
      waveformsLoading.value.delete(index);
    }
  }

  async function loadAssetWaveform(file: string) {
    if (assetWaveforms.value[file] || assetWaveformsLoading.value.has(file))
      return;
    assetWaveformsLoading.value.add(file);
    try {
      const waveform = await api.assetWaveform(playlistId.value, file);
      assetWaveforms.value = { ...assetWaveforms.value, [file]: waveform };
    } catch {
      // Same as a missing song: the block still sits on the timeline.
    } finally {
      assetWaveformsLoading.value.delete(file);
    }
  }

  /** Remember a block's speed without acting on it — used when its mixer is
   *  first opened, so the next change has something to compare against. */
  function noteBlockSpeed(blockId: string, speed: number) {
    if (speed > 0) blockSpeeds.value = { ...blockSpeeds.value, [blockId]: speed };
  }

  /**
   * Write a block's mixer without an undo step. Knob moves would otherwise
   * bury the arrangement history in hundreds of snapshots.
   *
   * `speed` is the block's resolved varispeed after this edit. Changing it
   * resizes the region so the same audio stays under it, which travels with
   * the mixer write rather than being a separate undoable move — turning a
   * pitch knob is one action, however many things it changes.
   */
  function setBlockMixer(blockId: string, mixer: MixerSettings | null, speed?: number) {
    let next = patchBlockMixer(mix.value, blockId, mixer);
    if (speed !== undefined && speed > 0) {
      const previous = blockSpeeds.value[blockId];
      if (previous !== undefined) next = scaleBlockForSpeed(next, blockId, previous, speed);
      noteBlockSpeed(blockId, speed);
    }
    mix.value = next;
    scheduleSave();
    schedulePreviewReload();
  }

  function select(ids: string[]) {
    selection.value = ids;
  }

  function toggleSelected(id: string) {
    selection.value = selection.value.includes(id)
      ? selection.value.filter((other) => other !== id)
      : [...selection.value, id];
  }

  function zoom(factor: number) {
    pixelsPerSecond.value = Math.min(
      400,
      Math.max(0.5, pixelsPerSecond.value * factor),
    );
  }

  function zoomTracks(factor: number) {
    laneHeight.value = Math.min(180, Math.max(48, laneHeight.value * factor));
  }

  return {
    open,
    playlistId,
    playlistName,
    mix,
    entries,
    loading,
    error,
    tool,
    selection,
    selected,
    pixelsPerSecond,
    laneHeight,
    playhead,
    playStartSecs,
    previewing,
    previewPaused,
    snapping,
    gridSnapping,
    followPlayhead,
    waveforms,
    assetWaveforms,
    duration,
    canUndo,
    canRedo,
    openFor,
    close,
    commit,
    undo,
    redo,
    flush,
    save,
    setEnabled,
    reset,
    play,
    pause,
    resume,
    reloadPreview,
    stop,
    previewEnded,
    applyEnginePosition,
    setPlayhead,
    loadWaveform,
    loadAssetWaveform,
    setBlockMixer,
    noteBlockSpeed,
    blockSpeeds,
    select,
    toggleSelected,
    zoom,
    zoomTracks,
  };
});
