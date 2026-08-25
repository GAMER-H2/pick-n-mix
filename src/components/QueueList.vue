<script setup lang="ts">
/**
 * The queue, shared by the side panel and the full-screen view.
 *
 * Rows are reordered by dragging a handle rather than the row itself, so a
 * drag can never be mistaken for a click on the song.
 */
import { computed, ref } from "vue";
import PnmIcon from "./icons/PnmIcon.vue";
import Artwork from "./Artwork.vue";
import { formatDuration, subtitleFor } from "@/lib/format";
import type { Track } from "@/lib/types";

withDefaults(
  defineProps<{
    items: Track[];
    currentIndex: number | null;
    playing?: boolean;
    /** Larger rows for the full-screen view. */
    roomy?: boolean;
  }>(),
  { playing: false, roomy: false },
);

const emit = defineEmits<{
  play: [index: number];
  remove: [index: number];
  move: [from: number, to: number];
  menu: [index: number, event: MouseEvent];
}>();

const listEl = ref<HTMLElement | null>(null);
const dragFrom = ref<number | null>(null);
const dropAt = ref<number | null>(null);

const isDragging = computed(() => dragFrom.value !== null);

/** Which gap the pointer is currently over, 0..items.length. */
function gapAt(clientY: number): number {
  const container = listEl.value;
  if (!container) return 0;
  const rows = Array.from(container.querySelectorAll<HTMLElement>("[data-row]"));
  for (let i = 0; i < rows.length; i += 1) {
    const rect = rows[i].getBoundingClientRect();
    if (clientY < rect.top + rect.height / 2) return i;
  }
  return rows.length;
}

function onHandleDown(event: PointerEvent, index: number) {
  event.preventDefault();
  dragFrom.value = index;
  dropAt.value = index;
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
}

function onHandleMove(event: PointerEvent) {
  if (dragFrom.value === null) return;
  dropAt.value = gapAt(event.clientY);
}

function onHandleUp(event: PointerEvent) {
  const from = dragFrom.value;
  const gap = dropAt.value;
  (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
  dragFrom.value = null;
  dropAt.value = null;
  if (from === null || gap === null) return;

  // A gap index above the dragged row means the row shifts down by one.
  const to = gap > from ? gap - 1 : gap;
  if (to !== from) emit("move", from, to);
}

function onHandleCancel() {
  dragFrom.value = null;
  dropAt.value = null;
}
</script>

<template>
  <div ref="listEl" class="queue-list" :class="{ 'is-roomy': roomy, 'is-dragging': isDragging }">
    <template v-for="(track, index) in items" :key="`${track.id}-${index}`">
      <div v-if="dropAt === index && isDragging" class="queue-list__drop" />

      <div
        data-row
        class="row"
        :class="{ 'is-current': index === currentIndex, 'is-lifted': dragFrom === index }"
        @dblclick="emit('play', index)"
        @contextmenu.prevent="emit('menu', index, $event)"
      >
        <button
          class="row__grip"
          title="Drag to reorder"
          aria-label="Drag to reorder"
          @pointerdown="onHandleDown($event, index)"
          @pointermove="onHandleMove"
          @pointerup="onHandleUp"
          @pointercancel="onHandleCancel"
        >
          <PnmIcon name="grip" :size="15" />
        </button>

        <button
          class="row__art"
          :aria-label="`Play ${track.title}`"
          :title="`Play ${track.title}`"
          @click="emit('play', index)"
        >
          <Artwork :artwork-id="track.artworkId" :size="roomy ? 44 : 38" :radius="5" />
          <span class="row__overlay">
            <PnmIcon
              :name="index === currentIndex && playing ? 'pause' : 'play'"
              :size="roomy ? 16 : 14"
            />
          </span>
        </button>

        <div class="row__text">
          <div class="row__title truncate">{{ track.title }}</div>
          <div class="row__subtitle truncate">
            {{ subtitleFor([track.artist, track.album]) }}
          </div>
        </div>

        <span class="row__duration">{{ formatDuration(track.durationSecs) }}</span>

        <button
          class="icon-button row__remove"
          aria-label="Remove from queue"
          title="Remove from queue"
          @click.stop="emit('remove', index)"
        >
          <PnmIcon name="close" :size="15" />
        </button>
      </div>
    </template>

    <div v-if="dropAt === items.length && isDragging" class="queue-list__drop" />
  </div>
</template>

<style scoped>
.queue-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.queue-list.is-dragging {
  cursor: grabbing;
}

/* Insertion marker, rather than animating every row out of the way. */
.queue-list__drop {
  height: 2px;
  margin: 1px 8px;
  border-radius: 2px;
  background: var(--accent);
}

.row {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 5px 6px;
  border-radius: var(--radius-sm);
}

.row:hover {
  background: var(--bg-hover);
}

.row.is-lifted {
  opacity: 0.4;
}

.row__grip {
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

.row:hover .row__grip,
.row__grip:focus-visible {
  opacity: 1;
}

.row__grip:active {
  cursor: grabbing;
}

.row__art {
  position: relative;
  flex: none;
  border-radius: 5px;
  overflow: hidden;
  display: block;
}

.row__overlay {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  background: rgba(0, 0, 0, 0.45);
  opacity: 0;
  transition: opacity 0.12s var(--ease);
}

.row:hover .row__overlay,
.row.is-current .row__overlay {
  opacity: 1;
}

.row.is-current .row__overlay {
  color: var(--accent);
  background: rgba(0, 0, 0, 0.55);
}

.row__text {
  flex: 1;
  min-width: 0;
}

.row__title {
  font-size: 12.5px;
  font-weight: 500;
}

.is-roomy .row__title {
  font-size: 13.5px;
}

.row.is-current .row__title {
  color: var(--accent);
}

.row__subtitle {
  font-size: 11px;
  color: var(--text-secondary);
}

.is-roomy .row__subtitle {
  font-size: 12px;
}

.row__duration {
  font-size: 11px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
  flex: none;
}

.row__remove {
  width: 24px;
  height: 24px;
  flex: none;
  opacity: 0;
}

.row:hover .row__remove,
.row__remove:focus-visible {
  opacity: 1;
}
</style>
