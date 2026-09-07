<script setup lang="ts">
/**
 * The full-screen "now playing" takeover: large artwork on the left, queue on
 * the right, over a backdrop built from the artwork itself.
 *
 * The background uses the same artwork scaled up and blurred, giving each
 * track a matching colour field without needing a separate palette service.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { onBeforeRouteLeave, useRouter } from "vue-router";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import PnmIcon from "../icons/PnmIcon.vue";
import Artwork from "../media/Artwork.vue";
import PlaylistArtwork from "../media/PlaylistArtwork.vue";
import QueueList from "../media/QueueList.vue";
import IconButton from "../ui/IconButton.vue";
import { artUrl, formatDuration } from "@/lib/format";
import { usePlayerStore } from "@/stores/player";
import { useSettingsStore } from "@/stores/settings";
import { useNowPlayingMeta } from "@/composables/useNowPlayingMeta";
import { useQueueActions } from "@/composables/useQueueActions";

const player = usePlayerStore();
const settings = useSettingsStore();
const router = useRouter();

const { mix, track, title, subtitle } = useNowPlayingMeta();
const { current, items, jump, remove, move, clear, saveAsPlaylist, openMenu } =
  useQueueActions();

/** What one artwork layer draws: a mix's quilt, or a single track's cover. */
interface ArtRef {
  mix: { artwork: string | null; artworkIds: string[] } | null;
  artworkId: string | null;
}

const currentArt = computed<ArtRef>(() => ({
  mix: mix.value ? { artwork: mix.value.artwork, artworkIds: mix.value.artworkIds } : null,
  artworkId: track.value?.artworkId ?? null,
}));

// Blown up 1.6x and blurred 64px, so detail beyond this is invisible; no
// reason to decode the original multi-megapixel picture for it.
function backdropFor(art: ArtRef): string | null {
  return artUrl(art.mix ? (art.mix.artwork ?? art.mix.artworkIds[0] ?? null) : art.artworkId, 640);
}

const queueReady = ref(false);
/** The queue list owns its scrolling; the header only asks it to go there. */
const queueListEl = ref<InstanceType<typeof QueueList> | null>(null);

/*
 * Crossfaded art handoff.
 *
 * The engine emits `crossfade-started` when the blend actually begins: the
 * first fade leg of its curve (incoming or outgoing, whichever starts
 * earlier) is crossed, so the outgoing song is fading or the incoming one is
 * about to become audible. The incoming track's art and the matching backdrop
 * are layered on top and fade in over the event's lead seconds — the same
 * window the audio blend spans — completing as the outgoing track ends, when
 * the engine promotes it and fires `track-changed`.
 *
 * While a fade runs, the layer *underneath* it is pinned to the outgoing art
 * rather than following the store. Both ends of the blend then stay put for
 * its whole length, and the only thing that changes is the overlay's opacity.
 * Letting the base follow the store instead was what made the transition look
 * wrong: `track-changed` can land well before the fade is up — a curve whose
 * lead is longer than the audible tail, or a file that hits EOF before its
 * declared duration — and the backdrop would snap from the blend straight to
 * the incoming art while the overlay was still on its way in.
 *
 * Only the engine knows a handoff is a crossfade. Guessing from the remaining
 * track time mistook a manual start near a track's end for one, so every
 * other track change is an instant swap. `track-changed` therefore only ever
 * *ends* a fade — by confirming the handoff it predicted, or by revealing a
 * different track that the listener asked for — and never starts one.
 */
interface ArtFade {
  /** Pinned under the overlay: what was playing when the blend began. */
  from: ArtRef;
  /** Layered on top and faded in: what the engine is blending into. */
  to: ArtRef;
  /** The track the engine named, so its arrival can be recognised. */
  trackId: string;
  /** Wall-clock fade length. */
  secs: number;
}

/** How long the overlay takes to unwind when a blend is abandoned or lands early. */
const UNWIND_SECS = 0.24;
/**
 * How long after the fade the overlay may be held waiting for `track-changed`
 * to confirm the handoff. Only reached if the event never comes at all; the
 * overlay is opaque and showing the incoming art by then, so holding it is
 * invisible either way.
 */
