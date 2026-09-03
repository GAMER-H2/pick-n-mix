//! Waveform peaks for the master mixer's timeline.
//!
//! Drawing a block means drawing the shape of its audio, and the only way to
//! know that shape is to decode the file. That is far too slow to do while the
//! user drags something, so peaks are computed once per file and cached on
//! disk, keyed by the file's path, size and modification time — so a file that
//! is edited or replaced is recomputed, and one that is merely moved through
//! the library is not.
//!
//! The stored resolution is deliberately modest. At [`PEAKS_PER_SEC`] a
//! four-minute song is a few kilobytes, which is small enough to hand to the
//! webview whole and let it decimate for whatever zoom level it is showing.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::Serialize;

use crate::audio::decode::TrackDecoder;
use crate::audio::dsp::CHANNELS;

/// Peaks stored per second of audio. 25 is one every 40 ms: fine enough that a
/// drum hit is visible, coarse enough that a long DJ set stays a small file.
pub const PEAKS_PER_SEC: u32 = 25;
/// Bumped if the on-disk layout changes, so old caches are ignored rather than
/// misread.
const CACHE_VERSION: u8 = 1;
const MAGIC: &[u8; 4] = b"PNMW";
/// Refuse to analyse anything longer than this. Guards against a corrupt
/// header claiming an absurd duration.
const MAX_SECS: f64 = 24.0 * 60.0 * 60.0;

/// One file's waveform, as the UI receives it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Waveform {
    /// Peak magnitudes, 0–255, one per 1/`peaks_per_sec` of audio.
    pub peaks: Vec<u8>,
    pub peaks_per_sec: u32,
    /// The duration the decoder reported, which is authoritative — a file's
    /// tags can disagree with its actual content.
    pub duration_secs: f64,
}

impl Waveform {
    fn empty() -> Self {
        Waveform {
            peaks: Vec::new(),
            peaks_per_sec: PEAKS_PER_SEC,
            duration_secs: 0.0,
        }
    }
}

/// The waveform for `path`, from the cache when it is still valid and from a
/// full decode otherwise.
///
/// Slow — seconds for a long file — so this belongs on a background thread,
/// never on the one serving the webview.
pub fn waveform(path: &Path, cache_dir: &Path) -> Result<Waveform> {
    let key = cache_key(path)?;
    let cached = cache_dir.join(format!("{key}.pnmw"));
    if let Some(found) = read_cache(&cached) {
        return Ok(found);
    }

    let computed = analyse(path)?;
    // A failure to cache is not a failure to draw: the waveform is already in
    // hand, and the next call simply recomputes it.
    if let Err(e) = write_cache(&cached, &computed) {
        eprintln!("peaks: could not cache {}: {e}", cached.display());
    }
    Ok(computed)
}

/// Decode the whole file, keeping the loudest sample in each bucket.
///
/// Peaks rather than an average: an average of a bucket of a loud track and a
/// quiet one look alike, whereas peaks keep the transients that make a
/// waveform recognisable at a glance.
fn analyse(path: &Path) -> Result<Waveform> {
    // Opened twice, deliberately. `TrackDecoder` resamples to whatever rate it
    // is told, and sinc resampling an entire file is by far the most expensive
    // thing in this function — so the first open exists only to learn the
    // file's own rate, and the second asks for exactly that, which makes the
    // resampler a no-op.
    let probe_rate = TrackDecoder::open(path, 48_000)
        .with_context(|| format!("opening {} for analysis", path.display()))?
        .source_rate();
    let mut decoder = TrackDecoder::open(path, probe_rate)
        .with_context(|| format!("opening {} for analysis", path.display()))?;

    let duration_secs = decoder.info.duration_secs;
    if !(duration_secs.is_finite() && duration_secs > 0.0) || duration_secs > MAX_SECS {
        return Ok(Waveform::empty());
    }

    let frames_per_peak = (probe_rate as f64 / PEAKS_PER_SEC as f64).max(1.0) as usize;
    let expected = (duration_secs * PEAKS_PER_SEC as f64).ceil() as usize + 1;
    let mut peaks: Vec<u8> = Vec::with_capacity(expected);

    let mut buffer = vec![0.0f32; frames_per_peak * CHANNELS];
    let mut carried = 0usize;
    let mut running = 0.0f32;

    loop {
        let got = decoder.read(&mut buffer[carried..])?;
        if got == 0 {
            break;
        }
        let filled = carried + got;
        let frames = filled / CHANNELS;
        for f in 0..frames {
            let l = buffer[f * CHANNELS];
            let r = buffer[f * CHANNELS + 1];
            running = running.max(l.abs()).max(r.abs());
        }
        // A short read leaves a partial frame; carry its samples so channels
        // stay paired across the boundary.
        carried = filled - frames * CHANNELS;
        for i in 0..carried {
            buffer[i] = buffer[frames * CHANNELS + i];
        }

        if frames >= frames_per_peak || decoder.is_eof() {
            peaks.push(quantise(running));
            running = 0.0;
        }
        if peaks.len() > expected + PEAKS_PER_SEC as usize {
            break;
        }
    }
    if running > 0.0 {
        peaks.push(quantise(running));
    }

    Ok(Waveform {
        peaks,
        peaks_per_sec: PEAKS_PER_SEC,
        duration_secs,
    })
}

