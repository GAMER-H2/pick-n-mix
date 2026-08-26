# Implementation notes for the remaining laundry list

Sketches only — enough to judge the size and spot the decisions that need
making before any code gets written.

---

## Medium: duplicate song handling

**Size:** ~1–2 days. Not hard, but it changes the library's central idea.

Today one row = one file. This needs one row = one *song*, with files hanging
off it. That is a schema change, not a tweak:

```sql
CREATE TABLE songs  (id, title, artist, album, ..., preferred_file_id);
CREATE TABLE files  (id, song_id, location, format, bitrate, sample_rate, ...);
```

Grouping key: the existing `match_key` (normalised artist|title|album) plus
duration within ~2 s, so a remix or a live cut is not folded into the studio
version. MusicBrainz recording id wins outright when both sides have one.

Quality ranking for `preferred_file_id`, in order: lossless before lossy
(FLAC/ALAC/WAV > everything), then bit depth × sample rate, then bitrate, then
file size as a tiebreak.

Metadata merge: prefer the most complete value per field rather than the
highest-quality file's, so a well-tagged MP3 can fill gaps a bare FLAC leaves.

**Decisions you'd need to make:**
- Should a duplicate ever be *shown*? A "2 versions" affordance on the row is
  useful; silent collapsing makes missing songs hard to explain.
- Do the two files have to agree on album? Same song on an album and on a
  compilation are arguably different entries.
- What happens when the preferred file goes missing — fall back silently, or
  flag it?

---

## Medium: settings page

**Size:** ~1 day for most of it; the device override is the awkward part.

Easy, all frontend plus a key in the existing `settings` table:
- **Theme** — the CSS already keys off `data-theme`; add system/light/dark.
- **Fade on pause/play** — the callback already ramps over ~8 ms; expose the
  constant as a setting.
- **Library management** — folder add/remove/rescan already exist as commands;
  this is a screen for them.

The two that need engine work:
- **Keep reverb on pause.** Right now pause stops the callback draining the
  ring, which is what makes it instant. To let a tail ring out, pause has to
  keep pulling and feed silence into the effect chain until it decays, then
  stop. Doable, but it undoes the "resumes exactly where it stopped" property
  unless you track how many frames of silence you pushed and rewind by that
  much.
- **Output device override.** `AudioEngine` picks the default device once at
  start-up and the ring is sized from its rate. Changing device means tearing
  down the stream, the ring and the worker, then reopening at the new rate and
  reloading the current track at its current position. Cleanest as a
  `restart(device_id)` on the engine; roughly half a day on its own.

---

## Medium: expanded EQ modal (Logic-style)

**Size:** ~2 days.

The DSP side already supports it — bands are arbitrary
`{kind, freq, gainDb, q, enabled}` and the advanced panel edits all of them.
What is missing is the *graph*.

- Compute the response by evaluating each biquad's transfer function at ~256
  log-spaced frequencies and summing the dB. That maths belongs in TypeScript
  so it redraws at pointer speed; it duplicates the coefficient formulas in
  `dsp.rs`, so a shared test-vector fixture is worth having to stop the two
  drifting.
- Draw the summed curve plus a faint per-band curve, as an SVG path.
- Each band is a draggable node: x = frequency (log), y = gain, scroll or
  vertical drag with a modifier = Q.
- Add an analyser overlay later — that needs the engine to publish FFT
  magnitudes, which is a separate chunk.

**Decisions:** how many bands (Logic has 8 including fixed HP/LP)? Does the
modal edit the same layer the panel is pointed at, or always global?

---

## Medium: crossfade controls

**Size:** the UI is small; the engine work underneath is not — see below.

The slider and the keyframe graph are a couple of days of frontend. But
neither means anything until the engine can play two tracks at once, which is
the large task. I'd build the engine part first and give it a single "crossfade
length" slider, then add the graph once there is something to graph.

---

## Medium: home page

