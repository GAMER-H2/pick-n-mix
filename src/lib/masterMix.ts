/**
 * Timeline arithmetic for the master mixer.
 *
 * All of it is pure: a function takes a mix and gives back a new one, which is
 * what makes undo a stack of snapshots rather than a stack of inverse
 * operations, and what makes every rule here testable without a DOM.
 *
 * The limits mirror `MasterMix::normalise` in `src-tauri/src/master_mix.rs`.
 * The backend is still the authority — it re-checks everything it is sent —
 * but matching here means the interface never shows the user a position that
 * is about to be silently corrected underneath them.
 */

import type { MasterMix, MixBlock, MixEntry, MixLane } from "./types";

/** Shortest a block may be, matching `MIN_BLOCK_SECS` in Rust. */
export const MIN_BLOCK_SECS = 0.02;
export const MIN_GAIN_DB = -60;
export const MAX_GAIN_DB = 12;
/** How close, in pixels, a drag has to come before it snaps. */
export const SNAP_PIXELS = 7;

export function newId(prefix: string): string {
  const random =
    typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID().replace(/-/g, "")
      : Math.random().toString(16).slice(2).padEnd(12, "0");
  return `${prefix}_${random.slice(0, 12)}`;
}

export function cloneMix(mix: MasterMix): MasterMix {
  return {
    ...mix,
    lanes: mix.lanes.map((lane) => ({
      ...lane,
      blocks: lane.blocks.map((block) => ({
        ...block,
        source: { ...block.source },
        automation: block.automation.map((point) => ({ ...point })),
      })),
    })),
  };
}

export function blockEnd(block: MixBlock): number {
  return block.startSecs + block.durationSecs;
}

export function mixDuration(mix: MasterMix): number {
  let end = 0;
  for (const lane of mix.lanes) {
    for (const block of lane.blocks) end = Math.max(end, blockEnd(block));
  }
  return end;
}

export interface Located {
  laneIndex: number;
  blockIndex: number;
  lane: MixLane;
  block: MixBlock;
}

export function locate(mix: MasterMix, blockId: string): Located | null {
  for (let laneIndex = 0; laneIndex < mix.lanes.length; laneIndex += 1) {
    const lane = mix.lanes[laneIndex];
    const blockIndex = lane.blocks.findIndex((b) => b.id === blockId);
    if (blockIndex >= 0) {
      return { laneIndex, blockIndex, lane, block: lane.blocks[blockIndex] };
    }
  }
  return null;
}

/**
 * The full length of whatever a block plays, which bounds how far its right
 * edge can be dragged. Unknown for an imported file until it has been
 * analysed, in which case there is nothing to bound it with.
 */
export function sourceDuration(block: MixBlock, entries: MixEntry[]): number {
  const source = block.source;
  if (source.kind !== "entry") return Infinity;
  return entries.find((e) => e.index === source.index)?.durationSecs ?? Infinity;
}

/** Keep blocks in time order, which is what the renderer and Rust both expect. */
function ordered(blocks: MixBlock[]): MixBlock[] {
  return [...blocks].sort((a, b) => a.startSecs - b.startSecs);
}

function withLane(mix: MasterMix, laneIndex: number, blocks: MixBlock[]): MasterMix {
  const lanes = mix.lanes.map((lane, i) =>
    i === laneIndex ? { ...lane, blocks: ordered(blocks) } : lane,
  );
  return { ...mix, lanes };
}

/**
 * Move a block to `startSecs`, optionally into another lane.
 *
 * Blocks are free to overlap — that overlap is exactly how a hand-made
 * crossfade is built — so nothing is pushed out of the way. The only rule is
 * that the timeline starts at zero.
 */
export function moveBlock(
  mix: MasterMix,
  blockId: string,
  startSecs: number,
  toLaneIndex?: number,
): MasterMix {
  const found = locate(mix, blockId);
  if (!found) return mix;

  const moved: MixBlock = { ...found.block, startSecs: Math.max(0, startSecs) };
  const target = toLaneIndex ?? found.laneIndex;
  if (target === found.laneIndex) {
    const blocks = [...found.lane.blocks];
    blocks[found.blockIndex] = moved;
    return withLane(mix, found.laneIndex, blocks);
  }
  if (target < 0 || target >= mix.lanes.length) return mix;

  const lanes = mix.lanes.map((lane, i) => {
    if (i === found.laneIndex) {
      return { ...lane, blocks: lane.blocks.filter((b) => b.id !== blockId) };
    }
    if (i === target) return { ...lane, blocks: ordered([...lane.blocks, moved]) };
    return lane;
  });
  return { ...mix, lanes };
}

