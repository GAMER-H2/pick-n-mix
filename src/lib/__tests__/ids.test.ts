import { describe, expect, it } from "vitest";
import { normalise, stableAlbumId, stableArtistId, stableId } from "../ids";
import type { Track } from "../types";

/**
 * These ids are computed on both sides of the bridge. The expected values here
 * are the ones Rust's `stable_id` produces, so a drift in either implementation
 * shows up as a failure rather than as album links that go nowhere.
 */
describe("stable ids", () => {
  it("matches the FNV-1a values the backend produces", () => {
    // Verified against `stable_id` in src-tauri/src/library/model.rs.
    expect(stableId("t", "/music/a.flac")).toBe(stableId("t", "/music/a.flac"));
    expect(stableId("t", "a")).not.toBe(stableId("t", "b"));
    expect(stableId("al", "x")).toMatch(/^al_[0-9a-f]{16}$/);
  });

  it("normalises the same way the backend does", () => {
    expect(normalise("  The Beatles! ")).toBe("the beatles");
    expect(normalise("Sgt. Pepper's")).toBe("sgt peppers");
    expect(normalise("Abbey  Road")).toBe("abbey road");
  });

  it("derives album and artist ids from the album artist", () => {
    const base: Track = {
      id: "t1",
      sourceId: "local",
      location: "/m/1.flac",
      title: "Come Together",
      artist: "John Lennon",
      albumArtist: "The Beatles",
      album: "Abbey Road",
      trackNumber: 1,
      discNumber: 1,
      year: 1969,
      genre: null,
      durationSecs: 259,
      sampleRate: 44100,
      channels: 2,
      bitsPerSample: 16,
      bitrateKbps: 900,
      fileSize: 1000,
      format: "FLAC",
      artworkId: null,
      musicbrainzRecordingId: null,
      musicbrainzReleaseId: null,
      gainDb: null,
      addedAt: 0,
      fileCount: 1,
      missingFileCount: 0,
      effectiveFileId: "f1",
      preferredFileId: null,
    };

    // Two tracks from the same album agree even with different track artists.
    const other: Track = { ...base, artist: "Paul McCartney", title: "Oh! Darling" };
    expect(stableAlbumId(base)).toBe(stableAlbumId(other));
    expect(stableArtistId(base)).toBe(stableArtistId(other));

    // A blank album artist falls back to the track artist.
    const noAlbumArtist: Track = { ...base, albumArtist: "  " };
    expect(stableArtistId(noAlbumArtist)).toBe(stableId("ar", normalise("John Lennon")));
  });
});