**Size:** ~1–2 days. The easiest of the medium items, but it needs history
that is not being recorded yet.

Nothing currently tracks plays. That is the whole job; the page itself is a
few shelves reusing the existing card and row components.

```sql
CREATE TABLE plays (
    id         INTEGER PRIMARY KEY,
    track_id   TEXT NOT NULL,
    played_at  INTEGER NOT NULL,
    -- How much of it was actually heard, so skips do not count as plays.
    fraction   REAL NOT NULL
);
```

Write a row when a track ends or is skipped, from the same place in
`lib.rs` that already handles `TrackFinished`. Count it as a play only past
some threshold — 50% or 4 minutes, whichever comes first, is the usual rule.

The four shelves:
- **Recently played** — `SELECT DISTINCT track_id ... ORDER BY played_at DESC`.
- **Most played playlists** — needs the play row to record the playback
  context too, so add a nullable `context_id`.
- **Archive mix** — "things you have not heard in a while": tracks whose most
  recent play is older than N months, shuffled. Cheap and genuinely nice.
- **Simple recommendations** — no ML needed and none wanted. "More from an
  artist you played this week", "the rest of this album", "not played since
  2023". Rules like these are explainable, which matters more than clever.

**Decisions:**
- Does a skip count as a play? (Suggest: recorded, but flagged by `fraction`.)
- Is history private/clearable? A "clear listening history" button is worth
  having from the start rather than retrofitting.
- Do shelves refresh live, or on app start? Live means invalidating on every
  track end; on-open is simpler and nobody notices.

---

## Large: remote libraries (Navidrome and Jellyfin) with sync

**Size:** 1–2 weeks. Mostly plumbing, but the merge rules are where it gets
opinionated.

The `LibrarySource` trait and `Playable::Stream` already exist for this, and
`source_id` is on every track, so the shape is right.

### Per-server work

Both are documented HTTP APIs, so no SDK is needed — `reqwest` is already a
dependency.
- **Navidrome** speaks Subsonic: `ping`, `getArtists`, `getAlbumList2`,
  `getAlbum`, `stream`, `getCoverArt`. Auth is a salted token in the query
  string.
- **Jellyfin** has its own REST API: `/Users/{id}/Items` with
  `IncludeItemTypes=Audio`, `/Audio/{id}/stream`, `/Items/{id}/Images/Primary`.
  Auth is a token header.

### The engine change this forces

The same `MediaSource` refactor described under the mobile companion below —
`TrackDecoder` taking a `Box<dyn MediaSource>` rather than a path. For
streaming that means a range-requesting `Read + Seek` reader with a local cache
so seeking backwards does not refetch.

Simpler first version, worth shipping on its own: **download to a temporary
file, then play it**. A second or two of delay before a track starts, no
seeking problems, and about a day's work rather than a week.

Do the refactor once; it serves remote libraries, Android and iOS alike.

### Merging with the local library

This is the part to decide before writing code:
- The same song from two sources should collapse into one row — which is
  exactly the duplicate-handling work above. **Do that first**; remote sources
  make it mandatory rather than merely nice.
- Which source wins for playback? Suggest: local if present (instant, offline),
  remote otherwise, with a per-source priority the user can reorder.
- What happens offline? Remote tracks should stay visible and greyed, the same
  way unmatched playlist entries already are, rather than vanishing.
- Do playlists sync back to the server? Subsonic and Jellyfin both have their
  own playlist APIs, and they cannot express per-song mixer overrides. Suggest
  one-way import, and keep `.pnmx` as the source of truth.

**Credentials** belong in the OS keychain, not the settings table — the
`keyring` crate covers macOS, Linux (Secret Service) and Windows.

---

## Large: mobile companion (Android first)

**Shape:** the desktop is the workshop, the phone is the player. Authoring
happens in one place; the phone consumes what that produced, plus whatever
local files it has of its own.

That single decision is what makes this tractable rather than a second full
app. Sync is hard because of conflicting writes; with one writer there are
almost none.

