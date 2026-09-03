<script setup lang="ts">
/**
 * The transport bar along the bottom of every drawing: artwork and info on the
 * left, transport and scrubber in the middle, and the mixer, shuffle, repeat
 * and queue toggles on the right.
 */
import { computed, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useDismiss } from "@/lib/dismiss";
import PnmIcon from "./icons/PnmIcon.vue";
import Artwork from "./Artwork.vue";
import PlaylistArtwork from "./PlaylistArtwork.vue";
import AppSlider from "./AppSlider.vue";
import MixerPopover from "./mixer/MixerPopover.vue";
import InfoPopover from "./InfoPopover.vue";
import { formatDuration } from "@/lib/format";
import { stableAlbumId, stableArtistId } from "@/lib/ids";
import { usePlayerStore } from "@/stores/player";
import { useMixerStore } from "@/stores/mixer";
import { useUiStore } from "@/stores/ui";
import { useMasterMixStore } from "@/stores/masterMix";

const player = usePlayerStore();
const mixer = useMixerStore();
const ui = useUiStore();
const masterMix = useMasterMixStore();
const route = useRoute();
const router = useRouter();

const SKIP_SECONDS = 10;
/** Quarters, so the level can be set exactly without fighting the pointer. */
const VOLUME_DETENTS = [0, 0.25, 0.5, 0.75, 1];

const track = computed(() => player.track);

/**
 * The playlist being played as a mix, if that is what is playing.
 *
 * A mix has no current track: the engine holds one long timeline, so the bar
 * shows the playlist in a song's place and marks the songs along the scrubber.
 */
const mix = computed(() => player.masterMix);

/**
 * Where each song starts, as dots on the scrubber — but only the ones far
 * enough apart to read.
 *
 * A crossfaded join can put two starts a second apart, which at this width is
 * one smudge rather than two marks, so anything within a fiftieth of the
 * timeline of the mark before it is dropped. The two ends are left clear as
 * well: a dot under the handle's parked position says nothing.
 */
const chapterMarks = computed(() => {
  const current = mix.value;
  const total = player.duration;
  if (!current || total <= 0) return [];
  const apart = total / 50;
  const marks: number[] = [];
  for (const chapter of current.chapters) {
    const at = chapter.startSecs;
    if (at <= apart || at >= total - apart) continue;
    if (marks.length > 0 && at - marks[marks.length - 1] < apart) continue;
    marks.push(at);
  }
  return marks;
});

function goToPlaylist() {
  const current = mix.value;
  if (current) router.push({ name: "playlist", params: { id: current.playlistId } });
}

/**
 * The Master Mixer captures normal playback for as long as it is open, so the
 * queue is paused and the engine is playing a timeline instead. The transport
 * here would be reporting one thing's position under another thing's name, and
 * pressing it would fight the audition — so it stands aside and says so.
 */
const suspended = computed(() => masterMix.open);

const volumeIcon = computed(() => {
  const level = player.snapshot.volume;
  if (level === 0) return "volumeOff";
  return level < 0.5 ? "volumeLow" : "volume";
});
function goToArtist() {
  if (track.value) router.push({ name: "artist", params: { id: stableArtistId(track.value) } });
}

function goToAlbum() {
  if (track.value) router.push({ name: "album", params: { id: stableAlbumId(track.value) } });
}

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

const mixerPopover = ref<HTMLElement | null>(null);
const mixerButton = ref<HTMLElement | null>(null);
const infoPopover = ref<HTMLElement | null>(null);
const infoButton = ref<HTMLElement | null>(null);

// Clicking anywhere else dismisses either popover. The trigger is ignored so
// its own click still toggles rather than closing and immediately reopening.
useDismiss(
  () => mixer.popoverOpen,
  () => (mixer.popoverOpen = false),
  mixerPopover,
  { ignore: [mixerButton] },
);
useDismiss(
  () => infoOpen.value,
  () => {
    ui.infoTrack = null;
    ui.infoMixOpen = false;
  },
  infoPopover,
  { ignore: [infoButton] },
);

