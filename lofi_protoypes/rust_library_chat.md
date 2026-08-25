<img src="https://r2cdn.perplexity.ai/pplx-full-logo-primary-dark%402x.png" style="height:64px;margin-right:32px"/>

# I am planning on creating a desktop local music player built in Tauri. The unique thing about this app, is that I want it to be able to add effects to the music playback in real time. So you can change the pitch, the EQ, add reverb, add delay, normalisation, etc. My question is, how would it best be implemented in the backend? Tauri uses Rust, so are there capable and reliable enough libraries in that language that will allow me to achieve my goal, or will I need to include an external library from another language (like Python), in order to achieve these features. Also keep in mind that one of my priorities for building apps is having it be lightweight and self-contained (so all needed dependecnies compiled in the app) if at all possible.

Yes—this is very feasible as a predominantly Rust, self-contained Tauri application. You do **not** need Python; it would be a poor fit for the latency-sensitive audio callback and would complicate packaging. Build a native Rust audio engine, and use statically linked native DSP only if you decide a mature external pitch-shifter is worth the extra dependency.

## Recommended backend design

Treat Tauri as the UI shell and keep audio in a dedicated Rust engine. The engine should own OS audio output, decoding, buffering, DSP, and playback state; the frontend merely sends control messages and receives lightweight state/progress updates.

```text
Tauri WebView UI
   │ commands/events: play, seek, EQ settings, pitch, presets
   ▼
Audio-control thread
   │ atomic parameter updates / lock-free commands
   ▼
Decode thread ──> PCM ring buffer ──> real-time DSP graph ──> CPAL output callback ──> OS device
                  (f32, interleaved or planar)
```

Use these responsibilities:

- **Library/indexing thread:** scans files, reads tags/artwork, stores track metadata in SQLite if wanted. Never involve it in playback.
- **Decoder worker:** decodes compressed media ahead of the playback cursor into PCM chunks.
- **Real-time output callback:** pulls a fixed number of PCM frames, runs the DSP chain, and fills the device buffer.
- **Control path:** UI commands update atomics or push small commands into a lock-free queue. Do not have the Tauri frontend directly manipulate processor state on the callback thread.

For a local player, streaming decoded PCM from a worker into a bounded ring buffer is usually the best memory/seek/startup trade-off. You could predecode short files completely, but avoid doing so for large lossless albums or high-resolution tracks.

## Rust stack

| Need | Recommended Rust choice | Notes |
| :-- | :-- | :-- |
| Audio-device output | `cpal` | Cross-platform low-level audio I/O; it also offers optional support for raising the audio thread priority on supported systems. [^1_1] |
| Decode/demux/tags | `symphonia` | Pure Rust demuxing and decoding, covering common containers/codecs including FLAC, MP3, WAV, OGG/Vorbis, AAC/MP4, ALAC, AIFF, CAF, and WebM. [^1_2] |
| Resampling | `rubato` | High-quality or fast resamplers; its preallocated `process_into_buffer()` path is explicitly intended for real-time use without allocations or blocking work. [^1_3] |
| EQ filters | `biquad` | Parametric peak, shelf, LP/HP, notch, and related filters. Its DF1 implementation is specifically preferable when retuning filters live because it minimizes retuning artefacts. [^1_4] |
| DSP graph/prototyping | `fundsp` | Pure Rust DSP and synthesis library with an audio graph model and real-time-oriented facilities. [^1_5][^1_6] |
| Higher-level playback prototype | `rodio` | Convenient playback built atop CPAL with Symphonia decoding. Good for experimentation, but direct CPAL gives you better ownership of buffering, clocks, seeking, and your effects graph. [^1_7] |

A good first production-oriented dependency set is:

```toml
[dependencies]
cpal = "0.18"
symphonia = { version = "0.5", features = ["all"] }
rubato = "1"
biquad = "0.5"
crossbeam-channel = "0.5"
ringbuf = "0.4"
arc-swap = "1"
```

Exact feature/version choices should be validated at implementation time, especially for platform codec needs.

## Effects implementation

Most of the effects you listed are straightforward native Rust DSP:

