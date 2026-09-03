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
import AppSlider from "../ui/AppSlider.vue";
import AppKnob from "../ui/AppKnob.vue";
import AppToggle from "../ui/AppToggle.vue";
import EqSliders from "./EqSliders.vue";
import EqModal from "./EqModal.vue";
import PresetSelect from "./PresetSelect.vue";
import FilterGrid from "./FilterGrid.vue";
import SectionHeader from "./SectionHeader.vue";
import CrossfadeGraph from "./CrossfadeGraph.vue";
import { audibleMix, defaultBands, tempoPercent } from "@/lib/mixer";
import { formatHz, semitonesLabel } from "@/lib/format";
import { formatSeconds } from "@/lib/crossfadeCurve";
import { withCrossfadeLength } from "@/lib/crossfadeCurve";
import { useMixerStore } from "@/stores/mixer";
import { usePlayerStore } from "@/stores/player";
import { usePresetEditorStore } from "@/stores/presetEditor";
import { useUiStore } from "@/stores/ui";
import type { Section } from "@/lib/mixer";
import type { CrossfadeCurve, Eq, MixerSettings, PanningMode } from "@/lib/types";

const props = withDefaults(defineProps<{ mode?: "live" | "preset" }>(), { mode: "live" });
const mixer = useMixerStore();
const player = usePlayerStore();
const presetEditor = usePresetEditorStore();
const ui = useUiStore();

const isPreset = computed(() => props.mode === "preset");
const isEqPreset = computed(() => isPreset.value && presetEditor.session?.sourceKind === "eq");
const eqExpanded = ref(false);
const fx = computed(() => isPreset.value ? presetEditor.effective : mixer.effective);
const targetLabel = computed(() => isPreset.value
  ? `Preset · ${presetEditor.session?.name ?? "Untitled"}`
  : mixer.targetLabel,
);
const canOverride = computed(() => isPreset.value || mixer.target.kind !== "global");
const isBlockTarget = computed(() => !isPreset.value && mixer.target.kind === "block");
// Crossfades apply between playlist entries, not to an individual entry's
// mixer override. Keep the global and playlist controls available, but do not
// offer a misleading per-song crossfade editor.
const canEditCrossfade = computed(
  () => isPreset.value || (mixer.target.kind !== "entry" && mixer.target.kind !== "block"),
);

const MAX_CROSSFADE_SECS = 12;
const crossfadeSettings = computed(() => fx.value.crossfade);

function setSection<K extends Section>(section: K, value: MixerSettings[K]) {
  if (isPreset.value) presetEditor.setSection(section, value);
  else void mixer.setSection(section, value);
}

function clearSection(section: Section) {
  if (isPreset.value) presetEditor.clearSection(section);
  else void mixer.clearSection(section);
}

function setEnabled(enabled: boolean) {
  if (isPreset.value) presetEditor.setEnabled(enabled);
  else void mixer.setEnabled(enabled);
}

function closePanel() {
  if (isPreset.value) presetEditor.close();
  else mixer.panelOpen = false;
}

async function savePresetDraft() {
  try {
    await presetEditor.save();
    ui.notify("Preset saved");
  } catch (error) {
    ui.notify(`Could not save preset: ${error instanceof Error ? error.message : String(error)}`, "error");
  }
}

function onCrossfadeLength(lengthSecs: number) {
  setSection("crossfade", withCrossfadeLength(crossfadeSettings.value, lengthSecs));
}

function onCrossfadeCurve(curve: CrossfadeCurve) {
  setSection("crossfade", { ...crossfadeSettings.value, curve });
}

function overridden(section: Section) {
  if (isPreset.value) {
    const value = presetEditor.session?.draft[section];
    return value !== null && value !== undefined;
  }
  return mixer.overriddenSections.includes(section);
}

// -- pitch -------------------------------------------------------------------
const semitones = computed({
  get: () => fx.value.pitch.semitones,
  set: (semitones: number) => setSection("pitch", { ...fx.value.pitch, semitones }),
});
const cents = computed({
  get: () => fx.value.pitch.cents,
  set: (cents: number) => setSection("pitch", { ...fx.value.pitch, cents }),
});

// -- panning -----------------------------------------------------------------
function setPanning(patch: Partial<typeof fx.value.panning>) {
  setSection("panning", { ...fx.value.panning, ...patch });
}

