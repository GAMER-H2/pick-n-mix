//! Symphonia-backed decoding.
//!
//! Produces stereo f32 already converted to the output device's sample rate,
//! so the DSP stage downstream only ever deals with one rate. The varispeed
//! ratio is folded into the same resampler pass rather than being a second
//! conversion, which keeps quality and CPU cost down.

use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use symphonia::core::audio::GenericAudioBufferRef;
use symphonia::core::codecs::audio::AudioDecoder;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Time;

use crate::audio::dsp::CHANNELS;

/// Frames of source audio handed to the resampler at a time.
const CHUNK: usize = 1024;
/// Widest varispeed swing the resampler is built to allow, in either direction.
const MAX_RATIO_SWING: f64 = 4.0;

/// Facts about a track that the UI's info panel shows.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_secs: f64,
    pub codec: String,
    pub bits_per_sample: Option<u32>,
    pub bitrate_kbps: Option<u32>,
}

/// A single opened file, decoded on demand into stereo at `target_rate`.
pub struct TrackDecoder {
    reader: Box<dyn FormatReader + 'static>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<symphonia::core::units::TimeBase>,
    resampler: Option<SincFixedIn<f32>>,
    /// Planar scratch the resampler reads from, refilled to exactly CHUNK frames.
    in_buf: Vec<Vec<f32>>,
    in_filled: usize,
    out_buf: Vec<Vec<f32>>,
    /// Interleaved samples decoded but not yet consumed by the caller.
    pending: Vec<f32>,
    /// Interleaved source-rate samples awaiting resampling.
    staging: Vec<f32>,
    source_rate: u32,
    target_rate: u32,
    ratio_base: f64,
    speed: f64,
    /// Output frames the real (non-padding) input entitles us to. The final
    /// chunk is zero-padded to fill the resampler, so without this the tail of
    /// every track would gain a few tens of milliseconds of silence.
    output_budget: f64,
    output_emitted: u64,
    pub info: StreamInfo,
    /// Source frames decoded so far, used to report playback position.
    pub source_frames: u64,
    eof: bool,
}

impl TrackDecoder {
    pub fn open(path: &Path, target_rate: u32) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let reader = symphonia::default::get_probe()
            .probe(&hint, mss, Default::default(), MetadataOptions::default())
            .with_context(|| format!("probing {}", path.display()))?;

        let track = reader
            .default_track(TrackType::Audio)
            .or_else(|| reader.first_track(TrackType::Audio))
            .ok_or_else(|| anyhow!("no audio track in {}", path.display()))?;

        let track_id = track.id;
        let time_base = track.time_base;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or_else(|| anyhow!("no audio codec parameters for {}", path.display()))?
            .clone();

