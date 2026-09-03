//! Offline render of a master mix to a file.
//!
//! The same [`TimelineSource`] used for audition is driven as fast as the CPU
//! allows, then the master limiter runs, then the samples are encoded. Hearing
//! the mix and bouncing it therefore cannot disagree about what a fade or a
//! keyframe did.

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::audio::ambience::Bank;
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
///
/// `bank` supplies the decoded ambience beds any block asks for. A bounce is
/// offline, so there is nobody to decode one late: a bed missing from the bank
/// is simply absent from the file, which is why the caller loads them all
/// before getting here.
///
/// `cover` is an image file to embed as the finished track's artwork — the
/// playlist's own picture, so a bounced mix arrives in another player looking
/// like the playlist it came from.
pub fn render(
    plan: Plan,
    dest: &Path,
    options: &BounceOptions,
    normalisation: &Normalisation,
    bank: Arc<Bank>,
    cover: Option<&Path>,
    progress: &dyn Fn(f64),
) -> Result<()> {
    if plan.is_empty() {
        anyhow::bail!("there is nothing in this mix to bounce");
    }
    let rate = match options.sample_rate {
        44_100 | 48_000 | 96_000 => options.sample_rate,
        other => anyhow::bail!("unsupported sample rate {other}"),
    };
    let mut mix = OfflineMix::new(plan, rate, normalisation, bank);
    mix.progress = Some(progress);
    match options.format {
        BounceFormat::Wav => write_wav(&mut mix, dest, options.wav_bit_depth),
        // FLAC carries its picture in a metadata block this module writes
        // itself; see `write_flac`.
        BounceFormat::Flac => write_flac(&mut mix, dest, options.flac_compression, cover),
        BounceFormat::Mp3 => write_mp3(&mut mix, dest, options.mp3_bitrate),
    }?;
    if let Some(cover) = cover.filter(|_| options.format != BounceFormat::Flac) {
        // The audio is already on disk and correct. A picture that will not go
        // in — an unreadable image, or a container this build of lofty has no
        // tag for — is not worth throwing the render away for.
        if let Err(error) = embed_cover(dest, cover) {
            eprintln!("bounce: could not embed the playlist artwork: {error:#}");
        }
    }
    progress(1.0);
    Ok(())
}

/// Write `cover` into a finished WAV or MP3 as its front-cover picture.
///
/// Done afterwards rather than during encoding because both containers are
/// tagged differently and lofty knows them both. The tag is read back first so
/// an encoder that wrote one of its own — ffmpeg does — is added to rather
/// than replaced.
///
/// FLAC is deliberately not sent through here. lofty appends its `PICTURE`
/// block without clearing the last-metadata-block flag on the `STREAMINFO`
/// before it, which leaves a file every decoder reads the picture out of as
/// though it were audio: a burst of noise and then a stream it has to
/// resynchronise into. `write_flac` writes that block itself instead, in the
/// right place and with the right flags.
fn embed_cover(dest: &Path, cover: &Path) -> Result<()> {
    use lofty::config::WriteOptions;
    use lofty::file::TaggedFileExt;
    use lofty::picture::{Picture, PictureType};
    use lofty::probe::Probe;
    use lofty::tag::{Tag, TagExt};

    let data = std::fs::read(cover).with_context(|| format!("reading {}", cover.display()))?;
    let mut picture = Picture::from_reader(&mut std::io::Cursor::new(data))
        .with_context(|| format!("reading the image in {}", cover.display()))?;
    picture.set_pic_type(PictureType::CoverFront);

    let tagged = Probe::open(dest)
        .with_context(|| format!("opening {}", dest.display()))?
        .read()
        .with_context(|| format!("reading tags from {}", dest.display()))?;
    let tag_type = tagged.primary_tag_type();
    let mut tag = tagged
        .primary_tag()
        .cloned()
        .unwrap_or_else(|| Tag::new(tag_type));
    tag.remove_picture_type(PictureType::CoverFront);
    tag.push_picture(picture);
    tag.save_to_path(dest, WriteOptions::default())
        .with_context(|| format!("writing artwork into {}", dest.display()))?;
    Ok(())
}

struct OfflineMix<'a> {
    source: TimelineSource,
    limiter: Limiter,
    interleaved: Vec<f32>,
    planar: Vec<Vec<f32>>,
    rate: u32,
    flushing: bool,
    flush_left: usize,
    /// Frames handed out so far, against the length the arrangement claims —
    /// which is all a progress bar needs, and costs one add per block.
    produced: usize,
    expected: f64,
    reported: f64,
    progress: Option<&'a dyn Fn(f64)>,
}

