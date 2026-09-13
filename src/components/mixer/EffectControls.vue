<script setup lang="ts">
/**
 * The controls for one mixer section, with no heading of its own.
 *
 * Both surfaces that edit effects draw the same knobs: the sidebar stacks them
 * down the panel, and the master mixer's rack lays them out along the bottom
 * of the arrangement. Only the frame around them differs, so only the frame is
 * written twice — the controls themselves live here, and a change to what a
 * reverb offers reaches both at once.
 *
 * Emits one-section patches rather than writing anything: which layer of the
 * cascade this is editing is the caller's business, and this component is the
 * same whether that is a playlist, a track, a timeline block or a preset draft.
 */
import { computed } from "vue";
import AppSlider from "../ui/AppSlider.vue";
import AppKnob from "../ui/AppKnob.vue";
import AppToggle from "../ui/AppToggle.vue";
import SelectMenu, { type SelectOption } from "../ui/SelectMenu.vue";
import EqSliders from "./EqSliders.vue";
import { audibleMix, tempoPercent } from "@/lib/mixer";
import { formatHz, semitonesLabel } from "@/lib/format";
import { usePlayerStore } from "@/stores/player";
import type { Section } from "@/lib/mixer";
import type { Eq, MixerSettings, PanningMode, ResolvedMixer } from "@/lib/types";

const props = withDefaults(
  defineProps<{
    section: Section;
    /** The cascade as it currently resolves, which is what the controls show. */
    fx: ResolvedMixer;
    /**
     * True when this is editing one region on a master mix, which changes what
     * a couple of the hints can promise.
     */
    blockTarget?: boolean;
  }>(),
  { blockTarget: false },
);

const emit = defineEmits<{ change: [patch: MixerSettings] }>();

const player = usePlayerStore();
const deviceRate = computed(() => player.snapshot.deviceSampleRate);
const isBlockTarget = computed(() => props.blockTarget);
const fx = computed(() => props.fx);

// -- pitch -------------------------------------------------------------------
const semitones = computed({
  get: () => fx.value.pitch.semitones,
  set: (semitones: number) => emit("change", { pitch: { ...fx.value.pitch, semitones } }),
});
const cents = computed({
  get: () => fx.value.pitch.cents,
  set: (cents: number) => emit("change", { pitch: { ...fx.value.pitch, cents } }),
});

// -- panning -----------------------------------------------------------------
function setPanning(patch: Partial<ResolvedMixer["panning"]>) {
  emit("change", { panning: { ...fx.value.panning, ...patch } });
}

const PANNING_MODES: ReadonlyArray<SelectOption> = [
  { id: "monoPan", label: "Mono Pan" },
  { id: "stereoBalance", label: "Stereo Balance" },
  { id: "trueStereo", label: "True Stereo" },
];

const panningLabel = computed(() => {
  switch (fx.value.panning.mode) {
    case "monoPan":
      return "Pan";
    case "stereoBalance":
      return "Balance";
    case "trueStereo":
      return "Centre";
  }
});

function panningPositionDisplay(position: number): string {
  if (Math.abs(position) < 0.005) return "C";
  return `${position < 0 ? "L" : "R"} ${Math.round(Math.abs(position) * 100)}`;
}

// -- reverb ------------------------------------------------------------------
function setReverb(patch: Partial<ResolvedMixer["reverb"]>) {
  emit("change", { reverb: { ...fx.value.reverb, ...patch } });
}

// -- delay -------------------------------------------------------------------
function setDelay(patch: Partial<ResolvedMixer["delay"]>) {
  emit("change", { delay: { ...fx.value.delay, ...patch } });
}

// -- normalisation -----------------------------------------------------------
function setNorm(patch: Partial<ResolvedMixer["normalisation"]>) {
  emit("change", { normalisation: { ...fx.value.normalisation, ...patch } });
}

// -- lo-fi -------------------------------------------------------------------
function setLofi(patch: Partial<ResolvedMixer["lofi"]>) {
  emit("change", { lofi: { ...fx.value.lofi, ...patch } });
}

// -- eq ----------------------------------------------------------------------
function onEq(eq: Eq) {
  emit("change", { eq });
}
</script>

