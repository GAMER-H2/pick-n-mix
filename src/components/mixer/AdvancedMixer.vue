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
import AppToggle from "../ui/AppToggle.vue";
import EqModal from "./EqModal.vue";
import PresetSelect from "./PresetSelect.vue";
import FilterGrid from "./FilterGrid.vue";
import SectionHeader from "./SectionHeader.vue";
import EffectControls from "./EffectControls.vue";
import CrossfadeGraph from "./CrossfadeGraph.vue";
import { defaultBands, SECTION_LABELS } from "@/lib/mixer";
import { useDragReorder } from "@/lib/dragReorder";
import { formatSeconds } from "@/lib/crossfadeCurve";
import { withCrossfadeLength } from "@/lib/crossfadeCurve";
import { useMixerStore } from "@/stores/mixer";
import { usePlayerStore } from "@/stores/player";
import { usePresetEditorStore } from "@/stores/presetEditor";
import { useSettingsStore } from "@/stores/settings";
import { useUiStore } from "@/stores/ui";
import type { Section } from "@/lib/mixer";
import type { ChainStage, CrossfadeCurve, Eq, MixerSettings } from "@/lib/types";

const props = withDefaults(
  defineProps<{
    mode?: "live" | "preset";
    /**
     * Whether this panel is the sidebar, whose section list the user can trim
     * in Settings ▸ Mixer. The master mixer's block panel and the preset
     * editor always show everything they can edit: a section hidden from the
     * sidebar is still part of a preset, and hiding it there would leave a
     * saved setting with nothing to change it back with.
     */
    customisable?: boolean;
  }>(),
  { mode: "live", customisable: false },
);
const mixer = useMixerStore();
const settings = useSettingsStore();
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

/**
 * Whether a section is drawn at all.
 *
 * Hiding is a sidebar-only preference and never touches the settings
 * themselves: a hidden section keeps whatever it was set to, and showing it
 * again brings the same values back.
 */
function sectionShown(section: Section): boolean {
  return !props.customisable || !settings.preferences.hiddenMixerSections.includes(section);
}

/*
 * The chain, in the order it is heard.
 *
 * The panel used to list its sections in a fixed order that only looked like a
 * signal chain; now the list *is* the chain, top to bottom, and moving a row
 * moves the effect. The sections that are not stages — pitch, normalisation,
 * crossfade and the ambience beds — are drawn underneath in their own group
 * rather than pretending to a place in it.
 */
const chainStages = computed<ChainStage[]>(() =>
  isEqPreset.value ? ["eq"] : fx.value.chainOrder.filter((stage) => sectionShown(stage)),
);

/**
 * The sections that are not stages of the chain, in the order the user has put
 * them in. That order is layout and nothing else — none of these is a place
 * the signal passes through — but where a control sits is still worth being
 * able to decide, so they move like the chain's rows do.
 */
const layoutSections = computed<Section[]>(() =>
  isEqPreset.value
    ? []
    : (fx.value.layoutOrder as Section[]).filter(
        (section) =>
          sectionShown(section) && (section !== "crossfade" || canEditCrossfade.value),
      ),
);

/** Reordering an EQ preset's chain of one would mean nothing. */
const canReorder = computed(() => !isEqPreset.value && chainStages.value.length > 1);
const canReorderLayout = computed(() => layoutSections.value.length > 1);
const chainListEl = ref<HTMLElement | null>(null);
const layoutListEl = ref<HTMLElement | null>(null);

/**
 * Move a row, in indices into the *visible* list.
 *
 * The sidebar can have sections hidden, so a drop between two visible rows is
 * translated back into a position in the whole order: what the user sees moved
 * past is what it is moved past, hidden sections and all.
 */
function moveRow(visible: readonly string[], order: readonly string[], from: number, to: number) {
  const moved = visible[from];
  const landing = visible[Math.min(Math.max(to, 0), visible.length - 1)];
  if (!moved || !landing || moved === landing) return [-1, -1] as const;
  return [order.indexOf(moved), order.indexOf(landing)] as const;
}

function moveStage(from: number, to: number) {
  const [fromIndex, toIndex] = moveRow(chainStages.value, fx.value.chainOrder, from, to);
  if (fromIndex < 0 || toIndex < 0) return;
  if (isPreset.value) presetEditor.moveChainStage(fromIndex, toIndex);
  else void mixer.moveChainStage(fromIndex, toIndex);
}

function moveLayout(from: number, to: number) {
  const [fromIndex, toIndex] = moveRow(layoutSections.value, fx.value.layoutOrder, from, to);
  if (fromIndex < 0 || toIndex < 0) return;
  if (isPreset.value) presetEditor.moveLayoutSection(fromIndex, toIndex);
  else void mixer.moveLayoutSection(fromIndex, toIndex);
}