impl<'a> OfflineMix<'a> {
    fn new(plan: Plan, rate: u32, normalisation: &Normalisation, bank: Arc<Bank>) -> Self {
        let mut limiter = Limiter::new();
        limiter.prepare(rate as f32);
        limiter.update(normalisation, rate as f32);
        let mut source = TimelineSource::new(plan, rate);
        source.set_bank(bank);
        let expected = (source.info().duration_secs * rate as f64).max(1.0);
        OfflineMix {
            source,
            limiter,
            interleaved: vec![0.0; BLOCK * CHANNELS],
            planar: vec![vec![0.0; BLOCK]; CHANNELS],
            rate,
            flushing: false,
            // A few limiter lookahead windows of silence so the delay line
            // drains into the file rather than being cut off with it.
            flush_left: ((5.0 / 1000.0) * rate as f32).ceil() as usize + BLOCK,
            produced: 0,
            expected,
            reported: 0.0,
            progress: None,
        }
    }

    /// Tell whoever is watching how far in we are.
    ///
    /// Throttled to whole half-percents: a bounce reads a thousand blocks a
    /// second, and an event per block would cost more than the encoding.
    fn report(&mut self) {
        let Some(progress) = self.progress else { return };
        let fraction = (self.produced as f64 / self.expected).clamp(0.0, 0.999);
        if fraction < self.reported + 0.005 {
            return;
        }
        self.reported = fraction;
        progress(fraction);
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
        self.produced += got;
        self.report();
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

fn write_wav(mix: &mut OfflineMix<'_>, dest: &Path, bit_depth: u16) -> Result<()> {
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

/// Encode the mix as FLAC, one block at a time, straight to the file.
///
/// Written by hand around `flacenc`'s frame encoder rather than through its
/// whole-stream entry point, for two reasons:
///
/// * **Memory.** That entry point takes every sample at once and gives back
///   every frame at once. A 48-minute mix is a gigabyte of samples, another
///   gigabyte of encoded frames and a third of output buffer before a byte
///   reaches the disk. Here a block is read, encoded, written and forgotten,
///   so a mix of any length costs a few kilobytes.
/// * **The picture.** A FLAC cover is a metadata block, which has to be
///   written before the audio and with the flags around it kept straight.
///
/// `STREAMINFO` is written twice: once as a placeholder, and once at the end
/// when the frame sizes and the true length are known. Its MD5 field is left
/// zero, which the format defines as "not computed", rather than filled with
/// the digest of the zero-padded final block, which would be wrong.
fn write_flac(
    mix: &mut OfflineMix<'_>,
    dest: &Path,
    compression: u8,
    cover: Option<&Path>,
) -> Result<()> {
    use flacenc::bitsink::ByteSink;
    use flacenc::component::{BitRepr, StreamInfo};
    use flacenc::config;
    use flacenc::encode_fixed_size_frame;
    use flacenc::error::Verify;
    use flacenc::source::{Fill, FrameBuf};

    let block_size = if compression >= 8 {
        4096
    } else if compression == 0 {
        1024
    } else {
        2048
    };
    let mut encoder = config::Encoder::default();
    encoder.block_size = block_size;
    let config = encoder
        .into_verified()
        .map_err(|(_, e)| anyhow!("flac config: {e}"))?;

    let mut info = StreamInfo::new(mix.rate as usize, CHANNELS, FLAC_BITS)
        .map_err(|e| anyhow!("flac stream info: {e:?}"))?;
    let picture = match cover {
        Some(path) => picture_block(path)
            .map_err(|error| {
                eprintln!("bounce: could not embed the playlist artwork: {error:#}");
                error
            })
            .ok(),
        None => None,
    };

    let mut file = File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    write_flac_header(&mut file, &info, picture.as_deref())?;

    let mut framebuf =
        FrameBuf::with_size(CHANNELS, block_size).map_err(|e| anyhow!("flac buffer: {e:?}"))?;
    let mut interleaved: Vec<i32> = vec![0; block_size * CHANNELS];
    let mut sink = ByteSink::new();
    let mut frame_number = 0usize;
    let mut total_samples = 0usize;

    loop {
        // A block is filled from as many reads as it takes: the mix hands out
        // audio in its own block size, which is not this one.
        let mut filled = 0;
        while filled < block_size {
            let got = mix.next_planar(block_size - filled)?;
            if got == 0 {
                break;
            }
            for f in 0..got {
                for ch in 0..CHANNELS {
                    let sample = mix.planar[ch][f].clamp(-1.0, 1.0);
                    interleaved[(filled + f) * CHANNELS + ch] =
                        (sample * FLAC_FULL_SCALE).round() as i32;
                }
            }
            filled += got;
        }
        if filled == 0 {
            break;
        }
        // The last block is padded with silence and trimmed by the total
        // sample count in the header, which is what a fixed-block-size stream
        // is required to do.
        for value in interleaved[filled * CHANNELS..].iter_mut() {
            *value = 0;
        }

        framebuf
            .fill_interleaved(&interleaved)
            .map_err(|e| anyhow!("flac buffer: {e:?}"))?;
        let frame = encode_fixed_size_frame(&config, &framebuf, frame_number, &info)
            .map_err(|e| anyhow!("flac encode: {e:?}"))?;
        sink.clear();
        frame
            .write(&mut sink)
            .map_err(|e| anyhow!("flac write: {e:?}"))?;
        file.write_all(sink.as_slice())?;

        info.update_frame_info(&frame);
        frame_number += 1;
        total_samples += filled;
        if filled < block_size {
            break;
        }
    }

    if total_samples == 0 {
        anyhow::bail!("the mix produced no audio");
    }
    info.set_total_samples(total_samples);
    file.seek(SeekFrom::Start(0))?;
    write_flac_header(&mut file, &info, picture.as_deref())?;
    Ok(())
}

/// 24-bit, matching what the WAV writer defaults to and what the limiter feeds.
const FLAC_BITS: usize = 24;
const FLAC_FULL_SCALE: f32 = 8_388_607.0;

/// "fLaC", then `STREAMINFO`, then the picture if there is one.
///
/// The last-metadata-block flag belongs to whichever of them comes last, and
/// getting that wrong is exactly the bug that made an embedded cover play as
/// noise; see [`embed_cover`].
fn write_flac_header(
    file: &mut File,
    info: &flacenc::component::StreamInfo,
    picture: Option<&[u8]>,
) -> Result<()> {
    use flacenc::bitsink::ByteSink;
    use flacenc::component::BitRepr;

    let mut sink = ByteSink::new();
    info.write(&mut sink)
        .map_err(|e| anyhow!("flac stream info: {e:?}"))?;
    let body = sink.as_slice();

    file.write_all(b"fLaC")?;
    file.write_all(&[if picture.is_some() { 0x00 } else { 0x80 }])?;
    file.write_all(&[
        ((body.len() >> 16) & 0xff) as u8,
        ((body.len() >> 8) & 0xff) as u8,
        (body.len() & 0xff) as u8,
    ])?;
    file.write_all(body)?;

    if let Some(picture) = picture {
        // Type 6 is PICTURE, and it is last.
        file.write_all(&[0x86])?;
        file.write_all(&[
            ((picture.len() >> 16) & 0xff) as u8,
            ((picture.len() >> 8) & 0xff) as u8,
            (picture.len() & 0xff) as u8,
        ])?;
        file.write_all(picture)?;
    }
    Ok(())
}

/// The body of a FLAC `PICTURE` block holding `path` as the front cover.
///
/// Dimensions and colour depth are filled in where the image can be read and
/// left as "unknown" — zero, which the format allows — where it cannot, since
/// a player that wants them can always measure the picture itself.
fn picture_block(path: &Path) -> Result<Vec<u8>> {
    let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mime = match image_kind(&data) {
        Some(kind) => kind,
        None => anyhow::bail!("{} is not an image FLAC can carry", path.display()),
    };
    let (width, height) = image::image_dimensions(path).unwrap_or((0, 0));
    let depth: u32 = if width == 0 { 0 } else { 24 };

    let mut out = Vec::with_capacity(data.len() + 64);
    out.extend_from_slice(&3u32.to_be_bytes()); // front cover
    out.extend_from_slice(&(mime.len() as u32).to_be_bytes());
    out.extend_from_slice(mime.as_bytes());
    out.extend_from_slice(&0u32.to_be_bytes()); // no description
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&depth.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes()); // not an indexed-colour image
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&data);
    Ok(out)
}

/// The MIME type of an image, from its own first bytes rather than its name.
fn image_kind(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if data.len() > 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

/// MP3 goes out through ffmpeg rather than a linked encoder; see
/// [`crate::audio::ffmpeg`] for why. The limiter and every fade have already
/// run by the time a sample reaches the pipe, so what is encoded is what the
/// audition played.
fn write_mp3(mix: &mut OfflineMix<'_>, dest: &Path, bitrate: u16) -> Result<()> {
    crate::audio::ffmpeg::encode_mp3(dest, mix.rate, bitrate, CHANNELS, |out| {
        let frames = mix.next_planar(out.len() / CHANNELS)?;
        for f in 0..frames {
            for ch in 0..CHANNELS {
                out[f * CHANNELS + ch] = mix.planar[ch][f].clamp(-1.0, 1.0);
            }
        }
        Ok(frames)
    })
}
