# Pick n Mix

A local music player with a live DJ mixer: pitch, EQ, reverb, delay, lo-fi
crushing and ambience beds applied to your own files as they play.

## Running it

```bash
npm install
npm run tauri dev      # development, with hot reload
npm run tauri build    # release bundle
```

> Build the release binary with `npm run tauri build`, not a bare
> `cargo build --release`. Running cargo directly leaves Tauri's `dev` flag set,
> so the binary looks for the Vite dev server and opens a blank window.

## Testing

```bash
cd src-tauri && cargo test   # engine, library, playlists, live playback
npx vitest run               # shortcuts, mixer cascade, id parity
```

The playback tests open a real audio device at zero volume and skip themselves
if none is available.

## How it fits together

```
Vue UI  ──invoke/events──►  Tauri commands
                                 │
                    ┌────────────┼─────────────┐
                    ▼            ▼             ▼
                 Player      SQLite index   Playlist files
                (queue)      (a rebuildable  (.pnmx, shareable)
                    │          cache)
                    ▼
              Audio engine
   decode ─► varispeed ─► EQ ─► delay ─► reverb ─► lo-fi
          ─► ambience ─► gain ─► limiter ─► ring buffer ─► cpal
```

The audio worker owns decoding and DSP; the cpal callback only copies out of a
~120 ms ring buffer. Pausing stops the callback draining that ring, so it is
instant and resumes exactly where it stopped.

Pitch is **varispeed** — pitch and tempo move together, as decided in
`lofi_protoypes/rust_library_chat.md`. It is folded into the same resampling
pass that matches the file's rate to the device's, so there is only ever one
conversion.

## The mixer cascade

Settings resolve **global → playlist → track**, section by section:

- the mixer in the player bar edits the global layer;
- the mixer button on a playlist header edits that playlist;
- the mixer button on a row **inside a playlist** edits that song *within that
  playlist only*.

There is deliberately no per-song mixer outside a playlist: the same song in
another playlist, or played straight from the library, is unaffected.

A layer that only sets `reverb` leaves every other section inherited. See
[docs/playlist-format.md](docs/playlist-format.md).

## Your music

Point the app at a folder; tags, artwork and ReplayGain are read from the files
themselves. Nothing touches the network unless you pick **Look Up Online**,
which queries MusicBrainz and the Cover Art Archive.

Library sources sit behind a `LibrarySource` trait so Navidrome and Jellyfin can
be added without changing the UI or the player.

## Ambience filters

Rain, TV static, fireplace and the rest need audio files. Drop them into:

```
~/Library/Application Support/com.picknmix.app/filters/
```

named after the filter (`rain.wav`, `tv-static.flac`, …). Any extra file you add
becomes a filter of its own. Ones with no file are shown greyed out.

## Keyboard

| Key | Action |
| --- | --- |
| <kbd>Space</kbd> / <kbd>K</kbd> | Play / pause |
| <kbd>L</kbd> | Next track |
| <kbd>J</kbd> | Previous track |
| <kbd>←</kbd> / <kbd>→</kbd> | Scrub 5 seconds |
| <kbd>↑</kbd> / <kbd>↓</kbd> | Volume |
| <kbd>Esc</kbd> | Close the full-screen view, then the queue panel |

The queue button opens the full-screen now-playing view; hold <kbd>Shift</kbd>
for the compact side panel instead. In either, drag a row by its handle to
reorder and click its artwork to jump to it.

Shortcuts are ignored while a text field has focus.