- **Gain/volume and balance:** multiply samples; apply short ramps when values change so there are no clicks.
- **EQ:** implement a chain of biquad filters—e.g. low shelf, several bell/peak bands, then high shelf. Maintain separate filter state for each audio channel.
- **Delay:** circular buffer per channel with feedback, wet/dry mix, optional high/low-pass filtering in the feedback path.
- **Reverb:** start with a feedback-delay-network or Schroeder-style reverb. For an “IR reverb” feature later, use partitioned convolution rather than naïve time-domain convolution.
- **Limiter/normalisation protection:** a look-ahead limiter prevents clipping after EQ/reverb/pitch changes. It needs a small intentional latency.
- **Device resampling:** make the processing graph operate at one internal sample rate, and resample only where the decoded track and output device differ.

Use a fixed block size internally—often 128–1024 frames—and process each channel in contiguous buffers. Your graph can be a simple ordered chain at first:

```text
decoder → channel map → source-rate conversion → EQ → pitch processor
        → delay → reverb → gain → limiter → device-rate conversion → output
```

Keep wet/dry mixes and gain changes ramped over, say, 5–30 ms. When changing EQ coefficient sets, interpolate coefficients or crossfade two filter states; simply replacing filter coefficients can produce zipper noise or clicks.

## Pitch and normalisation caveats

**Pitch is the hard feature.** There are two meanings:

1. **Varispeed:** resample to alter pitch *and* playback speed together. This is relatively simple and `rubato` can do the resampling in real time.[^1_3]
2. **Independent pitch shift:** change pitch while retaining duration. This requires a time-stretch/pitch-shift algorithm such as WSOLA and/or a phase vocoder. A pure-Rust `timestretch` crate advertises a streaming real-time API plus independent pitch shifting and time stretching, so it is worth evaluating in a benchmark/prototype.[^1_8]

Test independent pitch shift with dense music, vocals, percussion, 44.1/48/96 kHz content, and rapid parameter changes. It is the feature most likely to determine whether a third-party native library is justified.

Do **not** calculate “normalisation” in the audio callback:

- **Peak normalisation:** can be calculated during background analysis by finding each track’s maximum sample peak.
- **Perceived-loudness normalisation:** requires analysing the whole track or album in advance, then saving a gain value in your library database or compatible metadata. Apply that cached gain during playback.
- **Safety limiting:** remains a real-time processor after all gain-changing effects, even if normalisation has already been calculated.


## Real-time rules

The callback must be intentionally boring:

- No allocations, file access, logging, database access, network calls, mutex waits, or Tauri IPC.
- Preallocate scratch buffers, delay lines, FFT workspaces, and resampler buffers before playback.
- Use bounded lock-free/SPSC ring buffers between decoding and audio output.
- Store simple changing parameters in atomics; swap larger immutable processor configurations with an atomic `Arc` swap or a command at a block boundary.
- Maintain an adequate decoded-buffer target—roughly a few hundred milliseconds is a sensible initial starting point—and measure underruns.
- Report current play position from the engine asynchronously; do not derive it from frontend timers.

`rubato`’s documentation makes the same key point: create the processor and buffers beforehand, then use its preallocated processing API in the time-sensitive path.[^1_3]

## When native FFI makes sense

Start pure Rust. If listening tests show that pitch shifting is insufficient, link a native DSP library through Rust FFI—not Python and not a separately installed runtime. Static linkage can still produce a single normal desktop application bundle.

Two important licensing implications:

- Rubber Band supports independent time/pitch changes and a lock-free real-time streaming mode, but its GPL licence means a proprietary distributed app needs a commercial licence.[^1_9][^1_10]
- SoundTouch provides independent tempo, pitch, and playback-rate manipulation under LGPL terms, which has its own distribution/compliance considerations.[^1_11][^1_12]

For your lightweight/self-contained priority, I would ship v1 with `cpal + symphonia + biquad + rubato`, hand-written delay/reverb/limiter nodes, and a Rust pitch-shift prototype behind a feature flag. Only add statically linked C/C++ DSP after objective profiling and listening tests show a clear benefit.

## Practical build order

1. Build gapless basic playback: Symphonia decode worker → ring buffer → CPAL.
2. Add gain, mute, seek, pause, device selection, and reliable end-of-track handling.
3. Add a block DSP graph with EQ and a limiter.
4. Add delay and basic algorithmic reverb.
5. Add background loudness/peak analysis and cached track gain.
6. Add pitch/tempo processing last, measuring latency, CPU use, and artefacts across platforms.

