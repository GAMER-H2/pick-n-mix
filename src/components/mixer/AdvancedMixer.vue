<script setup lang="ts">
/**
 * The full "DJ Advanced Mixer" panel that slides in from the right.
 *
 * The drawing sketched the sections; this fills in the detailed control each
 * one needs. Every section writes into whichever layer the mixer is pointed
 * at, so the same panel serves global, playlist and per-track settings.
 */
import { computed, ref } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import AppSlider from "../AppSlider.vue";
import AppKnob from "../AppKnob.vue";
import AppToggle from "../AppToggle.vue";
import EqSliders from "./EqSliders.vue";
import PresetSelect from "./PresetSelect.vue";
import FilterGrid from "./FilterGrid.vue";
import SectionHeader from "./SectionHeader.vue";
import CrossfadeGraph from "./CrossfadeGraph.vue";
import { audibleMix, DEFAULT_EQ_FREQS, tempoPercent } from "@/lib/mixer";
import { formatHz, semitonesLabel } from "@/lib/format";
import { formatSeconds } from "@/lib/crossfadeCurve";
import { useMixerStore } from "@/stores/mixer";
import { usePlayerStore } from "@/stores/player";
import { useCrossfadeStore } from "@/stores/crossfade";
import type { BandKind, CrossfadeCurve, Eq } from "@/lib/types";

const mixer = useMixerStore();
const player = usePlayerStore();
const crossfade = useCrossfadeStore();

const eqExpanded = ref(false);
const fx = computed(() => mixer.effective);
const canOverride = computed(() => mixer.target.kind !== "global");

const MAX_CROSSFADE_SECS = 12;

function onCrossfadeCurve(curve: CrossfadeCurve) {
  crossfade.setCurve(curve);
}

function overridden(section: string) {
  return mixer.overriddenSections.includes(section as never);
}

// -- pitch -------------------------------------------------------------------
const semitones = computed({
  get: () => fx.value.pitch.semitones,
  set: (semitones: number) => mixer.setSection("pitch", { ...fx.value.pitch, semitones }),
});
const cents = computed({
  get: () => fx.value.pitch.cents,
  set: (cents: number) => mixer.setSection("pitch", { ...fx.value.pitch, cents }),
});

// -- reverb ------------------------------------------------------------------
function setReverb(patch: Partial<typeof fx.value.reverb>) {
  mixer.setSection("reverb", { ...fx.value.reverb, ...patch });
}

// -- delay -------------------------------------------------------------------
function setDelay(patch: Partial<typeof fx.value.delay>) {
  mixer.setSection("delay", { ...fx.value.delay, ...patch });
}

// -- normalisation -----------------------------------------------------------
function setNorm(patch: Partial<typeof fx.value.normalisation>) {
  mixer.setSection("normalisation", { ...fx.value.normalisation, ...patch });
}

// -- lo-fi -------------------------------------------------------------------
function setLofi(patch: Partial<typeof fx.value.lofi>) {
  mixer.setSection("lofi", { ...fx.value.lofi, ...patch });
}

// -- eq ----------------------------------------------------------------------
function onEq(eq: Eq) {
  mixer.setSection("eq", eq);
}

function setBand(index: number, patch: Record<string, unknown>) {
  const bands = fx.value.eq.bands.map((b, i) => (i === index ? { ...b, ...patch } : b));
  onEq({ ...fx.value.eq, bands });
}

function resetEq() {
  onEq({
    enabled: true,
    preampDb: 0,
    bands: DEFAULT_EQ_FREQS.map((freq, i) => ({
      kind: (i === 0 ? "lowShelf" : i === 5 ? "highShelf" : "peak") as BandKind,
      freq,
      gainDb: 0,
      q: 0.9,
      enabled: true,
    })),
  });
}

const BAND_KINDS: { value: BandKind; label: string }[] = [
  { value: "lowShelf", label: "Low Shelf" },
  { value: "peak", label: "Peak" },
  { value: "highShelf", label: "High Shelf" },
  { value: "lowPass", label: "Low Pass" },
  { value: "highPass", label: "High Pass" },
];

const deviceRate = computed(() => player.snapshot.deviceSampleRate);
</script>

