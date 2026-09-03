# Pick n Mix

A local music player with a live DJ mixer: pitch, EQ, reverb, delay, lo-fi
crushing and ambience beds applied to your own files as they play.

## Running it

```bash
npm install
npm run tauri dev      # development, with hot reload
npm run tauri build    # release bundle
npm run build:appimage # Linux AppImage only
```

> Build the release binary with `npm run tauri build`, not a bare
> `cargo build --release`. Running cargo directly leaves Tauri's `dev` flag set,
> so the binary looks for the Vite dev server and opens a blank window.

### AppImage on rolling-release distributions

`build:appimage` exists because it sets `NO_STRIP=1`. linuxdeploy — which
Tauri downloads to assemble the AppDir — ships its own `strip` from binutils
2.35, released in 2020. Current toolchains emit `SHT_RELR` (`.relr.dyn`)
relative-relocation sections, which that `strip` does not recognise, so it
fails on nearly every system library it is asked to process:

```
strip: .../libzstd.so.1: unknown type [0x13] section `.relr.dyn'
```

linuxdeploy treats those failures as fatal and Tauri reports only
`failed to run linuxdeploy`. Setting `NO_STRIP=1` skips that step. Nothing is
lost: the Rust binary is already stripped by `strip = true` in the release
profile, and the copied system libraries are stripped by the distribution.

Because an AppImage bundles the build host's GTK, glib and ICU, one built on a
rolling distribution runs on that distribution but not on older ones. Build it
in a container matching the oldest target if you need portability — but note
that the reverse also holds, so a container-built AppImage may fail to start on
a much newer host, where the bundled libraries collide with the host's graphics
drivers and EGL initialisation aborts.

## Testing

```bash
cd src-tauri && cargo test   # engine, library, playlists, live playback
npx vitest run               # shortcuts, mixer cascade, EQ curve, id parity
```

The playback tests open a real audio device at zero volume and skip themselves
if none is available.

The EQ graph re-derives the engine's filter designs in TypeScript so it can
redraw while a node is dragged, so both languages assert against one generated
fixture. If you change a coefficient formula in `audio/dsp.rs`, regenerate it:

```bash
cd src-tauri && PNM_WRITE_FIXTURES=1 cargo test --test eq_parity
```

The Rust side is authoritative — it is what actually processes the audio — so
a TypeScript failure after regenerating means the graph needs updating to
match, not the other way round.

## How it fits together

```
Vue UI  ──invoke/events──►  Tauri commands
                                 │
                    ┌────────────┼─────────────┐
                    ▼            ▼             ▼
                 Player      SQLite library  Playlist files
                (queue)      (songs + file   (.pnmx, shareable)
                    │          versions)
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

## Home

The home page is built from what you actually listen to: three generated
mixes, a shelf of recommendations, and the playlists you last played from.

- **Replay Mix** — songs you have gone back to repeatedly in the last month.
- **Archive Mix** — songs you once played a lot but have not lately.
- **Discover Mix** — corners of your library you have barely touched.

Each mix opens as a playlist, and can be pinned into the sidebar or saved into
a real playlist. Saving takes a copy, which is the only way to keep a mix past
the next regeneration.

Mixes are built once when the app starts and then held, so a mix will not
reshuffle while you are listening to it. **Regenerate** builds them again.

Listening history is local, and skipping is not listening: a song has to be
heard for 25 seconds before it counts. Anything shorter is recorded but
ignored by every shelf, so hunting through a playlist does not distort what
gets recommended.

## The equaliser

The player bar's mixer shows a fader per band. The **expand** button on the
advanced panel's EQ section opens the full graph: eight bands by default —
high-pass, low shelf, four peaks, high shelf, low-pass — over a live spectrum
of the processed output.

Drag a node to move it (horizontally for frequency, vertically for gain),
scroll over it to change Q, and double-click it to flatten it. Bands can be
added up to twelve, removed, and switched between all five filter types. Built-in
EQ curves can be applied from the graph, and custom curves can be saved there or
managed under **Settings → Mixer**. Like every other section, the EQ edits
whichever layer the panel is pointed at.

The two pass filters start disabled: unlike a shelf or a peak, a pass filter
has no flat setting, so an enabled one would colour every mix by default.

The spectrum is only computed while the graph is open, and is tapped after the
whole effect chain — so it shows what is actually leaving the app, EQ included.

## Your music

Point the app at a folder; tags, artwork and ReplayGain are read from the files
themselves. Nothing touches the network unless you pick **Look Up Online**,
which queries MusicBrainz and the Cover Art Archive.

Files representing the same artist, title and album within two seconds are
collapsed into one song. Matching MusicBrainz recording IDs are stronger
evidence, but album agreement is still mandatory; untagged, albumless files are
only combined when both carry the same recording ID. Pick n Mix automatically
plays the best available version—lossless first, then bit depth and sample rate,
bitrate and size—while merging richer tags from the other files.

Right-click a collapsed song and choose **Show duplicate files** to compare or
preview versions, override automatic selection, relink missing files, or move a
version to the operating system Trash/Recycle Bin. Missing preferred files fall
back silently and become preferred again if restored. These preferences and
missing-file records make the SQLite library durable user state rather than a
fully disposable cache; folders and audio files remain the source of the music.

Library sources sit behind a `LibrarySource` trait so Navidrome and Jellyfin can
be added without changing the UI or the player.

## Atmospheres

Looping background beds mixed under the music. Rain, Fireplace, Forest, City,
Ocean and Vinyl Crackle ship with the app; while one is playing its button
animates as the thing it is, so an active atmosphere is recognisable at a
glance.

Add your own from **Settings → Mixer**, or drop audio into:

```
~/Library/Application Support/com.picknmix.app/filters/
```

The file name becomes the atmosphere (`coffee-shop.wav` → "Coffee Shop"). A
file named after a built-in replaces it. Built-ins can be hidden rather than
deleted; imported ones can be deleted outright.

## Playback settings

**Settings → Playback** covers how audio starts, stops and leaves the machine.

- **Fade on pause and play** ramps instead of cutting, in either direction or
  both.
- **Keep reverb on pause** lets reverb and delay ring out after you pause
  rather than stopping dead with the music. Your place is kept exactly: the
  audio already queued for the sound card is discarded and the track wound back
  to what you actually heard, so resuming continues from there. With neither
  effect on there is nothing to ring out, and pause stays instant.
- **Output device** moves playback to another output. The new device may run at
  a different sample rate, so the current track is reloaded at its position —
  a brief gap, then it carries on. A device that is unplugged falls back to the
  system default, and is picked up again when it returns.

## System integration

Transport keys and the desktop media widget work through
[souvlaki](https://crates.io/crates/souvlaki): MPRIS on Linux (KDE's panel and
media keys) and `MPNowPlayingInfoCenter` on macOS. On macOS this needs the
bundled `.app`, not the bare binary, for the system to recognise the process as
a media app.

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
reorder and click its artwork to jump to it. The full-screen view is a route,
so back and forward close and reopen it like any other page.

Hold <kbd>Shift</kbd> on the mixer button to skip the bubble and open the
advanced panel. Hold <kbd>Shift</kbd> while dragging the volume slider to
bypass the quarter detents.

Shortcuts are ignored while a text field has focus.
