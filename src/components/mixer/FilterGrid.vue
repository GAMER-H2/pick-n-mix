<script setup lang="ts">
/**
 * Atmospheres are looping background beds: rain, vinyl crackle, a fireplace
 * and other environmental sound. Built-ins ship with the app; custom sounds
 * continue to come from the user's ambience directory.
 */
import { computed } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import AppKnob from "../AppKnob.vue";
import { useMixerStore } from "@/stores/mixer";
import { useSettingsStore } from "@/stores/settings";
import type { FilterSetting } from "@/lib/types";

const props = withDefaults(
  defineProps<{ showVolumes?: boolean; settings?: FilterSetting[] }>(),
  { showVolumes: true, settings: undefined },
);
const emit = defineEmits<{
  toggle: [id: string, enabled: boolean];
  volume: [id: string, volume: number];
}>();
const mixer = useMixerStore();
const appSettings = useSettingsStore();

const activeSettings = computed(() => props.settings ?? mixer.effective.filters);
const settingsFor = computed(() => new Map(activeSettings.value.map((f) => [f.id, f])));
const catalogue = computed(() => mixer.filters.filter((filter) =>
  !filter.builtIn ||
  !appSettings.preferences.hiddenBuiltInFilterIds.includes(filter.id) ||
  isOn(filter.id),
));

function isOn(id: string) {
  return settingsFor.value.get(id)?.enabled ?? false;
}

function volumeOf(id: string) {
  return settingsFor.value.get(id)?.volume ?? 0.4;
}

function toggle(id: string) {
  const enabled = !isOn(id);
  if (props.settings) emit("toggle", id, enabled);
  else void mixer.toggleFilter(id, enabled);
}

function setVolume(id: string, volume: number) {
  if (props.settings) emit("volume", id, volume);
  else void mixer.setFilterVolume(id, volume);
}
</script>

<template>
  <div>
    <div class="grid">
      <button
        v-for="filter in catalogue"
        :key="filter.id"
        class="chip"
        :class="{
          'is-on': isOn(filter.id),
          'is-built-in': filter.builtIn,
          'is-unavailable': !filter.available,
        }"
        :data-atmosphere="filter.builtIn ? filter.id : undefined"
        :disabled="!filter.available"
        :title="
          filter.available
            ? filter.name
            : `Drop a ${filter.id} audio file into ${mixer.filtersDir} to enable this`
        "
        @click="toggle(filter.id)"
      >
        <span class="truncate">{{ filter.name }}</span>
      </button>
    </div>

    <div v-if="props.showVolumes && activeSettings.some((f) => f.enabled)" class="levels">
      <div
        v-for="filter in activeSettings.filter((f) => f.enabled)"
        :key="filter.id"
        class="levels__control"
      >
        <AppKnob
          :model-value="volumeOf(filter.id)"
          :label="`${mixer.filters.find((item) => item.id === filter.id)?.name ?? filter.id} volume`"
          :display="`${Math.round(volumeOf(filter.id) * 100)}%`"
          :size="42"
          @update:model-value="setVolume(filter.id, $event)"
        />
      </div>
    </div>

    <p v-if="!catalogue.some((f) => f.available)" class="hint">
      <PnmIcon name="folder" :size="13" />
      <span>Custom atmospheres can be added in Settings or placed in <code>{{ mixer.filtersDir }}</code>.</span>
    </p>
  </div>
</template>

<style scoped>
.grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 6px;
}

.chip {
  position: relative;
  isolation: isolate;
  overflow: hidden;
  height: 30px;
  padding: 0 8px;
  border-radius: 999px;
  border: 1px solid var(--separator);
  background: var(--bg-elevated);
  font-size: 11.5px;
  color: var(--text);
  transition: all 0.15s var(--ease);
}

.chip > span {
  position: relative;
  z-index: 1;
}

.chip:hover:not(:disabled) {
  border-color: var(--separator-strong);
}

.chip.is-on {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-contrast);
  font-weight: 600;
}

