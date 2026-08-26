//! The audio engine: one worker thread that decodes and processes audio into a
//! short ring buffer, and a cpal callback that does nothing but copy out of it.
//!
//! Keeping the callback trivial is deliberate. All the interesting work happens
//! on the worker, where allocation and the occasional slow path are harmless.
//!
//! ## Crossfading
//!
//! Normally the worker holds a single playing [`Voice`]. When crossfading is
//! enabled and the current voice is nearing its own natural end, the worker
//! asks the app layer to prepare the next one ([`EngineEvent::NeedNext`]) and
//! holds it as `next` once it arrives ([`Cmd::PrepareNext`]). Both voices are
//! then decoded, run through their own effect [`Chain`], and summed — each
//! scaled by the crossfade curve's gain for the outgoing and incoming song —
//! before a single master limiter. See `audio::crossfade` for the curve.
//!
//! The boundary between the two songs (`x = 0` on that curve) is defined as
//! the *outgoing* song's own natural end, so promotion happens the instant
//! `x` reaches zero or the outgoing decoder runs out of audio, whichever is
//! first — there is never an orphaned fading-out voice to manage afterwards.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use arc_swap::ArcSwap;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use serde::Serialize;

use crate::audio::ambience::{AmbienceMixer, Bank};
use crate::audio::crossfade::CrossfadeSettings;
use crate::audio::decode::{StreamInfo, TrackDecoder};
use crate::audio::dsp::{Chain, Limiter, CHANNELS};
use crate::audio::params::Resolved;

/// Frames processed per DSP block.
const BLOCK: usize = 512;
/// How much processed audio to keep queued. Short enough that a knob twist is
/// heard almost immediately, long enough to ride out scheduling hiccups.
const RING_MILLIS: usize = 120;
/// Extra lead time added on top of the crossfade curve's own lead, so a slow
/// decoder-open (a cold disk, a large file) still finishes before it's needed.
const TRIGGER_HEADROOM_SECS: f64 = 2.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum EngineEvent {
    /// The current track played through to its end with no crossfade partner
    /// ready to take over from it.
    TrackFinished,
    /// The current track is close enough to its own end that, if crossfading
    /// is enabled, the next one should be prepared now. `token` must be
    /// echoed back in the matching [`AudioEngine::prepare_next`] call; a
    /// mismatched or late reply is ignored.
    NeedNext { token: u64 },
    /// A prepared voice has taken over from the current one on the worker's
    /// own schedule. The app should move its queue cursor to match — it must
    /// *not* also call `engine.load()`, since the audio never stopped.
    TrackAdvanced {
        order_index: usize,
        track_id: String,
    },
    /// Something went wrong; the message is safe to show to the user.
    Error { message: String },
}

/// Everything the UI needs to draw the transport, polled on a timer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSnapshot {
    pub playing: bool,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub volume: f32,
    /// Effective playback rate, so the UI can show varispeed's tempo effect.
    pub speed: f64,
    pub limiter_reduction_db: f32,
    pub device_name: String,
    pub device_sample_rate: u32,
    pub stream: Option<StreamInfo>,
}

/// Shared state read by the worker and the callback without locking.
struct Shared {
    playing: AtomicBool,
    volume: AtomicU32,
    position_ms: AtomicU64,
    duration_ms: AtomicU64,
    speed_millis: AtomicU64,
    reduction_millidb: AtomicU32,
    device_rate: AtomicU32,
    settings: ArcSwap<Resolved>,
    crossfade: ArcSwap<CrossfadeSettings>,
    bank: ArcSwap<Bank>,
    /// Normalisation gain for the current track, worked out from its tags.
    track_gain_db: AtomicU32,
    stream_info: ArcSwap<Option<StreamInfo>>,
    device_name: ArcSwap<String>,
    /// Set by the worker after a seek or track change; the callback discards
    /// whatever is still queued so the old audio is never heard.
    flush: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Shared {
            playing: AtomicBool::new(false),
            volume: AtomicU32::new(1.0f32.to_bits()),
            position_ms: AtomicU64::new(0),
            duration_ms: AtomicU64::new(0),
            speed_millis: AtomicU64::new(1000),
            reduction_millidb: AtomicU32::new(0),
            device_rate: AtomicU32::new(48000),
            settings: ArcSwap::from_pointee(Resolved::default()),
            crossfade: ArcSwap::from_pointee(CrossfadeSettings::default()),
            bank: ArcSwap::from_pointee(Bank::new()),
            track_gain_db: AtomicU32::new(0.0f32.to_bits()),
            stream_info: ArcSwap::from_pointee(None),
            device_name: ArcSwap::from_pointee(String::new()),
            flush: AtomicBool::new(false),
        }
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }
}