function onPanningMode(event: Event) {
  setPanning({ mode: (event.target as HTMLSelectElement).value as PanningMode });
}

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
function setReverb(patch: Partial<typeof fx.value.reverb>) {
  setSection("reverb", { ...fx.value.reverb, ...patch });
}

// -- delay -------------------------------------------------------------------
function setDelay(patch: Partial<typeof fx.value.delay>) {
  setSection("delay", { ...fx.value.delay, ...patch });
}

// -- normalisation -----------------------------------------------------------
function setNorm(patch: Partial<typeof fx.value.normalisation>) {
  setSection("normalisation", { ...fx.value.normalisation, ...patch });
}

// -- lo-fi -------------------------------------------------------------------
function setLofi(patch: Partial<typeof fx.value.lofi>) {
  setSection("lofi", { ...fx.value.lofi, ...patch });
}

// -- eq ----------------------------------------------------------------------
function onEq(eq: Eq) {
  setSection("eq", eq);
}

function resetEq() {
  onEq({ enabled: true, preampDb: 0, bands: defaultBands() });
}

const deviceRate = computed(() => player.snapshot.deviceSampleRate);
</script>

<template>
  <aside class="panel" role="complementary" aria-label="DJ Advanced Mixer">
    <header class="panel__head">
      <div class="panel__heading">
        <p class="eyebrow">{{ isEqPreset ? "EQ Preset Editor" : "Advanced DJ Mixer" }}</p>
        <h2 class="panel__title truncate">
          <PnmIcon
            :name="
              isPreset || mixer.target.kind === 'global'
                ? 'mixer'
                : mixer.target.kind === 'playlist'
                  ? 'addToPlaylist'
                  : 'music'
            "
            :size="12"
          />
          <span>{{ targetLabel }}</span>
        </h2>
      </div>
      <button class="icon-button" aria-label="Close mixer" @click="closePanel">
        <PnmIcon name="close" :size="18" />
      </button>
    </header>

    <div class="panel__body scroll-area">
      <div v-if="!isEqPreset" class="panel__bypass">
        <span>Effects</span>
        <AppToggle
          :model-value="fx.enabled"
          label="Enable effects"
          @update:model-value="setEnabled($event)"
        />
      </div>

      <PresetSelect v-if="!isPreset" />

      <div v-if="isPreset && presetEditor.session" class="panel__preset-save">
        <label>
          <span>Preset name</span>
          <input v-model="presetEditor.session.name" class="text-field" maxlength="60" />
        </label>
        <button
          class="pill-button"
          :disabled="presetEditor.saving || !presetEditor.session.name.trim()"
          @click="savePresetDraft"
        >
          {{ presetEditor.session.sourceBuiltIn ? "Save as Custom" : "Save" }}
        </button>
      </div>

      <p v-if="isPreset" class="panel__scope">
        This is an isolated {{ isEqPreset ? "EQ " : "" }}preset draft. Playback does not change while you edit it.
      </p>
      <p v-else-if="canOverride" class="panel__scope">
        Changes here apply only to <strong>{{ targetLabel }}</strong
        >. Untouched sections follow your global mixer.
      </p>

      <!-- EQ ---------------------------------------------------------------->
      <section class="panel__section">
        <SectionHeader
          title="EQ"
          :overridden="overridden('eq')"
          :can-override="canOverride && !isEqPreset"
          @clear="clearSection('eq')"
        >
          <div class="panel__spacer" />
          <button class="panel__link" @click="resetEq">Reset</button>
          <button
            class="icon-button"
            aria-label="Expand EQ"
            title="Expand EQ"
            @click="eqExpanded = true"
          >
            <PnmIcon name="expand" :size="16" />
          </button>
        </SectionHeader>

        <EqSliders :eq="fx.eq" @change="onEq" />
      </section>

      <template v-if="!isEqPreset">

      <!-- Pitch ------------------------------------------------------------->
      <section class="panel__section">
        <SectionHeader
          title="Pitch"
          :overridden="overridden('pitch')"
          :can-override="canOverride"
          @clear="clearSection('pitch')"
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
          <template v-if="isBlockTarget">
            The region on the timeline resizes to match, so it keeps covering the
            same part of the song.
          </template>
        </p>
      </section>

      <!-- Reverb ------------------------------------------------------------>
      <section class="panel__section">
        <SectionHeader
          title="Reverb"
          :overridden="overridden('reverb')"
          :can-override="canOverride"
          @clear="clearSection('reverb')"
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
          @clear="clearSection('delay')"
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
          @clear="clearSection('normalisation')"
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

      <!-- Panning ----------------------------------------------------------->
      <section class="panel__section" data-testid="panning-section">
        <SectionHeader
          title="Panning"
          :overridden="overridden('panning')"
          :can-override="canOverride"
          @clear="clearSection('panning')"
        />
        <label class="panning-mode">
          <span>Mode</span>
          <select
            class="text-field"
            aria-label="Panning mode"
            :value="fx.panning.mode"
            @change="onPanningMode"
          >
            <option value="monoPan">Mono Pan</option>
            <option value="stereoBalance">Stereo Balance</option>
            <option value="trueStereo">True Stereo</option>
          </select>
        </label>
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
      </section>

      <!-- Crossfade ----------------------------------------------------------->
      <section v-if="canEditCrossfade" class="panel__section">
        <SectionHeader
          title="Crossfade"
          :overridden="overridden('crossfade')"
          :can-override="canOverride"
          @clear="clearSection('crossfade')"
        >
          <div class="panel__spacer" />
          <span v-if="!isPreset && mixer.target.kind === 'playlist'" class="panel__global-note">
            Applies to this playlist
          </span>
        </SectionHeader>

        <div class="row">
          <label>Length</label>
          <AppSlider
            :model-value="crossfadeSettings.lengthSecs"
            :min="0"
            :max="MAX_CROSSFADE_SECS"
            :step="0.5"
            @update:model-value="onCrossfadeLength($event)"
          />
          <span class="row__value">{{ formatSeconds(crossfadeSettings.lengthSecs) }}</span>
        </div>

        <CrossfadeGraph
          class="panel__crossfade-graph"
          :curve="crossfadeSettings.curve"
          :length-secs="crossfadeSettings.lengthSecs"
          :disabled="crossfadeSettings.lengthSecs <= 0"
          @change="onCrossfadeCurve"
        />

        <p class="panel__hint">
          Drag a point to change when each song starts or finishes fading. Orange is the song
          ending, blue is the one starting. Double-click a point to reset it.
        </p>
      </section>

      <!-- Atmospheres ------------------------------------------------------->
      <section class="panel__section">
        <SectionHeader
          title="Atmospheres"
          :overridden="overridden('filters')"
          :can-override="canOverride"
          @clear="clearSection('filters')"
        />
        <FilterGrid
          show-volumes
          :settings="isPreset ? fx.filters : undefined"
          @toggle="presetEditor.toggleFilter"
          @volume="presetEditor.setFilterVolume"
        />
        <p v-if="isBlockTarget" class="panel__hint">
          A bed here plays for as long as this region does and fades with it,
          rather than running under the whole mix.
        </p>
      </section>

      <!-- Sample rate ------------------------------------------------------->
      <section class="panel__section">
        <SectionHeader
          title="Sample Rate"
          :overridden="overridden('lofi')"
          :can-override="canOverride"
          @clear="clearSection('lofi')"
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
      </template>
    </div>

    <!-- Edits whichever layer this panel is pointed at, like every other
         section here. -->
    <EqModal
      v-if="eqExpanded"
      :eq="fx.eq"
      :target-label="targetLabel"
      :sample-rate="deviceRate"
      @change="onEq"
      @close="eqExpanded = false"
    />
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
  display: flex;
  align-items: center;
  gap: 5px;
  margin: 2px 0 0;
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

.panel__preset-save {
  display: flex;
  align-items: flex-end;
  gap: 8px;
}

.panel__preset-save label {
  flex: 1;
  min-width: 0;
}

.panel__preset-save label > span {
  display: block;
  margin-bottom: 5px;
  font-size: 10.5px;
  color: var(--text-tertiary);
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

.panning-mode {
  display: grid;
  grid-template-columns: 68px 1fr;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
  font-size: 11.5px;
  color: var(--text-secondary);
}

.knobs--panning {
  justify-content: flex-start;
  gap: 24px;
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