/// Magnitude to a byte, on a curve rather than linearly.
///
/// A linear mapping makes almost everything below the top few decibels look
/// like a flat line, because most music sits well under full scale. The square
/// root opens out the quiet half, which is the half a waveform is read for.
fn quantise(magnitude: f32) -> u8 {
    let clamped = magnitude.abs().min(1.0);
    (clamped.sqrt() * 255.0).round() as u8
}

/// A key that changes whenever the file's bytes could have.
fn cache_key(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)
        .with_context(|| format!("reading metadata for {}", path.display()))?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(crate::library::model::stable_id(
        "wf",
        &format!("{}|{}|{}", path.display(), meta.len(), modified),
    ))
}

fn read_cache(path: &Path) -> Option<Waveform> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 17 || &bytes[..4] != MAGIC || bytes[4] != CACHE_VERSION {
        return None;
    }
    let peaks_per_sec = u32::from_le_bytes(bytes[5..9].try_into().ok()?);
    let duration_secs = f64::from_le_bytes(bytes[9..17].try_into().ok()?);
    if peaks_per_sec == 0 || !duration_secs.is_finite() {
        return None;
    }
    Some(Waveform {
        peaks: bytes[17..].to_vec(),
        peaks_per_sec,
        duration_secs,
    })
}

fn write_cache(path: &Path, waveform: &Waveform) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut bytes = Vec::with_capacity(waveform.peaks.len() + 17);
    bytes.extend_from_slice(MAGIC);
    bytes.push(CACHE_VERSION);
    bytes.extend_from_slice(&waveform.peaks_per_sec.to_le_bytes());
    bytes.extend_from_slice(&waveform.duration_secs.to_le_bytes());
    bytes.extend_from_slice(&waveform.peaks);
    // Same temp-then-rename as the playlist file: a half-written cache that
    // still had the right magic would be read back as a truncated waveform.
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path).map_err(|e| anyhow!("replacing {}: {e}", path.display()))?;
    Ok(())
}

/// Where waveform caches live.
pub fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("waveforms")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantisation_opens_out_the_quiet_half() {
        assert_eq!(quantise(0.0), 0);
        assert_eq!(quantise(1.0), 255);
        // -20 dBFS is a normal-ish level and must not read as near-silence.
        assert!(quantise(0.1) > 70);
        // Out-of-range samples are clamped rather than wrapping.
        assert_eq!(quantise(4.0), 255);
        assert_eq!(quantise(-4.0), 255);
    }

    #[test]
    fn a_cache_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(crate::library::model::stable_id(
            "pnm-wf-test",
            &format!("{:?}", std::time::Instant::now()),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("one.pnmw");

        let waveform = Waveform {
            peaks: vec![0, 128, 255, 7],
            peaks_per_sec: PEAKS_PER_SEC,
            duration_secs: 12.5,
        };
        write_cache(&path, &waveform).unwrap();

        let back = read_cache(&path).unwrap();
        assert_eq!(back.peaks, waveform.peaks);
        assert_eq!(back.peaks_per_sec, PEAKS_PER_SEC);
        assert_eq!(back.duration_secs, 12.5);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_cache_from_another_version_is_ignored_rather_than_misread() {
        let dir = std::env::temp_dir().join(crate::library::model::stable_id(
            "pnm-wf-test",
            &format!("{:?}", std::time::Instant::now()),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("old.pnmw");

        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.push(CACHE_VERSION + 1);
        bytes.extend_from_slice(&PEAKS_PER_SEC.to_le_bytes());
        bytes.extend_from_slice(&1.0f64.to_le_bytes());
        bytes.push(9);
        std::fs::write(&path, bytes).unwrap();

        assert!(read_cache(&path).is_none());
        assert!(read_cache(&dir.join("missing.pnmw")).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