That approach stays idiomatic to Tauri/Rust, keeps all runtime components bundled, and avoids tying the player’s real-time reliability to a Python environment.
<span style="display:none">[^1_13][^1_14][^1_15][^1_16][^1_17][^1_18][^1_19][^1_20][^1_21][^1_22][^1_23][^1_24][^1_25][^1_26][^1_27][^1_28][^1_29][^1_30][^1_31][^1_32][^1_33][^1_34][^1_35][^1_36][^1_37][^1_38][^1_39][^1_40][^1_41][^1_42][^1_43][^1_44][^1_45]</span>

<div align="center">⁂</div>

[^1_1]: https://docs.rs/crate/cpal/0.18.1/source/README.md

[^1_2]: https://github.com/pdeljanov/Symphonia

[^1_3]: https://docs.rs/crate/rubato/latest

[^1_4]: https://docs.rs/biquad

[^1_5]: https://docs.rs/crate/fundsp/latest

[^1_6]: https://docs.rs/fundsp/latest/fundsp/?search=scale

[^1_7]: https://github.com/rustaudio/rodio

[^1_8]: https://docs.rs/timestretch

[^1_9]: https://github.com/jlank/rubberband

[^1_10]: https://github.com/breakfastquay/rubberband

[^1_11]: https://github.com/VinMing/soundtouch

[^1_12]: https://sourceforge.net/projects/soundtouch/

[^1_13]: https://users.rust-lang.org/t/decode-a-audio-file-that-cpal-can-consume-properly/110792

[^1_14]: https://github.com/emuell/phonic

[^1_15]: https://github.com/rustaudio

[^1_16]: https://github.com/ace-step/ACE-Step-DAW/issues/1522

[^1_17]: https://github.com/RustAudio/rodio

[^1_18]: https://lib.rs/crates/phonic

[^1_19]: https://lib.rs/multimedia/audio

[^1_20]: https://openapps.pro/packages/rodio

[^1_21]: https://gist.github.com/WeirdConstructor/276f7e0555b2dbe83614268b59a7a998

[^1_22]: https://pdeljanov-symphonia.mintlify.app/resources/examples

[^1_23]: https://www.reddit.com/r/rust/comments/13etzfs/announcing_awedio_a_lowoverhead_and_adaptable/

[^1_24]: https://whoisryosuke.com/blog/2026/creating-a-daw-in-rust/

[^1_25]: https://docs.rs/rubato/latest/rubato/trait.Resampler.html

[^1_26]: https://docs.rs/rubato/latest/rubato/

[^1_27]: https://docs.rs/biquad-filters-rust

[^1_28]: https://docs.rs/crate/rubato/0.4.3

[^1_29]: https://docs.rs/rubato/latest/x86_64-apple-darwin/rubato/?search=InterpolationType

[^1_30]: https://docs.rs/math-iir-fir

[^1_31]: https://docs.rs/math-iir-fir/latest/autoeq_iir/

[^1_32]: https://docs.rs/autoeq-iir

[^1_33]: https://docs.rs/oximedia-audio/latest/oximedia_audio/

[^1_34]: https://docs.rs/fixed-filters

[^1_35]: https://lib.rs/crates/fixed-resample

[^1_36]: https://context7.com/breakfastquay/rubberband

[^1_37]: https://github.com/koendv/rubberband/blob/master/README.txt

[^1_38]: https://github.com/breakfastquay/rubberband/blob/default/README.md

[^1_39]: https://github.com/rejc2/RubberBand

[^1_40]: https://github.com/Daninet/rubberband-wasm

[^1_41]: https://github.com/owoudenberg/soundtouch.net/tree/master

[^1_42]: https://github.com/falkTX/rubberband

[^1_43]: https://lib.rs/crates/signalsmith-dsp

[^1_44]: https://www.surina.net/soundtouch/

[^1_45]: https://breakfastquay.com/news.html


---

# For pitch shifting, a simple varispeed approach is all I want. Would that mean I can stick to just the rust stack you suggested?

Yes. If “pitch shifting” means **varispeed**—pitch and playback speed move together—you can remain entirely in the Rust stack: `symphonia` for decoding, `rubato` for variable-rate resampling, `biquad`/your own processors for effects, and `cpal` for device output. No Python or external C/C++ DSP library is necessary.

## How varispeed works

At a pitch ratio $r$:

- $r = 1.0$: original speed and pitch
- $r = 2.0$: one octave up, twice as fast
- $r = 0.5$: one octave down, half as fast
- Semitone conversion: $r = 2^{s/12}$, where $s$ is the semitone adjustment