/** The bubble describes whichever of the two things is playing. */
const infoOpen = computed(() => ui.infoTrack !== null || ui.infoMixOpen);

function toggleInfo() {
  if (infoOpen.value) {
    ui.infoTrack = null;
    ui.infoMixOpen = false;
    return;
  }
  if (mix.value) ui.infoMixOpen = true;
  else ui.infoTrack = track.value;
}

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

const onNowPlaying = computed(() => route.name === "nowPlaying");

/** Shift keeps the compact side panel; a plain click opens the takeover. */
function onQueueButton(event: MouseEvent) {
  if (event.shiftKey) {
    if (onNowPlaying.value) router.back();
    ui.queueOpen = !ui.queueOpen;
    return;
  }
  ui.queueOpen = false;
  // Navigation, so back and forward close and reopen it like any page.
  if (onNowPlaying.value) router.back();
  else router.push({ name: "nowPlaying" });
}

/** Shift skips the compact bubble and toggles the full panel. */
async function openMixer(event: MouseEvent) {
  const panelWasOpen = mixer.panelOpen;
  await mixer.editGlobal();
  if (event.shiftKey) {
    mixer.popoverOpen = false;
    mixer.panelOpen = !panelWasOpen;
    return;
  }
  mixer.popoverOpen = !mixer.popoverOpen;
}
</script>