/// Which queue entry a voice prepared via [`Cmd::PrepareNext`] corresponds
/// to, so the app can be told exactly what to move its cursor to once this
/// voice is promoted. `None` for a voice that arrived via `Cmd::Load` instead
/// (a fresh user-initiated load, not part of a crossfade handshake).
#[derive(Debug, Clone)]
struct QueueRef {
    order_index: usize,
    track_id: String,
}

/// One decoded, effects-processed audio source. The worker holds at most two
/// at once: `current` (always playing) and `next` (being pre-mixed in ahead
/// of a crossfade).
struct Voice {
    decoder: TrackDecoder,
    settings: Arc<Resolved>,
    track_gain_db: f32,
    /// Which of the worker's two persistent [`Chain`]s processes this voice.
    chain_ix: usize,
    queue_ref: Option<QueueRef>,
}

enum Cmd {
    Load {
        path: PathBuf,
        start_secs: f64,
        gain_db: f32,
        reply: Sender<Result<StreamInfo>>,
    },
    Seek(f64),
    Clear,
    /// A decoder opened and ready on the app side, in reply to `NeedNext`.
    /// Opening happens off the worker thread deliberately: `TrackDecoder::open`
    /// can take tens to hundreds of milliseconds (a cold disk, a large file),
    /// which the 120 ms ring cannot absorb if it happened here instead.
    PrepareNext {
        decoder: Box<TrackDecoder>,
        settings: Arc<Resolved>,
        gain_db: f32,
        token: u64,
        order_index: usize,
        track_id: String,
    },
    /// Abandon any pending or already-prepared next voice. Sent whenever the
    /// queue changes in a way that could make a prepared voice stale, and on
    /// every manual load/seek, which are instant cuts rather than fades.
    CancelNext,
    Shutdown,
}

pub struct AudioEngine {
    shared: Arc<Shared>,
    cmd_tx: Sender<Cmd>,
    /// Ids of beds the worker wants decoded, drained by the app layer.
    bed_requests: Receiver<String>,
    bed_tx: Sender<String>,
}

