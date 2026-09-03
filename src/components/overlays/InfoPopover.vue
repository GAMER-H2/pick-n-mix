<script setup lang="ts">
/**
 * The information bubble from the drawing: what the track is, and what the
 * engine is actually doing with it right now.
 */
import { computed } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import Artwork from "../media/Artwork.vue";
import PlaylistArtwork from "../media/PlaylistArtwork.vue";
import { formatBytes, formatDuration, formatHz } from "@/lib/format";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";

const ui = useUiStore();
const player = usePlayerStore();

const track = computed(() => ui.infoTrack);

/**
 * The mix, when the bubble was opened on one.
 *
 * Read from the player rather than copied when the bubble opened: a mix is
 * something in progress, so if it stops there is nothing left to describe and
 * this closes with it.
 */
const mix = computed(() => (ui.infoMixOpen ? player.masterMix : null));
/** The stream the engine built out of the arrangement: one file's worth. */
const mixStream = computed(() => (mix.value ? player.snapshot.stream : null));
const isCurrent = computed(() => !!track.value && player.track?.id === track.value.id);
/** Stream facts are only meaningful for the track actually being decoded. */
const stream = computed(() => (isCurrent.value ? player.snapshot.stream : null));

const effectiveRate = computed(() => {
  if (!isCurrent.value || !stream.value) return null;
  // Varispeed changes how fast source frames are consumed.
  return Math.round(stream.value.sampleRate * player.snapshot.speed);
});
</script>

