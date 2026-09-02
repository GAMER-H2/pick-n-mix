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
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
use crate::audio::timeline::TimelineSource;

/// Frames processed per DSP block.
const BLOCK: usize = 512;
/// How much processed audio to keep queued. Short enough that a knob twist is
/// heard almost immediately, long enough to ride out scheduling hiccups.
const RING_MILLIS: usize = 120;
/// Extra lead time added on top of the crossfade curve's own lead, so a slow
/// decoder-open (a cold disk, a large file) still finishes before it's needed.
const TRIGGER_HEADROOM_SECS: f64 = 2.0;
/// How long to wait before asking again for an ambience bed that has not
/// arrived. Long enough not to spam a bed that is simply slow to decode, short
/// enough that recovering from a failure is not something the listener notices.
const BED_RETRY: Duration = Duration::from_secs(2);

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
#[derive(Debug, Clone, PartialEq, Serialize)]
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

const FADE_PLAY: u8 = 1;
const FADE_PAUSE: u8 = 2;

fn fade_mode_bits(mode: &str) -> u8 {
    match mode {
        "play" => FADE_PLAY,
        "pause" => FADE_PAUSE,
        "both" => FADE_PLAY | FADE_PAUSE,
        _ => 0,
    }
}

fn fade_step(mode: u8, playing: bool, ramp_step: f32) -> f32 {
    let direction = if playing { FADE_PLAY } else { FADE_PAUSE };
    if mode & direction != 0 {
        ramp_step
    } else {
        1.0
    }
}

