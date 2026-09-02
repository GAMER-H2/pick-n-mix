# Implementation notes for the remaining laundry list

Sketches only — enough to judge the size and spot the decisions that need
making before any code gets written.

---

## Medium: duplicate song handling — **done**

**Implemented:** logical songs now own ranked file versions, merged metadata,
manual preference, missing-file fallback, temporary comparison previews,
relink/copy restoration, and cross-platform Trash deletion.

One public row now represents one *song*, with physical versions hanging off
it. Existing file-based IDs migrate through aliases so playlists and references
continue to resolve:

```sql
CREATE TABLE songs      (id, title, artist, album, ..., preferred_file_id);
CREATE TABLE track_files(id, song_id, location, format, bitrate, sample_rate, ...);
```

Grouping key: the existing `match_key` (normalised artist|title|album) plus
duration within ~2 s, so a remix or a live cut is not folded into the studio
version. MusicBrainz recording id wins outright when both sides have one.

Quality ranking for `preferred_file_id`, in order: lossless before lossy
(FLAC/ALAC/WAV > everything), then bit depth × sample rate, then bitrate, then
file size as a tiebreak.

Metadata merge: prefer the most complete value per field rather than the
highest-quality file's, so a well-tagged MP3 can fill gaps a bare FLAC leaves.

**Settled behavior:**
- Duplicates are exposed through **Show duplicate files**, with comparison,
  temporary previews, preference controls, relinking and Trash actions.
- Album agreement is mandatory. Albumless files only combine when both have the
  same non-empty MusicBrainz recording ID.
- A missing preferred file falls back silently, while the context-menu warning
  and modal explain what is missing and allow it to be restored.

---

## Medium: settings page — **done**

**Implemented:** a global Settings workspace opened from the new cog beside the
history buttons. macOS keeps the cog with Back/Forward, while the Linux custom
title bar moves it to the opposite edge. The modal uses the same elevated,
blurred treatment as the expanded EQ, with Theme, Playback, Recommendations,
Mixer and Library in a fixed left rail and the selected pane centred beside it.

Preferences are one camel-case `AppPreferences` document in the existing SQLite
`settings` table (`app.preferences`). Rust supplies defaults and clamps user
input, while `stores/settings.ts` applies appearance immediately and rolls an
optimistic change back if persistence fails.

The panes use the app's own vocabulary rather than a parallel set of controls
private to this modal: `SelectMenu` instead of native `<select>` (which renders
as the platform widget and sits badly against everything else), the global
pill-button styles instead of a second square-button family, `.text-field`'s
focus ring on inputs, `--bg-sidebar` and the accent-tinted active row from the
real sidebar, and the shared radius scale throughout.

**What shipped:**
- **Theme** follows system/light/dark and derives the hover, active and tint
  tokens from a persisted custom accent rather than changing only `--accent`.
- **Pause/play fade** defaults off. `fadeMode` is `off`, `play`, `pause` or
  `both`; the output callback stores that as a directional atomic bitmask and
  chooses its 12 ms ramp independently when rising and falling. Turning the
  control off is genuinely immediate rather than merely hiding the option.
- **Recommendations** exposes mix length and the Replay/Archive/Discover day and
  play thresholds. `AppState::generate_mix` and the SQL queries consume those
  values, invalidate held mixes when they change, and refresh the visible Home
  shelves immediately.
- **Listening history** reuses `QueueList` in a non-reorderable mode: same art,
  play affordance and context menu as the queue, but no grip or move path. Each
  row adds Played/Skipped, timestamp and listened duration; removed-library
  songs remain visible and clearable. Playing a row closes Settings. Per-song
  and confirmed clear-all commands invalidate recommendations, and resetting
  the in-flight `PlayTracker` prevents pre-clear progress being written back
  when the current song ends.
- **Preset management** opens the real Advanced DJ Mixer as a sidecar within the
  Settings workspace. `stores/presetEditor.ts` owns an isolated resolved draft,
  so turning knobs does not alter live playback and the first backend write is
  the explicit Save. Custom presets update by stable id; a compiled built-in is
  immutable and Save creates a custom copy instead. Built-ins can be hidden and
  restored, with hidden IDs persisted separately from the preset files.
- **Ambience filters** now carry an explicit `builtIn` flag. Built-ins can be
  hidden/restored without deleting their audio or stopping an active bed;
  imported custom files can be deleted. Hidden active filters remain visible in
  the mixer until switched off. Import and catalogue refresh use the existing
  app-data filters directory.
