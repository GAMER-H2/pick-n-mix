/**
 * Mirrors the id derivation in `src-tauri/src/library/model.rs` so the frontend
 * can build album and artist links without a round trip. Both sides must use
 * the same FNV-1a over the same normalised strings.
 */

import type { Track } from "./types";

export function normalise(value: string): string {
  return value
    .toLowerCase()
    .split("")
    .filter((c) => /[\p{L}\p{N}\s]/u.test(c))
    .join("")
    .split(/\s+/)
    .filter(Boolean)
    .join(" ");
}

export function stableId(prefix: string, seed: string): string {
  // 64-bit FNV-1a using BigInt, since JS numbers cannot hold it exactly.
  const MASK = (1n << 64n) - 1n;
  let hash = 0xcbf29ce484222325n;
  for (const byte of new TextEncoder().encode(seed)) {
    hash = (hash ^ BigInt(byte)) & MASK;
    hash = (hash * 0x100000001b3n) & MASK;
  }
  return `${prefix}_${hash.toString(16).padStart(16, "0")}`;
}

export function albumArtistOf(track: Track): string {
  return track.albumArtist.trim() === "" ? track.artist : track.albumArtist;
}

export function stableAlbumId(track: Track): string {
  return stableId("al", `${normalise(albumArtistOf(track))}|${normalise(track.album)}`);
}

export function stableArtistId(track: Track): string {
  return stableId("ar", normalise(albumArtistOf(track)));
}