### The one refactor that unlocks everything

`TrackDecoder::open` takes a `&Path` and does `File::open`. But the very next
line hands Symphonia a `Box<dyn MediaSource>` — the abstraction is already
there, we are just always passing a file.

```rust
// today
pub fn open(path: &Path, target_rate: u32) -> Result<Self>

// what mobile (and remote libraries) need
pub fn open_source(source: Box<dyn MediaSource>, hint: Hint, target_rate: u32) -> Result<Self>
```

`AudioEngine`'s `Cmd::Load` carries a `PathBuf` and would carry the source
instead. That is perhaps a day's work, and the same day buys:

- Android `content://` URIs from MediaStore and the Storage Access Framework,
- HTTP range-request streaming for Navidrome and Jellyfin,
- iOS's sandboxed file handles.

Do this once, before either the remote or the mobile work.

### What already ports untouched

About 3,000 lines — the whole interesting part:

| Module | Lines | Why it ports |
| --- | --- | --- |
| `audio/dsp.rs` | 900 | Pure arithmetic over buffers |
| `player.rs` | 598 | Queue logic, no I/O |
| `audio/engine.rs` | 542 | Only `Cmd::Load` mentions a path |
| `playlist.rs` | 387 | `serde` plus one file read |
| `audio/params.rs` | 355 | The cascade, pure data |
| `library/model.rs` | 190 | Identity and matching |

`library/scan.rs` is the one module that genuinely does not port: `walkdir`
over a directory is meaningless under scoped storage.

### What syncs, and how

Three different things travel, and they want three different answers:

| What | Size | Where it comes from |
| --- | --- | --- |
| Audio | Large | The server, or local files on the phone |
| Playlists (`.pnmx`) | Tiny | The desktop |
| Library identity | Small | Derived on the phone |

**Audio does not need syncing** — that is what Navidrome or Jellyfin is for,
and on Android local files are already on the device.

**Playlists do.** Navidrome and Jellyfin cannot store a `.pnmx`; their playlist
APIs have no room for mixer overrides or per-entry settings. Options:

1. **LAN pairing with the desktop.** The desktop advertises over mDNS and
   serves the playlist folder on a small authenticated endpoint; the phone
   pulls on open. No third party, no cloud, and the desktop is on whenever you
   are authoring anyway. *Recommended.*
2. **A shared folder** — Syncthing, a NAS mount, a cloud drive. Zero code, but
   the user has to set it up.
3. Stuffing them into the media server somehow. Don't.

Ship (2) first because it is free, and add (1) when it starts to annoy.

**Library identity needs nothing new.** This is the payoff from how `.pnmx`
was designed: entries identify songs by title, artist, album, duration and
MusicBrainz id, with the file path as a hint that is ignored when wrong. A
playlist authored on your desktop against `/Users/mh968/Music/...` already
resolves on the phone against a `content://` URI or a Jellyfin item id, with
no conversion step and no changes to the format. The existing
`Db::resolve` is the whole mechanism.

### Read-mostly, but not read-only

"No playlist editing on mobile" is the right instinct, but the useful line is
not editing versus viewing — it is **authoring versus performing**:

| Mobile can | Mobile cannot |
| --- | --- |
| Play, queue, reorder the queue | Reorder, rename or delete a playlist |
| Append to a playlist | Edit a playlist's mixer override |
| Drive the global DJ mixer live | Edit a song's per-playlist override |
| Favourite, download for offline | Author crossfades |

Appending is worth allowing: it is the one edit people actually want on a
phone, and it is the only edit that merges cleanly. Two devices appending to
the same playlist is a set union, not a conflict. Anything positional —
reordering, deleting — is where merges get genuinely hard, and that is exactly
what the desktop is for.

Everything the phone writes that is *not* an append (play history, queue state,
downloads) stays in the phone's own database and never goes back into the
`.pnmx`. One writer for the file, and the format stays clean.

