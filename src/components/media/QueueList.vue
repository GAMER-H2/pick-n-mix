<script setup lang="ts">
/**
 * The queue, shared by the side panel and the full-screen view.
 *
 * Rows are reordered by dragging a handle rather than the row itself, so a
 * drag can never be mistaken for a click on the song.
 */
import { nextTick, onMounted, ref } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import Artwork from "../media/Artwork.vue";
import PlaylistArtwork from "../media/PlaylistArtwork.vue";
import { formatDuration, subtitleFor } from "@/lib/format";
import { useDragReorder } from "@/lib/dragReorder";
import type { QueueMix, Track } from "@/lib/types";

/**
 * One row.
 *
 * A track may be null — the history list shows songs that have since left the
 * library — while a mix is always whole: it is the playlist block, and the
 * songs listed inside it are chapters of one entry rather than rows of their
 * own. That is what stops anything being dropped into the middle of a mix.
 */
export type QueueRow =
  | { kind: "track"; track: Track | null }
  | { kind: "mix"; mix: QueueMix };

const props = withDefaults(
  defineProps<{
    items: QueueRow[];
    currentIndex: number | null;
    playing?: boolean;
    /** Larger rows for the full-screen view. */
    roomy?: boolean;
    reorderable?: boolean;
    removable?: boolean;
    removeLabel?: string;
    /** Where the current row has got to, for marking a mix's chapters. */
    positionSecs?: number;
  }>(),
  {
    playing: false,
    positionSecs: 0,
    roomy: false,
    reorderable: true,
    removable: true,
    removeLabel: "Remove from queue",
  },
);

const emit = defineEmits<{
  /** `positionSecs` is set when a chapter inside a mix was clicked. */
  play: [index: number, positionSecs?: number];
  remove: [index: number];
  move: [from: number, to: number];
  menu: [index: number, event: MouseEvent];
}>();

defineSlots<{
  subtitle(props: { track: Track | null; index: number }): unknown;
  meta(props: { track: Track | null; index: number }): unknown;
}>();

const listEl = ref<HTMLElement | null>(null);
const { dragFrom, dropAt, isDragging, onHandleDown, onHandleMove, onHandleUp, onHandleCancel } =
  useDragReorder(listEl, (from, to) => {
    if (props.reorderable) emit("move", from, to);
  });

function play(track: Track | null, index: number) {
  if (track) emit("play", index);
}

function openMenu(track: Track | null, index: number, event: MouseEvent) {
  if (track) emit("menu", index, event);
}

/** Which song of a mix is sounding now, from where the player has got to. */
function currentChapter(mix: QueueMix, index: number): number {
  if (index !== props.currentIndex) return -1;
  let at = -1;
  mix.chapters.forEach((chapter, i) => {
    if (chapter.startSecs <= props.positionSecs + 0.05) at = i;
  });
  return at;
}

/** The ancestor that actually scrolls this list. */
function scrollParent(element: HTMLElement): HTMLElement | null {
  let parent = element.parentElement;
  while (parent) {
    const overflow = getComputedStyle(parent).overflowY;
    if (overflow === "auto" || overflow === "scroll") return parent;
    parent = parent.parentElement;
  }
  return null;
}

/**
 * Put the playing track in the middle of the view when the queue is opened.
 *
 * Done by hand rather than with `scrollIntoView({ block: "center" })`, which
 * walks every scrollable ancestor and would drag the page behind the panel
 * along with it.
 */
function centreOnCurrent() {
  const container = listEl.value;
  const index = props.currentIndex;
  if (!container || index === null) return;

  const rows = container.querySelectorAll<HTMLElement>("[data-row]");
  const row = rows[index];
  const scroller = scrollParent(container);
  if (!row || !scroller) return;

  // Measured against the scroller rather than via `offsetTop`, which is
  // relative to the nearest positioned ancestor and need not be this one.
  const rowBox = row.getBoundingClientRect();
  const scrollerBox = scroller.getBoundingClientRect();
  const offsetWithin = scroller.scrollTop + rowBox.top - scrollerBox.top;
  const target = offsetWithin - scroller.clientHeight / 2 + rowBox.height / 2;
  scroller.scrollTop = Math.max(0, target);
}

