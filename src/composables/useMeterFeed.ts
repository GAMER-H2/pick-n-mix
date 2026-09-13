/**
 * One reader for every meter on screen.
 *
 * Each meter used to fetch its own reading and then wait for the next
 * animation frame *after* the reply landed. That put a whole IPC round trip
 * inside every frame — two of them, with the rack open — so the bars updated
 * at whatever was left of the refresh rate rather than at it. The audio side
 * was never the problem: a reading is an atomic load.
 *
 * So the fetching and the drawing are separated. One reader keeps a single
 * request in flight at all times and writes whatever comes back into a frame
 * every painter can see; a single animation-frame loop then draws the latest
 * reading, whether that is a new one or the one before it. Meters move at the
 * display's rate, and how long the bridge takes stops deciding it.
 *
 * Readings are handed to painters as plain objects rather than through
 * reactive state on purpose: a meter is several hundred DOM writes a second
 * and nothing else on the screen depends on it, so it is drawn directly.
 */
import { onBeforeUnmount, onMounted } from "vue";
import * as api from "@/lib/api";
import type { ChainLevelFrame, OutputLevelFrame } from "@/lib/types";

export const FLOOR_DB = -60;

export const SILENCE: OutputLevelFrame = {
  levelsDb: [FLOOR_DB, FLOOR_DB],
  peaksDb: [FLOOR_DB, FLOOR_DB],
  floorDb: FLOOR_DB,
};

/** How often the wait below looks to see whether a frame has been drawn. */
const IDLE_STEP_MS = 2;

export interface MeterReading {
  output: OutputLevelFrame;
  /** `null` until a rack has asked for a block and one has sounded. */
  chain: ChainLevelFrame | null;
}

const latest: MeterReading = { output: SILENCE, chain: null };
/** Everything currently drawing, called once per animation frame. */
const painters = new Set<(reading: MeterReading) => void>();
/** Painters that want the chain taps as well, so a rack that is closed costs
 *  nothing to have been open. */
const chainReaders = new Set<unknown>();
let reading = false;
let frame: number | null = null;

async function pump() {
  while (painters.size > 0) {
    const at = drawn;
    try {
      const next = await api.meterFrames(chainReaders.size > 0);
      latest.output = next.output;
      latest.chain = next.chain;
    } catch {
      // A missed reading is harmless: the last one stands and we ask again.
    }
    // One reading per frame drawn. A reply that took longer than a frame
    // passes straight through; a faster one waits, because a second reading
    // inside one frame is a reading nothing will ever show.
    while (painters.size > 0 && drawn === at) {
      await new Promise((resolve) => setTimeout(resolve, IDLE_STEP_MS));
    }
  }
  reading = false;
  latest.output = SILENCE;
  latest.chain = null;
}

/** Frames drawn, which is what paces the reading above. */
let drawn = 0;

function onFrame() {
  frame = requestAnimationFrame(onFrame);
  drawn += 1;
  for (const paint of painters) paint(latest);
}

function start(paint: (reading: MeterReading) => void) {
  painters.add(paint);
  if (!reading) {
    reading = true;
    void pump();
  }
  if (frame === null) frame = requestAnimationFrame(onFrame);
}

/**
 * The engine does not maintain the master bus meter unless something is
 * watching it, and more than one thing can be: the rack closing must not take
 * the level meter beside it down with it.
 */
let outputWatchers = 0;

function watchOutput(delta: number) {
  const before = outputWatchers;
  outputWatchers = Math.max(0, outputWatchers + delta);
  if (before === 0 && outputWatchers > 0) {
    // The meters stay at their floor if the audio engine is unavailable.
    void api.setOutputMeterEnabled(true).catch(() => {});
  } else if (before > 0 && outputWatchers === 0) {
    void api.setOutputMeterEnabled(false).catch(() => {});
  }
}

function stop(paint: (reading: MeterReading) => void) {
  painters.delete(paint);
  if (painters.size === 0 && frame !== null) {
    cancelAnimationFrame(frame);
    frame = null;
  }
}

/**
 * Draw from the engine's meters for as long as this component is mounted.
 *
 * `paint` is called once per animation frame with the newest reading there is.
 * It runs outside Vue's reactivity: write to the DOM from it.
 */
export function useMeterFeed(
  paint: (reading: MeterReading) => void,
  options: { chain?: boolean } = {},
) {
  const token = {};
  let running = false;

  /**
   * Whether this meter is drawing at all.
   *
   * The rack only meters while the mix is sounding — stopped, the engine's own
   * meters are at the floor and there is nothing to ask it for.
   */
  function setActive(active: boolean) {
    if (active === running) return;
    running = active;
    if (active) {
      if (options.chain) chainReaders.add(token);
      start(paint);
    } else {
      chainReaders.delete(token);
      stop(paint);
    }
  }

  onMounted(() => watchOutput(1));

  onBeforeUnmount(() => {
    setActive(false);
    watchOutput(-1);
  });

  return { setActive };
}
