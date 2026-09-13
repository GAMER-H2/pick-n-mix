<script setup lang="ts">
/**
 * A stereo peak meter: two channels, a green-to-red gradient and a held peak.
 *
 * By default it is the Master Mixer's own output meter and draws the
 * post-limiter master bus. Marked `driven` it draws whatever its parent hands
 * it through `apply`, which is how the rack puts a small one between each pair
 * of effects — those readings all arrive in one frame, so the rack feeds every
 * meter in the row from a single reading rather than each meter asking
 * separately.
 *
 * The bars are written straight to the DOM rather than bound.
 *
 * A meter is the one thing on the screen that genuinely changes every frame,
 * and nothing else depends on what it reads. Routing it through reactive state
 * meant re-rendering the component — and, in the rack, its neighbours — sixty
 * times a second to move two boxes. Both moving parts are transforms now, so
 * the browser composites them instead of repainting a gradient under a
 * changing clip.
 */
import { onBeforeUnmount, onMounted, ref } from "vue";
import { FLOOR_DB, SILENCE, useMeterFeed } from "@/composables/useMeterFeed";
import type { OutputLevelFrame } from "@/lib/types";

const CHANNELS = [
  { index: 0, short: "L", label: "Left output level" },
  { index: 1, short: "R", label: "Right output level" },
] as const;
const SCALE_DB = [0, -6, -12, -24, -48] as const;

const props = withDefaults(
  defineProps<{
    /** Fed by its parent through `apply`, rather than reading the master bus. */
    driven?: boolean;
    /** Small enough to sit between two effects: no scale, narrower bars. */
    compact?: boolean;
    /** What this meter is measuring, for its tooltip. */
    label?: string;
    /**
     * Where in the signal this reading was taken, added to each channel's own
     * label. Several meters can be on screen at once in the rack, and "Left
     * output level" five times over names none of them.
     */
    scope?: string;
  }>(),
  { driven: false, compact: false, label: "Post-limiter master bus level", scope: "" },
);

/**
 * The bottom of the scale, which is the only part of a reading the marks down
 * the side depend on. It does not change in practice, so binding it costs a
 * render that never happens.
 */
const floorDb = ref(FLOOR_DB);

const tracks: (HTMLElement | null)[] = [null, null];
const fills: (HTMLElement | null)[] = [null, null];
const masks: (HTMLElement | null)[] = [null, null];
const rails: (HTMLElement | null)[] = [null, null];
const peaks: (HTMLElement | null)[] = [null, null];

function bind(into: (HTMLElement | null)[], index: number) {
  return (element: unknown) => {
    into[index] = (element as HTMLElement | null) ?? null;
  };
}

/** Built once: a fresh callback each render would rebind every element. */
const binders = CHANNELS.map((channel) => ({
  track: bind(tracks, channel.index),
  fill: bind(fills, channel.index),
  mask: bind(masks, channel.index),
  rail: bind(rails, channel.index),
  peak: bind(peaks, channel.index),
}));

function clampedDb(value: number, floor: number): number {
  return Math.min(0, Math.max(floor, value));
}

function percent(value: number, floor = floorDb.value): number {
  if (!Number.isFinite(value) || !Number.isFinite(floor) || floor >= 0) return 0;
  return ((clampedDb(value, floor) - floor) / -floor) * 100;
}

function valueText(frame: OutputLevelFrame, channel: number): string {
  const current = frame.levelsDb[channel];
  const peak = frame.peaksDb[channel];
  return `${current.toFixed(1)} dBFS, peak ${peak.toFixed(1)} dBFS`;
}

let painted: OutputLevelFrame | null = null;

function unchanged(frame: OutputLevelFrame): boolean {
  return (
    painted !== null &&
    painted.floorDb === frame.floorDb &&
    painted.levelsDb[0] === frame.levelsDb[0] &&
    painted.levelsDb[1] === frame.levelsDb[1] &&
    painted.peaksDb[0] === frame.peaksDb[0] &&
    painted.peaksDb[1] === frame.peaksDb[1]
  );
}

/** Draw a reading. Safe to call every frame: a still meter writes nothing. */
function apply(frame: OutputLevelFrame) {
  if (unchanged(frame)) return;
  painted = frame;
  if (frame.floorDb !== floorDb.value) floorDb.value = frame.floorDb;

  for (const { index } of CHANNELS) {
    const floor = frame.floorDb;
    const level = percent(frame.levelsDb[index], floor);
    const peak = percent(frame.peaksDb[index], floor);
    const over = frame.peaksDb[index] >= 0;

    // The mask covers the gradient from the top down to the level, so the
    // colour at a given height stays put as the bar moves.
    masks[index]?.style.setProperty("transform", `scaleY(${(1 - level / 100).toFixed(4)})`);
    // The rail is the full height of the track, so a percentage of it is a
    // percentage of the scale.
    rails[index]?.style.setProperty("transform", `translateY(${(-peak).toFixed(2)}%)`);
    fills[index]?.classList.toggle("is-over", over);
    peaks[index]?.classList.toggle("is-over", over);

    const track = tracks[index];
    if (track) {
      track.setAttribute("aria-valuenow", `${clampedDb(frame.levelsDb[index], floor)}`);
      track.setAttribute("aria-valuetext", valueText(frame, index));
    }
  }
}

defineExpose({ apply });

