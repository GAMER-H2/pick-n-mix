<script setup lang="ts">
/**
 * One row in a track list, matching the drawings: a play affordance on the
 * left, title over "Album · Artist · Year", and the mixer and overflow buttons
 * on the right.
 */
import { computed } from "vue";
import PnmIcon from "./icons/PnmIcon.vue";
import Artwork from "./Artwork.vue";
import { formatDuration, subtitleFor } from "@/lib/format";
import type { Track } from "@/lib/types";

const props = withDefaults(
  defineProps<{
    track: Track | null;
    /** Shown when a playlist entry has no match in this library. */
    fallbackTitle?: string;
    fallbackSubtitle?: string;
    current?: boolean;
    playing?: boolean;
    /** Per-row mixer override, drawn as a dot on the mixer button. */
    hasMixerOverride?: boolean;
    showArtwork?: boolean;
    showDuration?: boolean;
    /** Only playlists offer a per-song mixer, so this is off by default. */
    showMixer?: boolean;
    index?: number | null;
  }>(),
  {
    current: false,
    playing: false,
    hasMixerOverride: false,
    showArtwork: false,
    showDuration: true,
    showMixer: false,
    index: null,
  },
);

const emit = defineEmits<{
  play: [];
  menu: [event: MouseEvent];
  mixer: [event: MouseEvent];
}>();

const missing = computed(() => props.track === null);
const title = computed(() => props.track?.title ?? props.fallbackTitle ?? "Unknown Track");
const subtitle = computed(() =>
  props.track
    ? subtitleFor([props.track.album, props.track.artist, props.track.year])
    : (props.fallbackSubtitle ?? "Not in your library"),
);
</script>

<template>
  <div
    class="row"
    :class="{ 'is-current': current, 'is-missing': missing }"
    @dblclick="!missing && emit('play')"
    @contextmenu.prevent="emit('menu', $event)"
  >
    <button
      class="row__lead"
      :disabled="missing"
      :aria-label="playing ? 'Pause' : `Play ${title}`"
      @click="emit('play')"
    >
      <PnmIcon v-if="current && playing" name="pause" :size="13" class="row__state" />
      <PnmIcon v-else-if="current" name="play" :size="13" class="row__state" />
      <PnmIcon v-else name="play" :size="13" class="row__play" />
      <span v-if="index !== null" class="row__index">{{ index }}</span>
    </button>

    <Artwork v-if="showArtwork" :artwork-id="track?.artworkId" :size="38" :radius="5" />

    <div class="row__text">
      <div class="row__title truncate">{{ title }}</div>
      <div class="row__subtitle truncate">{{ subtitle }}</div>
    </div>

    <div class="row__actions">
      <span v-if="showDuration && track" class="row__duration">
        {{ formatDuration(track.durationSecs) }}
      </span>

      <button
        v-if="showMixer && !missing"
        class="icon-button row__button"
        :class="{ 'is-active': hasMixerOverride }"
        title="Mixer settings for this song in this playlist"
        aria-label="Mixer settings for this song in this playlist"
        @click.stop="emit('mixer', $event)"
      >
        <PnmIcon name="mixer" :size="17" />
      </button>

      <button
        class="icon-button row__button"
        title="More"
        aria-label="More actions"
        @click.stop="emit('menu', $event)"
      >
        <PnmIcon name="more" :size="17" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.row {
  display: flex;
  align-items: center;
  gap: 12px;
  height: 56px;
  content-visibility: auto;
  contain-intrinsic-block-size: auto 56px;
  padding: 0 10px 0 4px;
  border-radius: var(--radius-sm);
  position: relative;
}

/* Apple Music's inset separator: starts after the leading control. */
.row::after {
  content: "";
  position: absolute;
  left: 34px;
  right: 10px;
  bottom: 0;
  height: 1px;
  background: var(--separator);
}

.row:last-child::after {
  display: none;
}

.row:hover {
  background: var(--bg-hover);
}

.row.is-missing {
  opacity: 0.45;
}

.row__lead {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  flex: none;
  color: var(--text-tertiary);
  border-radius: var(--radius-sm);
}

.row__play {
  opacity: 0;
  transition: opacity 0.12s var(--ease);
}

.row:hover .row__play {
  opacity: 1;
  color: var(--text);
}

.row__state {
  color: var(--accent);
}

/* The number gives way to the play control on hover. */
.row__index {
  position: absolute;
  font-size: 12px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
}

.row:hover .row__index,
.row.is-current .row__index {
  opacity: 0;
}

.row__text {
  flex: 1;
  min-width: 0;
}

.row__title {
  font-size: 13.5px;
  font-weight: 500;
  line-height: 1.3;
}

.row.is-current .row__title {
  color: var(--accent);
}

.row__subtitle {
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.35;
}

.row__actions {
  display: flex;
  align-items: center;
  gap: 2px;
  flex: none;
}

.row__duration {
  font-size: 12px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
  margin-right: 6px;
}

.row__button {
  opacity: 0;
  transition: opacity 0.12s var(--ease);
}

.row:hover .row__button,
.row__button.is-active,
.row__button:focus-visible {
  opacity: 1;
}
</style>