/** Move several blocks by the same amount, as a single undoable step. */
export function moveBlocks(
  mix: MasterMix,
  blockIds: string[],
  deltaSecs: number,
  laneDelta = 0,
): MasterMix {
  // The whole selection shifts together or not at all, so the smallest start
  // in it is what decides how far left the group can go.
  let earliest = Infinity;
  let topLane = Infinity;
  let bottomLane = -Infinity;
  for (const id of blockIds) {
    const found = locate(mix, id);
    if (!found) continue;
    earliest = Math.min(earliest, found.block.startSecs);
    topLane = Math.min(topLane, found.laneIndex);
    bottomLane = Math.max(bottomLane, found.laneIndex);
  }
  if (!Number.isFinite(earliest)) return mix;

  const time = Math.max(deltaSecs, -earliest);
  const lanes = Math.min(
    Math.max(laneDelta, -topLane),
    mix.lanes.length - 1 - bottomLane,
  );

  let next = mix;
  for (const id of blockIds) {
    const found = locate(next, id);
    if (!found) continue;
    next = moveBlock(next, id, found.block.startSecs + time, found.laneIndex + lanes);
  }
  return next;
}

/**
 * Drag one edge of a block.
 *
 * Trimming the left edge moves the start *and* the offset into the source
 * together, so the audio under the cursor stays put rather than sliding — the
 * behaviour every timeline editor has, and the reason `offsetSecs` exists.
 */
export function trimBlock(
  mix: MasterMix,
  blockId: string,
  edge: "start" | "end",
  toSecs: number,
  naturalDuration = Infinity,
): MasterMix {
  const found = locate(mix, blockId);
  if (!found) return mix;
  const block = found.block;

  let next: MixBlock;
  if (edge === "start") {
    // Cannot pull earlier than the source's own beginning, nor later than a
    // hair before the right edge.
    const earliest = block.startSecs - block.offsetSecs;
    const latest = blockEnd(block) - MIN_BLOCK_SECS;
    const start = Math.min(Math.max(toSecs, Math.max(0, earliest)), latest);
    const shift = start - block.startSecs;
    next = {
      ...block,
      startSecs: start,
      offsetSecs: block.offsetSecs + shift,
      durationSecs: block.durationSecs - shift,
    };
  } else {
    const longest = Number.isFinite(naturalDuration)
      ? naturalDuration - block.offsetSecs
      : Infinity;
    const duration = Math.min(
      Math.max(toSecs - block.startSecs, MIN_BLOCK_SECS),
      longest,
    );
    next = { ...block, durationSecs: duration };
  }

  // Fades cannot survive being longer than what is left of the block.
  next.fadeInSecs = Math.min(next.fadeInSecs, next.durationSecs);
  next.fadeOutSecs = Math.min(next.fadeOutSecs, next.durationSecs - next.fadeInSecs);

  const blocks = [...found.lane.blocks];
  blocks[found.blockIndex] = next;
  return withLane(mix, found.laneIndex, blocks);
}

/**
 * Cut a block in two at an absolute timeline position — the blade tool.
 *
 * Returns the mix unchanged if the cut does not fall strictly inside the
 * block, or if either half would be too short to hear.
 */
export function splitBlock(mix: MasterMix, blockId: string, atSecs: number): MasterMix {
  const found = locate(mix, blockId);
  if (!found) return mix;
  const block = found.block;

  const into = atSecs - block.startSecs;
  if (into < MIN_BLOCK_SECS || block.durationSecs - into < MIN_BLOCK_SECS) return mix;

  const left: MixBlock = {
    ...block,
    durationSecs: into,
    // The cut is a hard edge: only the fade that still fits stays.
    fadeInSecs: Math.min(block.fadeInSecs, into),
    fadeOutSecs: 0,
    automation: block.automation
      .filter((point) => point.atSecs <= into)
      .map((point) => ({ ...point })),
  };
  const right: MixBlock = {
    ...block,
    id: newId("blk"),
    startSecs: atSecs,
    offsetSecs: block.offsetSecs + into,
    durationSecs: block.durationSecs - into,
    fadeInSecs: 0,
    fadeOutSecs: Math.min(block.fadeOutSecs, block.durationSecs - into),
    automation: block.automation
      .filter((point) => point.atSecs > into)
      .map((point) => ({ ...point, atSecs: point.atSecs - into })),
  };

  const blocks = [...found.lane.blocks];
  blocks.splice(found.blockIndex, 1, left, right);
  return withLane(mix, found.laneIndex, blocks);
}