<template>
  <footer class="bar">
    <!-- Left: what is playing -->
    <div class="bar__now">
      <PlaylistArtwork
        v-if="mix"
        :artwork="mix.artwork"
        :artwork-ids="mix.artworkIds"
        :size="46"
        :radius="5"
        shadow
      />
      <Artwork v-else :artwork-id="track?.artworkId" :size="46" :radius="5" shadow />
      <!-- A mix plays as one thing, so it is named as one thing. -->
      <div v-if="mix" class="bar__text">
        <div class="bar__title truncate" :title="mix.name">
          <PnmIcon
            name="timeline"
            :size="13"
            class="bar__badge"
            title="Playing as a master mix"
          />
          <span class="truncate">{{ mix.name }}</span>
        </div>
        <div class="bar__subtitle truncate">
          <button class="bar__link" :title="`Go to ${mix.name}`" @click="goToPlaylist">
            Master mix
          </button>
          <span class="bar__sep">·</span>
          <span>{{ mix.trackCount }} {{ mix.trackCount === 1 ? "song" : "songs" }}</span>
        </div>
      </div>
      <div v-else class="bar__text">
        <div class="bar__title truncate" :title="track?.title ?? ''">
          {{ track?.title ?? "Nothing Playing" }}
        </div>
        <div class="bar__subtitle truncate">
          <button
            v-if="track"
            class="bar__link"
            :title="`Go to ${track.artist}`"
            @click="goToArtist"
          >
            {{ track.artist }}
          </button>
          <template v-if="track?.album">
            <span class="bar__sep">·</span>
            <button class="bar__link" :title="`Go to ${track.album}`" @click="goToAlbum">
              {{ track.album }}
            </button>
          </template>
          <template v-if="track?.year">
            <span class="bar__sep">·</span>
            <span>{{ track.year }}</span>
          </template>
        </div>
      </div>
      <div class="bar__info">
        <button
          ref="infoButton"
          class="icon-button"
          :disabled="!player.hasPlayback"
          :aria-label="mix ? 'Mix information' : 'Track information'"
          @click="toggleInfo"
        >
          <PnmIcon name="info" :size="17" />
        </button>
        <Teleport to="body">
          <div v-if="infoOpen" ref="infoPopover" class="pnm-popover pnm-popover--info">
            <InfoPopover />
          </div>
        </Teleport>
      </div>
    </div>

    <!-- Middle: transport -->
    <div v-if="suspended" class="bar__transport bar__transport--suspended">
      <span>Playback is in the Master Mixer</span>
    </div>
    <div v-else class="bar__transport">
      <div class="bar__buttons">
        <button
          class="icon-button"
          :disabled="!player.hasPlayback"
          :title="`Back ${SKIP_SECONDS} seconds`"
          aria-label="Skip back ten seconds"
          @click="skip(-SKIP_SECONDS)"
        >
          <PnmIcon name="back10" :size="19" />
        </button>
        <button
          class="icon-button"
          :disabled="!player.hasPlayback"
          :title="mix ? 'Previous song in the mix' : 'Previous'"
          aria-label="Previous track"
          @click="player.previous()"
        >
          <PnmIcon name="previous" :size="19" />
        </button>
        <button
          class="bar__play"
          :disabled="!player.hasPlayback"
          :title="player.playing ? 'Pause' : 'Play'"
          :aria-label="player.playing ? 'Pause' : 'Play'"
          @click="player.toggle()"
        >
          <PnmIcon :name="player.playing ? 'pause' : 'play'" :size="21" />
        </button>
        <button
          class="icon-button"
          :disabled="!player.hasPlayback"
          :title="mix ? 'Next song in the mix' : 'Next'"
          aria-label="Next track"
          @click="player.next()"
        >
          <PnmIcon name="next" :size="19" />
        </button>
        <button
          class="icon-button"
          :disabled="!player.hasPlayback"
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
          :disabled="!player.hasPlayback"
          :markers="chapterMarks"
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
        <button
          class="bar__mute"
          :title="player.muted ? 'Unmute' : 'Mute'"
          :aria-label="player.muted ? 'Unmute' : 'Mute'"
          :aria-pressed="player.muted"
          @click="player.toggleMute()"
        >
          <PnmIcon :name="volumeIcon" :size="15" />
        </button>
        <AppSlider
          :model-value="player.snapshot.volume"
          :detents="VOLUME_DETENTS"
          @update:model-value="player.setVolume($event)"
        />
      </div>

      <div class="bar__mixer">
        <button
          ref="mixerButton"
          class="icon-button"
          :class="{ 'is-active': mixerActive || mixer.popoverOpen || mixer.panelOpen }"
          title="DJ Mixer (hold Shift for the advanced panel)"
          aria-label="DJ Mixer"
          @click="openMixer"
        >
          <PnmIcon name="mixer" :size="19" />
        </button>
        <Teleport to="body">
          <Transition name="pop">
            <div v-if="mixer.popoverOpen" ref="mixerPopover" class="pnm-popover pnm-popover--mixer">
              <MixerPopover />
            </div>
          </Transition>
        </Teleport>
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
        :class="{ 'is-active': onNowPlaying || ui.queueOpen }"
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
  display: flex;
  align-items: center;
  gap: 5px;
  min-width: 0;
  font-size: 13px;
  font-weight: 500;
}

/* Small, and in the accent, so "this is a mix" reads without a word for it. */
.bar__badge {
  flex: none;
  color: var(--accent);
}

.bar__subtitle {
  display: flex;
  align-items: baseline;
  gap: 4px;
  font-size: 11.5px;
  color: var(--text-secondary);
}

.bar__link {
  font: inherit;
  color: inherit;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.bar__link:hover {
  color: var(--text);
  text-decoration: underline;
}

.bar__sep {
  flex: none;
  opacity: 0.6;
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

.bar__transport--suspended {
  justify-content: center;
  /* The column layout above centres nothing when there is one line in it. */
  flex-direction: row;
  font-size: 12px;
  color: var(--text-tertiary);
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

.bar__mute {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  color: inherit;
  border-radius: var(--radius-sm);
  padding: 3px;
  transition: color 0.15s var(--ease);
}

.bar__mute:hover {
  color: var(--text);
}

.bar__mixer {
  position: relative;
}



</style>