<template>
  <template v-if="section === 'eq'">
    <EqSliders :eq="fx.eq" @change="onEq" />
  </template>

  <template v-else-if="section === 'pitch'">
    <div class="row">
      <label>Semitones</label>
      <AppSlider v-model="semitones" :min="-12" :max="12" :step="1" :origin="0" />
      <span class="row__value">{{ semitonesLabel(fx.pitch.semitones, 0) }}</span>
    </div>
    <div class="row">
      <label>Fine</label>
      <AppSlider v-model="cents" :min="-100" :max="100" :step="1" :origin="0" />
      <span class="row__value">{{ Math.round(fx.pitch.cents) }}¢</span>
    </div>
    <p class="hint">
      Varispeed: pitch and tempo move together, so this also changes speed by
      {{ tempoPercent(fx.pitch) > 0 ? "+" : "" }}{{ tempoPercent(fx.pitch).toFixed(1) }}%.
      <template v-if="isBlockTarget">
        The region on the timeline resizes to match, so it keeps covering the
        same part of the song.
      </template>
    </p>
  </template>

  <template v-else-if="section === 'panning'">
    <SelectMenu
      class="panning-mode"
      label="Mode"
      :model-value="fx.panning.mode"
      :options="PANNING_MODES"
      @update:model-value="setPanning({ mode: $event as PanningMode })"
    />
    <div class="knobs knobs--panning">
      <AppKnob
        :model-value="fx.panning.position"
        :min="-1"
        :max="1"
        :detents="[0]"
        :label="panningLabel"
        :display="panningPositionDisplay(fx.panning.position)"
        @update:model-value="setPanning({ position: $event })"
      />
      <AppKnob
        v-if="fx.panning.mode === 'trueStereo'"
        :model-value="fx.panning.width"
        label="Width"
        :display="`${Math.round(fx.panning.width * 100)}%`"
        @update:model-value="setPanning({ width: $event })"
      />
    </div>
  </template>

  <template v-else-if="section === 'reverb'">
    <div class="knobs">
      <AppKnob
        :model-value="fx.reverb.size"
        label="Size"
        :display="`${Math.round(fx.reverb.size * 100)}%`"
        :disabled="!fx.reverb.enabled"
        @update:model-value="setReverb({ size: $event })"
      />
      <AppKnob
        :model-value="fx.reverb.damping"
        label="Damping"
        :display="`${Math.round(fx.reverb.damping * 100)}%`"
        :disabled="!fx.reverb.enabled"
        @update:model-value="setReverb({ damping: $event })"
      />
      <AppKnob
        :model-value="audibleMix(fx.reverb)"
        label="Mix"
        :display="`${Math.round(audibleMix(fx.reverb) * 100)}%`"
        :disabled="!fx.reverb.enabled"
        @update:model-value="setReverb({ mix: $event })"
      />
      <AppKnob
        :model-value="fx.reverb.width"
        label="Width"
        :display="`${Math.round(fx.reverb.width * 100)}%`"
        :disabled="!fx.reverb.enabled"
        @update:model-value="setReverb({ width: $event })"
      />
      <AppKnob
        :model-value="fx.reverb.predelayMs"
        :min="0"
        :max="250"
        label="Pre-delay"
        :display="`${Math.round(fx.reverb.predelayMs)} ms`"
        :disabled="!fx.reverb.enabled"
        @update:model-value="setReverb({ predelayMs: $event })"
      />
    </div>
  </template>

  <template v-else-if="section === 'delay'">
    <div class="knobs">
      <AppKnob
        :model-value="fx.delay.timeMs"
        :min="10"
        :max="2000"
        label="Time"
        :display="`${Math.round(fx.delay.timeMs)} ms`"
        :disabled="!fx.delay.enabled"
        @update:model-value="setDelay({ timeMs: $event })"
      />
      <AppKnob
        :model-value="fx.delay.feedback"
        :max="0.95"
        label="Feedback"
        :display="`${Math.round(fx.delay.feedback * 100)}%`"
        :disabled="!fx.delay.enabled"
        @update:model-value="setDelay({ feedback: $event })"
      />
      <AppKnob
        :model-value="audibleMix(fx.delay)"
        label="Mix"
        :display="`${Math.round(audibleMix(fx.delay) * 100)}%`"
        :disabled="!fx.delay.enabled"
        @update:model-value="setDelay({ mix: $event })"
      />
      <AppKnob
        :model-value="fx.delay.toneHz"
        :min="500"
        :max="18000"
        label="Tone"
        :display="formatHz(Math.round(fx.delay.toneHz))"
        :disabled="!fx.delay.enabled"
        @update:model-value="setDelay({ toneHz: $event })"
      />
      <AppKnob
        :model-value="fx.delay.spread"
        label="Ping-Pong"
        :display="`${Math.round(fx.delay.spread * 100)}%`"
        :disabled="!fx.delay.enabled"
        @update:model-value="setDelay({ spread: $event })"
      />
    </div>
  </template>

  <template v-else-if="section === 'normalisation'">
    <div class="row">
      <label>Gain</label>
      <AppSlider
        :model-value="fx.normalisation.gainDb"
        :min="-12"
        :max="12"
        :step="0.5"
        :origin="0"
        @update:model-value="setNorm({ gainDb: $event })"
      />
      <span class="row__value">{{ fx.normalisation.gainDb.toFixed(1) }} dB</span>
    </div>

    <div class="row">
      <label>Limiter</label>
      <AppSlider
        :model-value="fx.normalisation.limiterCeilingDb"
        :min="-12"
        :max="0"
        :step="0.1"
        @update:model-value="setNorm({ limiterCeilingDb: $event })"
      />
      <span class="row__value">{{ fx.normalisation.limiterCeilingDb.toFixed(1) }} dB</span>
    </div>

    <div class="row">
      <label>Release</label>
      <AppSlider
        :model-value="fx.normalisation.limiterReleaseMs"
        :min="5"
        :max="1000"
        :step="5"
        :disabled="!fx.normalisation.limiterEnabled"
        @update:model-value="setNorm({ limiterReleaseMs: $event })"
      />
      <span class="row__value">{{ Math.round(fx.normalisation.limiterReleaseMs) }} ms</span>
    </div>

    <div class="row row--toggle">
      <label>Safety limiter</label>
      <AppToggle
        :model-value="fx.normalisation.limiterEnabled"
        label="Safety limiter"
        @update:model-value="setNorm({ limiterEnabled: $event })"
      />
    </div>

    <div class="meter" :title="`Gain reduction: ${player.snapshot.limiterReductionDb.toFixed(1)} dB`">
      <span class="meter__label">Reduction</span>
      <div class="meter__track">
        <div
          class="meter__fill"
          :style="{ width: `${Math.min(100, player.snapshot.limiterReductionDb * 8.33)}%` }"
        />
      </div>
      <span class="row__value">-{{ player.snapshot.limiterReductionDb.toFixed(1) }} dB</span>
    </div>

    <p class="hint">
      Per-track gain comes from ReplayGain tags where a file has them. The limiter stays
      active even when normalisation is off, so effects cannot clip the output.
    </p>
  </template>

  <template v-else-if="section === 'lofi'">
    <div class="row">
      <label>Rate</label>
      <AppSlider
        :model-value="fx.lofi.sampleRateHz"
        :min="1000"
        :max="48000"
        :step="100"
        :disabled="!fx.lofi.enabled"
        @update:model-value="setLofi({ sampleRateHz: $event })"
      />
      <span class="row__value">{{ formatHz(Math.round(fx.lofi.sampleRateHz)) }}</span>
    </div>

    <div class="row">
      <label>Bit depth</label>
      <AppSlider
        :model-value="fx.lofi.bitDepth"
        :min="2"
        :max="16"
        :step="1"
        :disabled="!fx.lofi.enabled"
        @update:model-value="setLofi({ bitDepth: $event })"
      />
      <span class="row__value">{{ Math.round(fx.lofi.bitDepth) }} bit</span>
    </div>

    <div class="row">
      <label>Mix</label>
      <AppSlider
        :model-value="audibleMix(fx.lofi)"
        :disabled="!fx.lofi.enabled"
        @update:model-value="setLofi({ mix: $event })"
      />
      <span class="row__value">{{ Math.round(audibleMix(fx.lofi) * 100) }}%</span>
    </div>

    <p class="hint">
      A creative crusher, not the output device's rate. Your device is running at
      {{ formatHz(deviceRate) }}.
    </p>
  </template>