- **Library** manages local folders and rescans through the existing commands.
  Navidrome and Jellyfin are deliberate preview cards describing merged-source
  sync and keychain-backed credentials; they do not collect or fake a server
  configuration before remote playback exists.

`QueueList` gained nullable rows, metadata slots and optional reorder/remove
controls rather than growing a separate history-only list. `ContextMenuState`
also gained an optional `onSelect` hook so a menu opened above Settings can close
the workspace only after an action is chosen, not merely on right-click.

**Keep reverb on pause** is now live. The awkward part was never rendering the
tail; it was that the ring already holds up to `RING_MILLIS` of *processed
music* that has not been heard yet. Letting that through makes pause feel late,
and discarding it loses the listener's place. So pausing with a tail does both:
the ring is flushed, and the decoder is wound back to `decoded_secs - queued`
— the position accounting the progress bar already did — which is exactly what
was heard. The worker then pushes silence through the chain until the output
falls below an audible floor or an eight-second budget runs out, and
`tail_active` keeps the callback consuming at full gain even though `playing`
is already false. Resuming mid-tail flushes again so leftover tail cannot play
in front of the music. A chain with neither reverb nor delay skips all of this,
so pause stays instant when there is nothing to ring out.

**Output-device override** is live too, without the wholesale
`AudioEngine::restart` the sketch assumed. The cpal stream is not `Send`, so it
already lived on its own thread; that thread now loops on a reopen channel
instead of sleeping forever, and dropping the old stream is what closes the
device. The worker is handed the new ring through `Cmd::Rebind` rather than
being torn down, so the queue, mixer and every other bit of state survive — it
only re-prepares the chains, limiter, ambience and analyser at the new rate.
Voices *are* dropped, because a decoder resamples to the rate it was opened
with, so the command layer reloads the current track at its position and play
state. A device that fails to open falls back to the default rather than
leaving the app silent, and a saved device that is no longer plugged in does
the same at startup while the preference keeps pointing at it.

**Still open:** nothing on this item.

---

## Medium: packaged atmospheres — **done**

**Implemented:** the six royalty-free beds in `audio_assets/` (Rain, Fireplace,
Forest, City, Ocean and Vinyl Crackle) are bundled as Tauri resources under
`audio_assets/`, then resolved into the existing ambience catalogue at startup.
The app does not copy packaged audio into mutable app data: imported user audio
stays separate, and a custom file with the same built-in id deliberately takes
precedence.

The old user-facing **Filters** label is now **Atmospheres**. An active sound
shows its small rotary volume knob directly below the button in both mixer
surfaces, while the existing per-layer settings continue to persist through the
mixer cascade.

Each active built-in is drawn as the thing it is rather than sharing one
generic shimmer: rain falls as discrete droplets over overcast grey, a
fireplace flickers with two embers deliberately out of step so it does not read
as a pulse, city windows drift past on two different column rhythms so the
skyline is not a barcode, and a record's grooves turn under surface noise.
Keyed off a `data-atmosphere` attribute, all CSS, and all `transform`/`opacity`
on pseudo-elements so the compositor handles it and nothing competes with the
audio thread. Imported sounds have no such vocabulary to draw on and keep the
plain accent fill. `prefers-reduced-motion` stops the motion but keeps the
colours — a still fire is still recognisably fire.

Built-ins remain hideable rather than deletable, custom atmospheres remain
importable/deletable from Settings, and the backend reports `builtIn` explicitly
so the two actions cannot be confused.

**Fixed since — two separate causes, both of which looked identical from the
outside** (a button that lights up and produces nothing):

*The request path.* The worker asked for a bed once and remembered it forever,
so a decode that failed, a file that was replaced, or a bed dropped from the
bank by hiding or deleting it left that atmosphere silent for the whole session
with no way back. Requests now repeat on an interval until the bed actually
lands (`ambience::BedRequests`), and are forgotten once it does, so a bed
removed later is fetched again from scratch. Two smaller holes went with it:
`set_playlist_entry_mixer` never asked for beds a per-song override turned on,
and `install_bed`/`remove_bed` were read-modify-write on an `ArcSwap`, so an
install could silently discard a concurrent removal.

