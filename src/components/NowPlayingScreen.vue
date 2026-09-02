<script setup lang="ts">
/**
 * The full-screen "now playing" takeover: large artwork on the left, queue on
 * the right, over a backdrop built from the artwork itself.
 *
 * The background uses the same artwork scaled up and blurred, giving each
 * track a matching colour field without needing a separate palette service.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { onBeforeRouteLeave, useRouter } from "vue-router";
import PnmIcon from "./icons/PnmIcon.vue";
import Artwork from "./Artwork.vue";
import QueueList from "./QueueList.vue";
import { artUrl, formatDuration, subtitleFor } from "@/lib/format";
import * as api from "@/lib/api";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";

const player = usePlayerStore();
const ui = useUiStore();
const router = useRouter();

const track = computed(() => player.track);
// Blown up 1.6x and blurred 64px, so detail beyond this is invisible; no
// reason to decode the original multi-megapixel picture for it.
const backdrop = computed(() => artUrl(track.value?.artworkId, 640));
const subtitle = computed(() => subtitleFor([track.value?.artist, track.value?.album]));
const items = computed(() => player.queue.items);
const current = computed(() => player.queue.currentIndex);
const queueReady = ref(false);
const curtainVisible = ref(true);
let queueTimer: number | null = null;
let curtainFrame: number | null = null;

onMounted(() => {
  // Give the webview two paints to rasterize the static artwork blur and panel
  // behind an opaque layer. Only that cheap layer changes opacity afterward.
  curtainFrame = window.requestAnimationFrame(() => {
    curtainFrame = window.requestAnimationFrame(() => {
      curtainVisible.value = false;
      curtainFrame = null;
    });
  });

  // Populate the expensive queue only after the curtain has finished revealing
  // the rest of the screen.
  queueTimer = window.setTimeout(() => {
    queueReady.value = true;
    queueTimer = null;
  }, 160);
});

onBeforeRouteLeave(async () => {
  // Likewise, unmount the expensive list before the shell begins fading out.
  if (queueTimer !== null) {
    window.clearTimeout(queueTimer);
    queueTimer = null;
  }
  queueReady.value = false;
  curtainVisible.value = true;
  await nextTick();
  await new Promise<void>((resolve) => window.setTimeout(resolve, 110));
  return true;
});

onBeforeUnmount(() => {
  if (queueTimer !== null) window.clearTimeout(queueTimer);
  if (curtainFrame !== null) window.cancelAnimationFrame(curtainFrame);
});

/** Clicking the row that is already playing toggles it instead of restarting. */
async function jump(index: number) {
  if (index === current.value && player.playing) {
    await player.toggle();
    return;
  }
  await api.playQueueIndex(index);
}

async function remove(index: number) {
  await api.removeFromQueue(index);
  await player.refreshQueue();
}

async function move(from: number, to: number) {
  await api.moveInQueue(from, to);
  await player.refreshQueue();
}

function openMenu(index: number, event: MouseEvent) {
  const item = items.value[index];
  if (item) ui.openContextMenu({ x: event.clientX, y: event.clientY, tracks: [item] });
}
</script>

<template>
  <section class="screen" :class="{ 'has-art': !!backdrop }">
    <!-- Backdrop: the cover, blown up and blurred. -->
    <div v-if="backdrop" class="screen__backdrop" aria-hidden="true">
      <img :src="backdrop" alt="" draggable="false" />
    </div>
    <div class="screen__veil" aria-hidden="true" />

    <header class="screen__bar" data-tauri-drag-region="deep">
      <div class="screen__from clamp clamp-1" :title="player.queue.context?.name ?? 'your library'">
        <span class="screen__from-label">Playing from</span>
        <strong>{{ player.queue.context?.name ?? "your library" }}</strong>
      </div>
      <button
        class="icon-button screen__close"
        aria-label="Close now playing"
        title="Close"
        @click="router.back()"
      >
        <PnmIcon name="collapse" :size="19" />
      </button>
    </header>

    <div class="screen__body">
      <div class="screen__art">
        <Artwork :artwork-id="track?.artworkId" :size="380" :radius="12" shadow />
        <div class="screen__meta">
          <h1 class="clamp" :title="track?.title ?? ''">
            {{ track?.title ?? "Nothing Playing" }}
          </h1>
          <p class="clamp" :title="subtitle">{{ subtitle }}</p>
          <p v-if="player.duration > 0" class="screen__time">
            {{ formatDuration(player.position) }} / {{ formatDuration(player.duration) }}
          </p>
        </div>
      </div>

      <aside class="screen__queue" :aria-busy="!queueReady">
        <h2>Playing Next</h2>
        <div v-if="queueReady && items.length === 0" class="screen__empty">Nothing queued.</div>
        <div v-else-if="queueReady" class="screen__list scroll-area">
          <QueueList
            :items="items"
            :current-index="current"
            :playing="player.playing"
            roomy
            @play="jump"
            @remove="remove"
            @move="move"
            @menu="openMenu"
          />
        </div>
      </aside>
    </div>

    <div
      class="screen__curtain"
      :class="{ 'is-visible': curtainVisible }"
      aria-hidden="true"
    />
  </section>
</template>

<style scoped>
.screen {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
  /* Fallback when a track has no artwork at all. */
  background: var(--bg);
  color: var(--text);
}

.screen__curtain {
  position: absolute;
  inset: 0;
  z-index: 10;
  background: var(--bg);
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.1s ease-out;
  will-change: opacity;
}