const CONFIRM_GRACE_MS = 3000;

const artFade = ref<ArtFade | null>(null);
/**
 * Bumped for each blend, and keyed onto the overlay layers so a new one
 * always remounts them. A fade that begins while the last one is still
 * unwinding would otherwise inherit that animation — running backwards,
 * under script control — instead of starting cleanly from transparent.
 */
const fadeKey = ref(0);
const artFadeEl = ref<HTMLElement | null>(null);
const backdropFadeEl = ref<HTMLElement | null>(null);
/** The engine has confirmed the handoff this fade predicted. */
let landed = false;
/** The fade has run its full length, so the overlay is fully opaque. */
let faded = false;
/** An unwind is already under way; further requests are ignored. */
let unwinding = false;
let fadeTimer: number | null = null;
let confirmTimer: number | null = null;
let unlistenCrossfade: UnlistenFn | null = null;
let unlistenCancelled: UnlistenFn | null = null;

const reducedMotion =
  typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-reduced-motion: reduce)")
    : null;

/** The art under the overlay: pinned while a fade runs, live otherwise. */
const baseArt = computed<ArtRef>(() => artFade.value?.from ?? currentArt.value);
const backdrop = computed(() => backdropFor(baseArt.value));
const fadeBackdrop = computed(() =>
  artFade.value ? backdropFor(artFade.value.to) : null,
);

const artStageStyle = computed(() => ({
  "--screen-fade": `${artFade.value?.secs ?? 0}s`,
}));

function clearTimers() {
  if (fadeTimer !== null) window.clearTimeout(fadeTimer);
  if (confirmTimer !== null) window.clearTimeout(confirmTimer);
  fadeTimer = null;
  confirmTimer = null;
}

/** Drop the overlay; the base layer goes back to following the store. */
function clearFade() {
  clearTimers();
  artFade.value = null;
  landed = false;
  faded = false;
  unwinding = false;
}

/**
 * The overlay's running CSS animations, where the browser exposes them.
 *
 * The fade itself is a plain CSS animation — it belongs on the compositor,
 * not in a per-frame script — but ending one early has to start from
 * wherever it has got to, which only the animation object knows. Where
 * `getAnimations` is missing there is nothing to adjust and the caller falls
 * back to dropping the overlay outright.
 */
function fadeAnimations(): Animation[] {
  return [artFadeEl.value, backdropFadeEl.value].flatMap((el) =>
    el && typeof el.getAnimations === "function" ? el.getAnimations() : [],
  );
}

function elapsedMs(animation: Animation): number {
  return typeof animation.currentTime === "number" ? animation.currentTime : 0;
}

/** Drop the overlay once the fade has run *and* the engine has confirmed it. */
function settleFade() {
  if (faded && landed) clearFade();
}

/**
 * Run the rest of the fade off in `UNWIND_SECS` instead of dropping it where
 * it stands: the blend was abandoned (a seek, a queue edit, a fresh load), so
 * the incoming art has to come back off the art that is still playing.
 */
function unwindFade() {
  if (!artFade.value || unwinding) return;
  const animations = fadeAnimations();
  if (animations.length === 0) {
    clearFade();
    return;
  }
  unwinding = true;
  clearTimers();
  const fade = artFade.value;
  for (const animation of animations) {
    const elapsed = elapsedMs(animation);
    animation.reverse();
    animation.updatePlaybackRate(-Math.max(elapsed, 1) / (UNWIND_SECS * 1000));
  }
  void Promise.all(animations.map((a) => a.finished))
    .then(() => {
      if (artFade.value === fade) clearFade();
    })
    .catch(() => {
      // Superseded by a newer fade, which owns the overlay now.
    });
}

/**
 * The handoff landed before the fade was up — an early promotion, most often
 * a file that reaches EOF short of its declared duration. The audio has
 * already switched, so the overlay finishes promptly rather than spending the
 * rest of its window arriving at art that is already playing.
 */
