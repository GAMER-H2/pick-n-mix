<script setup lang="ts">
/**
 * The compact "DJ Mixer" bubble from the drawing: preset, reverb and pitch
 * sliders, the six-band EQ, a normalisation switch and the filter chips, with
 * a link out to the full panel.
 */
import { computed } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import AppSlider from "../ui/AppSlider.vue";
import AppToggle from "../ui/AppToggle.vue";
import EqSliders from "./EqSliders.vue";
import PresetSelect from "./PresetSelect.vue";
import FilterGrid from "./FilterGrid.vue";
import { audibleMix, DEFAULTS, tempoPercent } from "@/lib/mixer";
import { semitonesLabel } from "@/lib/format";
import { formatSeconds, withCrossfadeLength } from "@/lib/crossfadeCurve";
import { useMixerStore } from "@/stores/mixer";
import type { Eq } from "@/lib/types";

const mixer = useMixerStore();

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

// Reads 0 while reverb is bypassed, and dragging off zero switches it on.
const reverbMix = computed({
  get: () => audibleMix(fx.value.reverb),
  set: (mix: number) =>
    mixer.setSection("reverb", { ...fx.value.reverb, mix, enabled: mix > 0.001 }),
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

    <div class="popover__row">
      <label class="popover__label">Reverb</label>
      <AppSlider v-model="reverbMix" />
      <span class="popover__value">{{ Math.round(reverbMix * 100) }}%</span>
    </div>

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

    <div class="popover__section">
      <label class="popover__label popover__label--block">EQ</label>
      <EqSliders :eq="fx.eq" @change="onEq" />
    </div>

    <div class="popover__row popover__row--toggle">
      <label class="popover__label">Normalisation</label>
      <AppToggle v-model="normalisation" label="Normalisation" />
    </div>

    <div v-if="canEditCrossfade" class="popover__row">
      <label class="popover__label">Crossfade</label>
      <AppSlider v-model="crossfadeLength" :min="0" :max="MAX_CROSSFADE_SECS" :step="0.5" />
      <span class="popover__value">{{ formatSeconds(crossfadeLength) }}</span>
    </div>

    <div class="popover__section">
      <label class="popover__label popover__label--block">Atmospheres</label>
      <FilterGrid />
    </div>
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
</style>