// On mount only: the queue is centred when it is brought up, and left alone
// afterwards so it cannot yank itself away from someone scrolling it.
onMounted(async () => {
  await nextTick();
  centreOnCurrent();
});

</script>

<template>
  <div
    ref="listEl"
    class="queue-list"
    :class="{ 'is-roomy': roomy, 'is-dragging': reorderable && isDragging }"
  >
    <template
      v-for="(row, index) in items"
      :key="`${row.kind === 'mix' ? row.mix.playlistId : (row.track?.id ?? 'removed')}-${index}`"
    >
      <div
        v-if="reorderable && dropAt === index && isDragging"
        class="queue-list__drop"
      />

      <!--
        A mix: one block, drawn as one thing. The songs in it are listed but
        are not rows — there is no grip on them and nothing can be dropped
        between them, which is exactly what the queue can and cannot do with
        an arrangement.
      -->
      <div
        v-if="row.kind === 'mix'"
        data-row
        class="row mix-block"
        :class="{
          'is-current': index === currentIndex,
          'is-lifted': reorderable && dragFrom === index,
        }"
      >
        <button
          v-if="reorderable"
          class="row__grip"
          title="Drag to reorder the whole mix"
          aria-label="Drag to reorder the whole mix"
          @pointerdown="onHandleDown($event, index)"
          @pointermove="onHandleMove"
          @pointerup="onHandleUp"
          @pointercancel="onHandleCancel"
        >
          <PnmIcon name="grip" :size="15" />
        </button>

        <div class="mix-block__body">
          <div class="mix-block__head">
            <button
              class="row__art"
              :title="`Play ${row.mix.name}`"
              :aria-label="`Play ${row.mix.name}`"
              @click="emit('play', index)"
            >
              <PlaylistArtwork
                :artwork="row.mix.artwork"
                :artwork-ids="row.mix.artworkIds"
                :size="roomy ? 44 : 38"
                :radius="5"
              />
              <span class="row__overlay">
                <PnmIcon
                  :name="index === currentIndex && playing ? 'pause' : 'play'"
                  :size="roomy ? 16 : 14"
                />
              </span>
            </button>
            <div class="row__text">
              <div class="row__title clamp clamp-1" :title="row.mix.name">{{ row.mix.name }}</div>
              <div class="row__subtitle clamp clamp-1">
                <PnmIcon name="timeline" :size="11" class="mix-block__badge" />
                Master mix · {{ row.mix.chapters.length }}
                {{ row.mix.chapters.length === 1 ? "song" : "songs" }}
              </div>
            </div>
            <span class="row__duration">{{ formatDuration(row.mix.durationSecs) }}</span>
            <button
              v-if="removable"
              class="icon-button row__remove"
              :aria-label="`Remove ${row.mix.name} from the queue`"
              :title="`Remove ${row.mix.name} from the queue`"
              @click.stop="emit('remove', index)"
            >
              <PnmIcon name="close" :size="15" />
            </button>
          </div>

          <ol class="mix-block__chapters">
            <li
              v-for="(chapter, chapterIndex) in row.mix.chapters"
              :key="`${chapter.startSecs}-${chapterIndex}`"
            >
              <button
                class="mix-block__chapter"
                :class="{ 'is-current': chapterIndex === currentChapter(row.mix, index) }"
                :title="`Jump to ${chapter.title}`"
                @click="emit('play', index, chapter.startSecs)"
              >
                <span class="mix-block__at">{{ formatDuration(chapter.startSecs) }}</span>
                <span class="mix-block__song truncate">{{ chapter.title }}</span>
                <span class="mix-block__artist truncate">{{ chapter.artist }}</span>
              </button>
            </li>
          </ol>
        </div>
      </div>

      <div
        v-else
        data-row
        class="row"
        :class="{
          'is-current': row.track && index === currentIndex,
          'is-lifted': reorderable && dragFrom === index,
          'is-removed': !row.track,
        }"
        @dblclick="play(row.track, index)"
        @contextmenu.prevent="openMenu(row.track, index, $event)"
      >
        <button
          v-if="reorderable"
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
          :aria-label="row.track ? `Play ${row.track.title}` : 'Removed track'"
          :title="row.track ? `Play ${row.track.title}` : 'Removed track'"
          :disabled="!row.track"
          @click="play(row.track, index)"
        >
          <Artwork :artwork-id="row.track?.artworkId ?? null" :size="roomy ? 44 : 38" :radius="5" />
          <span v-if="row.track" class="row__overlay">
            <PnmIcon
              :name="index === currentIndex && playing ? 'pause' : 'play'"
              :size="roomy ? 16 : 14"
            />
          </span>
        </button>

        <div class="row__text">
          <div
            class="row__title clamp"
            :class="{ 'clamp-1': !roomy }"
            :title="row.track?.title ?? 'Removed track'"
          >
            {{ row.track?.title ?? "Removed track" }}
          </div>
          <div
            class="row__subtitle clamp clamp-1"
            :title="
              row.track ? subtitleFor([row.track.artist, row.track.album]) : 'No longer in library'
            "
          >
            <slot name="subtitle" :track="row.track" :index="index">
              {{ row.track ? subtitleFor([row.track.artist, row.track.album]) : "No longer in library" }}
            </slot>
          </div>
        </div>

        <slot name="meta" :track="row.track" :index="index">
          <span v-if="row.track" class="row__duration">
            {{ formatDuration(row.track.durationSecs) }}
          </span>
        </slot>

        <button
          v-if="removable"
          class="icon-button row__remove"
          :aria-label="removeLabel"
          :title="removeLabel"
          @click.stop="emit('remove', index)"
        >
          <PnmIcon name="close" :size="15" />
        </button>
      </div>
    </template>

    <div
      v-if="reorderable && dropAt === items.length && isDragging"
      class="queue-list__drop"
    />
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
  content-visibility: auto;
  contain-intrinsic-block-size: auto 54px;
  border-radius: var(--radius-sm);
}

