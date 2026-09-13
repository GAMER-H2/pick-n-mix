<script setup lang="ts">
/**
 * The Master Mixer's effect rack: one region's effects, laid out left to right
 * in the order they are applied.
 *
 * The arrangement is a mini-DAW, so its effects read like one — devices along
 * the bottom rather than a sidebar of sections, leftmost first, with a small
 * stereo meter in every gap. Those meters are the point of the layout: what
 * one effect does to the level is visible as the difference between the meter
 * before it and the meter after it, which is the thing a sidebar of knobs
 * cannot show.
 *
 * The readings come from the engine and only while the mix is being
 * auditioned; stopped, they rest at the floor rather than showing a level that
 * is not being played. The chain is metered at every stage whether or not a
 * device is in the rack for it, so the reading either side of a device is the
 * reading either side of that stage of the engine's own chain.
 */
import { computed, onBeforeUnmount, ref, watch } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import AppToggle from "../ui/AppToggle.vue";
import EffectControls from "../mixer/EffectControls.vue";
import EqModal from "../mixer/EqModal.vue";
import FilterGrid from "../mixer/FilterGrid.vue";
import MixerPresetSelect from "../mixer/PresetSelect.vue";
import BlockEffectsMenu from "./BlockEffectsMenu.vue";
import StereoLevelMeter from "./StereoLevelMeter.vue";
import * as api from "@/lib/api";
import {
  DEFAULT_CHAIN_ORDER,
  PINNED_DEVICES,
  SECTION_LABELS,
  deviceDefault,
  hasEnableSwitch,
} from "@/lib/mixer";
import { useMasterMixStore } from "@/stores/masterMix";
import { useMixerStore } from "@/stores/mixer";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";
import type { DeviceSection } from "@/lib/mixer";
import type { ChainStage, Eq, MixerSettings, OutputLevelFrame } from "@/lib/types";

const props = defineProps<{
  /** The block being edited. The mixer store is already pointed at it. */
  blockId: string;
  blockName: string;
}>();

const store = useMasterMixStore();
const mixer = useMixerStore();
const player = usePlayerStore();
const ui = useUiStore();

const FLOOR_DB = -60;
const SILENCE: OutputLevelFrame = {
  levelsDb: [FLOOR_DB, FLOOR_DB],
  peaksDb: [FLOOR_DB, FLOOR_DB],
  floorDb: FLOOR_DB,
};

const fx = computed(() => mixer.effective);
/** Devices this block has, which is exactly the sections its own layer sets. */
const present = computed<DeviceSection[]>(() =>
  mixer.overriddenSections.filter((section): section is DeviceSection =>
    (DEFAULT_CHAIN_ORDER as string[]).concat(PINNED_DEVICES).includes(section),
  ),
);
/** The chain devices, in the order the engine applies them. */
const chain = computed<ChainStage[]>(() =>
  fx.value.chainOrder.filter((stage) => present.value.includes(stage)),
);
/**
 * The devices that are not chain stages, in the order the user has put them
 * in. Layout only — varispeed happens as the file is read, the gain ride comes
 * after the effects and a bed is laid over the region — but where they sit in
 * the row is still worth being able to decide.
 */
const pinned = computed<DeviceSection[]>(() =>
  (mixer.effective.layoutOrder as DeviceSection[]).filter(
    (section) => PINNED_DEVICES.includes(section) && present.value.includes(section),
  ),
);

/** The expanded EQ, opened from the EQ device the same way the sidebar does. */
const eqExpanded = ref(false);
const deviceRate = computed(() => player.snapshot.deviceSampleRate);

function onEq(eq: Eq) {
  void mixer.setSection("eq", eq);
}

// -- meters ------------------------------------------------------------------

const stageLevels = ref<OutputLevelFrame[]>([]);
let animationFrame: number | null = null;
let stopped = false;

/**
 * The reading at the point in the chain a meter sits at.
 *
 * `index` counts the rack's own gaps: 0 is what reaches the first device, and
 * `n` is what leaves the nth. Those are translated into taps on the engine's
 * full chain, so a gap that spans a stage with no device in the rack still
 * reads the level at the right place.
 */
function levelAt(index: number): OutputLevelFrame {
  const stage = chain.value[index] ?? chain.value[index - 1];
  if (!stage) return SILENCE;
  const tap = fx.value.chainOrder.indexOf(stage) + (index < chain.value.length ? 0 : 1);
  return stageLevels.value[tap] ?? SILENCE;
}

function meterScope(index: number): string {
  if (index === 0) return `entering ${SECTION_LABELS[chain.value[0]]}`;
  const previous = SECTION_LABELS[chain.value[index - 1]];
  const next = chain.value[index];
  return next ? `between ${previous} and ${SECTION_LABELS[next]}` : `leaving ${previous}`;
}

async function poll() {
  if (stopped) return;
  try {
    const frame = await api.chainLevelFrame();
    // A frame for another block is one the engine published just before the
    // selection moved; drawing it would put another region's levels in this
    // region's rack.
    if (!stopped && frame.blockId === props.blockId) stageLevels.value = frame.stages;
  } catch {
    // A missed frame is harmless: the last reading stands until the next one.
  }
  if (!stopped) animationFrame = requestAnimationFrame(() => void poll());
}