impl AudioEngine {
    pub fn new(events: Sender<EngineEvent>) -> Result<Self> {
        let shared = Arc::new(Shared::new());
        let (cmd_tx, cmd_rx) = unbounded::<Cmd>();
        let (bed_tx, bed_requests) = unbounded::<String>();

        // The cpal Stream is not Send, so it is created and kept on its own
        // thread. That thread reports the negotiated config back here.
        let (ready_tx, ready_rx) = bounded::<Result<(u32, usize)>>(1);
        let stream_shared = Arc::clone(&shared);
        let (ring_tx, ring_rx) = bounded::<rtrb::Producer<f32>>(1);

        std::thread::Builder::new()
            .name("pnm-audio-out".into())
            .spawn(move || match open_output(&stream_shared) {
                Ok((stream, rate, channels, producer)) => {
                    let _ = ring_tx.send(producer);
                    let _ = ready_tx.send(Ok((rate, channels)));
                    if let Err(e) = stream.play() {
                        eprintln!("audio: failed to start stream: {e}");
                    }
                    // Hold the stream open for the life of the process.
                    loop {
                        std::thread::sleep(Duration::from_secs(3600));
                    }
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| anyhow!("spawning audio thread: {e}"))?;

        let (device_rate, _channels) = ready_rx
            .recv()
            .map_err(|_| anyhow!("audio thread died during start-up"))??;
        let producer = ring_rx
            .recv()
            .map_err(|_| anyhow!("audio ring was never handed over"))?;

        let worker_shared = Arc::clone(&shared);
        let worker_beds = bed_tx.clone();
        std::thread::Builder::new()
            .name("pnm-audio-dsp".into())
            .spawn(move || {
                worker(
                    worker_shared,
                    cmd_rx,
                    producer,
                    events,
                    worker_beds,
                    device_rate,
                )
            })
            .map_err(|e| anyhow!("spawning dsp thread: {e}"))?;

        Ok(AudioEngine {
            shared,
            cmd_tx,
            bed_requests,
            bed_tx,
        })
    }

    pub fn load(&self, path: PathBuf, start_secs: f64, gain_db: f32) -> Result<StreamInfo> {
        let (reply, rx) = bounded(1);
        self.cmd_tx
            .send(Cmd::Load {
                path,
                start_secs,
                gain_db,
                reply,
            })
            .map_err(|_| anyhow!("audio worker is gone"))?;
        rx.recv()
            .map_err(|_| anyhow!("audio worker dropped the request"))?
    }

    pub fn play(&self) {
        self.shared.playing.store(true, Ordering::Relaxed);
    }

    pub fn pause(&self) {
        self.shared.playing.store(false, Ordering::Relaxed);
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    pub fn seek(&self, secs: f64) {
        let _ = self.cmd_tx.send(Cmd::Seek(secs));
    }

    pub fn clear(&self) {
        self.shared.playing.store(false, Ordering::Relaxed);
        let _ = self.cmd_tx.send(Cmd::Clear);
    }

    pub fn set_volume(&self, volume: f32) {
        self.shared
            .volume
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn set_settings(&self, settings: Resolved) {
        self.shared.settings.store(Arc::new(settings));
    }

    pub fn settings(&self) -> Arc<Resolved> {
        self.shared.settings.load_full()
    }

    pub fn set_track_gain_db(&self, db: f32) {
        self.shared
            .track_gain_db
            .store(db.to_bits(), Ordering::Relaxed);
    }

    pub fn set_crossfade(&self, settings: CrossfadeSettings) {
        self.shared.crossfade.store(Arc::new(settings));
    }

    pub fn crossfade(&self) -> Arc<CrossfadeSettings> {
        self.shared.crossfade.load_full()
    }

    /// Hand over a decoder opened and ready on the app side, in reply to
    /// [`EngineEvent::NeedNext`]. `token` must match the one the request
    /// carried; a stale reply (the queue changed in the meantime, or a newer
    /// request has already superseded this one) is silently dropped.
    pub fn prepare_next(
        &self,
        decoder: TrackDecoder,
        settings: Resolved,
        gain_db: f32,
        token: u64,
        order_index: usize,
        track_id: String,
    ) {
        let _ = self.cmd_tx.send(Cmd::PrepareNext {
            decoder: Box::new(decoder),
            settings: Arc::new(settings),
            gain_db,
            token,
            order_index,
            track_id,
        });
    }

    pub fn cancel_next(&self) {
        let _ = self.cmd_tx.send(Cmd::CancelNext);
    }

    pub fn device_sample_rate(&self) -> u32 {
        self.shared.device_rate.load(Ordering::Relaxed)
    }

    /// Publish a newly decoded ambience bed to the worker.
    pub fn install_bed(&self, id: String, samples: Arc<Vec<f32>>) {
        let mut next = (**self.shared.bank.load()).clone();
        next.insert(id, samples);
        self.shared.bank.store(Arc::new(next));
    }

    pub fn has_bed(&self, id: &str) -> bool {
        self.shared.bank.load().contains_key(id)
    }

    /// Bed ids the worker has asked for since the last call.
    pub fn take_bed_requests(&self) -> Vec<String> {
        self.bed_requests.try_iter().collect()
    }

    /// Ask for a bed to be decoded even though nothing is playing yet.
    pub fn request_bed(&self, id: &str) {
        let _ = self.bed_tx.send(id.to_string());
    }

    pub fn snapshot(&self) -> PlaybackSnapshot {
        let s = &self.shared;
        PlaybackSnapshot {
            playing: s.playing.load(Ordering::Relaxed),
            position_secs: s.position_ms.load(Ordering::Relaxed) as f64 / 1000.0,
            duration_secs: s.duration_ms.load(Ordering::Relaxed) as f64 / 1000.0,
            volume: s.volume(),
            speed: s.speed_millis.load(Ordering::Relaxed) as f64 / 1000.0,
            limiter_reduction_db: s.reduction_millidb.load(Ordering::Relaxed) as f32 / 1000.0,
            device_name: (**s.device_name.load()).clone(),
            device_sample_rate: s.device_rate.load(Ordering::Relaxed),
            stream: (**s.stream_info.load()).clone(),
        }
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(Cmd::Shutdown);
    }
}

// ---------------------------------------------------------------------------
// Output device
// ---------------------------------------------------------------------------

fn open_output(
    shared: &Arc<Shared>,
) -> Result<(cpal::platform::Stream, u32, usize, rtrb::Producer<f32>)> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("no audio output device is available"))?;
    let name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "Unknown device".into());
    let supported = device
        .default_output_config()
        .map_err(|e| anyhow!("querying default output config: {e}"))?;

    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.config();
    let rate = config.sample_rate;
    let channels = config.channels as usize;

    shared.device_rate.store(rate, Ordering::Relaxed);
    shared.device_name.store(Arc::new(name));

    let ring_frames = rate as usize * RING_MILLIS / 1000;
    let (producer, consumer) = rtrb::RingBuffer::<f32>::new(ring_frames * CHANNELS);

    let stream = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(&device, config, consumer, shared, channels),
        SampleFormat::I16 => build_stream::<i16>(&device, config, consumer, shared, channels),
        SampleFormat::U16 => build_stream::<u16>(&device, config, consumer, shared, channels),
        SampleFormat::I32 => build_stream::<i32>(&device, config, consumer, shared, channels),
        other => Err(anyhow!("unsupported output sample format: {other:?}")),
    }?;