        let duration_secs = track
            .num_frames
            .zip(params.sample_rate)
            .map(|(frames, rate)| frames as f64 / rate as f64)
            .or_else(|| {
                track
                    .duration
                    .zip(time_base)
                    .and_then(|(d, tb)| tb.calc_duration(d))
                    .map(|t| t.as_secs_f64())
            })
            .unwrap_or(0.0);

        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &Default::default())
            .with_context(|| format!("no decoder for {}", path.display()))?;

        let source_rate = params.sample_rate.unwrap_or(target_rate);
        let channels = params.channels.as_ref().map(|c| c.count()).unwrap_or(2) as u16;

        let byte_len = std::fs::metadata(path).map(|m| m.len()).ok();
        let bitrate_kbps = byte_len.and_then(|bytes| {
            (duration_secs > 0.1).then(|| (bytes as f64 * 8.0 / duration_secs / 1000.0) as u32)
        });

        let info = StreamInfo {
            sample_rate: source_rate,
            channels,
            duration_secs,
            codec: decoder.codec_info().short_name.to_string(),
            bits_per_sample: params.bits_per_sample,
            bitrate_kbps,
        };

        let mut dec = TrackDecoder {
            reader,
            decoder,
            track_id,
            time_base,
            resampler: None,
            in_buf: vec![vec![0.0; CHUNK]; CHANNELS],
            in_filled: 0,
            out_buf: Vec::new(),
            pending: Vec::new(),
            staging: Vec::new(),
            source_rate,
            target_rate,
            ratio_base: target_rate as f64 / source_rate as f64,
            speed: 1.0,
            output_budget: 0.0,
            output_emitted: 0,
            info,
            source_frames: 0,
            eof: false,
        };
        dec.build_resampler()?;
        Ok(dec)
    }

    fn build_resampler(&mut self) -> Result<()> {
        // A pass-through is still cheaper than special-casing every call site,
        // but skip it entirely when nothing needs converting.
        if self.source_rate == self.target_rate && (self.speed - 1.0).abs() < f64::EPSILON {
            self.resampler = None;
            return Ok(());
        }
        let params = SincInterpolationParameters {
            sinc_len: 128,
            f_cutoff: 0.95,
            oversampling_factor: 128,
            interpolation: SincInterpolationType::Linear,
            window: WindowFunction::BlackmanHarris2,
        };
        let resampler = SincFixedIn::<f32>::new(
            self.ratio_base / self.speed,
            MAX_RATIO_SWING,
            params,
            CHUNK,
            CHANNELS,
        )
        .map_err(|e| anyhow!("building resampler: {e}"))?;
        self.out_buf = resampler.output_buffer_allocate(true);
        self.resampler = Some(resampler);
        Ok(())
    }

    /// Playback rate multiplier; pitch and tempo move together.
    pub fn set_speed(&mut self, speed: f64) -> Result<()> {
        let speed = speed.clamp(1.0 / MAX_RATIO_SWING, MAX_RATIO_SWING);
        if (speed - self.speed).abs() < 1e-9 {
            return Ok(());
        }
        self.speed = speed;
        match self.resampler.as_mut() {
            // `ramp: true` glides the ratio internally so the change is silent.
            Some(r) => r
                .set_resample_ratio(self.ratio_base / speed, true)
                .map_err(|e| anyhow!("setting resample ratio: {e}"))?,
            None => self.build_resampler()?,
        }
        Ok(())
    }

    pub fn source_rate(&self) -> u32 {
        self.source_rate
    }

    pub fn speed(&self) -> f64 {
        self.speed
    }

    pub fn seek(&mut self, secs: f64) -> Result<()> {
        let time = Time::try_from_secs_f64(secs.max(0.0))
            .ok_or_else(|| anyhow!("seek position {secs} is out of range"))?;
        self.reader
            .seek(SeekMode::Accurate, SeekTo::Time { time, track_id: Some(self.track_id) })
            .map_err(|e| anyhow!("seeking: {e}"))?;
        self.decoder.reset();
        self.pending.clear();
        self.staging.clear();
        self.in_filled = 0;
        if let Some(r) = self.resampler.as_mut() {
            r.reset();
            // reset() drops the ratio glide, so restate it.
            let _ = r.set_resample_ratio(self.ratio_base / self.speed, false);
        }
        self.source_frames = (secs.max(0.0) * self.source_rate as f64) as u64;
        self.output_budget = 0.0;
        self.output_emitted = 0;
        self.eof = false;
        Ok(())
    }

    /// Position in seconds of the most recently decoded audio.
    pub fn decoded_secs(&self) -> f64 {
        self.source_frames as f64 / self.source_rate as f64
    }

    pub fn is_eof(&self) -> bool {
        self.eof && self.pending.is_empty() && self.staging.is_empty()
    }

    /// Fill `out` with up to `out.len()` interleaved stereo samples at the
    /// target rate. Returns how many were written; 0 means end of track.
    pub fn read(&mut self, out: &mut [f32]) -> Result<usize> {
        let mut written = 0;
        while written < out.len() {
            if self.pending.is_empty() {
                if !self.produce()? {
                    break;
                }
                continue;
            }
            let take = (out.len() - written).min(self.pending.len());
            out[written..written + take].copy_from_slice(&self.pending[..take]);
            self.pending.drain(..take);
            written += take;
        }
        Ok(written)
    }

    /// Decode and resample one more chunk into `pending`. Returns false at EOF.
    fn produce(&mut self) -> Result<bool> {
        let Some(_) = self.resampler.as_ref() else {
            // No rate conversion needed: hand decoded frames straight through.
            //
            // `fill_staging` reporting "no more packets" does not mean there is
            // nothing to play: the final, partial chunk is still sitting in
            // `staging`. Returning early here stranded it, which silently
            // clipped the end of every track and — because `is_eof` waits for
            // `staging` to empty — meant the engine never announced the track
            // had finished, so the queue stopped dead.
            self.fill_staging(CHUNK * CHANNELS)?;
            if self.staging.is_empty() {
                return Ok(false);
            }
            self.pending.append(&mut self.staging);
            return Ok(true);
        };

        let want = CHUNK * CHANNELS;
        let have = self.fill_staging(want)?;
        let available = self.staging.len() / CHANNELS;
        if !have && available == 0 {
            // Flush the resampler's internal tail once the source is exhausted.
            return self.flush_resampler();
        }

        let frames = available.min(CHUNK);
        for f in 0..frames {
            for ch in 0..CHANNELS {
                self.in_buf[ch][f] = self.staging[f * CHANNELS + ch];
            }
        }
        // The resampler needs a full chunk; pad the tail of the final one.
        for ch in 0..CHANNELS {
            for f in frames..CHUNK {
                self.in_buf[ch][f] = 0.0;
            }
        }
        self.staging.drain(..frames * CHANNELS);
        self.in_filled = frames;
        self.output_budget += frames as f64 * (self.ratio_base / self.speed);

        let resampler = self.resampler.as_mut().expect("checked above");
        let (_, out_frames) = resampler
            .process_into_buffer(&self.in_buf, &mut self.out_buf, None)
            .map_err(|e| anyhow!("resampling: {e}"))?;

        let emit = self.allowed_output(out_frames);
        self.emit(emit);
        Ok(true)
    }

    fn flush_resampler(&mut self) -> Result<bool> {
        if self.in_filled == 0 {
            return Ok(false);
        }
        self.in_filled = 0;
        let resampler = self.resampler.as_mut().expect("caller checked");
        let silence: Vec<Vec<f32>> = vec![vec![0.0; CHUNK]; CHANNELS];
        let (_, out_frames) = resampler
            .process_into_buffer(&silence, &mut self.out_buf, None)
            .map_err(|e| anyhow!("flushing resampler: {e}"))?;

        let emit = self.allowed_output(out_frames);
        self.emit(emit);
        Ok(emit > 0)
    }

    /// How many of `produced` frames we may actually emit.
    ///
    /// While the source is still supplying packets the resampler's own
    /// chunking makes the running total wobble by a frame either way, and
    /// dropping frames there would be an audible glitch, so the budget is only
    /// enforced once the source is exhausted and the padding starts.
    fn allowed_output(&self, produced: usize) -> usize {
        if !self.eof {
            return produced;
        }
        let budget = self.output_budget.floor().max(0.0) as u64;
        let remaining = budget.saturating_sub(self.output_emitted) as usize;
        produced.min(remaining)
    }

    fn emit(&mut self, frames: usize) {
        self.pending.reserve(frames * CHANNELS);
        for f in 0..frames {
            for ch in 0..CHANNELS {
                self.pending.push(self.out_buf[ch][f]);
            }
        }
        self.output_emitted += frames as u64;
    }

    /// Top `staging` up to at least `want` interleaved samples.
    /// Returns false once the source has no more packets.
    fn fill_staging(&mut self, want: usize) -> Result<bool> {
        let mut scratch: Vec<f32> = Vec::new();
        while self.staging.len() < want {
            if self.eof {
                return Ok(false);
            }
            let packet = match self.reader.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => {
                    self.eof = true;
                    return Ok(false);
                }
                Err(e) => {
                    self.eof = true;
                    return Err(anyhow!("reading packet: {e}"));
                }
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                // Recoverable glitches are common in the wild; skip the packet.
                Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
                Err(e) => {
                    self.eof = true;
                    return Err(anyhow!("decoding: {e}"));
                }
            };
            let frames = decoded.frames();
            if frames == 0 {
                continue;
            }
            let src_channels = decoded.spec().channels().count().max(1);
            scratch.clear();
            copy_interleaved(&decoded, &mut scratch);
            to_stereo(&scratch, src_channels, &mut self.staging);
            self.source_frames += frames as u64;

            if let Some(tb) = self.time_base {
                // Trust the container's timestamp when it has one.
                if let Some(t) = tb.calc_time(packet.pts) {
                    let secs = t.as_secs_f64();
                    self.source_frames = ((secs * self.source_rate as f64) as u64)
                        .saturating_add(frames as u64);
                }
            }
        }
        Ok(true)
    }
}