Driving the global mixer on the phone is fine and desirable — it is a live
control, like volume. Editing the playlist and entry layers is authoring, so it
stays on the desktop. That split falls straight out of the cascade that already
exists.

### The Android local library

Scoped storage means two sources, both giving `content://` URIs rather than
paths:

- **MediaStore** (`MediaStore.Audio.Media`) — everything the system has already
  indexed. It hands over title, artist, album, duration and track number for
  free, so no tag parsing is needed for the common case.
- **Storage Access Framework** (`ACTION_OPEN_DOCUMENT_TREE`, with a persisted
  permission grant) — for a folder the user picks, including an SD card.

`lofty` is still wanted for the things MediaStore does not expose — ReplayGain
tags and embedded artwork — and it can read from the URI's byte stream once the
`MediaSource` refactor is in.

This becomes an `AndroidSource` implementing the existing `LibrarySource`
trait. `scan.rs` stays desktop-only.

### Merging local and remote

The phone will see the same song twice: once on the server, once on disk. That
makes **duplicate handling a prerequisite, not a nice-to-have** — the same work
already on the medium list, which is another reason to do it early.

Resolution order for playback, once songs are grouped:

1. A local file, if there is one — instant, offline, no mobile data.
2. A downloaded-for-offline copy of a remote track.
3. The remote stream.

Which is worth exposing as a per-song "available offline" state rather than
hiding, so it is obvious what will work on a train.

### Background playback and the OS

This is the part that is not a Rust problem, and the part most likely to eat
the time:

- Android needs a foreground `MediaSessionService` with a persistent
  notification, or playback is killed when the app is backgrounded.
- **Audio focus** must be handled: duck for a notification, pause for a call,
  stop when headphones are unplugged. Easy to forget and immediately obvious
  when missing.
- `media.rs` is the natural place for this to plug in — it already owns
  "tell the OS what is playing", so it gains an Android implementation
  alongside the MPRIS and macOS ones.

### Interface

The existing full-screen now-playing view is already most of the way to a phone
screen. The rest:

- Tab bar instead of the sidebar; playlists live under Library.
- Now playing as a sheet dragged up from a mini player.
- The mixer as a bottom sheet, not a side panel.
- **Touch targets.** The knobs are 46 px, which is borderline; the EQ faders
  are about 20 px wide, which is too small. Both need reworking for touch, and
  the EQ probably wants to become one fader at a time with a band selector.

### Cost and staging

1. `MediaSource` refactor — ~1 day, desktop work, unblocks everything.
2. Duplicate handling — 1–2 days, desktop work, becomes mandatory here.
3. Remote library (download-then-play first) — ~1 week.
4. Android shell: audio backend, MediaSession, focus handling — 1–2 weeks.
5. Android library sources and the touch UI — 1–2 weeks.

Steps 1–3 are all desktop features you would want anyway, which is the main
argument for this order: nothing is wasted if mobile stalls.

**iOS**, on this model, is the same app minus the local library, since the
sandbox makes a local music folder close to pointless. It is a pure server
client. Worth doing only after Android proves the shape, and it brings its own
tax: `AVAudioSession` handling, the audio background entitlement, and App Store
review.

### Decisions worth settling before any code

- Does the phone ever author, or is appending the hard ceiling?
- Is the desktop required to be on for playlists to sync, or is a shared folder
  the primary channel?
- Does a downloaded-for-offline track count as "local" for playback priority,
  and is there a storage cap on that cache?
- Do the ambience filter files sync too? They are the one mixer input that is
  a binary blob rather than a number, and a playlist referencing "rain" on a
  phone with no `rain.wav` should degrade quietly.

---

## Large: per-playlist custom crossfades and bounce-to-file

**Size:** the biggest thing on the list. Weeks, not days. Worth splitting.

### What has to change in the engine