function hurryFade() {
  const animations = fadeAnimations();
  if (animations.length === 0) return;
  const fade = artFade.value;
  for (const animation of animations) {
    const total = artFade.value ? artFade.value.secs * 1000 : 0;
    const remaining = Math.max(total - elapsedMs(animation), 0);
    if (remaining <= UNWIND_SECS * 1000) continue;
    animation.updatePlaybackRate(remaining / (UNWIND_SECS * 1000));
  }
  clearTimers();
  fadeTimer = window.setTimeout(() => {
    fadeTimer = null;
    faded = true;
    if (artFade.value === fade) settleFade();
  }, UNWIND_SECS * 1000 + 80);
}

/** The queued track the engine named, which carries the cover to fade to. */
function artworkForTrack(trackId: string): string | null {
  const entries = [...player.queue.upcoming, ...player.queue.items];
  for (const entry of entries) {
    if (entry.kind === "track" && entry.track.id === trackId) return entry.track.artworkId;
  }
  return null;
}

/** The engine began mixing the next track in underneath the current one. */
function onCrossfadeStarted(payload: { trackId: string; leadSecs: number }) {
  // The preference disables the visual handoff entirely; reduced motion:
  // theme.css would hold the overlay at full opacity, which would show the
  // next cover before the audio has got to it.
  if (!settings.preferences.crossfadeArt || reducedMotion?.matches || mix.value) return;
  const artworkId = artworkForTrack(payload.trackId);
  if (artworkId === null) return;
  // The lead is track-seconds; rescale to wall-clock and let the overlay run
  // the whole window — up to the boundary itself, however long the user's
  // crossfade is. Clamping it shorter would drop the overlay mid-blend,
  // snapping back to the old art before the track changes.
  const speed = Math.max(player.snapshot.speed || 1, 0.05);
  const secs = Math.max(payload.leadSecs / speed, 0.1);
  clearTimers();
  fadeKey.value += 1;
  landed = false;
  faded = false;
  unwinding = false;
  artFade.value = {
    from: currentArt.value,
    to: { mix: null, artworkId },
    trackId: payload.trackId,
    secs,
  };
  const fade = artFade.value;
  // A little past the fade so the animation always finishes before the layer
  // is dropped.
  fadeTimer = window.setTimeout(() => {
    fadeTimer = null;
    faded = true;
    if (artFade.value === fade) settleFade();
  }, secs * 1000 + 100);
  confirmTimer = window.setTimeout(() => {
    confirmTimer = null;
    if (artFade.value === fade) clearFade();
  }, secs * 1000 + CONFIRM_GRACE_MS);
}

/**
 * The engine abandoned a blend it had already begun: the listener seeked,
 * loaded something else, or changed what comes next. The outgoing track plays
 * on, so the art it is playing under has to come back.
 */
function onCrossfadeCancelled() {
  unwindFade();
}

watch(
  () => [mix.value, track.value] as const,
  () => {
    const fade = artFade.value;
    // No fade running (manual start, skip, crossfade off): instant swap, and
    // the base layer follows the store on its own.
    if (!fade) return;
    if (!mix.value && track.value?.id === fade.trackId) {
      // The handoff the engine predicted: hold the overlay until it has run,
      // then drop it onto art that is now identical underneath.
      landed = true;
      if (!faded && !unwinding) hurryFade();
      settleFade();
      return;
    }
    // Something else is playing — a skip, or a queue jump — so what the
    // listener asked for wins over the blend that was under way.
    clearFade();
  },
);

const curtainVisible = ref(true);
let queueTimer: number | null = null;
let curtainFrame: number | null = null;

onMounted(() => {
  // The engine's own announcement that a crossfade began. Component-scoped:
  // the fan-out in `useBackendEvents` has nothing of its own to do with it.
  listen<{ trackId: string; leadSecs: number }>("crossfade-started", (e) =>
    onCrossfadeStarted(e.payload),
  ).then((fn) => {
    unlistenCrossfade = fn;
  });
  // Its counterpart: the engine tore the blend down again — a seek, a queue
  // edit — and the audio never changed track after all.
  listen("crossfade-cancelled", () => onCrossfadeCancelled()).then((fn) => {
    unlistenCancelled = fn;
  });

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
  if (unlistenCrossfade !== null) unlistenCrossfade();
  if (unlistenCancelled !== null) unlistenCancelled();
  clearTimers();
  if (queueTimer !== null) window.clearTimeout(queueTimer);
  if (curtainFrame !== null) window.cancelAnimationFrame(curtainFrame);
});
</script>

