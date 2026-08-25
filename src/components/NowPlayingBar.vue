<script setup lang="ts">
/**
 * The transport bar along the bottom of every drawing: artwork and info on the
 * left, transport and scrubber in the middle, and the mixer, shuffle, repeat
 * and queue toggles on the right.
 */
import { computed, ref } from "vue";
import PnmIcon from "./icons/PnmIcon.vue";
import Artwork from "./Artwork.vue";
import AppSlider from "./AppSlider.vue";
import MixerPopover from "./mixer/MixerPopover.vue";
import InfoPopover from "./InfoPopover.vue";
import { formatDuration, subtitleFor } from "@/lib/format";
import { usePlayerStore } from "@/stores/player";
import { useMixerStore } from "@/stores/mixer";
import { useUiStore } from "@/stores/ui";

const player = usePlayerStore();
const mixer = useMixerStore();
const ui = useUiStore();

const SKIP_SECONDS = 10;

const track = computed(() => player.track);
const subtitle = computed(() =>
  track.value ? subtitleFor([track.value.album, track.value.artist, track.value.year]) : "",
);

/** Any effect audibly engaged, which lights up the mixer button. */
const mixerActive = computed(() => {
  const fx = mixer.effective;
  if (!fx.enabled) return false;
  return (
    fx.reverb.enabled ||
    fx.delay.enabled ||
    fx.lofi.enabled ||
    fx.pitch.semitones !== 0 ||
    fx.pitch.cents !== 0 ||
    fx.eq.bands.some((b) => b.enabled && b.gainDb !== 0) ||
    fx.filters.some((f) => f.enabled)
  );
});

const scrubValue = ref(0);

function onScrubStart() {
  player.scrubbing = true;
  player.scrubPosition = player.position;
  scrubValue.value = player.position;
}

function onScrub(value: number) {
  scrubValue.value = value;
  player.scrubPosition = value;
}

async function onScrubEnd() {
  player.scrubbing = false;
  await player.seek(scrubValue.value);
}

function skip(seconds: number) {
  const next = Math.max(0, Math.min(player.duration, player.position + seconds));
  player.seek(next);
}

/** Shift keeps the compact side panel; a plain click opens the takeover. */
function onQueueButton(event: MouseEvent) {
  if (event.shiftKey) {
    ui.nowPlayingOpen = false;
    ui.queueOpen = !ui.queueOpen;
    return;
  }
  ui.queueOpen = false;
  ui.nowPlayingOpen = !ui.nowPlayingOpen;
}

async function openMixer() {
  if (!mixer.popoverOpen) await mixer.editGlobal();
  mixer.popoverOpen = !mixer.popoverOpen;
}
</script>

