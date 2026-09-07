<script setup lang="ts">
/**
 * The compact "DJ Mixer" bubble from the drawing: preset, reverb and pitch
 * sliders, the six-band EQ, a normalisation switch and the filter chips, with
 * a link out to the full panel.
 *
 * Which of those rows appear, and in what order, is the user's own choice from
 * Settings ▸ Mixer. Only this surface is arrangeable: the sidebar's sections
 * are laid out to be read top to bottom as a signal chain, while this is a
 * short list of shortcuts to whichever controls someone actually reaches for.
 */
import { computed } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import AppSlider from "../ui/AppSlider.vue";
import AppToggle from "../ui/AppToggle.vue";
import EqSliders from "./EqSliders.vue";
import PresetSelect from "./PresetSelect.vue";
import FilterGrid from "./FilterGrid.vue";
import {
  DEFAULTS,
  POPOVER_SECTIONS,
  orderedSections,
  reverbAmount,
  reverbForAmount,
  tempoPercent,
} from "@/lib/mixer";
import { semitonesLabel } from "@/lib/format";
import { formatSeconds, withCrossfadeLength } from "@/lib/crossfadeCurve";
import { useMixerStore } from "@/stores/mixer";
import { useSettingsStore } from "@/stores/settings";
import type { Eq } from "@/lib/types";

const mixer = useMixerStore();
const settings = useSettingsStore();

const sections = computed(() =>
  orderedSections(
    POPOVER_SECTIONS,
    settings.preferences.popoverSectionOrder,
    settings.preferences.hiddenPopoverSections,
  ),
);

/** Slider max: long enough to be a real DJ-style overlap, short enough that
 * dragging the whole track still feels precise. */
const MAX_CROSSFADE_SECS = 12;

const fx = computed(() => mixer.effective);
const canEditCrossfade = computed(() => mixer.target.kind !== "entry");

const crossfadeLength = computed({
  get: () => fx.value.crossfade.lengthSecs,
  set: (secs: number) =>
    mixer.setSection("crossfade", withCrossfadeLength(fx.value.crossfade, secs)),
});

/*
 * One slider for the whole reverb.
 *
 * Reads 0 while reverb is bypassed, and dragging off zero switches it on. What
 * it writes is a complete reverb — size, damping, width and pre-delay as well
 * as the wet/dry balance — so the tail opens out as it rises instead of just
 * getting louder. Those are the same values the panel's knobs show, and
 * setting one there and coming back here leaves the slider where the wet
 * balance puts it.
 */
const reverb = computed({
  get: () => reverbAmount(fx.value.reverb),
  set: (amount: number) => mixer.setSection("reverb", reverbForAmount(amount)),
});

const semitones = computed({
  get: () => fx.value.pitch.semitones + fx.value.pitch.cents / 100,
  set: (value: number) =>
    mixer.setSection("pitch", { semitones: Math.round(value * 100) / 100, cents: 0 }),
});

const normalisation = computed({
  get: () => fx.value.normalisation.enabled,
  set: (enabled: boolean) =>
    mixer.setSection("normalisation", { ...fx.value.normalisation, enabled }),
});

function onEq(eq: Eq) {
  mixer.setSection("eq", eq);
}

function resetPitch() {
  mixer.setSection("pitch", DEFAULTS.pitch());
}

function openAdvanced() {
  mixer.popoverOpen = false;
  mixer.panelOpen = true;
}
</script>

<template>
  <div class="popover" role="dialog" aria-label="DJ Mixer">
    <header class="popover__head">
      <div>
        <p class="eyebrow">DJ Mixer</p>
        <div class="popover__target truncate">{{ mixer.targetLabel }}</div>
      </div>
      <button class="popover__advanced" @click="openAdvanced">
        <span>Advanced</span>
        <PnmIcon name="expand" :size="14" />
      </button>
    </header>

    <PresetSelect />

    <!-- The rows the user has kept, in the order they put them in. -->
    <template v-for="section in sections" :key="section">
      <template v-if="section === 'reverb'">
        <div class="popover__row">
          <label
            class="popover__label"
            title="Sweeps the whole reverb — size, damping, width and pre-delay, not just how much of it you hear"
          >Reverb</label>
          <AppSlider v-model="reverb" />
          <span class="popover__value">{{ Math.round(reverb * 100) }}%</span>
        </div>
        <p v-if="reverb > 0.005" class="popover__note">
          Room {{ Math.round(fx.reverb.size * 100) }}% · width
          {{ Math.round(fx.reverb.width * 100) }}%
          <button class="popover__reset" @click="reverb = 0">Off</button>
        </p>
      </template>

      <template v-else-if="section === 'pitch'">
        <div class="popover__row">
          <label class="popover__label">Pitch</label>
          <AppSlider v-model="semitones" :min="-12" :max="12" :step="0.5" :origin="0" />
          <span class="popover__value" :title="`Tempo ${tempoPercent(fx.pitch).toFixed(1)}%`">
            {{ semitonesLabel(fx.pitch.semitones, fx.pitch.cents) }}
          </span>
        </div>
        <p v-if="Math.abs(tempoPercent(fx.pitch)) > 0.5" class="popover__note">
          Tempo {{ tempoPercent(fx.pitch) > 0 ? "+" : "" }}{{ tempoPercent(fx.pitch).toFixed(1) }}%
          <button class="popover__reset" @click="resetPitch">Reset</button>
        </p>
      </template>

      <div v-else-if="section === 'eq'" class="popover__section">
        <label class="popover__label popover__label--block">EQ</label>
        <EqSliders :eq="fx.eq" @change="onEq" />
      </div>

      <div v-else-if="section === 'normalisation'" class="popover__row popover__row--toggle">
        <label class="popover__label">Normalisation</label>
        <AppToggle v-model="normalisation" label="Normalisation" />
      </div>

      <div
        v-else-if="section === 'crossfade' && canEditCrossfade"
        class="popover__row"
      >
        <label class="popover__label">Crossfade</label>
        <AppSlider v-model="crossfadeLength" :min="0" :max="MAX_CROSSFADE_SECS" :step="0.5" />
        <span class="popover__value">{{ formatSeconds(crossfadeLength) }}</span>
      </div>

      <div v-else-if="section === 'filters'" class="popover__section">
        <label class="popover__label popover__label--block">Atmospheres</label>
        <FilterGrid />
      </div>
    </template>

    <p v-if="sections.length === 0" class="popover__empty">
      Every control is hidden. Settings ▸ Mixer decides what appears here.
    </p>
  </div>
</template>

<style scoped>
.popover {
  width: 320px;
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.popover__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 10px;
}

.popover__target {
  margin-top: 2px;
  font-size: 13px;
  font-weight: 600;
  max-width: 170px;
}

.popover__advanced {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11.5px;
  color: var(--accent);
  font-weight: 500;
}

.popover__row {
  display: grid;
  grid-template-columns: 74px 1fr 40px;
  align-items: center;
  gap: 10px;
}

.popover__row--toggle {
  grid-template-columns: 1fr auto;
}

.popover__label {
  font-size: 12.5px;
  color: var(--text);
}

.popover__label--block {
  display: block;
  margin-bottom: 7px;
}

.popover__value {
  font-size: 11.5px;
  color: var(--text-tertiary);
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.popover__note {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: -6px 0 0 84px;
  font-size: 10.5px;
  color: var(--text-tertiary);
}

.popover__reset {
  font-size: 10.5px;
  color: var(--accent);
}

.popover__section {
  margin-top: 2px;
}

.popover__empty {
  margin: 0;
  font-size: 11.5px;
  color: var(--text-tertiary);
}
</style>