<template>
  <section class="screen" :class="{ 'has-art': !!backdrop }" :style="artStageStyle">
    <!-- Backdrop: the cover, blown up and blurred. -->
    <!-- Two identical layers, one over the other: the outgoing backdrop and,
         while the engine crossfades, the incoming one fading in on top. They
         share a wrapper class and img styles down to the last property, so
         both rasterize their 64px blur the same way and the moment the top
         one is dropped — fully opaque, over the same picture the base layer
         has by then — nothing about the image changes. -->
    <div v-if="backdrop" class="screen__backdrop" aria-hidden="true">
      <div class="screen__backdrop-layer">
        <img :src="backdrop" alt="" draggable="false" />
      </div>
      <div
        v-if="fadeBackdrop"
        :key="fadeKey"
        ref="backdropFadeEl"
        class="screen__backdrop-layer screen__backdrop-fade"
      >
        <img :src="fadeBackdrop" alt="" draggable="false" />
      </div>
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
        <div class="screen__art-stage">
          <!-- `full`: the cover is the subject here, and the CSS below scales
               it well past `size` — up to 62vh — so a thumbnail sized for 380px
               would be visibly soft on a large display. -->
          <PlaylistArtwork
            v-if="baseArt.mix"
            :artwork="baseArt.mix.artwork"
            :artwork-ids="baseArt.mix.artworkIds"
            :size="380"
            :radius="12"
            shadow
            full
          />
          <Artwork v-else :artwork-id="baseArt.artworkId" :size="380" :radius="12" shadow full />
          <!-- The incoming cover, layered on top and fading in while the
               engine crossfades. Pinned in place, like the base layer: both
               ends of the blend hold still and only the opacity moves. -->
          <div
            v-if="artFade"
            :key="fadeKey"
            ref="artFadeEl"
            class="screen__art-fade"
            aria-hidden="true"
          >
            <PlaylistArtwork
              v-if="artFade.to.mix"
              :artwork="artFade.to.mix.artwork"
              :artwork-ids="artFade.to.mix.artworkIds"
              :size="380"
              :radius="12"
              shadow
              full
            />
            <Artwork
              v-else
              :artwork-id="artFade.to.artworkId"
              :size="380"
              :radius="12"
              shadow
              full
            />
          </div>
        </div>
        <div class="screen__meta">
          <h1 class="clamp" :title="title">{{ title }}</h1>
          <p class="clamp" :title="subtitle">{{ subtitle }}</p>
          <p v-if="player.duration > 0" class="screen__time">
            {{ formatDuration(player.position) }} / {{ formatDuration(player.duration) }}
          </p>
        </div>
      </div>

      <aside class="screen__queue" :aria-busy="!queueReady">
        <header class="screen__queue-head">
          <h2>Playing Next</h2>
          <div v-if="queueReady && items.length" class="screen__queue-actions">
            <IconButton
              v-if="current !== null"
              icon="locateCurrent"
              label="Jump to the playing song"
              :size="16"
              @click="queueListEl?.centreOnCurrent()"
            />
            <IconButton
              icon="addToPlaylist"
              label="Save queue as a playlist"
              :size="16"
              @click="saveAsPlaylist"
            />
            <button class="screen__clear" @click="clear">Clear</button>
          </div>
        </header>
        <div v-if="!queueReady" class="screen__skeleton" aria-hidden="true">
          <div v-for="row in 5" :key="row" class="screen__skeleton-row">
            <span class="screen__skeleton-art" />
            <span class="screen__skeleton-lines">
              <span class="screen__skeleton-bar screen__skeleton-bar--title" />
              <span class="screen__skeleton-bar screen__skeleton-bar--sub" />
            </span>
          </div>
        </div>
        <div v-else-if="items.length === 0" class="screen__empty">Nothing queued.</div>
        <div v-else class="screen__list scroll-area">
          <QueueList
            ref="queueListEl"
            :items="items"
            :current-index="current"
            :playing="player.playing"
            :position-secs="player.position"
            :follow-current="settings.preferences.queueFollowsCurrent"
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

