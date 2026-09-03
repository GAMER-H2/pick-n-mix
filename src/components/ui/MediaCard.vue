<script setup lang="ts">
/**
 * The standard grid cell: artwork over title over subtitle, hover lift on the
 * artwork, truncation, and a context-menu event for the overflow menu.
 * Every artwork grid in the app (home playlists, library albums and artists)
 * renders through this so the cells stay identical.
 */
defineProps<{
  title: string;
  subtitle?: string;
}>();

const emit = defineEmits<{
  open: [];
  menu: [event: MouseEvent];
}>();
</script>

<template>
  <button
    class="card"
    @click="emit('open')"
    @contextmenu.prevent="emit('menu', $event)"
  >
    <div class="card__art">
      <slot />
    </div>
    <div class="card__title truncate">{{ title }}</div>
    <div v-if="subtitle" class="card__subtitle truncate">{{ subtitle }}</div>
  </button>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  gap: 2px;
  text-align: left;
  min-width: 0;
  content-visibility: auto;
  contain-intrinsic-block-size: auto 196px;
  /* `content-visibility` paints with containment, which clips at this
     element's edge — the hover lift below would lose its top 2px. The
     headroom sits inside the card, and the negative margin keeps the grid
     rhythm unchanged. */
  padding-top: 4px;
  margin-top: -4px;
}

.card__art :deep(.artwork) {
  width: 100% !important;
  height: auto !important;
  aspect-ratio: 1;
  margin-bottom: 8px;
  transition: transform 0.18s var(--ease);
}

.card:hover .card__art :deep(.artwork) {
  transform: translateY(-2px);
}

.card__title {
  font-size: 12.5px;
  font-weight: 500;
}

.card__subtitle {
  font-size: 11.5px;
  color: var(--text-secondary);
}
</style>