*The decode path — the bigger one.* Bed decoding ran inline on the 5 Hz ticker,
which is the thread that emits the `playback` event driving the progress bar.
Measured on the packaged assets in a debug build: 0.1–0.5 s each, except
`vinyl_crackle` at **86 seconds**, because it is the only asset that is not
48 kHz and so the only one that goes through the sinc resampler. That single
decode froze the progress bar outright and left every bed queued behind it,
which is why in practice only the first atmosphere ever started. Two changes:

- Decoding moved to its own `pnm-ambience` thread, blocking on the request
  stream, so nothing the interface depends on can be held up by it.
- `[profile.dev.package."*"] opt-level = 2` in `Cargo.toml`. Decoding is almost
  entirely `symphonia` and `rubato`, and unoptimised they are dramatically
  slower — the six assets went from **101 s to 8.9 s** in debug, against 2.4 s
  in release. `tauri dev` builds in debug, so this is the profile the app is
  actually developed against; the app's own crate stays unoptimised and
  debuggable.

`tests/ambience_assets.rs` decodes every packaged bed and fails if one is
missing, silent, or takes long enough to read as broken. It prints each bed's
rate and timing, so a future asset that needs resampling is visible rather than
merely slow.

**Worth knowing:** `vinyl_crackle.mp3` is still 44.1 kHz and still ~7 s to load
in debug, against well under one for the rest. Re-encoding it to 48 kHz would
remove the resampler from the path entirely; it was left alone rather than
re-encoding someone's audio without asking.

---

## Medium: expanded EQ modal (Logic-style) — **done**

**Implemented:** `components/mixer/EqModal.vue`, opened by the expand button on
the advanced panel's EQ section, which used to unfold an inline band table.

**Settled decisions:**
- **Eight bands by default** — high-pass, low shelf, four peaks, high shelf,
  low-pass — but still editable: bands can be added up to `dsp::MAX_BANDS`
  (12), removed, and switched between all five kinds.
- **The two pass filters ship disabled.** A pass filter has no flat setting, so
  an enabled one would have quietly changed the sound of every existing mix the
  moment the default grew from six bands to eight.
- **The modal edits whichever layer the panel is pointed at**, like every other
  section. A modal that always edited global would silently discard the
  playlist or per-song context the panel was opened in.
- Existing six-band layers keep working untouched: `bands` is a `Vec`, so an
  older saved mixer is simply a shorter list.
- `EqSliders` draws a fader per *gain-bearing* band, so the simple mixer still
  shows the six faders it always did.

**The curve.** `lib/eqCurve.ts` re-derives the RBJ designs from `dsp.rs` and
evaluates `|H(e^jw)|` at 220 log-spaced points. That is a real duplication of
the engine's maths, so both sides assert against
`src/lib/__tests__/fixtures/eq-coefficients.json`, generated by
`src-tauri/tests/eq_parity.rs` (`PNM_WRITE_FIXTURES=1 cargo test --test
eq_parity`). The fixture deliberately includes the clamped inputs — sub-10 Hz,
past Nyquist, Q below the floor — since those are the easiest thing for a
reimplementation to miss.

**The analyser** was built rather than deferred. `audio/analyser.rs` holds a
hand-rolled radix-2 FFT (no new dependency for one fixed size), tapped on the
master bus *after* the limiter so the spectrum reflects the EQ being drawn over
it. It reduces to 96 log-spaced bins in Rust — with attack/release ballistics
and the dB conversion — so the UI receives a few dozen ready-to-draw numbers.
Pulled by `analyser_frame` on demand at frame rate rather than pushed as an
event, and gated by `set_analyser_enabled` so the FFT only runs while the modal
is actually open.

**Worth knowing:** nodes are absolutely positioned HTML over the SVG rather
than SVG circles. Under `preserveAspectRatio="none"` a circle renders as an
ellipse, and correcting for that means measuring the element and converting
radii on every resize — which is exactly the machinery `CrossfadeGraph.vue`
carries and this avoids.

**Still open:** the graph shows a spectrum but no per-band solo/listen, and the
node cannot be dragged for Q (the wheel does it). Neither seemed worth the
extra modifier-key surface.

---

## Medium: crossfade controls — **done**

Shipped: a length slider in the DJ Mixer popup and a four-point keyframe graph
in the advanced panel, backed by a dual-voice engine.

What it turned into, for reference:

- `audio/crossfade.rs` — the curve. Four points on one axis anchored at the
  *outgoing* song's own end (`x = 0`), equal-power (quarter-cosine) so an
  overlap does not dip ~3 dB in the middle.