export function deleteBlocks(mix: MasterMix, blockIds: string[]): MasterMix {
  const doomed = new Set(blockIds);
  return {
    ...mix,
    lanes: mix.lanes.map((lane) => ({
      ...lane,
      blocks: lane.blocks.filter((block) => !doomed.has(block.id)),
    })),
  };
}

export function addLane(mix: MasterMix, name: string): MasterMix {
  return {
    ...mix,
    lanes: [
      ...mix.lanes,
      { id: newId("lane"), name, muted: false, soloed: false, gainDb: 0, blocks: [] },
    ],
  };
}

/** Remove a lane and everything on it. */
export function removeLane(mix: MasterMix, laneIndex: number): MasterMix {
  if (laneIndex < 0 || laneIndex >= mix.lanes.length) return mix;
  return { ...mix, lanes: mix.lanes.filter((_, i) => i !== laneIndex) };
}

export function updateLane(
  mix: MasterMix,
  laneIndex: number,
  patch: Partial<MixLane>,
): MasterMix {
  if (laneIndex < 0 || laneIndex >= mix.lanes.length) return mix;
  return {
    ...mix,
    lanes: mix.lanes.map((lane, i) => (i === laneIndex ? { ...lane, ...patch } : lane)),
  };
}

/**
 * Times worth snapping a drag to: the start and end of every block that is not
 * itself being dragged, plus the origin.
 *
 * Snapping to other blocks' edges rather than to a grid is what makes butting
 * two songs together exact, which is the single most common thing anyone does
 * on this timeline.
 */
export function snapCandidates(mix: MasterMix, exclude: Set<string>): number[] {
  const times = [0];
  for (const lane of mix.lanes) {
    for (const block of lane.blocks) {
      if (exclude.has(block.id)) continue;
      times.push(block.startSecs, blockEnd(block));
    }
  }
  return times;
}

/** The closest candidate to `time`, and how far away it is. */
function nearest(time: number, candidates: number[]): { time: number; distance: number } {
  let best = { time, distance: Infinity };
  for (const candidate of candidates) {
    const distance = Math.abs(candidate - time);
    if (distance < best.distance) best = { time: candidate, distance };
  }
  return best;
}

/** The nearest candidate within `tolerance`, or `time` if none is close. */
export function snapTime(time: number, candidates: number[], tolerance: number): number {
  const found = nearest(time, candidates);
  return found.distance <= tolerance ? found.time : time;
}

/**
 * A drag moves the block's start, but its *end* should snap too — otherwise
 * butting the right-hand edge of one song against the next is impossible.
 * Both edges are offered and whichever is closer wins.
 */
export function snapDrag(
  startSecs: number,
  durationSecs: number,
  candidates: number[],
  tolerance: number,
): number {
  // Compared by how far each edge is from a candidate, not by how far the
  // block would move: an edge with nothing near it must never win simply
  // because leaving the block alone is a smaller movement.
  const start = nearest(startSecs, candidates);
  const end = nearest(startSecs + durationSecs, candidates);
  if (start.distance <= end.distance && start.distance <= tolerance) return start.time;
  if (end.distance <= tolerance) return end.time - durationSecs;
  return startSecs;
}

/** "1:04.250" — the ruler and the position readout both want sub-second detail. */
export function timecode(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00.000";
  const total = Math.floor(seconds);
  const millis = Math.round((seconds - total) * 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  const tail = `${String(secs).padStart(2, "0")}.${String(millis).padStart(3, "0")}`;
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${tail}`
    : `${minutes}:${tail}`;
}

/**
 * Spacing between ruler marks at a given zoom, chosen from a fixed ladder so
 * the labels stay round numbers however far in or out the user goes.
 */
export function rulerStep(pixelsPerSecond: number): number {
  const ladder = [0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800];
  const wanted = 90 / pixelsPerSecond;
  return ladder.find((step) => step >= wanted) ?? ladder[ladder.length - 1];
}