function stopPolling() {
  if (animationFrame !== null) cancelAnimationFrame(animationFrame);
  animationFrame = null;
}

watch(
  () => props.blockId,
  (blockId) => {
    stageLevels.value = [];
    void api.setChainMeterBlock(blockId).catch(() => {});
  },
  { immediate: true },
);

/**
 * Poll only while the mix is sounding. Stopped, the engine's own meters fall
 * to the floor and stay there, so there is nothing to ask it for.
 */
watch(
  () => store.previewing && !store.previewPaused,
  (sounding) => {
    if (sounding) {
      if (animationFrame === null) void poll();
    } else {
      stopPolling();
    }
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  stopped = true;
  stopPolling();
  void api.setChainMeterBlock(null).catch(() => {});
});

// -- editing -----------------------------------------------------------------

async function add(section: DeviceSection) {
  await mixer.setSection(section, deviceDefault(section));
  ui.notify(`${SECTION_LABELS[section]} added to ${props.blockName}`);
}

function onChange(patch: MixerSettings) {
  for (const [section, value] of Object.entries(patch)) {
    void mixer.setSection(section as DeviceSection, value as MixerSettings[DeviceSection]);
  }
}

function enabled(section: DeviceSection): boolean {
  const value = fx.value[section];
  return typeof value === "object" && value !== null && "enabled" in value
    ? Boolean(value.enabled)
    : true;
}

function setEnabled(section: DeviceSection, on: boolean) {
  const value = fx.value[section];
  if (typeof value !== "object" || value === null || Array.isArray(value)) return;
  void mixer.setSection(section, { ...value, enabled: on } as MixerSettings[DeviceSection]);
}

/** Take a device off this block, which is what clearing its section means. */
function remove(section: DeviceSection) {
  void mixer.clearSection(section);
}

/** Move a device along the chain, in positions within the rack's own row. */
function move(index: number, delta: number) {
  const order = fx.value.chainOrder;
  const moved = chain.value[index];
  const landing = chain.value[index + delta];
  if (!moved || !landing) return;
  void mixer.moveChainStage(order.indexOf(moved), order.indexOf(landing));
}

/** The same, for the devices that are not in the chain. Layout only. */
function movePinned(index: number, delta: number) {
  const order = mixer.effective.layoutOrder;
  const moved = pinned.value[index];
  const landing = pinned.value[index + delta];
  if (!moved || !landing) return;
  void mixer.moveLayoutSection(order.indexOf(moved), order.indexOf(landing));
}
</script>

