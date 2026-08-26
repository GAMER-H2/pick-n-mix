<script setup lang="ts">
/**
 * The full-screen "now playing" takeover: large artwork on the left, queue on
 * the right, over a backdrop built from the artwork itself.
 *
 * The background is the same image scaled up and heavily blurred rather than
 * an extracted palette. That gives the colours of the cover for free, stays
 * correct for every image, and costs one already-cached decode.
 */
import { computed } from "vue";
import { useRouter } from "vue-router";
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
const backdrop = computed(() => artUrl(track.value?.artworkId));
const subtitle = computed(() => subtitleFor([track.value?.artist, track.value?.album]));
const items = computed(() => player.queue.items);
const current = computed(() => player.queue.currentIndex);

async function jump(index: number) {
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
  <!-- `pnm-rises` is the hook the route transition uses to tell this apart
       from an ordinary page: it is the element that slides, while whatever is
       behind it stays still. Unscoped on purpose so global CSS can see it. -->
  <section class="screen pnm-rises" :class="{ 'has-art': !!backdrop }">
    <!-- Backdrop: the cover, blown up and blurred. -->
    <div v-if="backdrop" class="screen__backdrop" aria-hidden="true">
      <img :src="backdrop" alt="" draggable="false" />
    </div>
    <div class="screen__veil" aria-hidden="true" />

    <header class="screen__bar">
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

      <aside class="screen__queue">
        <h2>Playing Next</h2>
        <div v-if="items.length === 0" class="screen__empty">Nothing queued.</div>
        <div v-else class="screen__list scroll-area">
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

.screen__backdrop {
  position: absolute;
  inset: 0;
  overflow: hidden;
  /* Forces its own compositor layer, so the 64px blur below is rasterised
     once rather than on every frame of the slide. */
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
  padding: 12px 18px;
  -webkit-app-region: drag;
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
  -webkit-app-region: no-drag;
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
  width: min(380px, 34vw) !important;
  height: min(380px, 34vw) !important;
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
  /* A translucent card so the backdrop still shows through. */
  background: rgba(127, 127, 127, 0.14);
  backdrop-filter: blur(24px) saturate(160%);
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

@media (max-width: 980px) {
  .screen__body {
    grid-template-columns: 1fr;
    gap: 20px;
    padding: 8px 20px 20px;
  }

  .screen__art :deep(.artwork) {
    width: min(240px, 40vw) !important;
    height: min(240px, 40vw) !important;
  }
}
</style>