</template>

<style scoped>
.row {
  display: grid;
  grid-template-columns: 68px 1fr 58px;
  align-items: center;
  gap: 8px;
  margin-top: 7px;
}

.row--toggle {
  grid-template-columns: 1fr auto;
}

.row label {
  font-size: 11.5px;
  color: var(--text-secondary);
}

.row__value {
  font-size: 10.5px;
  color: var(--text-tertiary);
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.knobs {
  display: flex;
  flex-wrap: wrap;
  gap: 12px 6px;
  justify-content: space-between;
}

.panning-mode {
  margin-bottom: 10px;
}

.knobs--panning {
  justify-content: flex-start;
  gap: 24px;
}

.hint {
  margin: 10px 0 0;
  font-size: 10.5px;
  line-height: 1.5;
  color: var(--text-tertiary);
}

.meter {
  display: grid;
  grid-template-columns: 68px 1fr 58px;
  align-items: center;
  gap: 8px;
  margin-top: 10px;
}

.meter__label {
  font-size: 11.5px;
  color: var(--text-secondary);
}

.meter__track {
  height: 4px;
  border-radius: 999px;
  background: var(--control-track);
  overflow: hidden;
}

.meter__fill {
  height: 100%;
  background: var(--accent);
  transition: width 0.15s linear;
}
</style>