/// Shared state read by the worker and the callback without locking.
struct Shared {
    playing: AtomicBool,
    fade_mode: AtomicU8,
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
    /// Latest spectrum of the processed output, in dBFS. Only maintained
    /// while something is actually looking at it.
    analyser_bins: ArcSwap<Vec<f32>>,
    analyser_on: AtomicBool,
    /// Let reverb and delay tails ring out after a pause instead of stopping
    /// with the music.
    keep_tail: AtomicBool,
    /// Set by the worker while such a tail is draining. The callback keeps
    /// consuming and stays at full gain while it is set, even though
    /// `playing` is already false.
    tail_active: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Shared {
            playing: AtomicBool::new(false),
            fade_mode: AtomicU8::new(0),
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
            analyser_bins: ArcSwap::from_pointee(vec![
                crate::audio::analyser::FLOOR_DB;
                crate::audio::analyser::BINS
            ]),
            analyser_on: AtomicBool::new(false),
            keep_tail: AtomicBool::new(false),
            tail_active: AtomicBool::new(false),
        }
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    /// Whether the output should be sounding: either playing, or ringing out
    /// a reverb tail after a pause.
    fn audible(&self) -> bool {
        self.playing.load(Ordering::Relaxed) || self.tail_active.load(Ordering::Relaxed)
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

/// Where a voice's audio comes from.
///
/// Both variants answer the same handful of questions — how far in are you,
/// have you run out, give me some audio — so the worker below does not care
/// which it is holding. A master mix is a [`TimelineSource`]: many overlapping
/// blocks, each with its own file and effect chain, already summed by the time
/// the worker sees it.
enum Source {
    Track(TrackDecoder),
    Timeline(Box<TimelineSource>),
}

impl Source {
    fn read(&mut self, out: &mut [f32]) -> Result<usize> {
        match self {
            Source::Track(decoder) => decoder.read(out),
            Source::Timeline(timeline) => timeline.read(out),
        }
    }

    fn seek(&mut self, secs: f64) -> Result<()> {
        match self {
            Source::Track(decoder) => decoder.seek(secs),
            Source::Timeline(timeline) => timeline.seek(secs),
        }
    }

    fn set_speed(&mut self, speed: f64) -> Result<()> {
        match self {
            Source::Track(decoder) => decoder.set_speed(speed),
            Source::Timeline(timeline) => timeline.set_speed(speed),
        }
    }

    fn decoded_secs(&self) -> f64 {
        match self {
            Source::Track(decoder) => decoder.decoded_secs(),
            Source::Timeline(timeline) => timeline.decoded_secs(),
        }
    }

    fn is_eof(&self) -> bool {
        match self {
            Source::Track(decoder) => decoder.is_eof(),
            Source::Timeline(timeline) => timeline.is_eof(),
        }
    }

    fn info(&self) -> &StreamInfo {
        match self {
            Source::Track(decoder) => &decoder.info,
            Source::Timeline(timeline) => timeline.info(),
        }
    }

    /// A master mix is never crossfaded into the next queue entry: it *is* the
    /// arrangement, and its own ending is part of it.
    fn is_timeline(&self) -> bool {
        matches!(self, Source::Timeline(_))
    }
}

fn source_position_ms(source: &Source) -> u64 {
    (source.decoded_secs() * 1000.0) as u64
}

/// One decoded, effects-processed audio source. The worker holds at most two
/// at once: `current` (always playing) and `next` (being pre-mixed in ahead
/// of a crossfade).
struct Voice {
    source: Source,
    /// This voice's resolved mixer cascade. Note that its `crossfade` field is
    /// *not* what governs the transition — a crossfade belongs to the join
    /// between two tracks, not to either one, so the worker always reads
    /// `Shared::crossfade` instead. Reading it from here would silently use a
    /// per-voice value that may not be the one in force.
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
    /// Play a whole master mix in place of a single track. Built on the app
    /// side, like a decoder, because resolving it touches the library and the
    /// filesystem.
    LoadTimeline {
        source: Box<TimelineSource>,
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
    /// Leaves the worker free to ask again, since the queue may now have a
    /// different — and perfectly usable — next track.
    CancelNext,
    /// There is nothing to crossfade into for this request: the queue has
    /// ended, or the next track could not be opened. Distinct from
    /// `CancelNext` because the worker must *stop asking* until something
    /// changes, rather than re-firing the request on the very next block for
    /// the rest of the track.
    DeclineNext {
        token: u64,
    },
    /// The output moved to another device, so the worker must write into the
    /// new ring and re-prepare everything that was built for the old rate.
    Rebind {
        producer: rtrb::Producer<f32>,
        rate: u32,
    },
    Shutdown,
}

/// A request to reopen the output on a different device.
struct Reopen {
    /// Device name, or `None` for the system default.
    device: Option<String>,
    reply: Sender<Result<u32>>,
}

pub struct AudioEngine {
    shared: Arc<Shared>,
    cmd_tx: Sender<Cmd>,
    /// Ids of beds the worker wants decoded, drained by the app layer.
    bed_requests: Receiver<String>,
    bed_tx: Sender<String>,
    /// Asks the output thread to close its stream and open another.
    reopen_tx: Sender<Reopen>,
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
        let (reopen_tx, reopen_rx) = unbounded::<Reopen>();
        let rebind_tx = cmd_tx.clone();

        std::thread::Builder::new()
            .name("pnm-audio-out".into())
            .spawn(move || output_thread(stream_shared, ready_tx, ring_tx, reopen_rx, rebind_tx))
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
            reopen_tx,
        })
    }

    /// Output devices currently available, by name.
    pub fn output_devices() -> Vec<String> {
        let host = cpal::default_host();
        let Ok(devices) = host.output_devices() else {
            return Vec::new();
        };
        let mut names: Vec<String> = devices
            .filter(|device| device.default_output_config().is_ok())
            .filter_map(|device| device.description().ok().map(|d| d.name().to_string()))
            .collect();
        // A host can list the same endpoint more than once.
        names.sort();
        names.dedup();
        names
    }

