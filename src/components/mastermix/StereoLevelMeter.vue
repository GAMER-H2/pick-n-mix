<script setup lang="ts">
/**
 * A stereo peak meter: two channels, a green-to-red gradient and a held peak.
 *
 * By default it is the Master Mixer's own output meter and polls the engine
 * for the post-limiter master bus. Given a `reading` it draws that instead and
 * polls nothing, which is how the rack puts a small one between each pair of
 * effects — those readings all arrive in a single frame, so one poll feeds
 * every meter in the row rather than each meter asking separately.
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import * as api from "@/lib/api";
import type { OutputLevelFrame } from "@/lib/types";

const DEFAULT_FLOOR_DB = -60;
const SILENCE: OutputLevelFrame = {
  levelsDb: [DEFAULT_FLOOR_DB, DEFAULT_FLOOR_DB],
  peaksDb: [DEFAULT_FLOOR_DB, DEFAULT_FLOOR_DB],
  floorDb: DEFAULT_FLOOR_DB,
};
const CHANNELS = [
  { index: 0, short: "L", label: "Left output level" },
  { index: 1, short: "R", label: "Right output level" },
] as const;
const SCALE_DB = [0, -6, -12, -24, -48] as const;

const props = withDefaults(
  defineProps<{
    /** A reading from elsewhere. Given one, this meter does not poll. */
    reading?: OutputLevelFrame | null;
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
  { reading: null, compact: false, label: "Post-limiter master bus level", scope: "" },
);

const polled = ref<OutputLevelFrame>(SILENCE);
/** Named apart from the `reading` prop: a setup binding and a prop of the same
 *  name collide in the template. */
const level = computed<OutputLevelFrame>(() => props.reading ?? polled.value);
const selfDriven = computed(() => props.reading === null);
let animationFrame: number | null = null;
let stopped = false;

function clampedDb(value: number): number {
  return Math.min(0, Math.max(level.value.floorDb, value));
}

function percent(value: number): number {
  const floor = level.value.floorDb;
  if (!Number.isFinite(value) || !Number.isFinite(floor) || floor >= 0) return 0;
  return ((clampedDb(value) - floor) / -floor) * 100;
}

function levelClip(channel: 0 | 1): string {
  return `inset(${(100 - percent(level.value.levelsDb[channel])).toFixed(2)}% 0 0)`;
}

function peakPosition(channel: 0 | 1): string {
  return `${percent(level.value.peaksDb[channel]).toFixed(2)}%`;
}

function valueText(channel: 0 | 1): string {
  const current = level.value.levelsDb[channel];
  const peak = level.value.peaksDb[channel];
  return `${current.toFixed(1)} dBFS, peak ${peak.toFixed(1)} dBFS`;
}

async function poll() {
  if (stopped) return;
  try {
    const next = await api.outputLevelFrame();
    if (!stopped) polled.value = next;
  } catch {
    // A missed frame is harmless; keep the last reading and try next frame.
  }
  if (!stopped) animationFrame = requestAnimationFrame(() => void poll());
}

onMounted(async () => {
  if (!selfDriven.value) return;
  try {
    await api.setOutputMeterEnabled(true);
  } catch {
    // The meter stays at its floor if the audio engine is unavailable.
  }
  if (!stopped) void poll();
});

onBeforeUnmount(() => {
  stopped = true;
  if (!selfDriven.value) return;
  if (animationFrame !== null) cancelAnimationFrame(animationFrame);
  void api.setOutputMeterEnabled(false).catch(() => {});
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
      <div
        class="level-meter__track"
        role="meter"
        :aria-label="scope ? `${channel.label} ${scope}` : channel.label"
        :aria-valuemin="level.floorDb"
        aria-valuemax="0"
        :aria-valuenow="clampedDb(level.levelsDb[channel.index])"
        :aria-valuetext="valueText(channel.index)"
      >
        <div
          class="level-meter__fill"
          :class="{ 'is-over': level.peaksDb[channel.index] >= 0 }"
          :style="{ clipPath: levelClip(channel.index) }"
          aria-hidden="true"
        />
        <span
          class="level-meter__peak"
          :class="{ 'is-over': level.peaksDb[channel.index] >= 0 }"
          :style="{ bottom: peakPosition(channel.index) }"
          aria-hidden="true"
        />
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
  transition: clip-path 45ms linear;
}

.level-meter__fill.is-over {
  filter: saturate(1.2) brightness(1.08);
}

.level-meter__peak {
  position: absolute;
  z-index: 1;
  left: 1px;
  right: 1px;
  height: 2px;
  transform: translateY(1px);
  border-radius: 1px;
  background: var(--text);
  box-shadow: 0 0 0 0.5px var(--bg);
  transition: bottom 45ms linear;
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