    Ok((stream, rate, channels, producer))
}

fn build_stream<T>(
    device: &cpal::platform::Device,
    config: StreamConfig,
    mut consumer: rtrb::Consumer<f32>,
    shared: &Arc<Shared>,
    channels: usize,
) -> Result<cpal::platform::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let shared = Arc::clone(shared);
    let rate = config.sample_rate as f32;
    // ~8 ms of fade, enough to make pause and resume click-free.
    let fade_step = 1.0 / (rate * 0.008);
    let mut fade = 0.0f32;
    let mut volume = shared.volume();

    let stream = device
        .build_output_stream::<T, _, _>(
            config,
            move |data: &mut [T], _| {
                if shared.flush.swap(false, Ordering::AcqRel) {
                    let queued = consumer.slots();
                    if let Ok(chunk) = consumer.read_chunk(queued) {
                        chunk.commit_all();
                    }
                }

                let want_fade = if shared.playing.load(Ordering::Relaxed) {
                    1.0
                } else {
                    0.0
                };
                let target_volume = shared.volume();

                for frame in data.chunks_mut(channels) {
                    // Glide both fade and volume so neither ever steps.
                    fade += (want_fade - fade).clamp(-fade_step, fade_step);
                    volume += (target_volume - volume).clamp(-0.001, 0.001);

                    let (mut l, mut r) = (0.0f32, 0.0f32);
                    // Once fully faded out, stop draining the ring so playback
                    // resumes from exactly where it stopped.
                    if fade > 1e-4 {
                        l = consumer.pop().unwrap_or(0.0);
                        r = consumer.pop().unwrap_or(0.0);
                    }
                    let gain = fade * volume;
                    l *= gain;
                    r *= gain;

                    match channels {
                        1 => frame[0] = T::from_sample((l + r) * 0.5),
                        _ => {
                            frame[0] = T::from_sample(l);
                            frame[1] = T::from_sample(r);
                            // Leave any surround channels silent.
                            for slot in frame.iter_mut().skip(2) {
                                *slot = T::from_sample(0.0f32);
                            }
                        }
                    }
                }
            },
            move |err| eprintln!("audio: output stream error: {err}"),
            None,
        )
        .map_err(|e| anyhow!("building output stream: {e}"))?;

    Ok(stream)
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