- `audio/engine.rs` — the worker holds up to two `Voice`s, each with its own
  effect `Chain`, summed before a single master limiter. It asks the app layer
  for the next track ahead of time (`NeedNext`) rather than opening decoders on
  the audio thread.
- Crossfade cascades global → playlist, but deliberately **not** per entry:
  a crossfade belongs to the join between two tracks, so an entry-level value
  has no meaning. Enforced in `MixerSettings::resolve`.

Three bugs worth remembering, all found after the fact:

1. The refactor dropped `Chain::update` for the playing voice, which silently
   disabled EQ, reverb, delay, lo-fi and normalisation while leaving pitch and
   crossfade working — a very confusing partial failure.
2. The prepared voice was read as soon as it existed rather than when its fade
   began, so every incoming track started ~3 s in with its opening discarded.
3. Answering "no next track" by clearing the pending token re-armed the
   trigger immediately, firing the request every block for the rest of the
   track.

All three are covered by tests in `tests/playback.rs` that observe real audio
behaviour rather than internal state.

**Still open:** per-playlist custom crossfades (below) are a different, much
larger feature that builds on this engine work.

---

## Medium: home page — **done**

**Implemented:** `views/HomeView.vue` — three generated mixes across the top,
a Top Picks grid of explainable recommendations, and a Recent Playlists row.
A mix opens as a playlist (`views/MixView.vue`, route `/mix/:kind`) and can be
played, pinned into the sidebar above real playlists, or saved into a playlist.

### Listening history

Schema 3 adds `plays`, which every shelf is derived from:

```sql
CREATE TABLE plays (
    id             INTEGER PRIMARY KEY,
    song_id        TEXT    NOT NULL,
    played_at      INTEGER NOT NULL,
    seconds_played REAL    NOT NULL,   -- accumulated while playing only
    fraction       REAL    NOT NULL,
    counted        INTEGER NOT NULL,   -- passed the bar, i.e. not a skip
    context_kind   TEXT, context_id TEXT
);
```

There is deliberately **no foreign key to `songs`**: a rescan legitimately
deletes and recreates song rows, and history should outlive that. Reads join
back and ignore orphans.

`history.rs` measures the time rather than inferring it from the playhead.
This matters more than it looks: position-based counting means scrubbing to
the end of a track marks it played, and skipping through twenty songs leaves
twenty plays behind — which is exactly what fills a "most played" shelf with
music that was rejected on sight. Instead the tracker is ticked from the
existing 5 Hz ticker and adds only elapsed wall time while the engine reports
playing, with a per-tick cap so a machine waking from sleep cannot bank an
hour of "listening".

Every route by which one song replaces another funnels through
`AppState::begin_play`, so a play is recorded exactly once whether the track
ended, was skipped, or was promoted by a crossfade on the audio thread's own
schedule. The in-flight play is also banked on window close.

### Settled decisions

- **A skip does not count.** Under 25 seconds heard is a skip; the row is still
  written (an abandoned song is information) but `counted` is 0 and no shelf
  looks at it. A song shorter than 25 s counts when essentially finished, or it
  could be played daily forever and never appear.
- **Mixes are generated once per session and held** (`AppState::mix`). Two of
  the three are partly random and all three derive from history that playing
  them immediately changes, so a live query would reshuffle a mix while it was
  being listened to. "Regenerate" is the deliberate way to get a new one.
- **History is clearable.** `clear_listening_history` also drops the held
  mixes, since leaving them would keep serving recommendations built from data
  the listener just asked to be rid of. The screen for this belongs in
  Settings — see that section.

### The mixes