    /// Move playback to another output device, or back to the system default.
    ///
    /// The stream, its ring and the sample rate all change together, so the
    /// worker is rebound to a new ring rather than the engine being rebuilt —
    /// that keeps the queue, mixer and every other bit of state in place. The
    /// caller is responsible for reloading the current track afterwards: a
    /// decoder resamples to a fixed target rate chosen when it was opened, so
    /// one opened for the old device cannot feed the new one.
    pub fn set_output_device(&self, name: Option<&str>) -> Result<u32> {
        let (reply, rx) = bounded(1);
        self.reopen_tx
            .send(Reopen {
                device: name.filter(|n| !n.is_empty()).map(|n| n.to_string()),
                reply,
            })
            .map_err(|_| anyhow!("the audio output thread is not running"))?;
        rx.recv()
            .map_err(|_| anyhow!("the audio output thread stopped while switching device"))?
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

    /// Play a whole master mix. Replaces whatever was loaded, exactly as
    /// [`AudioEngine::load`] does.
    pub fn load_timeline(&self, source: TimelineSource) -> Result<StreamInfo> {
        let (reply, rx) = bounded(1);
        self.cmd_tx
            .send(Cmd::LoadTimeline {
                source: Box::new(source),
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

    pub fn set_fade_mode(&self, mode: &str) {
        self.shared
            .fade_mode
            .store(fade_mode_bits(mode), Ordering::Relaxed);
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

    /// Tell the engine there is no next track to crossfade into, so it stops
    /// asking until the situation changes.
    pub fn decline_next(&self, token: u64) {
        let _ = self.cmd_tx.send(Cmd::DeclineNext { token });
    }

    pub fn device_sample_rate(&self) -> u32 {
        self.shared.device_rate.load(Ordering::Relaxed)
    }

    /// Let reverb and delay tails ring out after a pause rather than stopping
    /// with the music.
    pub fn set_keep_tail(&self, enabled: bool) {
        self.shared.keep_tail.store(enabled, Ordering::Relaxed);
    }

    /// Start or stop maintaining the spectrum.
    ///
    /// Off by default: the FFT is cheap but not free, and nothing but the
    /// expanded EQ ever looks at it, so it runs only while that is open.
    pub fn set_analyser_enabled(&self, enabled: bool) {
        self.shared.analyser_on.store(enabled, Ordering::Relaxed);
    }

    /// The latest spectrum of the processed output, in dBFS.
    pub fn analyser_bins(&self) -> Arc<Vec<f32>> {
        self.shared.analyser_bins.load_full()
    }

    /// Publish a newly decoded ambience bed to the worker.
    ///
    /// `rcu` rather than load-modify-store: the ticker thread installs beds
    /// while command threads remove them, and a plain read-modify-write would
    /// let one silently discard the other's change.
    pub fn install_bed(&self, id: String, samples: Arc<Vec<f32>>) {
        self.shared.bank.rcu(|bank| {
            let mut next = (**bank).clone();
            next.insert(id.clone(), Arc::clone(&samples));
            next
        });
    }

    pub fn has_bed(&self, id: &str) -> bool {
        self.shared.bank.load().contains_key(id)
    }

    pub fn remove_bed(&self, id: &str) {
        self.shared.bank.rcu(|bank| {
            let mut next = (**bank).clone();
            next.remove(id);
            next
        });
    }

    /// Bed ids the worker has asked for since the last call.
    pub fn take_bed_requests(&self) -> Vec<String> {
        self.bed_requests.try_iter().collect()
    }

    /// A handle on the bed request stream, for a thread that does nothing but
    /// decode them.
    ///
    /// Decoding a bed is slow — seconds, and far longer for one that needs
    /// resampling — so it must not share a thread with anything the interface
    /// depends on being prompt.
    pub fn bed_request_stream(&self) -> Receiver<String> {
        self.bed_requests.clone()
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

/// Owns the cpal stream for the life of the process, reopening it whenever the
/// output device changes.
///
/// The stream lives here rather than in [`AudioEngine`] because it is not
/// `Send`. Dropping it is what closes the old device, which is why each
/// reopen happens at the top of this loop with the previous stream already out
/// of scope.
fn output_thread(
    shared: Arc<Shared>,
    ready_tx: Sender<Result<(u32, usize)>>,
    ring_tx: Sender<rtrb::Producer<f32>>,
    reopen_rx: Receiver<Reopen>,
    rebind_tx: Sender<Cmd>,
) {
    let mut handoff = Some((ready_tx, ring_tx));
    let mut wanted: Option<String> = None;
    let mut pending: Option<Sender<Result<u32>>> = None;

    loop {
        let opened = open_output(&shared, wanted.as_deref());
        let stream = match opened {
            Ok((stream, rate, channels, producer)) => {
                match handoff.take() {
                    // Start-up: `AudioEngine::new` is waiting for both of these.
                    Some((ready, ring)) => {
                        let _ = ring.send(producer);
                        let _ = ready.send(Ok((rate, channels)));
                    }
                    // A device change: the worker already exists and needs to
                    // be pointed at the new ring.
                    None => {
                        let _ = rebind_tx.send(Cmd::Rebind { producer, rate });
                    }
                }
                if let Err(e) = stream.play() {
                    eprintln!("audio: failed to start stream: {e}");
                }
                if let Some(reply) = pending.take() {
                    let _ = reply.send(Ok(rate));
                }
                Some(stream)
            }
            Err(e) => {
                if let Some((ready, _)) = handoff.take() {
                    // Nothing works without an output at start-up.
                    let _ = ready.send(Err(e));
                    return;
                }
                let message = e.to_string();
                if let Some(reply) = pending.take() {
                    let _ = reply.send(Err(e));
                }
                if wanted.is_some() {
                    // The chosen device failed. Fall back to the default
                    // rather than leaving the app with no audio at all.
                    eprintln!("audio: {message}; falling back to the default device");
                    wanted = None;
                    continue;
                }
                None
            }
        };

        let Ok(request) = reopen_rx.recv() else {
            return; // The engine is gone.
        };
        wanted = request.device;
        pending = Some(request.reply);
        // Closes the old device before the next iteration opens the new one.
        drop(stream);
    }
}

fn open_output(
    shared: &Arc<Shared>,
    wanted: Option<&str>,
) -> Result<(cpal::platform::Stream, u32, usize, rtrb::Producer<f32>)> {
    let host = cpal::default_host();
    let device = match wanted {
        Some(wanted) => host
            .output_devices()
            .map_err(|e| anyhow!("listing output devices: {e}"))?
            .find(|device| {
                device
                    .description()
                    .map(|d| d.name() == wanted)
                    .unwrap_or(false)
            })
            .ok_or_else(|| anyhow!("output device not found: {wanted}"))?,
        None => host
            .default_output_device()
            .ok_or_else(|| anyhow!("no audio output device is available"))?,
    };
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
    // When enabled for a direction, a short ramp makes its transition click-free.
    let ramp_fade_step = 1.0 / (rate * 0.012);
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
                    // A load or seek can jump between unrelated waveform
                    // values. Restart the short output ramp at silence so that
                    // discontinuity cannot become an audible click.
                    fade = 0.0;
                }

                // A ringing reverb tail counts as playing here: `playing` is
                // already false the moment pause is pressed, but the tail the
                // worker is still feeding in has to be heard out rather than
                // faded away with the music.
                let playing = shared.audible();
                let target_volume = shared.volume();

                for frame in data.chunks_mut(channels) {
                    // The ring can be empty briefly while a decoder opens or
                    // after a scheduling hiccup. Silence the envelope state on
                    // underrun, otherwise it can reach full gain before the
                    // first new sample arrives and recreate the discontinuity
                    // that the load flush was intended to remove.
                    let has_audio = consumer.slots() >= CHANNELS;
                    if playing && !has_audio {
                        fade = 0.0;
                    } else {
                        let want_fade = if playing { 1.0 } else { 0.0 };
                        let step = fade_step(
                            shared.fade_mode.load(Ordering::Relaxed),
                            playing,
                            ramp_fade_step,
                        );
                        fade += (want_fade - fade).clamp(-step, step);
                    }
                    volume += (target_volume - volume).clamp(-0.001, 0.001);

                    let (mut l, mut r) = (0.0f32, 0.0f32);
                    // During a pause, keep consuming only for the brief fade-out.
                    // Once silent, leave the ring untouched so resume is exact.
                    if fade > 1e-4 && has_audio {
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
    mut device_rate: u32,
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

    // Fed from the master bus so the spectrum reflects the effect chain. Only
    // driven while the UI has asked for it; see `set_analyser_enabled`.
    let mut analyser = crate::audio::analyser::Analyser::new(device_rate as f32);

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
    // Set once the app has said there is nothing to crossfade into. Without
    // it, a declined request would simply clear the token and the trigger
    // would fire again immediately, spraying events for the rest of the
    // track. Cleared whenever the situation could have changed.
    let mut next_declined = false;
    let mut next_token_gen: u64 = 0;

    let mut finished_reported = false;
    let mut requested_beds = crate::audio::ambience::BedRequests::new();
    let mut meter_countdown = 0u32;
    // Blocks of reverb tail still to render after a pause; 0 when not
    // draining one. See the pause handling in the loop below.
    let mut tail_blocks = 0usize;
    let mut was_playing = false;

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
                                source: Source::Track(d),
                                settings: shared.settings.load_full(),
                                track_gain_db: gain_db,
                                chain_ix: 0,
                                queue_ref: None,
                            });
                            // Both chains, not just the unused one: a manual
                            // load is an instant cut, so nothing from the
                            // previous track should carry over. Resetting only
                            // the spare left the outgoing track's reverb and
                            // delay tails sitting in the very chain the new
                            // track was about to be handed.
                            chains[0].prepare(device_rate as f32);
                            chains[1].prepare(device_rate as f32);
                            // Which includes a pause tail still ringing out.
                            tail_blocks = 0;
                            shared.tail_active.store(false, Ordering::Relaxed);
                            // Whatever was being prepared for a crossfade no
                            // longer applies either.
                            next = None;
                            next_wait_token = None;
                            next_declined = false;
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
                Cmd::LoadTimeline { source, reply } => {
                    let info = source.info().clone();
                    let timeline_source = Source::Timeline(source);
                    shared
                        .duration_ms
                        .store((info.duration_secs * 1000.0) as u64, Ordering::Relaxed);
                    shared
                        .position_ms
                        .store(source_position_ms(&timeline_source), Ordering::Relaxed);
                    shared.stream_info.store(Arc::new(Some(info.clone())));
                    shared
                        .track_gain_db
                        .store(0.0f32.to_bits(), Ordering::Relaxed);
                    current = Some(Voice {
                        source: timeline_source,
                        // Every block in a mix carries its own resolved
                        // cascade and runs through its own chain inside the
                        // timeline, so the voice-level chain is bypassed
                        // rather than applying the mixer a second time. The
                        // master limiter after it still runs.
                        settings: Arc::new(Resolved {
                            enabled: false,
                            ..Resolved::default()
                        }),
                        track_gain_db: 0.0,
                        chain_ix: 0,
                        queue_ref: None,
                    });
                    chains[0].prepare(device_rate as f32);
                    chains[1].prepare(device_rate as f32);
                    tail_blocks = 0;
                    shared.tail_active.store(false, Ordering::Relaxed);
                    next = None;
                    next_wait_token = None;
                    next_declined = false;
                    finished_reported = false;
                    drain(&shared);
                    let _ = reply.send(Ok(info));
                }
                Cmd::Seek(secs) => {
                    if let Some(cur) = current.as_mut() {
                        if let Err(e) = cur.source.seek(secs) {
                            let _ = events.send(EngineEvent::Error {
                                message: e.to_string(),
                            });
                        }
                        // A seek can move arbitrarily far from the track's own
                        // end, which invalidates any in-flight crossfade
                        // scheduling against it.
                        next = None;
                        next_wait_token = None;
                        next_declined = false;
                        drain(&shared);
                        finished_reported = false;
                        shared.position_ms.store(
                            (cur.source.decoded_secs() * 1000.0) as u64,
                            Ordering::Relaxed,
                        );
                    }
                }
                Cmd::Clear => {
                    current = None;
                    next = None;
                    next_wait_token = None;
                    next_declined = false;
                    tail_blocks = 0;
                    shared.tail_active.store(false, Ordering::Relaxed);
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
                            source: Source::Track(*decoder),
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
                    // The queue changed, so a previous "nothing to play next"
                    // answer may no longer be true.
                    next_declined = false;
                }
                Cmd::DeclineNext { token } => {
                    if next_wait_token == Some(token) {
                        next_wait_token = None;
                        next_declined = true;
                    }
                }
                Cmd::Rebind {
                    producer: rebound,
                    rate,
                } => {
                    producer = rebound;
                    device_rate = rate;
                    // Every filter, delay line and reverb comb was designed
                    // for the old rate, so all of it is rebuilt. Voices are
                    // dropped too: a decoder resamples to the rate it was
                    // opened with, and the app reloads the track afterwards.
                    chains[0].prepare(device_rate as f32);
                    chains[1].prepare(device_rate as f32);
                    master_limiter.prepare(device_rate as f32);
                    ambience.prepare(device_rate as f32);
                    analyser = crate::audio::analyser::Analyser::new(device_rate as f32);
                    current = None;
                    next = None;
                    next_wait_token = None;
                    tail_blocks = 0;
                    shared.tail_active.store(false, Ordering::Relaxed);
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
            // A master mix is deliberately left alone here: its blocks were
            // resolved when the mix was built, and pushing the live cascade
            // over the top would apply the playlist's mixer to the sum as
            // well as to every block inside it.
            if !cur.source.is_timeline() {
                cur.settings = shared.settings.load_full();
                cur.track_gain_db = f32::from_bits(shared.track_gain_db.load(Ordering::Relaxed));
            }
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
            let asked_at = Instant::now();
            for id in ambience.missing(filters, &bank) {
                if requested_beds.due(id, asked_at, BED_RETRY) {
                    let _ = bed_requests.send(id.to_string());
                }
            }
            requested_beds.settled(&bank);

            let speed = if cur.settings.enabled {
                cur.settings.pitch.ratio()
            } else {
                1.0
            };
            shared
                .speed_millis
                .store((speed * 1000.0) as u64, Ordering::Relaxed);
        }

        // --- pause: start or finish a reverb tail --------------------------
        let playing = shared.playing.load(Ordering::Relaxed);
        if was_playing && !playing {
            // Pause was just pressed. The ring still holds up to `RING_MILLIS`
            // of *music* that has been processed but not heard; letting that
            // through would make pause feel late. So it is discarded and the
            // decoder wound back to exactly what was heard, leaving the chain
            // free to ring out into a now-empty ring.
            if let Some(cur) = current.as_mut() {
                if shared.keep_tail.load(Ordering::Relaxed) && chain_has_tail(&cur.settings) {
                    let heard = heard_position(&producer, cur, device_rate, &shared);
                    drain(&shared);
                    if cur.source.seek(heard).is_ok() {
                        tail_blocks = tail_block_budget(device_rate);
                        shared.tail_active.store(true, Ordering::Relaxed);
                    }
                }
            }
        } else if !was_playing && playing && tail_blocks > 0 {
            // Resumed mid-tail. Whatever is left of it in the ring belongs to
            // the moment before the pause, so it is dropped rather than played
            // in front of the music.
            tail_blocks = 0;
            shared.tail_active.store(false, Ordering::Relaxed);
            drain(&shared);
        }
        was_playing = playing;

        // The voice can vanish mid-tail — the queue is cleared, or another
        // track is loaded — and there is nothing left to ring out when it does.
        if tail_blocks > 0 && current.is_none() {
            tail_blocks = 0;
            shared.tail_active.store(false, Ordering::Relaxed);
        }

        let draining = tail_blocks > 0;
        if draining {
            if producer.slots() >= BLOCK * CHANNELS {
                let quiet = match current.as_mut() {
                    Some(voice) => drain_tail_block(
                        &mut chains,
                        voice,
                        &mut master_limiter,
                        &mut mix,
                        &mut producer,
                        device_rate,
                    ),
                    None => true,
                };
                tail_blocks -= 1;
                if quiet {
                    tail_blocks = 0;
                }
            }
            if tail_blocks == 0 {
                shared.tail_active.store(false, Ordering::Relaxed);
                // The tail is over; anything left of it must not be sitting in
                // front of the music when playback resumes.
                drain(&shared);
            }
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }

        // --- idle / backpressure ------------------------------------------
        let idle = !playing || current.is_none();
        let room = producer.slots() >= BLOCK * CHANNELS;
        if idle || !room {
            // Let the spectrum fall away rather than freezing the last frame
            // on screen for as long as playback is stopped.
            if idle && shared.analyser_on.load(Ordering::Relaxed) && !analyser.is_silent() {
                analyser.decay();
                shared
                    .analyser_bins
                    .store(Arc::new(analyser.bins().to_vec()));
            }
            std::thread::sleep(Duration::from_millis(3));
            continue;
        }

        // --- crossfade: ask for the next track once close enough ----------
        let playing_a_mix = current
            .as_ref()
            .map(|cur| cur.source.is_timeline())
            .unwrap_or(false);
        if crossfade.enabled()
            && !playing_a_mix
            && next.is_none()
            && next_wait_token.is_none()
            && !next_declined
        {
            let cur = current.as_ref().expect("checked by `idle` above");
            let remaining_track =
                (cur.source.info().duration_secs - cur.source.decoded_secs()).max(0.0);
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
            let x = cur.source.decoded_secs() - cur.source.info().duration_secs;
            if x >= 0.0 || cur.source.is_eof() {
                let retiring_ix = cur.chain_ix;
                let promoted = next.take().expect("checked by outer `if`");
                chains[retiring_ix].prepare(device_rate as f32);

                shared.settings.store(Arc::clone(&promoted.settings));
                shared
                    .track_gain_db
                    .store(promoted.track_gain_db.to_bits(), Ordering::Relaxed);
                shared.duration_ms.store(
                    (promoted.source.info().duration_secs * 1000.0) as u64,
                    Ordering::Relaxed,
                );
                shared.position_ms.store(
                    (promoted.source.decoded_secs() * 1000.0) as u64,
                    Ordering::Relaxed,
                );
                shared
                    .stream_info
                    .store(Arc::new(Some(promoted.source.info().clone())));

                let queue_ref = promoted.queue_ref.clone();
                current = Some(promoted);
                next_wait_token = None;
                // A new track is playing, so ask again for whatever follows it.
                next_declined = false;
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
            if let Err(e) = cur.source.set_speed(speed) {
                let _ = events.send(EngineEvent::Error {
                    message: e.to_string(),
                });
            }

            let got_a = match cur.source.read(&mut interleaved_a) {
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
                if cur.source.is_eof() && queued == 0 && !finished_reported {
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

            // Push this voice's parameters into its chain before using it.
            // Without this the DSP nodes keep whatever state they were
            // constructed with — which means EQ, reverb, delay, lo-fi and the
            // normalisation gain all silently do nothing, while pitch (which
            // goes through the decoder, not the chain) still works. Cheap to
            // call every block: `update` diffs internally and only recomputes
            // coefficients when a value has actually changed.
            chains[cur.chain_ix].update(&cur.settings, cur.track_gain_db);

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
                cur.source.decoded_secs() - cur.source.info().duration_secs
            };
            let gain_a = crossfade.curve.gain_out(x as f32);
            for ch in 0..CHANNELS {
                for f in 0..frames {
                    mix[ch][f] *= gain_a;
                }
            }

            // The next voice is prepared early — deliberately, so a slow
            // decoder open cannot stall the transition — but it must not be
            // *read* until its fade actually begins. Reading it sooner would
            // advance its decoder while its gain was still zero, and the
            // incoming track would start several seconds in, having silently
            // thrown away its own opening.
            if x >= crossfade.curve.fade_in_start as f64 {
                let nx = next.as_mut().expect("checked by outer `if`");
                let nx_speed = if nx.settings.enabled {
                    nx.settings.pitch.ratio()
                } else {
                    1.0
                };
                if let Err(e) = nx.source.set_speed(nx_speed) {
                    let _ = events.send(EngineEvent::Error {
                        message: e.to_string(),
                    });
                }

                let want_b = frames * CHANNELS;
                let got_b = match nx.source.read(&mut interleaved_b[..want_b]) {
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

        // Tapped here, at the very end of the master bus, so the spectrum
        // shows what is actually leaving the app — including the EQ the graph
        // is drawn on top of.
        if shared.analyser_on.load(Ordering::Relaxed) && analyser.push(&mix, frames) {
            shared
                .analyser_bins
                .store(Arc::new(analyser.bins().to_vec()));
        }

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
        let position = (cur.source.decoded_secs() - queued_secs).max(0.0);
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

/// Whether this chain has anything that would still be sounding after its
/// input stops. Without one of these, a tail would be silence and pausing
/// should stay instant.
fn chain_has_tail(settings: &Resolved) -> bool {
    settings.enabled && (settings.reverb.enabled || settings.delay.enabled)
}

/// Longest tail to render, as a block count. Generous enough for a large
/// reverb, bounded so a runaway setting cannot hold the output open.
fn tail_block_budget(device_rate: u32) -> usize {
    const MAX_TAIL_SECS: f64 = 8.0;
    ((device_rate as f64 * MAX_TAIL_SECS) / BLOCK as f64).ceil() as usize
}

/// Where the listener actually got to, in track time.
///
/// The same accounting the progress bar uses: what has been decoded, less
/// what is still sitting unheard in the ring.
fn heard_position(
    producer: &rtrb::Producer<f32>,
    voice: &Voice,
    device_rate: u32,
    shared: &Shared,
) -> f64 {
    let capacity = producer.buffer().capacity();
    let queued_frames = (capacity - producer.slots()) / CHANNELS;
    let speed = f64::from(shared.speed_millis.load(Ordering::Relaxed) as u32) / 1000.0;
    let queued_secs = queued_frames as f64 / device_rate as f64 * speed.max(0.05);
    (voice.source.decoded_secs() - queued_secs).max(0.0)
}

/// Render one block of pure tail: silence through the effect chain, so only
/// what the reverb and delay still hold comes out.
///
/// Returns true once the result has decayed into inaudibility, so the caller
/// can stop early rather than always running the full budget.
fn drain_tail_block(
    chains: &mut [Chain; 2],
    voice: &mut Voice,
    master_limiter: &mut Limiter,
    mix: &mut [Vec<f32>],
    producer: &mut rtrb::Producer<f32>,
    device_rate: u32,
) -> bool {
    /// Below this the tail is inaudible at any sane volume.
    const SILENCE: f32 = 1e-4;

    for channel in mix.iter_mut() {
        for sample in channel[..BLOCK].iter_mut() {
            *sample = 0.0;
        }
    }

    let chain = &mut chains[voice.chain_ix];
    chain.update(&voice.settings, voice.track_gain_db);
    chain.process_music(mix, BLOCK);
    chain.apply_gain(mix, BLOCK);
    master_limiter.update(&voice.settings.normalisation, device_rate as f32);
    master_limiter.process(mix, BLOCK);

    let mut peak = 0.0f32;
    for f in 0..BLOCK {
        for channel in mix.iter() {
            peak = peak.max(channel[f].abs());
            let _ = producer.push(channel[f]);
        }
    }
    peak < SILENCE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_modes_ramp_only_the_selected_directions() {
        let ramp = 0.01;

        assert_eq!(fade_step(fade_mode_bits("off"), true, ramp), 1.0);
        assert_eq!(fade_step(fade_mode_bits("off"), false, ramp), 1.0);
        assert_eq!(fade_step(fade_mode_bits("play"), true, ramp), ramp);
        assert_eq!(fade_step(fade_mode_bits("play"), false, ramp), 1.0);
        assert_eq!(fade_step(fade_mode_bits("pause"), true, ramp), 1.0);
        assert_eq!(fade_step(fade_mode_bits("pause"), false, ramp), ramp);
        assert_eq!(fade_step(fade_mode_bits("both"), true, ramp), ramp);
        assert_eq!(fade_step(fade_mode_bits("both"), false, ramp), ramp);
    }

    #[test]
    fn loading_a_preseeked_timeline_reports_its_source_position() {
        let plan = crate::audio::timeline::Plan {
            blocks: Vec::new(),
            duration_secs: 30.0,
        };
        let mut timeline = TimelineSource::new(plan, 48_000);
        timeline.seek(12.25).unwrap();
        let source = Source::Timeline(Box::new(timeline));
        assert_eq!(source_position_ms(&source), 12_250);
    }

    #[test]
    fn unknown_engine_fade_modes_are_off() {
        assert_eq!(fade_mode_bits("unexpected"), 0);
    }
}
