<script setup lang="ts">
/**
 * The header shared by playlist, album and artist pages: artwork, title,
 * subtitle, and the play / shuffle / mixer row from the drawings.
 */
import PnmIcon from "./icons/PnmIcon.vue";
import Artwork from "./Artwork.vue";

withDefaults(
  defineProps<{
    title: string;
    subtitle?: string;
    meta?: string;
    artworkId?: string | null;
    /** Artists get a round portrait; albums and playlists get a square. */
    round?: boolean;
    showMixer?: boolean;
    mixerActive?: boolean;
    disabled?: boolean;
  }>(),
  { round: false, showMixer: true, mixerActive: false, disabled: false },
);

const emit = defineEmits<{ play: []; shuffle: []; mixer: []; menu: [event: MouseEvent] }>();
</script>

<template>
  <header class="collection">
    <Artwork
      :artwork-id="artworkId"
      :size="188"
      :radius="round ? 94 : 8"
      shadow
      class="collection__art"
    />

    <div class="collection__body">
      <h1 class="collection__title">{{ title }}</h1>
      <p v-if="subtitle" class="collection__subtitle">{{ subtitle }}</p>
      <p v-if="meta" class="collection__meta">{{ meta }}</p>

      <div class="collection__actions">
        <button class="pill-button" :disabled="disabled" @click="emit('play')">
          <PnmIcon name="play" :size="13" />
          <span>Play</span>
        </button>
        <button
          class="pill-button is-secondary"
          :disabled="disabled"
          @click="emit('shuffle')"
        >
          <PnmIcon name="shuffle" :size="14" />
          <span>Shuffle</span>
        </button>
        <button
          v-if="showMixer"
          class="icon-button collection__mixer"
          :class="{ 'is-active': mixerActive }"
          title="Mixer settings for this collection"
          aria-label="Mixer settings for this collection"
          @click="emit('mixer')"
        >
          <PnmIcon name="mixer" :size="18" />
        </button>
        <!-- Actions only one kind of collection has; playlists use it for the
             master mixer, which means nothing on an album or artist page. -->
        <slot name="actions" />
        <button
          class="icon-button"
          title="More"
          aria-label="More actions"
          @click="emit('menu', $event)"
        >
          <PnmIcon name="more" :size="18" />
        </button>
      </div>
    </div>
  </header>
</template>

<style scoped>
.collection {
  display: flex;
  align-items: flex-end;
  gap: 24px;
  padding: 8px 0 26px;
}

.collection__art {
  flex: none;
}

.collection__body {
  flex: 1;
  min-width: 0;
  padding-bottom: 6px;
}

.collection__title {
  margin: 0;
  font-size: 34px;
  font-weight: 700;
  letter-spacing: -0.02em;
  line-height: 1.1;
}

.collection__subtitle {
  margin: 6px 0 0;
  font-size: 15px;
  color: var(--text-secondary);
}

.collection__meta {
  margin: 4px 0 0;
  font-size: 12px;
  color: var(--text-tertiary);
}

.collection__actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 18px;
}
</style>
