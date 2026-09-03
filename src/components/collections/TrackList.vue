<script setup lang="ts">
/**
 * A list of tracks: `TrackRow` wiring (current/playing state, play, menu,
 * optional mixer), a compact empty state, and optional grip-based drag
 * reorder. Views no longer hand-roll the `v-for` + `TrackRow` plumbing.
 *
 * Rows are only wrapped in a flex container when reorderable, so the plain
 * list keeps `TrackRow`'s own `:last-child` separator behaviour untouched.
 */
import { ref } from "vue";
import PnmIcon from "@/components/icons/PnmIcon.vue";
import TrackRow from "@/components/media/TrackRow.vue";
import EmptyState from "@/components/ui/EmptyState.vue";
import { useDragReorder } from "@/lib/dragReorder";
import type { Track } from "@/lib/types";

/** One entry in the list: a resolved track, or a playlist entry that did not match. */
export interface TrackListItem {
  /** Stable key when the same track appears twice (playlist positions). */
  key?: string;
  track: Track | null;
  /** Shown when the entry has no match in this library. */
  fallbackTitle?: string;
  fallbackSubtitle?: string;
  /** Per-row mixer override, drawn as a dot on the mixer button. */
  mixerOverride?: boolean;
}

const props = withDefaults(
  defineProps<{
    items: TrackListItem[];
    /** Id of the playing track, or null when this list owns none of it. */
    currentId: string | null;
    playing?: boolean;
    showArtwork?: boolean;
    showMixer?: boolean;
    /** Number rows by track number (albums) rather than position. */
    numbered?: boolean;
    /** Compact empty-state message when there is nothing to list. */
    emptyMessage?: string;
    /** Show drag grips and emit `reorder` instead of leaving rows static. */
    reorderable?: boolean;
  }>(),
  {
    playing: false,
    showArtwork: false,
    showMixer: false,
    numbered: false,
    emptyMessage: undefined,
    reorderable: false,
  },
);

const emit = defineEmits<{
  play: [index: number];
  menu: [event: MouseEvent, index: number];
  mixer: [event: MouseEvent, index: number];
  reorder: [from: number, to: number];
}>();

const listEl = ref<HTMLElement | null>(null);
const { dragFrom, dropAt, isDragging, onHandleDown, onHandleMove, onHandleUp, onHandleCancel } =
  useDragReorder(listEl, (from, to) => emit("reorder", from, to));

function rowProps(item: TrackListItem, index: number) {
  return {
    track: item.track,
    fallbackTitle: item.fallbackTitle,
    fallbackSubtitle: item.fallbackSubtitle,
    hasMixerOverride: item.mixerOverride ?? false,
    showArtwork: props.showArtwork,
    showMixer: props.showMixer,
    index: props.numbered ? (item.track?.trackNumber ?? index + 1) : null,
    current: props.currentId !== null && item.track?.id === props.currentId,
    playing: props.playing,
  };
}
</script>

<template>
  <div ref="listEl" class="track-list" :class="{ 'is-dragging': isDragging }">
    <template v-for="(item, index) in items" :key="item.key ?? item.track?.id ?? index">
      <!-- Insertion marker, rather than animating every row out of the way. -->
      <div v-if="reorderable && isDragging && dropAt === index" class="track-list__drop" />

      <div
        v-if="reorderable"
        data-row
        class="track-list__row"
        :class="{ 'is-lifted': dragFrom === index }"
      >
        <button
          class="track-list__grip"
          title="Drag to reorder"
          aria-label="Drag to reorder"
          @pointerdown="onHandleDown($event, index)"
          @pointermove="onHandleMove"
          @pointerup="onHandleUp"
          @pointercancel="onHandleCancel"
        >
          <PnmIcon name="grip" :size="15" />
        </button>

        <TrackRow
          class="track-list__row-track"
          v-bind="rowProps(item, index)"
          @play="emit('play', index)"
          @mixer="emit('mixer', $event, index)"
          @menu="emit('menu', $event, index)"
        />
      </div>

      <TrackRow
        v-else
        v-bind="rowProps(item, index)"
        @play="emit('play', index)"
        @mixer="emit('mixer', $event, index)"
        @menu="emit('menu', $event, index)"
      />
    </template>

    <div v-if="reorderable && isDragging && dropAt === items.length" class="track-list__drop" />

    <EmptyState v-if="items.length === 0 && emptyMessage" compact :message="emptyMessage" />
  </div>
</template>

<style scoped>
.track-list.is-dragging {
  cursor: grabbing;
}

.track-list__row {
  display: flex;
  align-items: center;
  gap: 2px;
}

.track-list__row.is-lifted {
  opacity: 0.4;
}

.track-list__row-track {
  flex: 1;
  min-width: 0;
}

.track-list__grip {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  flex: none;
  color: var(--text-tertiary);
  cursor: grab;
  opacity: 0;
  touch-action: none;
  transition: opacity 0.12s var(--ease);
}

.track-list__row:hover .track-list__grip,
.track-list__grip:focus-visible {
  opacity: 1;
}

.track-list__grip:active {
  cursor: grabbing;
}

/* Insertion marker, rather than animating every row out of the way. */
.track-list__drop {
  height: 2px;
  margin: 1px 8px;
  border-radius: 2px;
  background: var(--accent);
}
</style>
