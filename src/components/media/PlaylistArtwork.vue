<script setup lang="ts">
/**
 * A playlist's picture: its own if it has one, otherwise a quilt of the covers
 * of the first four different songs in it.
 *
 * Four or nothing. A quilt of two or three covers is an unbalanced shape that
 * reads as a mistake, so anything short of four falls back to the first cover
 * on its own — which is what a playlist of one album should look like anyway.
 */
import { computed } from "vue";
import Artwork from "../media/Artwork.vue";

const props = withDefaults(
  defineProps<{
    /** The playlist's own image, when the user has chosen one. */
    artwork?: string | null;
    /** Covers to fall back to, most representative first. */
    artworkIds?: string[];
    size?: number;
    radius?: number;
    shadow?: boolean;
  }>(),
  { artwork: null, artworkIds: () => [], size: 44, radius: 6, shadow: false },
);

const covers = computed(() => props.artworkIds.slice(0, 4));
const quilted = computed(() => !props.artwork && covers.value.length >= 4);
</script>

<template>
  <div
    v-if="quilted"
    class="quilt"
    :class="{ 'has-shadow': shadow }"
    :style="{ width: `${size}px`, height: `${size}px`, borderRadius: `${radius}px` }"
  >
    <Artwork
      v-for="id in covers"
      :key="id"
      :artwork-id="id"
      :size="Math.ceil(size / 2)"
      :radius="0"
    />
  </div>
  <Artwork
    v-else
    :artwork-id="artwork ?? covers[0] ?? null"
    :size="size"
    :radius="radius"
    :shadow="shadow"
  />
</template>

<style scoped>
.quilt {
  display: grid;
  grid-template-columns: 1fr 1fr;
  grid-template-rows: 1fr 1fr;
  overflow: hidden;
  background: var(--bg-sunken);
}

.quilt.has-shadow {
  box-shadow: var(--shadow-art);
}

/* The tiles fill their quarter exactly, whatever size was asked for. */
.quilt :deep(.artwork) {
  width: 100% !important;
  height: 100% !important;
  border-radius: 0 !important;
}
</style>