fn copy_interleaved(buf: &GenericAudioBufferRef<'_>, out: &mut Vec<f32>) {
    buf.copy_to_vec_interleaved(out);
}

/// Fold any channel count down (or up) to stereo, appending to `out`.
fn to_stereo(input: &[f32], src_channels: usize, out: &mut Vec<f32>) {
    match src_channels {
        1 => {
            out.reserve(input.len() * 2);
            for &s in input {
                out.push(s);
                out.push(s);
            }
        }
        2 => out.extend_from_slice(input),
        n => {
            // Take the first two channels; they are L/R in every common layout.
            out.reserve(input.len() / n * 2);
            for frame in input.chunks_exact(n) {
                out.push(frame[0]);
                out.push(frame[1]);
            }
        }
    }
}

/// Decode a whole file to stereo at `target_rate`. Used for the ambience beds,
/// which are short and are held in memory for looping.
pub fn decode_whole(path: &PathBuf, target_rate: u32) -> Result<Vec<f32>> {
    let mut dec = TrackDecoder::open(path, target_rate)?;
    let mut out = Vec::new();
    let mut chunk = vec![0.0f32; 8192];
    loop {
        let n = dec.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&chunk[..n]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_is_duplicated_across_both_channels() {
        let mut out = Vec::new();
        to_stereo(&[0.5, -0.5], 1, &mut out);
        assert_eq!(out, vec![0.5, 0.5, -0.5, -0.5]);
    }

    #[test]
    fn surround_keeps_the_front_pair() {
        let mut out = Vec::new();
        to_stereo(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 6, &mut out);
        assert_eq!(out, vec![1.0, 2.0]);
    }

    #[test]
    fn stereo_passes_straight_through() {
        let mut out = Vec::new();
        to_stereo(&[1.0, 2.0, 3.0, 4.0], 2, &mut out);
        assert_eq!(out, vec![1.0, 2.0, 3.0, 4.0]);
    }
}