.screen__curtain.is-visible {
  opacity: 1;
}

.screen__backdrop {
  position: absolute;
  inset: 0;
  overflow: hidden;
  /* Keep the expensive blur on its own compositor layer. */
  transform: translateZ(0);
}

.screen__backdrop img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  /* Scaled past the edges so the blur has no visible border. */
  transform: scale(1.6);
  filter: blur(64px) saturate(180%);
}

/* Keeps text legible whatever the cover happens to look like. */
.screen__veil {
  position: absolute;
  inset: 0;
  background: linear-gradient(
    180deg,
    rgba(0, 0, 0, 0.42),
    rgba(0, 0, 0, 0.62) 55%,
    rgba(0, 0, 0, 0.78)
  );
}

:root[data-theme="light"] .screen.has-art .screen__veil {
  background: linear-gradient(
    180deg,
    rgba(255, 255, 255, 0.5),
    rgba(255, 255, 255, 0.68) 55%,
    rgba(255, 255, 255, 0.82)
  );
}

/* With a backdrop behind it the panel always reads as dark-on-light or
   light-on-dark, so force the matching text colours. */
.screen.has-art {
  color: #fff;
}

:root[data-theme="light"] .screen.has-art {
  color: #1d1d1f;
}

.screen__bar,
.screen__body {
  position: relative;
  z-index: 1;
}

.screen__bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  /* Leaves room for the window controls, hoisted in `App.vue` to sit above
     everything at the app's own corner rather than under this bar. */
  padding: 12px calc(18px + var(--titlebar-controls)) 12px 18px;
}

.screen__from {
  font-size: 12px;
  opacity: 0.85;
}

.screen__from-label {
  opacity: 0.7;
  margin-right: 5px;
}

.screen__close {
  color: inherit;
}

.screen__close:hover {
  background: rgba(127, 127, 127, 0.25);
}

.screen__body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(320px, 1fr) minmax(340px, 460px);
  gap: 40px;
  padding: 8px 40px 40px;
  align-items: center;
}

.screen__art {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 22px;
  min-width: 0;
}

.screen__art :deep(.artwork) {
  /*
   * `cqw`, not `vw`: the art sits beside a fixed-ish queue column, so what
   * matters is the width `.app__main` actually has to give — which shrinks
   * when the sidebar's advanced mixer or queue panel opens — not the raw
   * window width. `vw` never saw that shrink, so the art stayed oversized and
   * ran into the queue column. `.app__main` is the query container; see
   * `App.vue`. The upper limit keeps the cover visually balanced with the
   * queue on ultrawide displays.
   */
  width: clamp(220px, min(46cqw, 62vh), 720px) !important;
  height: clamp(220px, min(46cqw, 62vh), 720px) !important;
}

.screen__meta {
  text-align: center;
  max-width: 100%;
  min-width: 0;
}

.screen__meta h1 {
  margin: 0;
  --clamp-lines: 3;
  font-size: 27px;
  font-weight: 700;
  letter-spacing: -0.02em;
}

.screen__meta p {
  margin: 6px 0 0;
  font-size: 14px;
  opacity: 0.78;
}

.screen__time {
  font-size: 12px !important;
  opacity: 0.6 !important;
  font-variant-numeric: tabular-nums;
}

.screen__queue {
  display: flex;
  flex-direction: column;
  min-height: 0;
  max-height: 100%;
  padding: 14px;
  border-radius: var(--radius-lg);
  /* A flat translucent card rather than its own `backdrop-filter`: it already
     sits on the backdrop image's own 64px blur, so a second blur here was
     doing nothing but adding a live-scrolling list to a compositor's most
     expensive filter path. */
  background: rgba(127, 127, 127, 0.18);
  border: 0.5px solid rgba(127, 127, 127, 0.22);
}

.screen__queue h2 {
  margin: 0 0 10px;
  font-size: 14px;
  font-weight: 600;
}

.screen__list {
  flex: 1;
  min-height: 0;
  margin: 0 -4px;
  padding: 0 4px;
}

.screen__empty {
  padding: 24px 0;
  font-size: 12.5px;
  opacity: 0.65;
}

/*
 * A `@container` query against `.app__main` (declared in `App.vue`), not a
 * `@media` query against the window: it reacts to the space this view
 * actually has, which shrinks when a side panel (the advanced mixer, the
 * compact queue) opens, not just when the window itself is resized.
 *
 * The threshold is not a rounded guess — it is the two-column grid's own
 * breaking point, worked backwards from the layout above: 320px + 340px for
 * the column minimums, 40px for the gap between them, 80px for this
 * container's own left+right padding. Below that, `minmax()` cannot shrink
 * the columns any further, so the grid overflows rather than fitting — which
 * is exactly the crowding this query exists to pre-empt. Anything short of
 * that true minimum (736px, an earlier approximation from the window-based
 * breakpoint this replaced, was 44px short of it) leaves a dead zone where
 * the old two-column layout is still forced on a container too narrow for it.
 */
@container app-main (max-width: 780px) {
  .screen__body {
    grid-template-columns: 1fr;
    gap: 20px;
    padding: 8px 24px 20px;
  }

  .screen__art :deep(.artwork) {
    /* The single-column layout still leaves meaningful room for the queue. */
    width: clamp(220px, min(54cqw, 50vh), 420px) !important;
    height: clamp(220px, min(54cqw, 50vh), 420px) !important;
  }
}
</style>
