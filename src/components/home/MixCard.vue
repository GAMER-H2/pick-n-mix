<script setup lang="ts">
/**
 * One generated mix, as a large square card.
 *
 * A mix with too little behind it is shown but disabled rather than hidden:
 * the three mixes are a fixed row, and dropping one out would leave a gap the
 * listener cannot explain. Saying "not enough listening yet" is more useful.
 */
import { computed } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import Artwork from "../Artwork.vue";
import type { MixSummary } from "@/lib/types";

const props = defineProps<{ mix: MixSummary; ready: boolean }>();
defineEmits<{ open: []; play: []; pin: []; menu: [event: MouseEvent] }>();

/**
 * Up to four covers, as a quilt. Fewer than four is drawn as a single cover
 * rather than an unbalanced grid.
 */
const covers = computed(() => props.mix.artworkIds.slice(0, 4));
const isQuilt = computed(() => covers.value.length >= 4);
</script>

<template>
  <div
    class="mix"
    :class="{ 'is-empty': !ready }"
    @contextmenu.prevent="ready && $emit('menu', $event)"
  >
    <button
      class="mix__art"
      :disabled="!ready"
      :title="ready ? `Open ${mix.name}` : 'Not enough listening yet'"
      @click="$emit('open')"
    >
      <div v-if="isQuilt" class="mix__quilt">
        <Artwork v-for="id in covers" :key="id" :artwork-id="id" :size="80" :radius="0" />
      </div>
      <Artwork v-else :artwork-id="covers[0] ?? null" :size="160" :radius="0" />

      <span v-if="ready" class="mix__play" @click.stop="$emit('play')">
        <PnmIcon name="play" :size="18" />
      </span>

      <button
        v-if="ready"
        class="mix__pin"
        :class="{ 'is-on': mix.pinned }"
        :title="mix.pinned ? 'Unpin from the sidebar' : 'Pin to the sidebar'"
        :aria-label="mix.pinned ? 'Unpin from the sidebar' : 'Pin to the sidebar'"
        @click.stop="$emit('pin')"
      >
        <PnmIcon name="addToPlaylist" :size="14" />
      </button>
    </button>

    <div class="mix__name">{{ mix.name }}</div>
    <div class="mix__meta truncate">
      {{ ready ? mix.description : "Not enough listening yet" }}
    </div>
  </div>
</template>

<style scoped>
.mix {
  display: flex;
  flex-direction: column;
  align-items: center;
  min-width: 0;
  text-align: center;
}

.mix__art {
  position: relative;
  width: 100%;
  aspect-ratio: 1;
  border-radius: var(--radius);
  overflow: hidden;
  background: var(--bg-sunken);
  border: 0.5px solid var(--separator);
  box-shadow: var(--shadow-art);
  transition: transform 0.18s var(--ease);
}

.mix:not(.is-empty) .mix__art:hover {
  transform: translateY(-2px);
}

.mix.is-empty .mix__art {
  box-shadow: none;
  opacity: 0.5;
  cursor: default;
}

.mix__art :deep(.artwork) {
  width: 100% !important;
  height: 100% !important;
  box-shadow: none;
}

.mix__quilt {
  display: grid;
  grid-template-columns: 1fr 1fr;
  width: 100%;
  height: 100%;
}

.mix__quilt :deep(.artwork) {
  width: 100% !important;
  height: 100% !important;
  aspect-ratio: 1;
}

.mix__play {
  position: absolute;
  right: 9px;
  bottom: 9px;
  display: grid;
  place-items: center;
  width: 36px;
  height: 36px;
  border-radius: 50%;
  color: var(--accent-contrast);
  background: var(--accent);
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.35);
  opacity: 0;
  transform: translateY(4px);
  transition: opacity 0.14s var(--ease), transform 0.14s var(--ease);
}

.mix__art:hover .mix__play,
.mix__play:focus-visible {
  opacity: 1;
  transform: none;
}

.mix__pin {
  position: absolute;
  top: 8px;
  right: 8px;
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  color: #fff;
  background: rgba(0, 0, 0, 0.45);
  opacity: 0;
  transition: opacity 0.14s var(--ease);
}

.mix__art:hover .mix__pin,
.mix__pin:focus-visible,
/* A pinned mix keeps its badge visible, since that is the only place the
   state is shown on this page. */
.mix__pin.is-on {
  opacity: 1;
}

.mix__pin.is-on {
  color: var(--accent-contrast);
  background: var(--accent);
}

.mix__name {
  margin-top: 9px;
  font-size: 14px;
  font-weight: 550;
}

.mix__meta {
  margin-top: 1px;
  font-size: 11.5px;
  color: var(--text-secondary);
  max-width: 100%;
}
</style>