const chainDrag = useDragReorder(chainListEl, moveStage);
const layoutDrag = useDragReorder(layoutListEl, moveLayout);

/** Arrow keys move a row too: a reorder only a pointer can do is not one
 *  everybody can do. */
function gripKey(event: KeyboardEvent, index: number, move: (from: number, to: number) => void) {
  const delta = event.key === "ArrowUp" ? -1 : event.key === "ArrowDown" ? 1 : 0;
  if (delta === 0) return;
  event.preventDefault();
  move(index, index + delta);
}

const MAX_CROSSFADE_SECS = 12;
const crossfadeSettings = computed(() => fx.value.crossfade);

function setSection<K extends Section>(section: K, value: MixerSettings[K]) {
  if (isPreset.value) presetEditor.setSection(section, value);
  else void mixer.setSection(section, value);
}

/**
 * A change from one section's controls. They emit a patch naming their own
 * section rather than writing anything themselves, so the same controls serve
 * this panel and the master mixer's rack.
 */
function onControlChange(patch: MixerSettings) {
  for (const [section, value] of Object.entries(patch)) {
    setSection(section as Section, value as MixerSettings[Section]);
  }
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

/** The enable toggle an effect's heading carries. */
function setSectionEnabled(
  section: "eq" | "delay" | "reverb" | "lofi" | "normalisation",
  enabled: boolean,
) {
  setSection(section, { ...fx.value[section], enabled });
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

      <!-- The chain, in the order it is applied ---------------------------->
      <div v-if="chainStages.length" class="panel__chain">
        <SectionHeader
          v-if="canReorder"
          :title="SECTION_LABELS.chainOrder"
          :overridden="overridden('chainOrder')"
          :can-override="canOverride"
          @clear="clearSection('chainOrder')"
        >
          <div class="panel__spacer" />
          <span class="panel__global-note">Top is applied first</span>
        </SectionHeader>
        <p v-if="canReorder" class="panel__hint panel__hint--lead">
          These run in the order they are listed. Drag one by its handle, or focus a
          handle and use the arrow keys, to change what the signal meets first.
        </p>

        <ul ref="chainListEl" class="panel__chain-list">
          <template v-for="(stage, index) in chainStages" :key="stage">
            <li
              v-if="chainDrag.isDragging.value && chainDrag.dropAt.value === index"
              class="panel__chain-drop"
              aria-hidden="true"
            />
            <li
              data-row
              class="panel__chain-row"
              :class="{ 'is-lifted': chainDrag.dragFrom.value === index }"
            >
              <section class="panel__section" :data-testid="`${stage}-section`">
                <SectionHeader
                  :title="SECTION_LABELS[stage]"
                  :overridden="overridden(stage)"
                  :can-override="canOverride && !isEqPreset"
                  @clear="clearSection(stage)"
                >
                  <template v-if="canReorder" #lead>
                    <button
                      class="panel__grip"
                      type="button"
                      :aria-label="`Reorder ${SECTION_LABELS[stage]}`"
                      :title="`Drag, or use the arrow keys, to move ${SECTION_LABELS[stage]} along the chain`"
                      @pointerdown="chainDrag.onHandleDown($event, index)"
                      @pointermove="chainDrag.onHandleMove"
                      @pointerup="chainDrag.onHandleUp"
                      @pointercancel="chainDrag.onHandleCancel"
                      @keydown="gripKey($event, index, moveStage)"
                    >
                      <PnmIcon name="grip" :size="15" />
                    </button>
                  </template>
                  <template v-if="stage === 'eq'">
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
                  </template>
                  <template v-if="stage === 'delay'">
                      <div class="panel__spacer" />
                      <AppToggle
                        :model-value="fx.delay.enabled"
                        label="Enable delay"
                        @update:model-value="setSectionEnabled('delay', $event)"
                      />
                  </template>
                  <template v-if="stage === 'reverb'">
                      <div class="panel__spacer" />
                      <AppToggle
                        :model-value="fx.reverb.enabled"
                        label="Enable reverb"
                        @update:model-value="setSectionEnabled('reverb', $event)"
                      />
                  </template>
                  <template v-if="stage === 'lofi'">
                      <div class="panel__spacer" />
                      <AppToggle
                        :model-value="fx.lofi.enabled"
                        label="Enable lo-fi"
                        @update:model-value="setSectionEnabled('lofi', $event)"
                      />
                  </template>
                </SectionHeader>

                <EffectControls
                  :section="stage"
                  :fx="fx"
                  :block-target="isBlockTarget"
                  @change="onControlChange"
                />
              </section>
            </li>
          </template>
          <li
            v-if="chainDrag.isDragging.value && chainDrag.dropAt.value === chainStages.length"
            class="panel__chain-drop"
            aria-hidden="true"
          />
        </ul>
      </div>

      <!-- Everything that is not a stage of the chain ---------------------->
      <div v-if="layoutSections.length" class="panel__chain panel__layout">
        <SectionHeader
          :title="SECTION_LABELS.layoutOrder"
          :overridden="overridden('layoutOrder')"
          :can-override="canOverride"
          @clear="clearSection('layoutOrder')"
        >
          <div class="panel__spacer" />
          <span class="panel__global-note">Order is layout only</span>
        </SectionHeader>
        <p class="panel__hint panel__hint--lead">
          None of these is a stage the signal passes through — varispeed happens as
          the file is read, the gain ride comes after the effects, a crossfade
          belongs to the join between two songs, and a bed is laid over the top. Move
          them to suit how you work; the sound does not change.
        </p>

        <ul ref="layoutListEl" class="panel__chain-list panel__layout-list">
          <template v-for="(section, index) in layoutSections" :key="section">
            <li
              v-if="layoutDrag.isDragging.value && layoutDrag.dropAt.value === index"
              class="panel__chain-drop"
              aria-hidden="true"
            />
            <li
              data-row
              class="panel__chain-row"
              :class="{ 'is-lifted': layoutDrag.dragFrom.value === index }"
            >
              <section class="panel__section" :data-testid="`${section}-section`">
                <SectionHeader
                  :title="SECTION_LABELS[section]"
                  :overridden="overridden(section)"
                  :can-override="canOverride"
                  @clear="clearSection(section)"
                >
                  <template v-if="canReorderLayout" #lead>
                    <button
                      class="panel__grip"
                      type="button"
                      :aria-label="`Reorder ${SECTION_LABELS[section]}`"
                      :title="`Drag, or use the arrow keys, to move ${SECTION_LABELS[section]} up or down the panel`"
                      @pointerdown="layoutDrag.onHandleDown($event, index)"
                      @pointermove="layoutDrag.onHandleMove"
                      @pointerup="layoutDrag.onHandleUp"
                      @pointercancel="layoutDrag.onHandleCancel"
                      @keydown="gripKey($event, index, moveLayout)"
                    >
                      <PnmIcon name="grip" :size="15" />
                    </button>
                  </template>
                  <template v-if="section === 'normalisation'">
                      <div class="panel__spacer" />
                      <AppToggle
                        :model-value="fx.normalisation.enabled"
                        label="Enable normalisation"
                        @update:model-value="setSectionEnabled('normalisation', $event)"
                      />
                  </template>
                  <template v-if="section === 'crossfade'">
                      <div class="panel__spacer" />
                      <span v-if="!isPreset && mixer.target.kind === 'playlist'" class="panel__global-note">
                        Applies to this playlist
                      </span>
                  </template>
                </SectionHeader>

                <template v-if="section === 'pitch'">
                  <EffectControls section="pitch" :fx="fx" :block-target="isBlockTarget" @change="onControlChange" />
                </template>
                <template v-if="section === 'normalisation'">
                  <EffectControls section="normalisation" :fx="fx" :block-target="isBlockTarget" @change="onControlChange" />
                </template>
                <template v-if="section === 'crossfade'">
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
                </template>
                <template v-if="section === 'filters'">
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
                </template>
              </section>
            </li>
          </template>
          <li
            v-if="layoutDrag.isDragging.value && layoutDrag.dropAt.value === layoutSections.length"
            class="panel__chain-drop"
            aria-hidden="true"
          />
        </ul>
      </div>
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

/* The chain's rows carry the hairline themselves, so the sections inside them
   do not draw a second one. */
.panel__chain-list {
  list-style: none;
  margin: 0;
  padding: 0;
}

.panel__layout {
  border-top: 1px solid var(--separator);
  padding-top: 16px;
}

.panel__chain-row + .panel__chain-row {
  margin-top: 14px;
  border-top: 1px solid var(--separator);
  padding-top: 14px;
}

.panel__chain-row .panel__section {
  border-top: 0;
  padding-top: 0;
}

.panel__chain-row.is-lifted {
  opacity: 0.45;
}

/* The insertion marker the queue and the sidebar both use: a hairline in the
   gap the row will land in, not a filled row of its own. */
.panel__chain-drop {
  height: 2px;
  margin: 6px 0;
  border-radius: 2px;
  background: var(--accent);
}

.panel__grip {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 20px;
  height: 26px;
  margin-left: -5px;
  color: var(--text-tertiary);
  cursor: grab;
  touch-action: none;
}

.panel__grip:active {
  cursor: grabbing;
}

.panel__hint--lead {
  margin: -2px 0 12px;
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

</style>