- **Replay** — ≥2 counted plays in the last 30 days, most played first.
- **Archive** — ≥3 lifetime counted plays but nothing in the last 60 days.
- **Discover** — built in tiers, because the obvious query ("everything never
  played") surfaces whatever stray non-music files sit in the library. Every
  tier needs evidence the song is wanted music: played 1–3 times, or never
  played but on an album the listener has played, or by an artist they have.

**Top Picks** interleaves three rules so one with many matches cannot fill the
shelf: "More from {artist}" (played this week, suggesting something not played
recently), "{n} of {total} played" (a part-heard album), and "Not played since
{year}". Each pick carries its reason as text — a recommendation that cannot
explain itself is indistinguishable from a random one, and reads as broken the
moment it suggests something unwanted.

**Still open:** the history screen itself. `listening_history` and
`clear_listening_history` exist and are tested; nothing calls them yet.

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

## Large: plugin support

**Size:** the largest architectural commitment on the list — not because any
one piece is hard, but because a plugin API is a *promise*. Every extension
point becomes something that cannot be refactored freely afterwards. Worth
starting deliberately small.

The three worked examples pull in different directions, and it is worth being
explicit about that up front:

| Example | What it actually needs |
| --- | --- |
| yt-dlp downloader | Run an external binary, write into the library, add a UI surface |
| ffmpeg transcode | Add a context-menu item, run a binary, replace/add a file |
| A new DJ Mixer effect | Run **inside the real-time audio thread** |

The first two are "app automation". The third is categorically different, and
mixing them into one mechanism is the main design trap here.

### Two plugin kinds, not one

**Kind A — app plugins (yt-dlp, transcode).** Out of process. They react to
events, call a command API, and contribute UI. They can be slow, they can fail,
they can be killed. This is where the value is and where I would start.

**Kind B — DSP plugins (a new mixer effect).** These run in the audio worker
under a hard deadline: a few hundred microseconds per 512-frame block, no
allocation, no locks, no I/O. A plugin that takes 20 ms stutters the output.

Do **not** try to serve both with one mechanism. Ship Kind A first; treat Kind
B as a separate, later project (notes at the end).

### Runtime for app plugins

The realistic options, given "lightweight and self-contained":

| Option | Verdict |
| --- | --- |
| **WASM** (`wasmtime`) | Sandboxed by default, capabilities are explicit, hot-reloadable. Adds ~5 MB. Awkward for the *UI* half, and plugin authors must compile. |
| **Deno/QuickJS embedded JS** | Familiar language, no build step for authors, but another runtime to ship. |
| **Node/Python subprocess** | No sandbox, and it breaks self-containment: the user must have the runtime. |
| **Native `dylib`** | Fast and simple, but no sandbox at all, and ABI-fragile across Rust versions. |

**Recommendation: WASM via `wasmtime`, with WASI for the filesystem.** It is
the only option that gives a real capability boundary, and the boundary is the
whole point — a plugin that can silently read the library database or shell out
unsandboxed is not a plugin, it is arbitrary code with a nicer name.

The build-step objection is real but manageable: publish a small Rust template
plus a JS/AssemblyScript one, so authors are not forced into Rust.

### Manifest and capabilities

Plugins live in `<app data>/plugins/<id>/`, each with a manifest:

```json
{
  "id": "yt-dlp-import",
  "name": "YouTube Import",
  "version": "0.1.0",
  "apiVersion": 1,
  "entry": "plugin.wasm",
  "ui": "panel.js",
  "capabilities": {
    "library": ["read", "addTracks"],
    "commands": ["download-url"],
    "subprocess": ["yt-dlp"],
    "network": ["youtube.com", "*.googlevideo.com"],
    "fs": { "write": ["downloads"] }
  }
}
```

Capabilities are **declared, shown to the user at install, and enforced** —
not advisory. `subprocess` in particular should name the binaries; "can run
anything" is not a capability, it is a bypass.

`apiVersion` is a hard integer, checked at load. A plugin built for an older
API is refused with a clear message rather than half-working.

### The backend API surface

The temptation is to expose the existing Tauri commands. Resist it: those are
an internal surface that changes freely, and freezing them would make the app
harder to work on. Define a *separate*, deliberately smaller plugin API that
happens to be implemented on top of them.

Roughly:

```rust
trait PluginHost {
    // Library
    fn search(&self, query: &str) -> Vec<TrackRef>;
    fn add_track(&self, path: &Path) -> Result<TrackRef>;
    fn track_metadata(&self, id: &TrackRef) -> Option<TrackMeta>;

    // Playback (read-mostly; plugins should not fight the user for transport)
    fn now_playing(&self) -> Option<TrackRef>;
    fn queue_append(&self, ids: &[TrackRef]) -> Result<()>;

    // Their own storage and progress reporting
    fn store_get(&self, key: &str) -> Option<String>;
    fn store_set(&self, key: &str, value: &str) -> Result<()>;
    fn report_progress(&self, job: &str, fraction: f32, note: &str);
    fn notify(&self, message: &str, level: Level);
}
```

Note what is deliberately absent: no raw SQL, no filesystem paths outside a
plugin-owned directory, no direct engine access. `TrackRef` is an opaque id
rather than a path, so a plugin cannot quietly walk the user's disk.

### Events

Plugins subscribe rather than poll:

`track-changed`, `track-finished`, `library-scanned`, `playlist-changed`,
`queue-changed`. These already exist internally as Tauri events; the plugin
versions should be a curated subset with a stable shape.

### The UI half — the hard part

Letting plugins "add custom Vue items" is the piece most likely to go wrong.
Loading arbitrary plugin Vue components into the main app means they share the
app's scope, its CSS, and its reactivity — a plugin can then break the host,
and the CSP has to be loosened to allow remote code.

Three options, worst to best:

1. **Plugin ships a Vue SFC compiled to JS, loaded dynamically.** Maximum
   power, zero isolation, requires relaxing the CSP that currently blocks
   exactly this. Not worth it.
2. **Declarative UI manifest.** The plugin describes its surface as data —
   context-menu items, a settings pane of typed fields, a sidebar entry — and
   the *host* renders it with its own components. No plugin code touches the
   DOM. Limited, but everything looks native and nothing can break the app.
3. **Sandboxed `<iframe>` panel** for anything richer, communicating over
   `postMessage`, with its own origin and CSP.

**Recommendation: (2) for the common cases, (3) as the escape hatch.** All
three worked examples are satisfied by (2):

- yt-dlp: a sidebar item + a form with one URL field + a progress row.
- transcode: a context-menu entry + a format/quality dialog.
- (a DSP effect's UI would be sliders and knobs — also declarative.)

That keeps `img-src`/`script-src` as tight as they are today, which matters:
the current CSP is the reason a hostile plugin cannot exfiltrate the library.

### Extension points to define first

Small and concrete beats general:

- `contextMenu.track` — add an item to the track context menu.
- `sidebar.section` — add an entry that opens a declarative panel.
- `settings.pane` — a page of typed settings under a Settings screen.
- `library.importer` — register a source of new tracks.
- `track.action` — a long-running job with progress.

### Where subprocess plugins get uncomfortable

yt-dlp and ffmpeg are not shipped with the app and should not be. That means:

- The plugin declares the binary; the **host** locates and runs it, so the
  plugin never gets a raw shell. Arguments come from a typed structure, not a
  string, so there is no quoting to get wrong.
- Missing binary is a first-class state with a clear message, not a crash.
- Downloading copyrighted material is the user's business, but the app should
  not *ship* a YouTube downloader — which is exactly the argument for plugins
  rather than a built-in feature. Worth being clear-eyed that this is the main
  motivation, and it is a good one.

### DSP plugins, later

If Kind B is eventually wanted, the shape is different again:

- A WASM module implementing `process(&mut [f32], frames)`, instantiated
  **per voice** and pre-warmed off the audio thread.
- Wasmtime with fuel metering and no allocator, so a runaway plugin is cut off
  rather than blowing the deadline. Even so, expect to raise `RING_MILLIS`
  above the current 120 ms when a DSP plugin is active.
- Parameters declared in the manifest so the host renders the knobs, exactly
  as the mixer already does for built-ins.
- It slots in beside `Chain`'s existing nodes, which is why keeping `Chain` a
  plain ordered list of processors is worth preserving.

Realistically: measure first. A `process` call crossing the WASM boundary per
block is fine; per *sample* is not.

### Staging

1. Manifest, loader, `apiVersion` check, capability prompts — no API yet.
2. Events + a read-only library API. Ship a trivial "now playing to a text
   file" example.
3. `contextMenu.track` + `track.action` with progress → the ffmpeg transcode
   plugin becomes possible.
4. `library.importer` + subprocess capability → yt-dlp.
5. Sandboxed iframe panels, if (2)-style declarative UI proves too limiting.
6. DSP plugins, as a separate project.

Stop after any step and the feature is still coherent.

### Decisions worth settling before any code

- Is the plugin API **stable** (versioned, deprecation cycles) or explicitly
  unstable while the app is young? Say so in the manifest, loudly.
- Where do plugins come from — a folder the user drops files into, or a
  registry? A registry means signing, review, and revocation; a folder means
  the user takes responsibility. Start with the folder.
- Does a plugin failure ever take down playback? It must not. Every plugin
  call should be timeout-bounded and failure-isolated.
- Does the app become a *host* — i.e. is a plugin's crash the app's problem in
  the user's mind? Almost always yes, so error attribution in the UI needs to
  name the plugin.

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