<template>
  <Transition name="pop">
    <!--
      A mix is not a song: there is no file, no album and no track gain, but
      there is a playlist, one summed stream, and the same output as anything
      else. That is what this shows in a song's place.
    -->
    <div v-if="mix" class="info" role="dialog" aria-label="Mix information">
      <header class="info__head">
        <PlaylistArtwork
          :artwork="mix.artwork"
          :artwork-ids="mix.artworkIds"
          :size="56"
          :radius="6"
        />
        <div class="info__title">
          <div class="info__name clamp" :title="mix.name">{{ mix.name }}</div>
          <div class="info__artist clamp clamp-1">Master mix</div>
        </div>
        <button class="icon-button" aria-label="Close" @click="ui.infoMixOpen = false">
          <PnmIcon name="close" :size="16" />
        </button>
      </header>

      <dl class="info__list">
        <div><dt>Playlist</dt><dd class="clamp" :title="mix.name">{{ mix.name }}</dd></div>
        <div v-if="mix.description">
          <dt>Description</dt>
          <dd class="clamp" :title="mix.description">{{ mix.description }}</dd>
        </div>
        <div><dt>Songs</dt><dd>{{ mix.trackCount }}</dd></div>
        <div><dt>Arrangement</dt><dd>{{ mix.blockCount }} regions on {{ mix.laneCount }} tracks</dd></div>
        <div><dt>Length</dt><dd>{{ formatDuration(mix.durationSecs) }}</dd></div>
        <div>
          <dt>ID</dt>
          <dd class="info__mono clamp clamp-1">{{ mix.playlistId }}</dd>
        </div>
      </dl>

      <div class="info__section">Mixed Audio</div>

      <dl class="info__list">
        <div>
          <dt>File Type</dt>
          <dd>
            Rendered live
            <span class="info__tag">no file</span>
          </dd>
        </div>
        <div v-if="mixStream"><dt>Codec</dt><dd>{{ mixStream.codec }}</dd></div>
        <div v-if="mixStream">
          <dt>Mix Rate</dt>
          <dd>
            {{ formatHz(mixStream.sampleRate) }}
            <span v-if="mixStream.bitsPerSample"> · {{ mixStream.bitsPerSample }} bit float</span>
          </dd>
        </div>
        <div v-if="mixStream"><dt>Channels</dt><dd>{{ mixStream.channels }}</dd></div>
        <div><dt>Duration</dt><dd>{{ formatDuration(player.duration) }}</dd></div>
        <div><dt>Position</dt><dd>{{ formatDuration(player.position) }}</dd></div>
      </dl>

      <div class="info__section">Output</div>
      <dl class="info__list">
        <div>
          <dt>Device</dt>
          <dd class="clamp clamp-1">{{ player.snapshot.deviceName || "Unknown" }}</dd>
        </div>
        <div><dt>Device Rate</dt><dd>{{ formatHz(player.snapshot.deviceSampleRate) }}</dd></div>
        <div v-if="Math.abs(player.snapshot.speed - 1) > 0.001">
          <dt>Playback Rate</dt>
          <dd><span class="info__tag">{{ player.snapshot.speed.toFixed(3) }}x</span></dd>
        </div>
      </dl>
    </div>

    <div v-else-if="track" class="info" role="dialog" aria-label="Track information">
      <header class="info__head">
        <Artwork :artwork-id="track.artworkId" :size="56" :radius="6" />
        <div class="info__title">
          <div class="info__name clamp" :title="track.title">{{ track.title }}</div>
          <div class="info__artist clamp clamp-1" :title="track.artist">{{ track.artist }}</div>
        </div>
        <button class="icon-button" aria-label="Close" @click="ui.infoTrack = null">
          <PnmIcon name="close" :size="16" />
        </button>
      </header>

      <dl class="info__list">
        <div><dt>Artist</dt><dd class="clamp" :title="track.artist">{{ track.artist }}</dd></div>
        <div v-if="track.album"><dt>Album</dt><dd class="clamp" :title="track.album">{{ track.album }}</dd></div>
        <div><dt>Release</dt><dd>{{ track.year ?? "Unknown" }}</dd></div>
        <div v-if="track.genre"><dt>Genre</dt><dd class="clamp clamp-1">{{ track.genre }}</dd></div>
        <div>
          <dt>ID</dt>
          <dd class="info__mono clamp clamp-1">{{ track.musicbrainzRecordingId ?? track.id }}</dd>
        </div>
      </dl>

      <div class="info__section">Playback Information</div>

      <dl class="info__list">
        <div><dt>File Type</dt><dd>{{ track.format ?? "Unknown" }}</dd></div>
        <div>
          <dt>Bitrate</dt>
          <dd>{{ track.bitrateKbps ? `${track.bitrateKbps} kbps` : "Unknown" }}</dd>
        </div>
        <div><dt>File Size</dt><dd>{{ formatBytes(track.fileSize) }}</dd></div>
        <div><dt>Duration</dt><dd>{{ formatDuration(track.durationSecs) }}</dd></div>
        <div>
          <dt>Source Rate</dt>
          <dd>
            {{ formatHz(track.sampleRate) }}
            <span v-if="track.bitsPerSample"> · {{ track.bitsPerSample }} bit</span>
          </dd>
        </div>
        <div v-if="track.gainDb !== null">
          <dt>Track Gain</dt>
          <dd>{{ track.gainDb.toFixed(2) }} dB</dd>
        </div>
      </dl>

      <template v-if="isCurrent">
        <div class="info__section">Output</div>
        <dl class="info__list">
          <div>
            <dt>Device</dt>
            <dd class="clamp clamp-1">{{ player.snapshot.deviceName || "Unknown" }}</dd>
          </div>
          <div><dt>Device Rate</dt><dd>{{ formatHz(player.snapshot.deviceSampleRate) }}</dd></div>
          <div v-if="effectiveRate">
            <dt>Playback Rate</dt>
            <dd>
              {{ formatHz(effectiveRate) }}
              <span v-if="Math.abs(player.snapshot.speed - 1) > 0.001" class="info__tag">
                {{ player.snapshot.speed.toFixed(3) }}x
              </span>
            </dd>
          </div>
          <div v-if="stream"><dt>Codec</dt><dd>{{ stream.codec }}</dd></div>
        </dl>
      </template>
      <p v-else class="info__note">
        Playback details appear here while this track is playing.
      </p>
    </div>
  </Transition>
</template>

<style scoped>
.info {
  width: 320px;
  max-height: 70vh;
  overflow-y: auto;
  padding: 14px;
}

.info__head {
  display: flex;
  align-items: center;
  gap: 11px;
  margin-bottom: 12px;
}

.info__title {
  flex: 1;
  min-width: 0;
}

.info__name {
  font-size: 14px;
  font-weight: 600;
  line-height: 1.3;
}

.info__artist {
  font-size: 12px;
  color: var(--text-secondary);
}

.info__list {
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.info__list > div {
  display: grid;
  grid-template-columns: 92px 1fr;
  gap: 8px;
  align-items: baseline;
}

dt {
  font-size: 11.5px;
  color: var(--text-tertiary);
}

dd {
  margin: 0;
  font-size: 12px;
  user-select: text;
}

.info__mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10.5px;
}

.info__section {
  margin: 14px 0 8px;
  padding-top: 11px;
  border-top: 1px solid var(--separator);
  font-size: 12.5px;
  font-weight: 600;
}

.info__tag {
  margin-left: 5px;
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--accent-tint);
  color: var(--accent);
  font-size: 10px;
  font-weight: 600;
}

.info__note {
  margin: 12px 0 0;
  font-size: 11px;
  color: var(--text-tertiary);
}
</style>
