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
