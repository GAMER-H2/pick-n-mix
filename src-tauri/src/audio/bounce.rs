//! Offline render of a master mix to a file.
//!
//! The same [`TimelineSource`] used for audition is driven as fast as the CPU
//! allows, then the master limiter runs, then the samples are encoded. Hearing
//! the mix and bouncing it therefore cannot disagree about what a fade or a
//! keyframe did.

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::audio::dsp::{Limiter, CHANNELS};
use crate::audio::params::Normalisation;
use crate::audio::timeline::{Plan, TimelineSource};

const BLOCK: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BounceFormat {
    Wav,
    Flac,
    Mp3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BounceOptions {
    pub format: BounceFormat,
    pub sample_rate: u32,
    pub wav_bit_depth: u16,
    pub flac_compression: u8,
    pub mp3_bitrate: u16,
}

impl Default for BounceOptions {
    fn default() -> Self {
        BounceOptions {
            format: BounceFormat::Wav,
            sample_rate: 48_000,
            wav_bit_depth: 24,
            flac_compression: 5,
            mp3_bitrate: 320,
        }
    }
}

/// Render `plan` into `dest`, applying the master limiter with `normalisation`.
pub fn render(
    plan: Plan,
    dest: &Path,
    options: &BounceOptions,
    normalisation: &Normalisation,
) -> Result<()> {
    if plan.is_empty() {
        anyhow::bail!("there is nothing in this mix to bounce");
    }
    let rate = match options.sample_rate {
        44_100 | 48_000 | 96_000 => options.sample_rate,
        other => anyhow::bail!("unsupported sample rate {other}"),
    };
    let mut mix = OfflineMix::new(plan, rate, normalisation);
    match options.format {
        BounceFormat::Wav => write_wav(&mut mix, dest, options.wav_bit_depth),
        BounceFormat::Flac => write_flac(&mut mix, dest, options.flac_compression),
        BounceFormat::Mp3 => write_mp3(&mut mix, dest, options.mp3_bitrate),
    }
}

struct OfflineMix {
    source: TimelineSource,
    limiter: Limiter,
    interleaved: Vec<f32>,
    planar: Vec<Vec<f32>>,
    rate: u32,
    flushing: bool,
    flush_left: usize,
}

impl OfflineMix {
    fn new(plan: Plan, rate: u32, normalisation: &Normalisation) -> Self {
        let mut limiter = Limiter::new();
        limiter.prepare(rate as f32);
        limiter.update(normalisation, rate as f32);
        OfflineMix {
            source: TimelineSource::new(plan, rate),
            limiter,
            interleaved: vec![0.0; BLOCK * CHANNELS],
            planar: vec![vec![0.0; BLOCK]; CHANNELS],
            rate,
            flushing: false,
            // A few limiter lookahead windows of silence so the delay line
            // drains into the file rather than being cut off with it.
            flush_left: ((5.0 / 1000.0) * rate as f32).ceil() as usize + BLOCK,
        }
    }

    /// Fill `planar` with the next `frames` of limited audio. 0 means the mix
    /// and the limiter flush are both finished.
    fn next_planar(&mut self, frames: usize) -> Result<usize> {
        let frames = frames.min(BLOCK);
        if frames == 0 {
            return Ok(0);
        }
        self.ensure(frames);
        let got = if self.flushing {
            0
        } else {
            self.source
                .read_offline(&mut self.interleaved[..frames * CHANNELS])?
                / CHANNELS
        };
        if got == 0 {
            self.flushing = true;
            if self.flush_left == 0 {
                return Ok(0);
            }
            let n = self.flush_left.min(frames);
            for ch in 0..CHANNELS {
                for s in self.planar[ch][..n].iter_mut() {
                    *s = 0.0;
                }
            }
            self.limiter.process(&mut self.planar, n);
            self.flush_left -= n;
            return Ok(n);
        }
        for f in 0..got {
            for ch in 0..CHANNELS {
                self.planar[ch][f] = self.interleaved[f * CHANNELS + ch];
            }
        }
        self.limiter.process(&mut self.planar, got);
        Ok(got)
    }

    fn ensure(&mut self, frames: usize) {
        if self.interleaved.len() < frames * CHANNELS {
            self.interleaved.resize(frames * CHANNELS, 0.0);
        }
        for channel in self.planar.iter_mut() {
            if channel.len() < frames {
                channel.resize(frames, 0.0);
            }
        }
    }
}

fn write_wav(mix: &mut OfflineMix, dest: &Path, bit_depth: u16) -> Result<()> {
    let bits = match bit_depth {
        16 | 24 | 32 => bit_depth,
        other => anyhow::bail!("WAV bit depth must be 16, 24 or 32, not {other}"),
    };
    let float = bits == 32;
    let block_align = CHANNELS as u16 * (bits / 8);
    let byte_rate = mix.rate * block_align as u32;
    let mut file = File::create(dest).with_context(|| format!("creating {}", dest.display()))?;

    file.write_all(b"RIFF")?;
    file.write_all(&0u32.to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&(if float { 3u16 } else { 1u16 }).to_le_bytes())?;
    file.write_all(&(CHANNELS as u16).to_le_bytes())?;
    file.write_all(&mix.rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&bits.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&0u32.to_le_bytes())?;

    let mut data_bytes: u32 = 0;
    loop {
        let frames = mix.next_planar(BLOCK)?;
        if frames == 0 {
            break;
        }
        for f in 0..frames {
            for ch in 0..CHANNELS {
                let sample = mix.planar[ch][f].clamp(-1.0, 1.0);
                match bits {
                    16 => {
                        let pcm = (sample * i16::MAX as f32).round() as i16;
                        file.write_all(&pcm.to_le_bytes())?;
                        data_bytes += 2;
                    }
                    24 => {
                        let pcm = (sample * 8_388_607.0).round() as i32;
                        file.write_all(&[
                            (pcm & 0xff) as u8,
                            ((pcm >> 8) & 0xff) as u8,
                            ((pcm >> 16) & 0xff) as u8,
                        ])?;
                        data_bytes += 3;
                    }
                    _ => {
                        file.write_all(&sample.to_le_bytes())?;
                        data_bytes += 4;
                    }
                }
            }
        }
    }

    let riff_size = 36u32.saturating_add(data_bytes);
    file.seek(SeekFrom::Start(4))?;
    file.write_all(&riff_size.to_le_bytes())?;
    file.seek(SeekFrom::Start(40))?;
    file.write_all(&data_bytes.to_le_bytes())?;
    Ok(())
}

fn write_flac(mix: &mut OfflineMix, dest: &Path, compression: u8) -> Result<()> {
    use flacenc::bitsink::ByteSink;
    use flacenc::component::BitRepr;
    use flacenc::config;
    use flacenc::encode_with_fixed_block_size;
    use flacenc::error::Verify;
    use flacenc::source::MemSource;

    let mut samples: Vec<i32> = Vec::new();
    loop {
        let frames = mix.next_planar(BLOCK)?;
        if frames == 0 {
            break;
        }
        for f in 0..frames {
            for ch in 0..CHANNELS {
                let sample = mix.planar[ch][f].clamp(-1.0, 1.0);
                samples.push((sample * 8_388_607.0).round() as i32);
            }
        }
    }
    if samples.is_empty() {
        anyhow::bail!("the mix produced no audio");
    }

    let mut encoder = config::Encoder::default();
    encoder.block_size = if compression >= 8 {
        4096
    } else if compression == 0 {
        1024
    } else {
        2048
    };
    let block_size = encoder.block_size;
    let verified = encoder
        .into_verified()
        .map_err(|(_, e)| anyhow!("flac config: {e}"))?;
    let source = MemSource::from_samples(&samples, CHANNELS, 24, mix.rate as usize);
    let stream = encode_with_fixed_block_size(&verified, source, block_size)
        .map_err(|e| anyhow!("flac encode: {e:?}"))?;
    let mut sink = ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| anyhow!("flac write: {e:?}"))?;
    std::fs::write(dest, sink.as_slice()).with_context(|| format!("writing {}", dest.display()))
}

