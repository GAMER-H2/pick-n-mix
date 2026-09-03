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
import PnmIcon from "../icons/PnmIcon.vue";
import Artwork from "../media/Artwork.vue";
import PlaylistArtwork from "../media/PlaylistArtwork.vue";
import QueueList from "../media/QueueList.vue";
import { artUrl, formatDuration } from "@/lib/format";
import { usePlayerStore } from "@/stores/player";
import { useNowPlayingMeta } from "@/composables/useNowPlayingMeta";
import { useQueueActions } from "@/composables/useQueueActions";

const player = usePlayerStore();
const router = useRouter();

const { mix, track, title, subtitle } = useNowPlayingMeta();
const { current, items, jump, remove, move, openMenu } = useQueueActions();

// Blown up 1.6x and blurred 64px, so detail beyond this is invisible; no
// reason to decode the original multi-megapixel picture for it.
const backdrop = computed(() =>
  artUrl(mix.value ? (mix.value.artwork ?? mix.value.artworkIds[0] ?? null) : track.value?.artworkId, 640),
);
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
        <PlaylistArtwork
          v-if="mix"
          :artwork="mix.artwork"
          :artwork-ids="mix.artworkIds"
          :size="380"
          :radius="12"
          shadow
        />
        <Artwork v-else :artwork-id="track?.artworkId" :size="380" :radius="12" shadow />
        <div class="screen__meta">
          <h1 class="clamp" :title="title">{{ title }}</h1>
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
            :position-secs="player.position"
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
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 36px;
  padding: 8px 40px 40px;
}

.screen__art {
  /* Flexes and shrinks with the space actually available — which shrinks when
     the sidebar's advanced mixer or queue panel opens — instead of holding a
     fixed minimum and overflowing. `min-width: 0` is what allows the shrink. */
  flex: 1 1 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 22px;
  min-width: 0;
}

.screen__art :deep(.artwork),
.screen__art :deep(.quilt) {
  /*
   * A percentage of the art column, not `vw`/`cqw`: the column is a flex
   * item that shrinks with `.app__main` — when the window narrows or a side
   * panel opens — so the cover shrinks with it in every webview, including
   * ones without container-query support. The `62vh` cap keeps the cover
   * visually balanced with the queue on short windows; the upper clamp is
   * unnecessary now that the column itself bounds the size. The quilt is
   * overridden alongside the single cover: `PlaylistArtwork` sizes the quilt
   * inline in px, which would otherwise stay fixed while everything around
   * it shrank.
   */
  width: min(100%, 62vh) !important;
  height: auto !important;
  aspect-ratio: 1 !important;
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
  /* Shrinks with the row rather than holding a fixed minimum: at narrow
     widths the art gives up space first, then this card, and only below the
     breakpoint below does the layout stack. */
  flex: 0 1 440px;
  min-width: 0;
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
 * Stacks the layout when the main view itself is narrow — including when the
 * advanced mixer or queue panel eats into it from the sidebar, which a
 * window-width `@media` query cannot see. The art moves above the queue
 * widget, exactly as in the single-column layout below.
 *
 * The shrink behaviour above is plain flexbox and works everywhere; this
 * query only decides *when to stack*. Where container queries are
 * unsupported the `@media` fallback beneath still catches narrow windows.
 */
@container app-main (max-width: 780px) {
  .screen__body {
    flex-direction: column;
    align-items: center;
    justify-content: flex-start;
    gap: 20px;
    padding: 8px 24px 20px;
    overflow-y: auto;
  }

  .screen__art {
    flex: none;
    width: 100%;
  }

  .screen__art :deep(.artwork),
  .screen__art :deep(.quilt) {
    /* The single-column layout still leaves meaningful room for the queue. */
    width: min(100%, 38vh) !important;
  }

  .screen__queue {
    flex: none;
    width: 100%;
    max-height: none;
  }
}

/*
 * Window fallback for webviews without container-query support: same stacked
 * layout, keyed on the window instead of `.app__main`. Where container
 * queries work, both queries agree at their boundaries and nothing changes.
 */
@media (max-width: 720px) {
  .screen__body {
    flex-direction: column;
    align-items: center;
    justify-content: flex-start;
    gap: 20px;
    padding: 8px 24px 20px;
    overflow-y: auto;
  }

  .screen__art {
    flex: none;
    width: 100%;
  }

  .screen__art :deep(.artwork),
  .screen__art :deep(.quilt) {
    width: min(100%, 38vh) !important;
  }

  .screen__queue {
    flex: none;
    width: 100%;
    max-height: none;
  }
}
</style>
