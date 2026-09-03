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
import PlaylistArtwork from "./PlaylistArtwork.vue";
import Artwork from "./Artwork.vue";
import type { MixSummary } from "@/lib/types";

const props = defineProps<{ mix: MixSummary; ready: boolean }>();
defineEmits<{ open: []; play: []; pin: []; menu: [event: MouseEvent] }>();

/**
 * The mix's picture. The quilt-or-single rule lives in `PlaylistArtwork`, so
 * a mix and a playlist with the same covers always look the same.
 */
const hasCovers = computed(() => props.mix.artworkIds.length > 0);
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
      <PlaylistArtwork
        v-if="hasCovers"
        :artwork-ids="mix.artworkIds"
        :size="160"
        :radius="0"
      />
      <Artwork v-else :artwork-id="null" :size="160" :radius="0" />

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

/* The quilt fills the card exactly, whatever nominal size was asked for. */
.mix__art :deep(.quilt) {
  width: 100% !important;
  height: 100% !important;
  border-radius: 0 !important;
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
