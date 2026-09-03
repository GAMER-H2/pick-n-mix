import { defineStore } from "pinia";
import { computed, ref } from "vue";
import * as api from "@/lib/api";
import type {
  MasterMixNowPlaying,
  PlaybackSnapshot,
  PlayContext,
  QueueView,
  Repeat,
  Track,
} from "@/lib/types";

export const usePlayerStore = defineStore("player", () => {
  const snapshot = ref<PlaybackSnapshot>({
    playing: false,
    positionSecs: 0,
    durationSecs: 0,
    volume: 1,
    speed: 1,
    limiterReductionDb: 0,
    deviceName: "",
    deviceSampleRate: 48000,
    stream: null,
  });
  const track = ref<Track | null>(null);
  const queue = ref<QueueView>({
    items: [],
    currentIndex: null,
    upcoming: [],
    shuffle: false,
    repeat: "off",
    context: null,
  });

  /**
   * The playlist being played as a mix, when one is.
   *
   * A mix has no current track — the engine holds one long timeline and the
   * queue is empty — so this is what the player bar shows in a track's place.
   */
  const masterMix = ref<MasterMixNowPlaying | null>(null);

  /** Set while the user drags the scrubber, so ticks do not fight the drag. */
  const scrubbing = ref(false);
  const scrubPosition = ref(0);

  /** Level to restore on unmute. Null when not muted. */
  const mutedFrom = ref<number | null>(null);

  const playing = computed(() => snapshot.value.playing);
  const position = computed(() =>
    scrubbing.value ? scrubPosition.value : snapshot.value.positionSecs,
  );
  const duration = computed(
    () =>
      snapshot.value.durationSecs ||
      track.value?.durationSecs ||
      masterMix.value?.durationSecs ||
      0,
  );

  const progress = computed(() =>
    duration.value > 0 ? Math.min(1, position.value / duration.value) : 0,
  );
  /** Something is loaded, so the transport means something. */
  const hasPlayback = computed(() => track.value !== null || masterMix.value !== null);

  function applySnapshot(next: PlaybackSnapshot) {
    snapshot.value = next;
  }

  async function refresh() {
    const [snap, current, q, mix] = await Promise.all([
      api.playbackState(),
      api.currentTrack(),
      api.queueState(),
      api.masterMixNowPlaying(),
    ]);
    snapshot.value = snap;
    track.value = current;
    queue.value = q;
    masterMix.value = mix;
  }

  /** Ask again which mix, if any, is playing — cheaper than a full refresh. */
  async function refreshMasterMix() {
    masterMix.value = await api.masterMixNowPlaying();
  }

  async function refreshQueue() {
    queue.value = await api.queueState();
  }

  async function toggle() {
    // Optimistic, so the button responds on the same frame it is pressed.
    if (hasPlayback.value) {
      snapshot.value = { ...snapshot.value, playing: !snapshot.value.playing };
    }
    await api.togglePlay();
  }

  async function playTracks(
    tracks: Track[] | string[],
    startIndex = 0,
    context: PlayContext | null = null,
  ) {
    const trackIds = tracks.map((t) => (typeof t === "string" ? t : t.id));
    await api.playTracks({ trackIds, startIndex, context });
  }

  async function next() {
    await api.nextTrack();
  }

  async function previous() {
    await api.previousTrack();
  }

  async function seek(seconds: number) {
    snapshot.value = { ...snapshot.value, positionSecs: seconds };
    await api.seek(seconds);
  }

  const muted = computed(() => snapshot.value.volume === 0);

  async function setVolume(volume: number) {
    // Dragging the slider is an explicit choice, so it cancels a mute.
    mutedFrom.value = null;
    snapshot.value = { ...snapshot.value, volume };
    await api.setVolume(volume);
  }

  /**
   * Mute, remembering where the level was. Unmuting restores it; unmuting
   * something that was already silent goes to full, since restoring silence
   * would look like the button had done nothing.
   */
  async function toggleMute() {
    if (snapshot.value.volume > 0) {
      const previous = snapshot.value.volume;
      snapshot.value = { ...snapshot.value, volume: 0 };
      await api.setVolume(0);
      mutedFrom.value = previous;
      return;
    }
    const restored = mutedFrom.value && mutedFrom.value > 0 ? mutedFrom.value : 1;
    mutedFrom.value = null;
    snapshot.value = { ...snapshot.value, volume: restored };
    await api.setVolume(restored);
  }

  async function setShuffle(enabled: boolean) {
    await api.setShuffle(enabled);
  }

  async function cycleRepeat() {
    const order: Repeat[] = ["off", "all", "one"];
    const nextMode = order[(order.indexOf(queue.value.repeat) + 1) % order.length];
    await api.setRepeat(nextMode);
  }

  return {
    snapshot,
    track,
    masterMix,
    queue,
    scrubbing,
    scrubPosition,
    mutedFrom,
    muted,
    playing,
    position,
    duration,
    progress,
    hasPlayback,
    applySnapshot,
    refresh,
    refreshQueue,
    refreshMasterMix,
    toggle,
    playTracks,
    next,
    previous,
    seek,
    setVolume,
    toggleMute,
    setShuffle,
    cycleRepeat,
  };
});