<template>
  <aside class="panel" role="complementary" aria-label="DJ Advanced Mixer">
    <header class="panel__head">
      <div class="panel__heading">
        <h2>DJ Advanced Mixer</h2>
        <p class="panel__target truncate">
          <PnmIcon
            :name="
              mixer.target.kind === 'global'
                ? 'mixer'
                : mixer.target.kind === 'playlist'
                  ? 'addToPlaylist'
                  : 'music'
            "
            :size="12"
          />
          <span>{{ mixer.targetLabel }}</span>
        </p>
      </div>
      <button class="icon-button" aria-label="Close mixer" @click="mixer.panelOpen = false">
        <PnmIcon name="close" :size="18" />
      </button>
    </header>

    <div class="panel__body scroll-area">
      <div class="panel__bypass">
        <span>Effects</span>
        <AppToggle
          :model-value="fx.enabled"
          label="Enable effects"
          @update:model-value="mixer.setEnabled($event)"
        />
      </div>

      <PresetSelect />

      <p v-if="canOverride" class="panel__scope">
        Changes here apply only to <strong>{{ mixer.targetLabel }}</strong
        >. Untouched sections follow your global mixer.
      </p>

      <!-- EQ ---------------------------------------------------------------->
      <section class="panel__section">
        <SectionHeader
          title="EQ"
          :overridden="overridden('eq')"
          :can-override="canOverride"
          @clear="mixer.clearSection('eq')"
        >
          <div class="panel__spacer" />
          <button class="panel__link" @click="resetEq">Reset</button>
          <button
            class="icon-button"
            :aria-label="eqExpanded ? 'Collapse EQ' : 'Expand EQ'"
            @click="eqExpanded = !eqExpanded"
          >
            <PnmIcon :name="eqExpanded ? 'collapse' : 'expand'" :size="16" />
          </button>
        </SectionHeader>

        <EqSliders :eq="fx.eq" :height="eqExpanded ? 130 : 96" @change="onEq" />

        <div v-if="eqExpanded" class="bands">
          <div class="bands__head">
            <span>Band</span><span>Type</span><span>Freq</span><span>Q</span>
          </div>
          <div v-for="(band, index) in fx.eq.bands" :key="index" class="bands__row">
            <AppToggle
              :model-value="band.enabled"
              :label="`Band ${index + 1}`"
              @update:model-value="setBand(index, { enabled: $event })"
            />
            <select
              class="bands__select"
              :value="band.kind"
              @change="setBand(index, { kind: ($event.target as HTMLSelectElement).value })"
            >
              <option v-for="k in BAND_KINDS" :key="k.value" :value="k.value">{{ k.label }}</option>
            </select>
            <input
              class="bands__number"
              type="number"
              :value="Math.round(band.freq)"
              min="20"
              max="20000"
              @change="
                setBand(index, { freq: Number(($event.target as HTMLInputElement).value) })
              "
            />
            <input
              class="bands__number"
              type="number"
              :value="band.q.toFixed(2)"
              min="0.1"
              max="12"
              step="0.1"
              @change="setBand(index, { q: Number(($event.target as HTMLInputElement).value) })"
            />
          </div>

          <div class="row">
            <label>Preamp</label>
            <AppSlider
              :model-value="fx.eq.preampDb"
              :min="-12"
              :max="12"
              :step="0.5"
              :origin="0"
              @update:model-value="onEq({ ...fx.eq, preampDb: $event })"
            />
            <span class="row__value">{{ fx.eq.preampDb.toFixed(1) }} dB</span>
          </div>
        </div>
      </section>

      <!-- Pitch ------------------------------------------------------------->
      <section class="panel__section">
        <SectionHeader
          title="Pitch"
          :overridden="overridden('pitch')"
          :can-override="canOverride"
          @clear="mixer.clearSection('pitch')"
        />
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
        <p class="panel__hint">
          Varispeed: pitch and tempo move together, so this also changes speed by
          {{ tempoPercent(fx.pitch) > 0 ? "+" : "" }}{{ tempoPercent(fx.pitch).toFixed(1) }}%.
        </p>
      </section>

      <!-- Reverb ------------------------------------------------------------>
      <section class="panel__section">
        <SectionHeader
          title="Reverb"
          :overridden="overridden('reverb')"
          :can-override="canOverride"
          @clear="mixer.clearSection('reverb')"
        >
          <div class="panel__spacer" />
          <AppToggle
            :model-value="fx.reverb.enabled"
            label="Enable reverb"
            @update:model-value="setReverb({ enabled: $event })"
          />
        </SectionHeader>
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
      </section>

      <!-- Delay ------------------------------------------------------------->
      <section class="panel__section">
        <SectionHeader
          title="Delay"
          :overridden="overridden('delay')"
          :can-override="canOverride"
          @clear="mixer.clearSection('delay')"
        >
          <div class="panel__spacer" />
          <AppToggle
            :model-value="fx.delay.enabled"
            label="Enable delay"
            @update:model-value="setDelay({ enabled: $event })"
          />
        </SectionHeader>
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
      </section>

      <!-- Normalisation ----------------------------------------------------->
      <section class="panel__section">
        <SectionHeader
          title="Normalisation"
          :overridden="overridden('normalisation')"
          :can-override="canOverride"
          @clear="mixer.clearSection('normalisation')"
        >
          <div class="panel__spacer" />
          <AppToggle
            :model-value="fx.normalisation.enabled"
            label="Enable normalisation"
            @update:model-value="setNorm({ enabled: $event })"
          />
        </SectionHeader>

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

        <p class="panel__hint">
          Per-track gain comes from ReplayGain tags where a file has them. The limiter stays
          active even when normalisation is off, so effects cannot clip the output.
        </p>
      </section>

      <!-- Crossfade ----------------------------------------------------------->
      <section class="panel__section">
        <SectionHeader title="Crossfade">
          <div class="panel__spacer" />
          <span v-if="canOverride" class="panel__global-note">Applies to all playback</span>
        </SectionHeader>

        <div class="row">
          <label>Length</label>
          <AppSlider
            :model-value="crossfade.settings.lengthSecs"
            :min="0"
            :max="MAX_CROSSFADE_SECS"
            :step="0.5"
            @update:model-value="crossfade.setLength($event)"
          />
          <span class="row__value">{{ formatSeconds(crossfade.settings.lengthSecs) }}</span>
        </div>

        <CrossfadeGraph
          class="panel__crossfade-graph"
          :curve="crossfade.settings.curve"
          :length-secs="crossfade.settings.lengthSecs"
          :disabled="crossfade.settings.lengthSecs <= 0"
          @change="onCrossfadeCurve"
        />

        <p class="panel__hint">
          Drag a point to change when each song starts or finishes fading. Orange is the song
          ending, blue is the one starting. Double-click a point to reset it.
        </p>
      </section>

      <!-- Filters ----------------------------------------------------------->
      <section class="panel__section">
        <SectionHeader
          title="Filters"
          :overridden="overridden('filters')"
          :can-override="canOverride"
          @clear="mixer.clearSection('filters')"
        />
        <FilterGrid show-volumes />
      </section>

      <!-- Sample rate ------------------------------------------------------->
      <section class="panel__section">
        <SectionHeader
          title="Sample Rate"
          :overridden="overridden('lofi')"
          :can-override="canOverride"
          @clear="mixer.clearSection('lofi')"
        >
          <div class="panel__spacer" />
          <AppToggle
            :model-value="fx.lofi.enabled"
            label="Enable lo-fi"
            @update:model-value="setLofi({ enabled: $event })"
          />
        </SectionHeader>

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

        <p class="panel__hint">
          A creative crusher, not the output device's rate. Your device is running at
          {{ formatHz(deviceRate) }}.
        </p>
      </section>
    </div>
  </aside>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  width: var(--mixer-width);
  flex: none;
  border-left: 1px solid var(--separator);
  background: var(--bg-elevated);
}