<template>
  <section class="rack" aria-label="Effects for the selected block">
    <header class="rack__head">
      <div class="rack__identity">
        <p class="eyebrow">Effect Chain</p>
        <h3 class="rack__title truncate">
          <PnmIcon name="music" :size="11" />
          <span>{{ blockName }}</span>
        </h3>
      </div>
      <span class="rack__note">Left to right is the order they are applied</span>
      <div class="rack__actions">
        <MixerPresetSelect master-mix :stretch="false" />
        <BlockEffectsMenu :present="present" @add="add" />
      </div>
    </header>

    <div class="rack__row scroll-area">
      <p v-if="!present.length" class="rack__empty">
        This block has no effects yet. Add one or choose a mixer preset.
      </p>
      <template v-for="(stage, index) in chain" :key="stage">
        <StereoLevelMeter
          compact
          class="rack__meter"
          :reading="levelAt(index)"
          :label="`Level ${meterScope(index)}`"
          :scope="meterScope(index)"
        />

        <article class="rack__device" :class="{ 'is-off': !enabled(stage) }">
          <header class="rack__device-head">
            <button
              class="rack__arrow"
              type="button"
              :disabled="index === 0"
              :aria-label="`Move ${SECTION_LABELS[stage]} earlier in the chain`"
              :title="`Move ${SECTION_LABELS[stage]} earlier in the chain`"
              @click="move(index, -1)"
            >
              <PnmIcon name="chevronLeft" :size="13" />
            </button>
            <span class="rack__device-name truncate">{{ SECTION_LABELS[stage] }}</span>
            <button
              class="rack__arrow"
              type="button"
              :disabled="index === chain.length - 1"
              :aria-label="`Move ${SECTION_LABELS[stage]} later in the chain`"
              :title="`Move ${SECTION_LABELS[stage]} later in the chain`"
              @click="move(index, 1)"
            >
              <PnmIcon name="chevronRight" :size="13" />
            </button>
            <button
              v-if="stage === 'eq'"
              class="rack__remove"
              type="button"
              aria-label="Expand EQ"
              title="Open the full equaliser for this block"
              @click="eqExpanded = true"
            >
              <PnmIcon name="expand" :size="13" />
            </button>
            <AppToggle
              v-if="hasEnableSwitch(stage)"
              :model-value="enabled(stage)"
              :label="`Enable ${SECTION_LABELS[stage]}`"
              @update:model-value="setEnabled(stage, $event)"
            />
            <button
              class="rack__remove"
              type="button"
              :aria-label="`Remove ${SECTION_LABELS[stage]} from this block`"
              :title="`Remove ${SECTION_LABELS[stage]} from this block`"
              @click="remove(stage)"
            >
              <PnmIcon name="close" :size="12" />
            </button>
          </header>

          <div class="rack__device-body scroll-area">
            <EffectControls :section="stage" :fx="fx" block-target @change="onChange" />
          </div>
        </article>
      </template>

      <StereoLevelMeter
        v-if="chain.length"
        compact
        class="rack__meter"
        :reading="levelAt(chain.length)"
        :label="`Level ${meterScope(chain.length)}`"
        :scope="meterScope(chain.length)"
      />

      <!-- Not stages of the chain, so they are not in it: varispeed happens as
           the file is read, the gain ride comes after the effects, and a bed is
           laid over the region rather than passed through it. -->
      <template v-for="(section, index) in pinned" :key="section">
        <article class="rack__device rack__device--pinned">
          <header class="rack__device-head">
            <button
              class="rack__arrow"
              type="button"
              :disabled="index === 0"
              :aria-label="`Move ${SECTION_LABELS[section]} left`"
              :title="`Move ${SECTION_LABELS[section]} left. These are not in the chain, so this changes the layout and not the sound.`"
              @click="movePinned(index, -1)"
            >
              <PnmIcon name="chevronLeft" :size="13" />
            </button>
            <span class="rack__device-name truncate">{{ SECTION_LABELS[section] }}</span>
            <button
              class="rack__arrow"
              type="button"
              :disabled="index === pinned.length - 1"
              :aria-label="`Move ${SECTION_LABELS[section]} right`"
              :title="`Move ${SECTION_LABELS[section]} right. These are not in the chain, so this changes the layout and not the sound.`"
              @click="movePinned(index, 1)"
            >
              <PnmIcon name="chevronRight" :size="13" />
            </button>
            <AppToggle
              v-if="hasEnableSwitch(section)"
              :model-value="enabled(section)"
              :label="`Enable ${SECTION_LABELS[section]}`"
              @update:model-value="setEnabled(section, $event)"
            />
            <button
              class="rack__remove"
              type="button"
              :aria-label="`Remove ${SECTION_LABELS[section]} from this block`"
              :title="`Remove ${SECTION_LABELS[section]} from this block`"
              @click="remove(section)"
            >
              <PnmIcon name="close" :size="12" />
            </button>
          </header>

          <div class="rack__device-body scroll-area">
            <FilterGrid v-if="section === 'filters'" show-volumes />
            <EffectControls v-else :section="section" :fx="fx" block-target @change="onChange" />
          </div>
        </article>
      </template>
    </div>

    <!-- Edits this block's layer, like every control in the rack does. -->
    <EqModal
      v-if="eqExpanded"
      :eq="fx.eq"
      :target-label="blockName"
      :sample-rate="deviceRate"
      @change="onEq"
      @close="eqExpanded = false"
    />
  </section>
</template>

<style scoped>
.rack {
  display: flex;
  flex-direction: column;
  flex: none;
  border-top: 0.5px solid var(--separator);
  background: var(--bg-sidebar);
}

.rack__head {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 48px;
  padding: 7px 14px;
  border-bottom: 0.5px solid var(--separator);
}

.rack__identity {
  min-width: 0;
}

.rack__title {
  display: flex;
  align-items: center;
  gap: 5px;
  margin: 0;
  font-size: 12.5px;
  font-weight: 600;
}

.rack__note {
  margin-left: auto;
  font-size: 10.5px;
  color: var(--text-tertiary);
  white-space: nowrap;
}

.rack__actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.rack__row {
  display: flex;
  align-items: stretch;
  gap: 8px;
  height: 196px;
  padding: 10px 14px;
  overflow-x: auto;
  overflow-y: hidden;
}

.rack__empty {
  align-self: center;
  margin: auto;
  color: var(--text-tertiary);
  font-size: 12px;
}

.rack__meter {
  align-self: center;
  height: 116px;
}

.rack__device {
  display: flex;
  flex-direction: column;
  flex: none;
  width: 316px;
  border: 0.5px solid var(--separator-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-elevated);
}

.rack__device.is-off {
  opacity: 0.6;
}

.rack__device--pinned {
  border-style: dashed;
}

.rack__device-head {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 5px 6px 5px 8px;
  border-bottom: 0.5px solid var(--separator);
}

.rack__device-name {
  flex: 1;
  min-width: 0;
  font-size: 11.5px;
  font-weight: 600;
}

.rack__arrow,
.rack__remove {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 22px;
  height: 26px;
  border-radius: var(--radius-sm);
  color: var(--text-tertiary);
}

.rack__arrow:hover:not(:disabled),
.rack__remove:hover {
  background: var(--bg-hover);
  color: var(--text);
}

.rack__arrow:disabled {
  opacity: 0.3;
  cursor: default;
}

.rack__device-body {
  flex: 1;
  min-height: 0;
  padding: 8px 10px 10px;
  overflow-y: auto;
}
</style>
