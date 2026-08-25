# The `.pnmx` playlist format

A playlist is one plain JSON file. It is meant to be handed to someone else who
owns the same music, and to keep working as Pick n Mix grows.

## Example

```json
{
  "format": "pick-n-mix.playlist",
  "schemaVersion": 1,
  "id": "pl_bbfc09a6d3eebd67",
  "name": "Late Night Drive",
  "description": "For the motorway at 2am",
  "artwork": null,
  "createdAt": 1756141200,
  "updatedAt": 1756141200,
  "mixer": {
    "reverb": { "enabled": true, "mix": 0.4 }
  },
  "tracks": [
    {
      "title": "Come Together",
      "artist": "The Beatles",
      "album": "Abbey Road",
      "albumArtist": "The Beatles",
      "durationSecs": 259.0,
      "trackNumber": 1,
      "year": 1969,
      "musicbrainzRecordingId": null,
      "localPath": "/Users/someone/Music/Abbey Road/01 Come Together.flac",
      "mixer": null,
      "addedAt": 1756141200
    }
  ]
}
```

## Why entries look like that

`localPath` is a **hint, not an identity**. On the machine that receives the
file it will usually be wrong, so entries are matched against the local library
in descending order of confidence:

1. `musicbrainzRecordingId`, when both sides have one.
2. The normalised `artist|title|album` key — case, punctuation and repeated
   whitespace are ignored, so "The Beatles" and "the  beatles" agree.
3. `artist|title` alone, which catches the same song on a different release.

An entry that matches nothing is **kept and shown greyed out**, never dropped.
Add the music later and it lights up.

## The three compatibility rules

These are what let the format grow without breaking older files. They are
covered by tests in `src-tauri/src/playlist.rs`.

1. **Every field is optional.** Each has a serde default, so a file written by
   an older version — or by hand — still loads. The minimum viable playlist is
   `{"name": "x", "tracks": [{"title": "y"}]}`.

2. **Unknown fields are preserved.** Both the playlist and each entry carry a
   `#[serde(flatten)]` catch-all. A file that has been through a newer version
   and back out again loses nothing, so sharing between versions is lossless.

3. **`schemaVersion` only moves for breaking changes.** Adding a field is not
   breaking, because of rules 1 and 2. Bump it only when the meaning of an
   existing field changes.

### Adding a feature

Adding, say, a crossfade time means adding one optional field:

```rust
pub struct Entry {
    // ...
    pub crossfade_ms: Option<u32>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
```

No version bump, no migration. Old files load with `None`; older versions of
the app carry the new field through untouched in `extra`.

## Mixer overrides

`mixer` may appear at two levels, and both are partial. Resolution runs
global → playlist → track, section by section:

- A playlist-level `mixer` applies to everything played from that playlist.
- An entry-level `mixer` applies to that one song within that one playlist.
- A section nobody mentions falls through to the global mixer, then to defaults.

Because merging happens per section, a playlist that only sets `reverb` leaves
the listener's EQ exactly as they left it.

## Where files live

`~/Library/Application Support/com.picknmix.app/playlists/` on macOS. They are
ordinary files: copy one out to share it, drop one in to import it.