.panel__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 10px;
  padding: 14px 12px 10px 16px;
  border-bottom: 1px solid var(--separator);
}

.panel__heading h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.panel__target {
  display: flex;
  align-items: center;
  gap: 5px;
  margin: 3px 0 0;
  font-size: 11px;
  color: var(--text-tertiary);
  max-width: 230px;
}

.panel__body {
  flex: 1;
  padding: 14px 16px 28px;
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.panel__bypass {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 13px;
  font-weight: 600;
}

.panel__scope {
  margin: 0;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  background: var(--accent-tint);
  color: var(--accent);
  font-size: 11px;
  line-height: 1.45;
}

.panel__section {
  border-top: 1px solid var(--separator);
  padding-top: 14px;
}

.panel__section:first-of-type {
  border-top: 0;
  padding-top: 0;
}

.panel__spacer {
  flex: 1;
}

.panel__link {
  font-size: 11px;
  color: var(--accent);
}

.panel__hint {
  margin: 10px 0 0;
  font-size: 10.5px;
  line-height: 1.5;
  color: var(--text-tertiary);
}

.panel__global-note {
  font-size: 10px;
  color: var(--text-tertiary);
  white-space: nowrap;
}

.panel__crossfade-graph {
  margin-top: 10px;
}

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

.bands {
  margin-top: 10px;
}

.bands__head,
.bands__row {
  display: grid;
  grid-template-columns: 40px 1fr 58px 46px;
  align-items: center;
  gap: 6px;
}

.bands__head {
  margin-bottom: 4px;
  font-size: 9.5px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--text-tertiary);
}

.bands__row {
  margin-bottom: 5px;
}

.bands__select,
.bands__number {
  height: 24px;
  padding: 0 5px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--separator);
  background: var(--bg-elevated);
  font-size: 11px;
  outline: none;
  min-width: 0;
}

.bands__number {
  font-variant-numeric: tabular-nums;
  user-select: text;
}

.bands__select:focus,
.bands__number:focus {
  border-color: var(--accent);
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
