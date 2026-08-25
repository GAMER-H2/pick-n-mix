//! The audio engine: one worker thread that decodes and processes audio into a
//! short ring buffer, and a cpal callback that does nothing but copy out of it.
//!
//! Keeping the callback trivial is deliberate. All the interesting work happens
//! on the worker, where allocation and the occasional slow path are harmless.

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
use crate::audio::decode::{StreamInfo, TrackDecoder};
use crate::audio::dsp::{Chain, CHANNELS};
use crate::audio::params::Resolved;

/// Frames processed per DSP block.
const BLOCK: usize = 512;
/// How much processed audio to keep queued. Short enough that a knob twist is
/// heard almost immediately, long enough to ride out scheduling hiccups.
const RING_MILLIS: usize = 120;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum EngineEvent {
    /// The current track played through to its end.
    TrackFinished,
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

enum Cmd {
    Load { path: PathBuf, start_secs: f64, gain_db: f32, reply: Sender<Result<StreamInfo>> },
    Seek(f64),
    Clear,
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
        let producer = ring_rx.recv().map_err(|_| anyhow!("audio ring was never handed over"))?;

        let worker_shared = Arc::clone(&shared);
        let worker_beds = bed_tx.clone();
        std::thread::Builder::new()
            .name("pnm-audio-dsp".into())
            .spawn(move || {
                worker(worker_shared, cmd_rx, producer, events, worker_beds, device_rate)
            })
            .map_err(|e| anyhow!("spawning dsp thread: {e}"))?;

        Ok(AudioEngine { shared, cmd_tx, bed_requests, bed_tx })
    }

    pub fn load(&self, path: PathBuf, start_secs: f64, gain_db: f32) -> Result<StreamInfo> {
        let (reply, rx) = bounded(1);
        self.cmd_tx
            .send(Cmd::Load { path, start_secs, gain_db, reply })
            .map_err(|_| anyhow!("audio worker is gone"))?;
        rx.recv().map_err(|_| anyhow!("audio worker dropped the request"))?
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
        self.shared.volume.store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn set_settings(&self, settings: Resolved) {
        self.shared.settings.store(Arc::new(settings));
    }

    pub fn settings(&self) -> Arc<Resolved> {
        self.shared.settings.load_full()
    }

    pub fn set_track_gain_db(&self, db: f32) {
        self.shared.track_gain_db.store(db.to_bits(), Ordering::Relaxed);
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

                let want_fade = if shared.playing.load(Ordering::Relaxed) { 1.0 } else { 0.0 };
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
    let mut chain = Chain::new();
    chain.prepare(device_rate as f32);
    let mut ambience = AmbienceMixer::new();
    ambience.prepare(device_rate as f32);

    let mut planar: Vec<Vec<f32>> = vec![vec![0.0; BLOCK]; CHANNELS];
    let mut interleaved = vec![0.0f32; BLOCK * CHANNELS];
    let mut decoder: Option<TrackDecoder> = None;
    let mut finished_reported = false;
    let mut requested_beds: Vec<String> = Vec::new();
    let mut meter_countdown = 0u32;

    loop {
        // --- commands ---------------------------------------------------
        let mut shutdown = false;
        while let Ok(cmd) = cmds.try_recv() {
            match cmd {
                Cmd::Load { path, start_secs, gain_db, reply } => {
                    shared.track_gain_db.store(gain_db.to_bits(), Ordering::Relaxed);
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
                            shared.position_ms.store(
                                (d.decoded_secs() * 1000.0) as u64,
                                Ordering::Relaxed,
                            );
                            shared.stream_info.store(Arc::new(Some(info.clone())));
                            decoder = Some(d);
                            finished_reported = false;
                            drain(&shared);
                            let _ = reply.send(Ok(info));
                        }
                        Err(e) => {
                            decoder = None;
                            shared.stream_info.store(Arc::new(None));
                            let _ = reply.send(Err(e));
                        }
                    }
                }
                Cmd::Seek(secs) => {
                    if let Some(d) = decoder.as_mut() {
                        if let Err(e) = d.seek(secs) {
                            let _ = events.send(EngineEvent::Error { message: e.to_string() });
                        }
                        drain(&shared);
                        finished_reported = false;
                        shared
                            .position_ms
                            .store((d.decoded_secs() * 1000.0) as u64, Ordering::Relaxed);
                    }
                }
                Cmd::Clear => {
                    decoder = None;
                    shared.stream_info.store(Arc::new(None));
                    shared.position_ms.store(0, Ordering::Relaxed);
                    shared.duration_ms.store(0, Ordering::Relaxed);
                    drain(&shared);
                }
                Cmd::Shutdown => shutdown = true,
            }
        }
        if shutdown {
            return;
        }

        // --- parameters -------------------------------------------------
        let settings = shared.settings.load_full();
        let bank = shared.bank.load_full();
        chain.update(&settings, f32::from_bits(shared.track_gain_db.load(Ordering::Relaxed)));

        let filters: &[crate::audio::params::Filter] =
            if settings.enabled { &settings.filters } else { &[] };
        ambience.sync(filters, &bank);
        for id in ambience.missing(filters, &bank) {
            if !requested_beds.iter().any(|r| r == id) {
                requested_beds.push(id.to_string());
                let _ = bed_requests.send(id.to_string());
            }
        }

        let speed = if settings.enabled { settings.pitch.ratio() } else { 1.0 };
        shared.speed_millis.store((speed * 1000.0) as u64, Ordering::Relaxed);

        // --- produce ----------------------------------------------------
        let idle = !shared.playing.load(Ordering::Relaxed) || decoder.is_none();
        let room = producer.slots() >= BLOCK * CHANNELS;
        if idle || !room {
            std::thread::sleep(Duration::from_millis(3));
            continue;
        }

        let d = decoder.as_mut().expect("checked above");
        if let Err(e) = d.set_speed(speed) {
            let _ = events.send(EngineEvent::Error { message: e.to_string() });
        }

        let got = match d.read(&mut interleaved) {
            Ok(n) => n,
            Err(e) => {
                let _ = events.send(EngineEvent::Error { message: e.to_string() });
                0
            }
        };
        let frames = got / CHANNELS;

        if frames == 0 {
            // Wait for the ring to empty so the tail is actually heard.
            let queued = BLOCK * CHANNELS - producer.slots().min(BLOCK * CHANNELS);
            if d.is_eof() && queued == 0 && !finished_reported {
                finished_reported = true;
                let _ = events.send(EngineEvent::TrackFinished);
            }
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }

        for f in 0..frames {
            for ch in 0..CHANNELS {
                planar[ch][f] = interleaved[f * CHANNELS + ch];
            }
        }

        if settings.enabled {
            chain.process_music(&mut planar, frames);
            if !ambience.is_silent() {
                ambience.process(&mut planar, frames);
            }
        }
        chain.finish(&mut planar, frames);

        for f in 0..frames {
            for ch in 0..CHANNELS {
                // The room check above guarantees these pushes succeed.
                let _ = producer.push(planar[ch][f]);
            }
        }

        // --- reporting --------------------------------------------------
        // What is decoded, less what is still queued, converted back into
        // track time so varispeed does not skew the progress bar.
        let capacity = producer.buffer().capacity();
        let queued_frames = (capacity - producer.slots()) / CHANNELS;
        let queued_secs = queued_frames as f64 / device_rate as f64 * speed;
        let position = (d.decoded_secs() - queued_secs).max(0.0);
        shared.position_ms.store((position * 1000.0) as u64, Ordering::Relaxed);

        meter_countdown += 1;
        if meter_countdown >= 4 {
            meter_countdown = 0;
            let red = chain.limiter.take_reduction_db();
            shared.reduction_millidb.store((red * 1000.0) as u32, Ordering::Relaxed);
        }
    }
}

/// Ask the output callback to throw away anything still queued.
fn drain(shared: &Shared) {
    shared.flush.store(true, Ordering::Release);
}