For example, +3 semitones is approximately $2^{3/12} = 1.1892$. Feed source audio through a resampler at that ratio, then send the result to the normal effects/output chain.

`rubato` is a particularly good match: its asynchronous resamplers allow the ratio to be changed while running; it supports arbitrary ratios and preallocated processing buffers, including a `process_into_buffer` route designed to avoid allocations or blocking in real-time processing.[^2_1][^2_2]

## Keep two resampling concerns separate

You will normally have **two distinct sample-rate conversions**:

```text
Decoded track at source rate
  └─ varispeed resampler (user-controlled ratio)
       └─ EQ / delay / reverb / limiter
            └─ device-rate resampler (only if required)
                 └─ CPAL output device
```

For a 44.1 kHz file playing through a 48 kHz sound device:

- Normal playback: conversion is $48{,}000 / 44{,}100$
- At +12 semitones / 2× varispeed: effective conversion is $2 \times 48{,}000 / 44{,}100$

You can implement that as one combined resampling stage if your engine’s internal format is the device rate. Keeping it conceptually separate is often easier at first: one parameter represents device compatibility, the other represents musical speed/pitch.

CPAL represents a stream’s sample rate and exposes the supported output configurations for each device, so select a supported configuration and adapt decoded tracks to it.[^2_3][^2_4]

## Suggested implementation choice

Use a streaming, variable-ratio `rubato` resampler on a non-callback audio-processing worker. Let the CPAL callback only consume already-processed PCM from its output ring buffer.

```text
decode worker
  → decoded PCM ring buffer
  → DSP worker: varispeed + effects + limiter
  → output PCM ring buffer
  → CPAL callback: copy PCM into requested device buffer
```

This is slightly more buffered than putting DSP directly in the CPAL callback, but is much easier to make robust for a desktop music player. It gives your DSP worker enough room to process a 512- or 1024-frame block while the callback stays deterministic. Rubato itself recommends chunked processing and suggests beginning around 1024 frames for one suitable resampler type.[^2_1]

## UX and implementation details

- Change the ratio at a block boundary, rather than per sample.
- Smooth the transition when users drag a pitch control—either ramp the ratio over roughly 20–50 ms or briefly crossfade old/new processing paths. Sudden jumps in read position or ratio can click.
- Expose a **speed** parameter even if the UI calls it pitch; with varispeed those are inherently the same setting.
- Set bounds such as 0.5×–2× initially. Wider settings are possible, but downsampling far below the original rate and then converting back can become more CPU-intensive and more prone to audible quality loss.
- Use anti-aliased/band-limited resampling for music quality. Rubato notes that its non-anti-aliased asynchronous modes are faster but lower quality; its sinc-based modes trade CPU for quality.[^2_1]
- Account for resampler latency when computing the displayed position and when implementing sample-accurate seeking.

So your original lightweight, compiled-in plan remains valid: a pure-Rust binary/app bundle with no separate runtime dependency.
<span style="display:none">[^2_10][^2_11][^2_12][^2_13][^2_14][^2_15][^2_5][^2_6][^2_7][^2_8][^2_9]</span>

<div align="center">⁂</div>

[^2_1]: https://github.com/HEnquist/rubato

[^2_2]: https://github.com/HEnquist/rubato/blob/master/README.md

[^2_3]: https://docs.rs/cpal/latest/cpal/

[^2_4]: https://docs.rs/cpal/latest/cpal/struct.SupportedStreamConfig.html

[^2_5]: https://docs.rs/cpal/latest/cpal/type.SampleRate.html

[^2_6]: https://docs.rs/shura/latest/shura/audio/cpal/index.html

[^2_7]: https://stackoverflow.com/questions/76496624/how-do-i-prevent-crackles-and-pops-while-passing-input-audio-to-output-in-cpal

[^2_8]: https://github.com/RustAudio/cpal/issues/753

[^2_9]: https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md

[^2_10]: https://github.com/RustAudio/cpal/issues/593

[^2_11]: https://github.com/RustAudio/cpal/issues/465

[^2_12]: https://users.rust-lang.org/t/help-about-audio-playback-use-cpal-and-lewton/25524

[^2_13]: https://www.reddit.com/r/rust/comments/taj2dk/rubato_audio_resampler_011_is_realtime_safe/

[^2_14]: https://deepwiki.com/NaturalDevCR/TCP-Streamer/9.2-cpal-(cross-platform-audio-library)

[^2_15]: https://docs.rs/rubato/latest/rubato/trait.Resampler.html

