import { defineStore } from "pinia";
import { computed, ref } from "vue";
import * as api from "@/lib/api";
import { mixDuration, setBlockMixer as patchBlockMixer } from "@/lib/masterMix";
import type { MasterMix, MixEntry, MixerSettings, Waveform } from "@/lib/types";

/** Which of the three tools in the drawing is armed. */
export type Tool = "select" | "blade" | "automation";

const EMPTY: MasterMix = { enabled: false, revision: 0, lanes: [] };
/** How many edits back the user can go. Snapshots are small — a mix is a few
 *  hundred numbers — so this can be generous. */
const UNDO_DEPTH = 100;
/** Edits are batched into one write rather than one per drag frame. */
const SAVE_DELAY_MS = 600;

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
  const previewing = ref(false);
  const previewPaused = ref(false);

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
    undoStack.value = [
      ...undoStack.value.slice(-(UNDO_DEPTH - 1)),
      snapshotMix(mix.value),
    ];
    redoStack.value = [];
    mix.value = next;
    scheduleSave();
  }

  function undo() {
    const previous = undoStack.value.pop();
    if (!previous) return;
    redoStack.value = [...redoStack.value, snapshotMix(mix.value)];
    mix.value = previous;
    pruneSelection();
    scheduleSave();
  }

  function redo() {
    const next = redoStack.value.pop();
    if (!next) return;
    undoStack.value = [...undoStack.value, snapshotMix(mix.value)];
    mix.value = next;
    pruneSelection();
    scheduleSave();
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
    try {
      await api.playMasterMix(
        playlistId.value,
        mix.value,
        Math.max(0, fromSecs),
        token,
      );
      if (sessionToken !== token) return;
      previewing.value = true;
      previewPaused.value = false;
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
    await play(absolutePosition);
    if (sessionToken !== token) return;
    // Polling can briefly report the replacement decoder at zero while it is
    // loading. Reloading is an implementation detail, not a playhead move.
    playhead.value = absolutePosition;
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
    try {
      await api.stopMasterMix(token);
    } catch {
      // Nothing useful to do: the engine is already not playing this mix.
    }
  }

  /** Called when the engine reports the arrangement has run out. */
  function previewEnded() {
    previewing.value = false;
    previewPaused.value = false;
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

  /**
   * Write a block's mixer without an undo step. Knob moves would otherwise
   * bury the arrangement history in hundreds of snapshots.
   */
  function setBlockMixer(blockId: string, mixer: MixerSettings | null) {
    mix.value = patchBlockMixer(mix.value, blockId, mixer);
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
    previewing,
    previewPaused,
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
    loadWaveform,
    loadAssetWaveform,
    setBlockMixer,
    select,
    toggleSelected,
    zoom,
    zoomTracks,
  };
});