.row:hover {
  background: var(--bg-hover);
}

/*
 * The mix block.
 *
 * Drawn as one enclosed thing — its own surface, an accent edge down the side
 * and the songs indented inside it — because that is what it is in the queue:
 * a single entry nothing can be dropped into. The border is the whole point,
 * so it is not a hover treatment.
 */
.mix-block {
  align-items: stretch;
  padding: 7px 8px;
  border: 0.5px solid var(--separator-strong);
  border-left: 2px solid var(--accent);
  background: var(--bg-sunken);
}

.mix-block:hover {
  background: var(--bg-sunken);
}

.mix-block.is-current {
  border-left-color: var(--accent);
  background: var(--accent-tint);
}

.mix-block__body {
  flex: 1;
  min-width: 0;
}

.mix-block__head {
  display: flex;
  align-items: center;
  gap: 9px;
}

.mix-block__badge {
  vertical-align: -1px;
  margin-right: 4px;
  color: var(--accent);
}

.mix-block__chapters {
  margin: 6px 0 0;
  padding: 0 0 0 4px;
  list-style: none;
  border-left: 1px solid var(--separator);
}

.mix-block__chapter {
  display: flex;
  align-items: baseline;
  gap: 8px;
  width: 100%;
  padding: 3px 6px;
  border-radius: var(--radius-sm);
  font-size: 11.5px;
  color: var(--text-secondary);
  text-align: left;
}

.mix-block__chapter:hover {
  background: var(--bg-hover);
  color: var(--text);
}

/* Which song is sounding now, inside a mix that has no rows to highlight. */
.mix-block__chapter.is-current {
  color: var(--accent);
  font-weight: 600;
}

.mix-block__at {
  flex: none;
  width: 44px;
  font-variant-numeric: tabular-nums;
  color: var(--text-tertiary);
}

.mix-block__song {
  min-width: 0;
}

.mix-block__artist {
  min-width: 0;
  margin-left: auto;
  color: var(--text-tertiary);
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