Today there is one `TrackDecoder` and one DSP `Chain`. A crossfade needs
**two voices** playing at once, each with its own decoder, varispeed resampler
and effect chain, summed before the master gain and limiter. That is a real
refactor of `engine.rs`: `Voice { decoder, chain, gain }`, with the worker
mixing `Vec<Voice>` instead of one.

Sampled inserts (a chunk of a song, or an imported file, possibly looping) are
just more voices, fed from memory instead of a decoder. Once voices exist,
inserts are cheap.

### The timeline

A transition needs its own document, stored inside the playlist file (rule 2
of the format means this can be added without a version bump):

```json
"transitions": [
  {
    "fromIndex": 0, "toIndex": 1,
    "outStartSecs": 182.0, "outCurve": "equalPower",
    "inStartSecs": 12.5,  "inCurve": "equalPower",
    "lengthSecs": 8.0,
    "inserts": [
      { "source": {"kind": "file", "path": "riser.wav"},
        "atSecs": 4.0, "loop": false, "gainDb": -6,
        "effects": { "reverb": {"enabled": true, "mix": 0.4} } }
    ]
  }
]
```

**Decisions worth settling early:**
- Are transition times absolute in each song, or relative to the join? Absolute
  is easier to reason about and survives a track being re-timed.
- Equal-power vs linear fades, and is the curve per-side or shared?
- Do effects on an insert layer over the global mixer or replace it? (The
  cascade already answers this: layer.)
- What happens when a transition references a song that is not in the
  listener's library — skip the transition, or the whole playlist?

### Bounce to file

Once voices exist this is mostly plumbing, and it is the part I would do
**offline rather than in real time**: run the same graph faster than real time
into a buffer, then encode.

- WAV: trivial, write it yourself.
- FLAC: needs an encoder — `flacenc` is pure Rust, so the self-contained goal
  survives.
- MP3: needs LAME via `mp3lame-encoder` (C, statically linkable) — check the
  licence position before committing to it.

"Retains all of this and other settings" is worth pinning down: a bounced file
is a flat rendering, so mixer settings are baked in and no longer adjustable.
If you want them to stay editable, that is a project file, not an MP3.

---

## Large: live visualiser in the full-screen view

**Size:** a week for something good, and it is open-ended by nature.

The view is already built and the artwork backdrop is in place, so this slots
in as a `<canvas>` behind the same veil.

### Getting the data out

The engine needs to publish something to react to. Cheapest useful version: the
DSP worker already touches every block, so have it write an FFT (say 512 bins,
~20 ms) plus RMS and a beat flag into a lock-free slot the UI polls at frame
rate. Do **not** send this over Tauri events at 60 Hz — one shared buffer read
on demand.

Beat detection: energy-flux onset detection over the low bands is enough for
pulsing, and is maybe 60 lines.

### Rendering

2D via WebGL is the right call, not 3D. A fragment shader over a full-screen
quad, fed the FFT as a 1D texture plus the cover's average colours as uniforms,
gets pulsing, flowing and squash-and-stretch cheaply. Three.js would be a large
dependency for what is really one quad — plain WebGL2 or a thin wrapper is
enough. Budget the whole thing to one draw call and cap it at 60 fps, and stop
rendering entirely when the window is hidden or the view is closed, or it will
quietly eat the battery.

### Lyrics

Two separate problems:
- **Fetching** — LRCLIB is free, needs no key, and returns synced `.lrc`. Much
  easier than MusicBrainz here. Embedded `USLT`/`SYLT` tags are worth checking
  first, same as artwork.
- **Detection from audio** — genuinely hard, needs a speech model, and would
  undo "lightweight and self-contained". I would not.

So: synced lyrics where a source exists, nothing where it does not, and the
random-placement effect driven by the timestamps.

**Decisions:** does the visualiser have presets that follow the DJ mixer preset,
or is it independent? Should it react to the *processed* signal (so reverb and
lo-fi are visible) or the dry one? Processed is more fun and is what the tap
point above gives you for free.
