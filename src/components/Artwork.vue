<script setup lang="ts">
/** Cover art with a graceful fallback when a track has none. */
import { computed, ref, watch } from "vue";
import PnmIcon from "./icons/PnmIcon.vue";
import { artUrl } from "@/lib/format";

const props = withDefaults(
  defineProps<{ artworkId?: string | null; size?: number; radius?: number; shadow?: boolean }>(),
  { size: 44, radius: 6, shadow: false },
);

const failed = ref(false);
const src = computed(() => (failed.value ? null : artUrl(props.artworkId)));

// A new track means a new chance for the image to work.
watch(
  () => props.artworkId,
  () => (failed.value = false),
);
</script>

<template>
  <div
    class="artwork"
    :class="{ 'has-shadow': shadow }"
    :style="{ width: `${size}px`, height: `${size}px`, borderRadius: `${radius}px` }"
  >
    <img
      v-if="src"
      :src="src"
      alt=""
      loading="lazy"
      decoding="async"
      draggable="false"
      @error="failed = true"
    />
    <PnmIcon v-else name="music" :size="Math.max(14, size * 0.36)" class="artwork__fallback" />
  </div>
</template>

<style scoped>
.artwork {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  flex: none;
  overflow: hidden;
  background: var(--art-placeholder);
  box-shadow: inset 0 0 0 0.5px var(--separator);
}

.artwork.has-shadow {
  box-shadow: var(--shadow-art), inset 0 0 0 0.5px var(--separator);
}

.artwork img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.artwork__fallback {
  color: var(--text-tertiary);
  opacity: 0.7;
}
</style>
