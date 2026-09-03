/**
 * What is playing right now, shared by the player bar and the full-screen
 * player.
 *
 * A mix has no current track: the engine holds one long timeline, so both
 * surfaces show the playlist in a song's place and name it as one thing.
 * The two render the same information with different markup (the bar inlines
 * links and a mix badge; the screen uses plain clamped headings), so only the
 * state is shared here — each surface keeps its own markup.
 */
import { computed } from "vue";
import { subtitleFor } from "@/lib/format";
import { usePlayerStore } from "@/stores/player";

export function useNowPlayingMeta() {
  const player = usePlayerStore();

  const track = computed(() => player.track);

  /**
   * The playlist being played as a mix, if that is what is playing.
   *
   * A mix has no current track: the engine holds one long timeline, so the
   * playlist stands in for the song and the songs appear as chapters along
   * the scrubber instead.
   */
  const mix = computed(() => player.masterMix);

  /** A mix plays as one thing, so it is named as one thing. */
  const title = computed(() => mix.value?.name ?? track.value?.title ?? "Nothing Playing");

  const subtitle = computed(() =>
    mix.value
      ? `Master mix · ${mix.value.trackCount} ${mix.value.trackCount === 1 ? "song" : "songs"}`
      : subtitleFor([track.value?.artist, track.value?.album]),
  );

  return { mix, track, title, subtitle };
}