/** Driven meters are fed by their parent, so they read nothing themselves. */
const feed = props.driven ? null : useMeterFeed((reading) => apply(reading.output));

onBeforeUnmount(() => {
  feed?.setActive(false);
  painted = null;
});

onMounted(() => {
  // Until the engine says otherwise a meter reads its floor, which is what the
  // untransformed mask already shows; this is what labels it as such.
  apply(SILENCE);
  feed?.setActive(true);
});
</script>

<template>
  <aside
    class="level-meter"
    :class="{ 'is-compact': compact }"
    :aria-label="label"
    :title="label"
  >
    <div v-if="!compact" class="level-meter__scale" aria-hidden="true">
      <span
        v-for="mark in SCALE_DB"
        :key="mark"
        :style="{ bottom: `${percent(mark)}%` }"
      >{{ mark }}</span>
    </div>

    <div
      v-for="channel in CHANNELS"
      :key="channel.short"
      class="level-meter__channel"
    >
      <!-- `aria-valuenow` and `aria-valuetext` are written by `apply` rather
           than bound, because that is where the reading is. -->
      <div
        :ref="binders[channel.index].track"
        class="level-meter__track"
        role="meter"
        :aria-label="scope ? `${channel.label} ${scope}` : channel.label"
        :aria-valuemin="floorDb"
        aria-valuemax="0"
      >
        <div
          :ref="binders[channel.index].fill"
          class="level-meter__fill"
          aria-hidden="true"
        />
        <div
          :ref="binders[channel.index].mask"
          class="level-meter__mask"
          aria-hidden="true"
        />
        <div
          :ref="binders[channel.index].rail"
          class="level-meter__peak-rail"
          aria-hidden="true"
        >
          <span :ref="binders[channel.index].peak" class="level-meter__peak" />
        </div>
      </div>
      <span v-if="!compact" class="level-meter__label" aria-hidden="true">{{ channel.short }}</span>
    </div>
  </aside>
</template>

<style scoped>
.level-meter {
  flex: 0 0 72px;
  display: grid;
  grid-template-columns: 22px repeat(2, 14px);
  justify-content: center;
  gap: 5px;
  min-height: 0;
  padding: 12px 7px 8px;
  border-left: 0.5px solid var(--separator);
  background: var(--bg-sidebar);
  color: var(--text-tertiary);
}

.level-meter__scale {
  position: relative;
  min-height: 0;
  margin-bottom: 18px;
  font-size: 8.5px;
  font-variant-numeric: tabular-nums;
}

.level-meter__scale span {
  position: absolute;
  right: 1px;
  transform: translateY(50%);
}

.level-meter__channel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  gap: 5px;
}

.level-meter__track {
  position: relative;
  flex: 1;
  min-height: 80px;
  overflow: hidden;
  border: 0.5px solid var(--separator-strong);
  border-radius: 3px;
  background: var(--control-track);
}

/* The gradient stays put at full height so a given colour always means the
   same level; what moves is the mask over it. */
.level-meter__fill {
  position: absolute;
  inset: 0;
  background: linear-gradient(
    to top,
    #35a853 0%,
    #64b944 65%,
    #e0b62f 80%,
    #e56a32 91%,
    #d7373f 100%
  );
}

.level-meter__fill.is-over {
  filter: saturate(1.2) brightness(1.08);
}

/* Veils the gradient from the top down to the level, rather than hiding it:
   the part of the scale the signal is not reaching keeps a dimmed version of
   the colour it will turn, which makes how much headroom is left something you
   can see rather than something you have to know.

   A scale rather than a clip because the browser can composite one without
   repainting what is underneath, which is what lets these run at the display's
   rate. */
.level-meter__mask {
  position: absolute;
  inset: 0;
  transform: scaleY(1);
  transform-origin: top;
  background: var(--meter-unlit);
  transition: transform 45ms linear;
  will-change: transform;
}

/* Full height, so shifting it by a percentage shifts the marker by that much
   of the scale — and, again, as a transform rather than a new `bottom`. */
.level-meter__peak-rail {
  position: absolute;
  inset: 0;
  z-index: 1;
  transition: transform 45ms linear;
  will-change: transform;
  pointer-events: none;
}

.level-meter__peak {
  position: absolute;
  bottom: 0;
  left: 1px;
  right: 1px;
  height: 2px;
  transform: translateY(1px);
  border-radius: 1px;
  background: var(--text);
  box-shadow: 0 0 0 0.5px var(--bg);
}

.level-meter__peak.is-over {
  height: 3px;
  background: #d7373f;
}

.level-meter__label {
  height: 13px;
  font-size: 10px;
  font-weight: 650;
  line-height: 13px;
  text-align: center;
  color: var(--text-secondary);
}

/* The rack's meters: a pair of thin bars with no scale and no lettering, sized
   to sit in the gap between two devices without becoming one. */
.level-meter.is-compact {
  flex: none;
  grid-template-columns: repeat(2, 7px);
  gap: 3px;
  padding: 0;
  border-left: 0;
  background: none;
}

.level-meter.is-compact .level-meter__track {
  min-height: 0;
}

@media (max-width: 760px) {
  .level-meter {
    flex-basis: 48px;
    grid-template-columns: repeat(2, 12px);
    gap: 4px;
    padding-inline: 6px;
  }

  .level-meter__scale {
    display: none;
  }
}
</style>
