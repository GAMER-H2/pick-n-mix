<script setup lang="ts">
/**
 * The ambience beds ("Filters") from the drawings: rain, TV static and the
 * rest. Each needs an audio file in the app's filters folder; ones with no
 * file are shown but disabled, so it is clear what is missing.
 */
import { computed } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import AppSlider from "../AppSlider.vue";
import { useMixerStore } from "@/stores/mixer";

const props = withDefaults(defineProps<{ showVolumes?: boolean }>(), { showVolumes: false });
const mixer = useMixerStore();

const settingsFor = computed(() => new Map(mixer.effective.filters.map((f) => [f.id, f])));

function isOn(id: string) {
  return settingsFor.value.get(id)?.enabled ?? false;
}

function volumeOf(id: string) {
  return settingsFor.value.get(id)?.volume ?? 0.4;
}
</script>

<template>
  <div>
    <div class="grid">
      <button
        v-for="filter in mixer.filters"
        :key="filter.id"
        class="chip"
        :class="{ 'is-on': isOn(filter.id), 'is-unavailable': !filter.available }"
        :disabled="!filter.available"
        :title="
          filter.available
            ? filter.name
            : `Drop a ${filter.id} audio file into ${mixer.filtersDir} to enable this`
        "
        @click="mixer.toggleFilter(filter.id, !isOn(filter.id))"
      >
        <span class="truncate">{{ filter.name }}</span>
      </button>
    </div>

    <div v-if="props.showVolumes && mixer.effective.filters.some((f) => f.enabled)" class="levels">
      <div
        v-for="filter in mixer.effective.filters.filter((f) => f.enabled)"
        :key="filter.id"
        class="levels__row"
      >
        <span class="levels__name truncate">
          {{ mixer.filters.find((f) => f.id === filter.id)?.name ?? filter.id }}
        </span>
        <AppSlider
          :model-value="volumeOf(filter.id)"
          @update:model-value="mixer.setFilterVolume(filter.id, $event)"
        />
        <span class="levels__value">{{ Math.round(volumeOf(filter.id) * 100) }}%</span>
      </div>
    </div>

    <p v-if="!mixer.filters.some((f) => f.available)" class="hint">
      <PnmIcon name="folder" :size="13" />
      <span>Add audio files to <code>{{ mixer.filtersDir }}</code> to use these.</span>
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
  height: 30px;
  padding: 0 8px;
  border-radius: 999px;
  border: 1px solid var(--separator);
  background: var(--bg-elevated);
  font-size: 11.5px;
  color: var(--text);
  transition: all 0.15s var(--ease);
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

.chip.is-unavailable {
  opacity: 0.4;
  cursor: not-allowed;
}

.levels {
  margin-top: 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.levels__row {
  display: grid;
  grid-template-columns: 74px 1fr 34px;
  align-items: center;
  gap: 8px;
}

.levels__name {
  font-size: 11.5px;
  color: var(--text-secondary);
}

.levels__value {
  font-size: 11px;
  color: var(--text-tertiary);
  text-align: right;
  font-variant-numeric: tabular-nums;
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