/*
 * An active built-in atmosphere is drawn as the thing it is rather than as a
 * generic shimmer: rain falls, fire flickers, a record turns. At 30px tall
 * there is no room for detail, so each leans on one unmistakable motion and a
 * colour the sound already carries.
 *
 * All CSS, and all of it `transform`/`opacity` on a pseudo-element, so the
 * compositor handles it and nothing here competes with the audio thread.
 * Imported audio has no such vocabulary to draw on and keeps the plain accent.
 */
.chip.is-built-in.is-on[data-atmosphere] {
  border-color: transparent;
  color: #fff;
  background: var(--atmosphere-base, var(--accent));
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
}

.chip.is-built-in.is-on[data-atmosphere]::before,
.chip.is-built-in.is-on[data-atmosphere]::after {
  content: "";
  position: absolute;
  z-index: 0;
  pointer-events: none;
}

/* Rain: pale streaks falling through overcast grey. */
.chip[data-atmosphere="rain"] {
  --atmosphere-base: linear-gradient(180deg, #6a747e, #4d565f);
}

.chip.is-on[data-atmosphere="rain"]::before {
  inset: -150% 0;
  background: repeating-linear-gradient(
    10deg,
    transparent 0 6px,
    rgba(190, 222, 250, 0.9) 6px 7.5px,
    transparent 7.5px 12px
  );
  /* Chopped across the streaks so they read as separate droplets rather than
     as continuous diagonal hatching. */
  -webkit-mask-image: repeating-linear-gradient(
    100deg,
    #000 0 5px,
    transparent 5px 15px
  );
  mask-image: repeating-linear-gradient(100deg, #000 0 5px, transparent 5px 15px);
  animation: rain-fall 0.7s linear infinite;
}

/* A third of the tile height, which is one whole repeat of the pattern. */
@keyframes rain-fall {
  to { transform: translateY(33.333%); }
}

/* Fireplace: two embers breathing out of step, so it reads as a flicker
   rather than a pulse. */
.chip[data-atmosphere="fireplace"] {
  --atmosphere-base: linear-gradient(180deg, #7a2408, #351004);
}

.chip.is-on[data-atmosphere="fireplace"]::before {
  inset: 0;
  background:
    radial-gradient(60% 90% at 32% 105%, rgba(255, 214, 92, 0.95), transparent 70%),
    radial-gradient(55% 85% at 68% 110%, rgba(255, 138, 30, 0.9), transparent 72%);
  animation: fire-flicker 0.45s ease-in-out infinite alternate;
}

.chip.is-on[data-atmosphere="fireplace"]::after {
  inset: 0;
  background: radial-gradient(70% 100% at 50% 115%, rgba(255, 92, 26, 0.7), transparent 68%);
  animation: fire-flicker 0.67s ease-in-out infinite alternate-reverse;
}

@keyframes fire-flicker {
  from { opacity: 0.5; transform: scaleY(0.86); }
  to { opacity: 1; transform: scaleY(1.14); }
}

/* Forest: dappled light drifting across the canopy. */
.chip[data-atmosphere="forest"] {
  --atmosphere-base: linear-gradient(180deg, #2c6138, #17361f);
}

.chip.is-on[data-atmosphere="forest"]::before {
  inset: -40% -25%;
  background:
    radial-gradient(28% 45% at 22% 32%, rgba(190, 240, 150, 0.5), transparent 70%),
    radial-gradient(24% 40% at 62% 68%, rgba(214, 255, 176, 0.42), transparent 72%),
    radial-gradient(20% 36% at 85% 28%, rgba(160, 220, 130, 0.38), transparent 70%);
  animation: forest-sway 7s ease-in-out infinite alternate;
}

@keyframes forest-sway {
  from { transform: translate3d(-4%, -2%, 0); }
  to { transform: translate3d(4%, 2%, 0); }
}

/* City: lit windows sliding past, with one block blinking out. */
.chip[data-atmosphere="city"] {
  --atmosphere-base: linear-gradient(180deg, #2b3346, #161b26);
}

.chip.is-on[data-atmosphere="city"]::before {
  inset: 0 -60%;
  /* Two column rhythms at different periods, so the skyline reads as uneven
     buildings rather than as an evenly spaced barcode. The horizontal band
     cuts them into floors. */
  background-image: repeating-linear-gradient(
      90deg,
      rgba(255, 205, 140, 0.9) 0 2px,
      transparent 2px 7px
    ),
    repeating-linear-gradient(
      90deg,
      rgba(255, 186, 110, 0.55) 0 3px,
      transparent 3px 17px
    ),
    repeating-linear-gradient(0deg, rgba(0, 0, 0, 0.6) 0 2px, transparent 2px 5px);
  animation: city-drift 9s linear infinite;
}

.chip.is-on[data-atmosphere="city"]::after {
  inset: 0;
  background: radial-gradient(38% 60% at 74% 40%, rgba(255, 224, 170, 0.55), transparent 70%);
  animation: city-blink 2.3s steps(1, end) infinite;
}

@keyframes city-drift {
  to { transform: translateX(-18%); }
}

@keyframes city-blink {
  0%, 62% { opacity: 0.15; }
  64%, 100% { opacity: 0.6; }
}

/* Ocean: swell rolling sideways while the surface rises and falls. */
.chip[data-atmosphere="ocean"] {
  --atmosphere-base: linear-gradient(180deg, #1d6f9e, #0b3350);
}

.chip.is-on[data-atmosphere="ocean"]::before {
  inset: -30% -50%;
  background: repeating-linear-gradient(
    -8deg,
    rgba(255, 255, 255, 0.26) 0 2px,
    transparent 2px 11px
  );
  animation: ocean-swell 3.4s ease-in-out infinite alternate;
}

@keyframes ocean-swell {
  from { transform: translate3d(-6%, 3%, 0); }
  to { transform: translate3d(6%, -3%, 0); }
}

/* Vinyl: grooves turning under the needle, with surface noise over them. */
.chip[data-atmosphere="vinyl"] {
  --atmosphere-base: linear-gradient(180deg, #3b312a, #1d1815);
}

.chip.is-on[data-atmosphere="vinyl"]::before {
  /* Square and centred, so the grooves stay circular in a wide chip. */
  width: 220%;
  aspect-ratio: 1;
  top: 50%;
  left: 50%;
  margin: -110% 0 0 -110%;
  background: repeating-radial-gradient(
    circle at 50% 50%,
    rgba(255, 255, 255, 0.15) 0 1px,
    transparent 1px 5px
  );
  animation: vinyl-spin 2.6s linear infinite;
}

.chip.is-on[data-atmosphere="vinyl"]::after {
  inset: 0;
  background:
    radial-gradient(circle at 18% 34%, rgba(255, 255, 255, 0.5) 0 1px, transparent 1px),
    radial-gradient(circle at 71% 62%, rgba(255, 255, 255, 0.45) 0 1px, transparent 1px),
    radial-gradient(circle at 44% 78%, rgba(255, 255, 255, 0.4) 0 1px, transparent 1px);
  animation: vinyl-crackle 0.32s steps(1, end) infinite;
}

@keyframes vinyl-spin {
  to { transform: rotate(1turn); }
}

@keyframes vinyl-crackle {
  0% { opacity: 0.7; transform: translate(0, 0); }
  33% { opacity: 0.2; transform: translate(28%, -18%); }
  66% { opacity: 0.55; transform: translate(-22%, 24%); }
  100% { opacity: 0.3; transform: translate(14%, 10%); }
}

.chip.is-unavailable {
  opacity: 0.4;
  cursor: not-allowed;
}

.levels {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 10px 14px;
  margin-top: 11px;
  padding-top: 10px;
  border-top: 1px solid var(--separator);
}

.levels__control {
  min-width: 58px;
}

/* Motion off, but the colours stay: a still fire is still recognisably fire,
   which is more use than falling back to a flat accent fill. */
@media (prefers-reduced-motion: reduce) {
  .chip.is-built-in.is-on[data-atmosphere]::before,
  .chip.is-built-in.is-on[data-atmosphere]::after {
    animation: none;
  }
}

.hint {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin: 10px 0 0;
  font-size: 11px;
  line-height: 1.45;
  color: var(--text-tertiary);
}

code {
  font-size: 10.5px;
  word-break: break-all;
  user-select: text;
}
</style>