fn write_mp3(mix: &mut OfflineMix, dest: &Path, bitrate: u16) -> Result<()> {
    use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, Quality};

    let mut builder = Builder::new().ok_or_else(|| anyhow!("could not start the MP3 encoder"))?;
    builder
        .set_num_channels(CHANNELS as u8)
        .map_err(|e| anyhow!("mp3 channels: {e:?}"))?;
    builder
        .set_sample_rate(mix.rate)
        .map_err(|e| anyhow!("mp3 sample rate: {e:?}"))?;
    let brate = match bitrate {
        128 => Bitrate::Kbps128,
        192 => Bitrate::Kbps192,
        256 => Bitrate::Kbps256,
        _ => Bitrate::Kbps320,
    };
    builder
        .set_brate(brate)
        .map_err(|e| anyhow!("mp3 bitrate: {e:?}"))?;
    builder
        .set_quality(Quality::Best)
        .map_err(|e| anyhow!("mp3 quality: {e:?}"))?;
    let mut encoder = builder.build().map_err(|e| anyhow!("mp3 encoder: {e:?}"))?;

    let mut file = File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut out = Vec::new();

    loop {
        let frames = mix.next_planar(BLOCK)?;
        if frames == 0 {
            break;
        }
        left.clear();
        right.clear();
        for f in 0..frames {
            left.push(mix.planar[0][f].clamp(-1.0, 1.0));
            right.push(mix.planar[1][f].clamp(-1.0, 1.0));
        }
        out.clear();
        out.reserve(mp3lame_encoder::max_required_buffer_size(frames));
        encoder
            .encode_to_vec(
                DualPcm {
                    left: &left,
                    right: &right,
                },
                &mut out,
            )
            .map_err(|e| anyhow!("mp3 encode: {e:?}"))?;
        file.write_all(&out)?;
    }
    out.clear();
    out.reserve(7200);
    encoder
        .flush_to_vec::<FlushNoGap>(&mut out)
        .map_err(|e| anyhow!("mp3 flush: {e:?}"))?;
    file.write_all(&out)?;
    Ok(())
}
