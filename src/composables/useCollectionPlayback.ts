import { usePlayerStore } from "@/stores/player";
import type { PlayContext, Track } from "@/lib/types";

/**
 * The play idioms shared by every collection page.
 *
 * Clicking the row that is already playing toggles it rather than restarting
 * it from the beginning; shuffle is "turn shuffle on, then play from the top".
 * Collections that play through the backend (playlists, mixes) pass their own
 * `play` closure to `playOrToggle`; list-backed ones use `playFromList`.
 */
export function useCollectionPlayback() {
  const player = usePlayerStore();

  /** Toggle playback when `isCurrent`, otherwise run `play`. */
  async function playOrToggle(isCurrent: boolean, play: () => Promise<unknown>) {
    if (isCurrent) {
      await player.toggle();
      return;
    }
    await play();
  }

  /** Toggle if that row is the current one, otherwise start the list from it. */
  async function playFromList(tracks: Track[], index: number, context: PlayContext) {
    const track = tracks[index];
    if (track && player.track?.id === track.id) {
      await player.toggle();
      return;
    }
    await player.playTracks(tracks, index, context);
  }

  /** Turn shuffle on, then play — the meaning of the header's Shuffle button. */
  async function shuffleAndPlay(play: () => Promise<unknown>) {
    await player.setShuffle(true);
    await play();
  }

  return { player, playOrToggle, playFromList, shuffleAndPlay };
}