fn worker(
    shared: Arc<Shared>,
    cmds: Receiver<Cmd>,
    mut producer: rtrb::Producer<f32>,
    events: Sender<EngineEvent>,
    bed_requests: Sender<String>,
    device_rate: u32,
) {
    // Two persistent chains, ping-ponged between voices across a crossfade,
    // rather than one built fresh per track: a `Chain` owns the delay lines,
    // Freeverb combs and limiter lookahead buffer, which is real allocation
    // worth keeping. Whichever chain a retiring voice was using is re-prepared
    // (state zeroed) before it can be reused, so one track's reverb tail can
    // never bleed into the next track that happens to land on the same chain.
    let mut chains: [Chain; 2] = [Chain::new(), Chain::new()];
    chains[0].prepare(device_rate as f32);
    chains[1].prepare(device_rate as f32);

    // The limiter lives on the master bus, after voices are summed, not one
    // per chain: two chains each limiting to the ceiling independently and
    // then being added together could still clip by several dB.
    let mut master_limiter = Limiter::new();
    master_limiter.prepare(device_rate as f32);

    let mut ambience = AmbienceMixer::new();
    ambience.prepare(device_rate as f32);

    // Scratch buffers, sized once. `mix` doubles as voice A's buffer: voice B
    // (when present) is decoded into `scratch_b` and added into `mix` in
    // place, rather than allocating a third buffer.
    let mut mix: Vec<Vec<f32>> = vec![vec![0.0; BLOCK]; CHANNELS];
    let mut scratch_b: Vec<Vec<f32>> = vec![vec![0.0; BLOCK]; CHANNELS];
    let mut interleaved_a = vec![0.0f32; BLOCK * CHANNELS];
    let mut interleaved_b = vec![0.0f32; BLOCK * CHANNELS];

    let mut current: Option<Voice> = None;
    let mut next: Option<Voice> = None;
    // Set once `NeedNext` has been asked and not yet answered (or cancelled),
    // so the trigger does not fire again on every subsequent block.
    let mut next_wait_token: Option<u64> = None;
    let mut next_token_gen: u64 = 0;

    let mut finished_reported = false;
    let mut requested_beds: Vec<String> = Vec::new();
    let mut meter_countdown = 0u32;

    loop {
        // --- commands ---------------------------------------------------
        let mut shutdown = false;
        while let Ok(cmd) = cmds.try_recv() {
            match cmd {
                Cmd::Load {
                    path,
                    start_secs,
                    gain_db,
                    reply,
                } => {
                    shared
                        .track_gain_db
                        .store(gain_db.to_bits(), Ordering::Relaxed);
                    match TrackDecoder::open(&path, device_rate) {
                        Ok(mut d) => {
                            if start_secs > 0.0 {
                                if let Err(e) = d.seek(start_secs) {
                                    eprintln!("audio: seek on load failed: {e}");
                                }
                            }
                            let info = d.info.clone();
                            shared
                                .duration_ms
                                .store((info.duration_secs * 1000.0) as u64, Ordering::Relaxed);
                            shared
                                .position_ms
                                .store((d.decoded_secs() * 1000.0) as u64, Ordering::Relaxed);
                            shared.stream_info.store(Arc::new(Some(info.clone())));
                            current = Some(Voice {
                                decoder: d,
                                settings: shared.settings.load_full(),
                                track_gain_db: gain_db,
                                chain_ix: 0,
                                queue_ref: None,
                            });
                            chains[1].prepare(device_rate as f32);
                            // A manual load is an instant cut: whatever was
                            // being prepared for a crossfade no longer applies.
                            next = None;
                            next_wait_token = None;
                            finished_reported = false;
                            drain(&shared);
                            let _ = reply.send(Ok(info));
                        }
                        Err(e) => {
                            current = None;
                            next = None;
                            next_wait_token = None;
                            shared.stream_info.store(Arc::new(None));
                            let _ = reply.send(Err(e));
                        }
                    }
                }
                Cmd::Seek(secs) => {
                    if let Some(cur) = current.as_mut() {
                        if let Err(e) = cur.decoder.seek(secs) {
                            let _ = events.send(EngineEvent::Error {
                                message: e.to_string(),
                            });
                        }
                        // A seek can move arbitrarily far from the track's own
                        // end, which invalidates any in-flight crossfade
                        // scheduling against it.
                        next = None;
                        next_wait_token = None;
                        drain(&shared);
                        finished_reported = false;
                        shared.position_ms.store(
                            (cur.decoder.decoded_secs() * 1000.0) as u64,
                            Ordering::Relaxed,
                        );
                    }
                }
                Cmd::Clear => {
                    current = None;
                    next = None;
                    next_wait_token = None;
                    shared.stream_info.store(Arc::new(None));
                    shared.position_ms.store(0, Ordering::Relaxed);
                    shared.duration_ms.store(0, Ordering::Relaxed);
                    drain(&shared);
                }
                Cmd::PrepareNext {
                    decoder,
                    settings,
                    gain_db,
                    token,
                    order_index,
                    track_id,
                } => {
                    // Only accepted if it answers the request currently
                    // outstanding; anything else is stale.
                    if next_wait_token == Some(token) {
                        let chain_ix = current.as_ref().map(|c| 1 - c.chain_ix).unwrap_or(1);
                        next = Some(Voice {
                            decoder: *decoder,
                            settings,
                            track_gain_db: gain_db,
                            chain_ix,
                            queue_ref: Some(QueueRef {
                                order_index,
                                track_id,
                            }),
                        });
                        next_wait_token = None;
                    }
                }
                Cmd::CancelNext => {
                    next = None;
                    next_wait_token = None;
                }
                Cmd::Shutdown => shutdown = true,
            }
        }
        if shutdown {
            return;
        }

        // --- current voice's live parameters -----------------------------
        // Only the current voice tracks `Shared` live, so tweaking the mixer
        // while a crossfade is pending affects what is actually playing, not
        // the queued-up next track. The next voice keeps the settings it was
        // resolved with at prepare time until it is promoted.
        if let Some(cur) = current.as_mut() {
            cur.settings = shared.settings.load_full();
            cur.track_gain_db = f32::from_bits(shared.track_gain_db.load(Ordering::Relaxed));
        }

        let crossfade = shared.crossfade.load_full();
        let bank = shared.bank.load_full();

        if let Some(cur) = current.as_ref() {
            let filters: &[crate::audio::params::Filter] = if cur.settings.enabled {
                &cur.settings.filters
            } else {
                &[]
            };
            ambience.sync(filters, &bank);
            for id in ambience.missing(filters, &bank) {
                if !requested_beds.iter().any(|r| r == id) {
                    requested_beds.push(id.to_string());
                    let _ = bed_requests.send(id.to_string());
                }
            }

            let speed = if cur.settings.enabled {
                cur.settings.pitch.ratio()
            } else {
                1.0
            };
            shared
                .speed_millis
                .store((speed * 1000.0) as u64, Ordering::Relaxed);
        }

        // --- idle / backpressure ------------------------------------------
        let idle = !shared.playing.load(Ordering::Relaxed) || current.is_none();
        let room = producer.slots() >= BLOCK * CHANNELS;
        if idle || !room {
            std::thread::sleep(Duration::from_millis(3));
            continue;
        }

        // --- crossfade: ask for the next track once close enough ----------
        if crossfade.enabled() && next.is_none() && next_wait_token.is_none() {
            let cur = current.as_ref().expect("checked by `idle` above");
            let remaining_track =
                (cur.decoder.info.duration_secs - cur.decoder.decoded_secs()).max(0.0);
            let speed = if cur.settings.enabled {
                cur.settings.pitch.ratio()
            } else {
                1.0
            };
            let remaining_wall = remaining_track / speed.max(0.05);
            let lead = crossfade.lead_secs() as f64 + TRIGGER_HEADROOM_SECS;
            if remaining_wall <= lead {
                let token = next_token_gen;
                next_token_gen += 1;
                next_wait_token = Some(token);
                let _ = events.send(EngineEvent::NeedNext { token });
            }
        }

        // --- crossfade: promote once the boundary is reached --------------
        // Checked *before* attempting another read from the current voice, so
        // a voice that is about to be promoted never has the chance to reach
        // "EOF with an empty ring" and fire `TrackFinished` on its way out —
        // that would otherwise skip the very track being promoted to.
        if next.is_some() {
            let cur = current.as_ref().expect("checked by `idle` above");
            let x = cur.decoder.decoded_secs() - cur.decoder.info.duration_secs;
            if x >= 0.0 || cur.decoder.is_eof() {
                let retiring_ix = cur.chain_ix;
                let promoted = next.take().expect("checked by outer `if`");
                chains[retiring_ix].prepare(device_rate as f32);

                shared.settings.store(Arc::clone(&promoted.settings));
                shared
                    .track_gain_db
                    .store(promoted.track_gain_db.to_bits(), Ordering::Relaxed);
                shared.duration_ms.store(
                    (promoted.decoder.info.duration_secs * 1000.0) as u64,
                    Ordering::Relaxed,
                );
                shared.position_ms.store(
                    (promoted.decoder.decoded_secs() * 1000.0) as u64,
                    Ordering::Relaxed,
                );
                shared
                    .stream_info
                    .store(Arc::new(Some(promoted.decoder.info.clone())));

                let queue_ref = promoted.queue_ref.clone();
                current = Some(promoted);
                next_wait_token = None;
                finished_reported = false;

                if let Some(qref) = queue_ref {
                    let _ = events.send(EngineEvent::TrackAdvanced {
                        order_index: qref.order_index,
                        track_id: qref.track_id,
                    });
                }
                continue;
            }
        }

        // --- produce: current voice ----------------------------------------
        // Scoped tightly to the decode calls, which are the only part that
        // needs `current` mutably: everything after this block only ever
        // reads it, and holding a `&mut` across the next-voice section too
        // would fight the immutable borrow that section needs.
        let (frames, speed) = {
            let cur = current.as_mut().expect("checked by `idle` above");
            let speed = if cur.settings.enabled {
                cur.settings.pitch.ratio()
            } else {
                1.0
            };
            if let Err(e) = cur.decoder.set_speed(speed) {
                let _ = events.send(EngineEvent::Error {
                    message: e.to_string(),
                });
            }

            let got_a = match cur.decoder.read(&mut interleaved_a) {
                Ok(n) => n,
                Err(e) => {
                    let _ = events.send(EngineEvent::Error {
                        message: e.to_string(),
                    });
                    0
                }
            };
            let frames = got_a / CHANNELS;

            if frames == 0 {
                // Wait for the ring to empty so the tail is actually heard.
                let queued = BLOCK * CHANNELS - producer.slots().min(BLOCK * CHANNELS);
                if cur.decoder.is_eof() && queued == 0 && !finished_reported {
                    finished_reported = true;
                    let _ = events.send(EngineEvent::TrackFinished);
                }
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }

            for f in 0..frames {
                for ch in 0..CHANNELS {
                    mix[ch][f] = interleaved_a[f * CHANNELS + ch];
                }
            }

            if cur.settings.enabled {
                chains[cur.chain_ix].process_music(&mut mix, frames);
            }
            chains[cur.chain_ix].apply_gain(&mut mix, frames);

            (frames, speed)
        };

        // --- produce: next voice, mixed in under the crossfade curve -------
        if next.is_some() {
            let x = {
                let cur = current.as_ref().expect("checked by `idle` above");
                cur.decoder.decoded_secs() - cur.decoder.info.duration_secs
            };
            let gain_a = crossfade.curve.gain_out(x as f32);
            for ch in 0..CHANNELS {
                for f in 0..frames {
                    mix[ch][f] *= gain_a;
                }
            }

            let nx = next.as_mut().expect("checked by outer `if`");
            let nx_speed = if nx.settings.enabled {
                nx.settings.pitch.ratio()
            } else {
                1.0
            };
            if let Err(e) = nx.decoder.set_speed(nx_speed) {
                let _ = events.send(EngineEvent::Error {
                    message: e.to_string(),
                });
            }

            let want_b = frames * CHANNELS;
            let got_b = match nx.decoder.read(&mut interleaved_b[..want_b]) {
                Ok(n) => n,
                Err(e) => {
                    let _ = events.send(EngineEvent::Error {
                        message: e.to_string(),
                    });
                    0
                }
            };
            let frames_b = got_b / CHANNELS;

            for ch in 0..CHANNELS {
                for f in 0..frames {
                    scratch_b[ch][f] = if f < frames_b {
                        interleaved_b[f * CHANNELS + ch]
                    } else {
                        0.0
                    };
                }
            }

            chains[nx.chain_ix].update(&nx.settings, nx.track_gain_db);
            if nx.settings.enabled {
                chains[nx.chain_ix].process_music(&mut scratch_b, frames);
            }
            chains[nx.chain_ix].apply_gain(&mut scratch_b, frames);

            let gain_b = crossfade.curve.gain_in(x as f32);
            for ch in 0..CHANNELS {
                for f in 0..frames {
                    mix[ch][f] += scratch_b[ch][f] * gain_b;
                }
            }
        }

        // --- master bus: ambience, then the one limiter --------------------
        // A single fresh immutable borrow, used through both this and the
        // reporting step below: nothing mutates `current` in between.
        let cur = current.as_ref().expect("checked by `idle` above");
        if cur.settings.enabled && !ambience.is_silent() {
            ambience.process(&mut mix, frames);
        }
        master_limiter.update(&cur.settings.normalisation, device_rate as f32);
        master_limiter.process(&mut mix, frames);

        for f in 0..frames {
            for ch in 0..CHANNELS {
                // The room check above guarantees these pushes succeed.
                let _ = producer.push(mix[ch][f]);
            }
        }

        // --- reporting --------------------------------------------------
        // What is decoded, less what is still queued, converted back into
        // track time so varispeed does not skew the progress bar. Reports
        // against whichever voice is `current`, which is always the one the
        // callback is (about to be) audibly dominated by.
        let capacity = producer.buffer().capacity();
        let queued_frames = (capacity - producer.slots()) / CHANNELS;
        let queued_secs = queued_frames as f64 / device_rate as f64 * speed;
        let position = (cur.decoder.decoded_secs() - queued_secs).max(0.0);
        shared
            .position_ms
            .store((position * 1000.0) as u64, Ordering::Relaxed);

        meter_countdown += 1;
        if meter_countdown >= 4 {
            meter_countdown = 0;
            let red = master_limiter.take_reduction_db();
            shared
                .reduction_millidb
                .store((red * 1000.0) as u32, Ordering::Relaxed);
        }
    }
}

/// Ask the output callback to throw away anything still queued.
fn drain(shared: &Shared) {
    shared.flush.store(true, Ordering::Release);
}
