//! Optional ffmpeg support, used for encoding formats this app will not ship
//! an encoder for.
//!
//! WAV and FLAC are written in-process — the first is trivial and the second
//! has a pure-Rust encoder — but MP3 does not have an encoder whose licence
//! sits comfortably inside a statically linked binary. Rather than take that
//! on, the app looks for an ffmpeg the user already has and treats MP3 as a
//! capability that either is or is not present on this machine.
//!
//! Detection is therefore two questions, not one: is there an ffmpeg, and was
//! *that* ffmpeg built with the encoder being asked for. A distribution build
//! without `libmp3lame` is common enough that assuming otherwise would turn a
//! clear "MP3 is unavailable" into a failed bounce.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use serde::Serialize;

/// Places to look beyond `PATH`.
///
/// A bundled app launched from Finder or a desktop launcher inherits a minimal
/// environment that frequently omits exactly the directories package managers
/// install into, so `PATH` alone would report ffmpeg missing on a machine that
/// plainly has it.
const EXTRA_DIRS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/opt/local/bin",
    "/snap/bin",
    "/var/lib/flatpak/exports/bin",
];

/// What this machine's ffmpeg, if any, can do for us.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegStatus {
    /// Absolute path to the binary, or empty when none was found.
    pub path: String,
    pub available: bool,
    /// Whether the MP3 encoder is compiled into it.
    pub mp3: bool,
    /// First line of `ffmpeg -version`, for the settings page to show.
    pub version: String,
}

/// Probing spawns a process, so the answer is worked out once and reused. The
/// user installing ffmpeg while the app is running is handled by [`refresh`],
/// which the frontend calls when it wants to look again.
static CACHED: Mutex<Option<FfmpegStatus>> = Mutex::new(None);

pub fn status() -> FfmpegStatus {
    if let Some(cached) = CACHED.lock().clone() {
        return cached;
    }
    let found = probe();
    *CACHED.lock() = Some(found.clone());
    found
}

/// Look again, forgetting what was found last time.
pub fn refresh() -> FfmpegStatus {
    CACHED.lock().take();
    status()
}

fn probe() -> FfmpegStatus {
    let Some(path) = locate() else {
        return FfmpegStatus::default();
    };
    let version = run(&path, &["-hide_banner", "-version"])
        .unwrap_or_default()
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    // `-encoders` lists what this build can write. Asking is cheaper than
    // discovering the answer part-way through a bounce.
    let mp3 = run(&path, &["-hide_banner", "-encoders"])
        .map(|out| out.contains("libmp3lame"))
        .unwrap_or(false);
    FfmpegStatus {
        path: path.to_string_lossy().into_owned(),
        available: true,
        mp3,
        version,
    }
}

fn locate() -> Option<PathBuf> {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let from_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>());
    for dir in from_path.chain(EXTRA_DIRS.iter().map(PathBuf::from)) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn run(path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("running {}", path.display()))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Encode interleaved stereo `f32` frames, pulled from `next`, into `dest`.
///
/// `next` fills the slice it is given and returns how many frames it wrote,
/// with 0 meaning the end; it is called on this thread, so the caller decides
/// how much work happens between writes.
///
/// The audio goes down a pipe rather than through a temporary WAV: a long mix
/// would otherwise be written to disk twice, and a 32-bit float pipe keeps the
/// encoder's input identical to what the audition heard.
pub fn encode_mp3(
    dest: &Path,
    rate: u32,
    bitrate: u16,
    channels: usize,
    mut next: impl FnMut(&mut [f32]) -> Result<usize>,
) -> Result<()> {
    let status = status();
    if !status.available {
        return Err(anyhow!(
            "MP3 needs ffmpeg, which was not found on this system. Install ffmpeg, or bounce to WAV or FLAC."
        ));
    }
    if !status.mp3 {
        return Err(anyhow!(
            "this system's ffmpeg was built without the MP3 encoder (libmp3lame). Bounce to WAV or FLAC instead."
        ));
    }

    let rate_arg = rate.to_string();
    let channels_arg = channels.to_string();
    let bitrate_arg = format!("{bitrate}k");
    let mut child = Command::new(&status.path)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "f32le",
            "-ar",
            &rate_arg,
            "-ac",
            &channels_arg,
            "-i",
            "pipe:0",
            "-c:a",
            "libmp3lame",
            "-b:a",
            &bitrate_arg,
        ])
        .arg(dest)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("starting {}", status.path))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("could not write to ffmpeg"))?;
    const FRAMES: usize = 1024;
    let mut buffer = vec![0.0f32; FRAMES * channels];
    let mut bytes = Vec::with_capacity(buffer.len() * 4);

    let write_result = (|| -> Result<()> {
        loop {
            let frames = next(&mut buffer)?;
            if frames == 0 {
                return Ok(());
            }
            bytes.clear();
            for sample in &buffer[..frames * channels] {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            // A broken pipe means ffmpeg gave up; its stderr says why, so the
            // error it reports is more useful than this one.
            if stdin.write_all(&bytes).is_err() {
                return Ok(());
            }
        }
    })();
    drop(stdin);

    let output = child.wait_with_output().context("waiting for ffmpeg")?;
    write_result?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        let detail = message.lines().last().unwrap_or("").trim();
        return Err(anyhow!(
            "ffmpeg could not write that MP3{}{}",
            if detail.is_empty() { "" } else { ": " },
            detail
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_binary_reports_no_capabilities() {
        let status = FfmpegStatus::default();
        assert!(!status.available);
        assert!(!status.mp3);
    }

    /// End to end, but only where there is an ffmpeg to run: on a machine
    /// without one the interesting assertion is the error, not the file.
    #[test]
    fn encodes_a_short_tone_when_this_machine_can() {
        let dest = std::env::temp_dir().join(format!("pnm-ffmpeg-test-{}.mp3", std::process::id()));
        let _ = std::fs::remove_file(&dest);
        let mut frames_left = 44_100usize;
        let mut phase = 0.0f32;
        let result = encode_mp3(&dest, 44_100, 128, 2, |out| {
            let frames = (out.len() / 2).min(frames_left);
            for f in 0..frames {
                phase += 440.0 * std::f32::consts::TAU / 44_100.0;
                let sample = phase.sin() * 0.25;
                out[f * 2] = sample;
                out[f * 2 + 1] = sample;
            }
            frames_left -= frames;
            Ok(frames)
        });

        let capable = status().available && status().mp3;
        if capable {
            result.expect("encoding should succeed where ffmpeg has libmp3lame");
            let written = std::fs::metadata(&dest).expect("an MP3 should exist").len();
            assert!(written > 1_000, "a second of audio should not be {written} bytes");
            let _ = std::fs::remove_file(&dest);
        } else {
            let message = result.expect_err("without ffmpeg this cannot succeed").to_string();
            assert!(
                message.contains("ffmpeg"),
                "the error should say what is missing, not {message}"
            );
        }
    }

    /// Whatever this machine has, asking twice must not disagree with itself.
    #[test]
    fn status_is_stable_across_calls() {
        let first = status();
        let second = status();
        assert_eq!(first.available, second.available);
        assert_eq!(first.mp3, second.mp3);
        assert_eq!(first.path, second.path);
    }
}
