import { computed, toValue, type MaybeRefOrGetter } from "vue";
import { formatTotal } from "@/lib/format";
import type { Track } from "@/lib/types";

/**
 * Song count and total duration for a collection of tracks, plus the standard
 * meta string ("3 songs · 12 min") shown under a collection's title.
 *
 * Views that need a richer line (an artist's album count, a playlist's missing
 * count) compose their extra parts around `meta` or use `count`/`totalDuration`
 * directly.
 */
export function useCollectionMeta(tracks: MaybeRefOrGetter<Track[]>) {
  const count = computed(() => toValue(tracks).length);
  const totalDuration = computed(() =>
    toValue(tracks).reduce((sum, track) => sum + track.durationSecs, 0),
  );
  const meta = computed(
    () =>
      `${count.value} ${count.value === 1 ? "song" : "songs"} · ${formatTotal(totalDuration.value)}`,
  );

  return { count, totalDuration, meta };
}