<template>
  <footer class="bar">
    <!-- Left: what is playing -->
    <div class="bar__now">
      <Artwork :artwork-id="track?.artworkId" :size="46" :radius="5" shadow />
      <div class="bar__text">
        <div class="bar__title truncate">{{ track?.title ?? "Nothing Playing" }}</div>
        <div class="bar__subtitle truncate">{{ subtitle }}</div>
      </div>
      <div class="bar__info">
        <button
          class="icon-button"
          :disabled="!track"
          aria-label="Track information"
          @click="ui.infoTrack = ui.infoTrack ? null : track"
        >
          <PnmIcon name="info" :size="17" />
        </button>
        <Teleport to="body">
          <div v-if="ui.infoTrack" class="bar__popover bar__popover--info">
            <InfoPopover />
          </div>
        </Teleport>
      </div>
    </div>

    <!-- Middle: transport -->
    <div class="bar__transport">
      <div class="bar__buttons">
        <button
          class="icon-button"
          :disabled="!track"
          :title="`Back ${SKIP_SECONDS} seconds`"
          aria-label="Skip back ten seconds"
          @click="skip(-SKIP_SECONDS)"
        >
          <PnmIcon name="back10" :size="19" />
        </button>
        <button
          class="icon-button"
          :disabled="!track"
          title="Previous"
          aria-label="Previous track"
          @click="player.previous()"
        >
          <PnmIcon name="previous" :size="19" />
        </button>
        <button
          class="bar__play"
          :disabled="!track"
          :title="player.playing ? 'Pause' : 'Play'"
          :aria-label="player.playing ? 'Pause' : 'Play'"
          @click="player.toggle()"
        >
          <PnmIcon :name="player.playing ? 'pause' : 'play'" :size="21" />
        </button>
        <button
          class="icon-button"
          :disabled="!track"
          title="Next"
          aria-label="Next track"
          @click="player.next()"
        >
          <PnmIcon name="next" :size="19" />
        </button>
        <button
          class="icon-button"
          :disabled="!track"
          :title="`Forward ${SKIP_SECONDS} seconds`"
          aria-label="Skip forward ten seconds"
          @click="skip(SKIP_SECONDS)"
        >
          <PnmIcon name="forward10" :size="19" />
        </button>
      </div>

      <div class="bar__scrub">
        <span class="bar__time">{{ formatDuration(player.position) }}</span>
        <AppSlider
          :model-value="player.position"
          :min="0"
          :max="Math.max(player.duration, 0.1)"
          :step="0.1"
          :disabled="!track"
          subtle
          @start="onScrubStart"
          @update:model-value="onScrub"
          @end="onScrubEnd"
        />
        <span class="bar__time">-{{ formatDuration(Math.max(0, player.duration - player.position)) }}</span>
      </div>
    </div>

    <!-- Right: mixer and playback modes -->
    <div class="bar__right">
      <div class="bar__volume">
        <PnmIcon :name="player.snapshot.volume === 0 ? 'volumeMute' : 'volume'" :size="15" />
        <AppSlider
          :model-value="player.snapshot.volume"
          @update:model-value="player.setVolume($event)"
        />
      </div>

      <div class="bar__mixer">
        <button
          class="icon-button"
          :class="{ 'is-active': mixerActive || mixer.popoverOpen }"
          title="DJ Mixer"
          aria-label="DJ Mixer"
          @click="openMixer"
        >
          <PnmIcon name="mixer" :size="19" />
        </button>
        <Transition name="pop">
          <div v-if="mixer.popoverOpen" class="bar__popover">
            <MixerPopover />
          </div>
        </Transition>
        <div v-if="mixer.popoverOpen" class="bar__scrim" @click="mixer.popoverOpen = false" />
      </div>

      <button
        class="icon-button"
        :class="{ 'is-active': player.queue.shuffle }"
        title="Shuffle"
        aria-label="Shuffle"
        @click="player.setShuffle(!player.queue.shuffle)"
      >
        <PnmIcon name="shuffle" :size="19" />
      </button>

      <button
        class="icon-button"
        :class="{ 'is-active': player.queue.repeat !== 'off' }"
        :title="`Repeat: ${player.queue.repeat}`"
        aria-label="Repeat"
        @click="player.cycleRepeat()"
      >
        <PnmIcon :name="player.queue.repeat === 'one' ? 'repeatOne' : 'repeat'" :size="19" />
      </button>

      <button
        class="icon-button"
        :class="{ 'is-active': ui.nowPlayingOpen || ui.queueOpen }"
        title="Playing Next (hold Shift for the side panel)"
        aria-label="Playing next"
        @click="onQueueButton"
      >
        <PnmIcon name="queue" :size="19" />
      </button>
    </div>
  </footer>
</template>

<style scoped>
.bar {
  display: grid;
  grid-template-columns: minmax(200px, 1fr) minmax(320px, 2fr) minmax(200px, 1fr);
  align-items: center;
  gap: 16px;
  height: var(--player-height);
  padding: 0 14px;
  flex: none;
  border-top: 1px solid var(--separator);
  background: var(--bg-bar);
  backdrop-filter: saturate(180%) blur(20px);
}

.bar__now {
  display: flex;
  align-items: center;
  gap: 11px;
  min-width: 0;
}

.bar__text {
  min-width: 0;
}

.bar__title {
  font-size: 13px;
  font-weight: 500;
}

.bar__subtitle {
  font-size: 11.5px;
  color: var(--text-secondary);
}

.bar__info {
  position: relative;
  flex: none;
}

.bar__transport {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  min-width: 0;
}

.bar__buttons {
  display: flex;
  align-items: center;
  gap: 4px;
}

.bar__play {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 50%;
  color: var(--text);
  transition: transform 0.1s var(--ease), background 0.15s var(--ease);
}

.bar__play:hover {
  background: var(--bg-hover);
}

.bar__play:active {
  transform: scale(0.93);
}

.bar__play:disabled {
  opacity: 0.32;
  pointer-events: none;
}

.bar__scrub {
  display: grid;
  grid-template-columns: 40px 1fr 44px;
  align-items: center;
  gap: 9px;
  width: 100%;
  max-width: 520px;
}

.bar__time {
  font-size: 10.5px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
  text-align: center;
}

.bar__right {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 2px;
}

.bar__volume {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 108px;
  flex: none;
  margin-right: 8px;
  color: var(--text-secondary);
}

.bar__volume :deep(.slider) {
  min-width: 0;
}

.bar__mixer {
  position: relative;
}

.bar__popover {
  position: absolute;
  bottom: calc(100% + 12px);
  right: -6px;
  z-index: 300;
  border-radius: var(--radius-lg);
  background: var(--bg-elevated);
  border: 0.5px solid var(--separator);
  box-shadow: var(--shadow-popover);
  transform-origin: bottom right;
}

/* The info bubble is teleported, so it positions against the window. */
.bar__popover--info {
  position: fixed;
  bottom: calc(var(--player-height) + 12px);
  left: 14px;
  right: auto;
  transform-origin: bottom left;
}

.bar__scrim {
  position: fixed;
  inset: 0;
  z-index: 290;
}
</style>