/* Both backdrops — the one playing and the one fading in over it — are this
   same box. The animation goes on the wrapper, never on the blurred img: a
   self-animated img is promoted to its own compositor layer and rasterizes
   its blur differently from a static one, which is visible the moment the
   layer is dropped. `will-change` is declared for both, so neither is the
   odd one out. */
.screen__backdrop-layer {
  position: absolute;
  inset: 0;
  pointer-events: none;
  will-change: opacity;
}

.screen__backdrop-layer img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  /* Scaled past the edges so the blur has no visible border. */
  transform: scale(1.6);
  filter: blur(64px) saturate(180%);
}

.screen__backdrop-fade {
  animation: screen-art-in var(--screen-fade) var(--ease) forwards;
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

/*
 * The artwork layers: the current cover, plus — while the engine crossfades
 * into the next track — the incoming cover layered on top, fading in over
 * `--screen-fade` (set on the section root so the backdrop layer shares it).
 * The stage is positioned so the overlay covers exactly the artwork box, not
 * the meta text beneath it.
 */
.screen__art-stage {
  position: relative;
  display: flex;
  justify-content: center;
  width: 100%;
}

.screen__art-fade {
  position: absolute;
  inset: 0;
  display: flex;
  justify-content: center;
  pointer-events: none;
  animation: screen-art-in var(--screen-fade) var(--ease) forwards;
}

@keyframes screen-art-in {
  from {
    opacity: 0;
  }
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

/* The same pairing as the compact queue panel's header: keep, or clear. */
.screen__queue-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin: 0 0 10px;
}

.screen__queue h2 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
}

.screen__queue-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  /* Over the artwork backdrop the panel sets its own text colour; the buttons
     follow it rather than the app's default greys, as the close button does. */
  color: inherit;
}

.screen__queue-actions .icon-button {
  color: inherit;
}

.screen__queue-actions .icon-button:hover {
  background: rgba(127, 127, 127, 0.25);
}

.screen__clear {
  font-size: 11.5px;
  color: inherit;
  opacity: 0.8;
}

.screen__clear:hover {
  opacity: 1;
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
 * Placeholder rows while the queue is deferred behind the curtain reveal.
 * The shimmer is a transform sweep on a pseudo-element, so the compositor
 * handles it; under reduced motion it is removed entirely and the rows sit
 * as a static tint.
 */
.screen__skeleton {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 2px 6px;
  overflow: hidden;
}

.screen__skeleton-row {
  display: flex;
  align-items: center;
  gap: 9px;
}

.screen__skeleton-art {
  position: relative;
  width: 44px;
  height: 44px;
  flex: none;
  border-radius: 5px;
  overflow: hidden;
  background: var(--bg-hover);
}

.screen__skeleton-lines {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 7px;
}

.screen__skeleton-bar {
  position: relative;
  height: 11px;
  border-radius: 3px;
  overflow: hidden;
  background: var(--bg-hover);
}

.screen__skeleton-bar--title {
  width: 62%;
}

.screen__skeleton-bar--sub {
  width: 40%;
  height: 10px;
}

.screen__skeleton-art::after,
.screen__skeleton-bar::after {
  content: "";
  position: absolute;
  inset: 0;
  transform: translateX(-100%);
  background: linear-gradient(90deg, transparent, var(--bg-active), transparent);
  animation: screen-shimmer 1.2s linear infinite;
}

@keyframes screen-shimmer {
  to {
    transform: translateX(100%);
  }
}

@media (prefers-reduced-motion: reduce) {
  .screen__skeleton-art::after,
  .screen__skeleton-bar::after {
    /* Overrides the duration-only neutralisation in theme.css with no
       animation at all: an infinite loop at 0.01ms is a flicker. */
    animation: none !important;
  }
}

/* The queue fades in quickly once it is finally rendered. */
.screen__list,
.screen__empty {
  animation: screen-reveal 0.18s var(--ease);
}

@keyframes screen-reveal {
  from {
    opacity: 0;
  }
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
