//! End-to-end check of the decode path against a real file.
//!
//! A WAV is generated on the fly rather than committed, so the test needs no
//! fixtures and still exercises Symphonia, the channel mapping and the
//! varispeed resampler for real.

use std::f32::consts::TAU;
use std::io::Write;
use std::path::PathBuf;

use pick_n_mix_lib::audio::decode::TrackDecoder;

/// Write a 16-bit PCM WAV containing a sine wave.
fn write_wav(path: &PathBuf, sample_rate: u32, channels: u16, seconds: f32, freq: f32) {
    let frames = (sample_rate as f32 * seconds) as u32;
    let data_len = frames * channels as u32 * 2;

    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * channels as u32 * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&(channels * 2).to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());

    for frame in 0..frames {
        let t = frame as f32 / sample_rate as f32;
        let sample = (TAU * freq * t).sin() * 0.5;
        let pcm = (sample * i16::MAX as f32) as i16;
        for _ in 0..channels {
            out.extend_from_slice(&pcm.to_le_bytes());
        }
    }

    let mut file = std::fs::File::create(path).expect("creating the test wav");
    file.write_all(&out).expect("writing the test wav");
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pnm-decode-test-{name}"));
    std::fs::create_dir_all(&dir).expect("creating the temp dir");
    dir
}

fn read_all(dec: &mut TrackDecoder) -> Vec<f32> {
    let mut out = Vec::new();
    let mut chunk = vec![0.0f32; 4096];
    loop {
        let n = dec.read(&mut chunk).expect("reading decoded audio");
        if n == 0 {
            break;
        }
        out.extend_from_slice(&chunk[..n]);
    }
    out
}

#[test]
fn decodes_a_wav_and_reports_its_properties() {
    let dir = temp_dir("props");
    let path = dir.join("tone.wav");
    write_wav(&path, 44100, 2, 1.0, 440.0);

    let dec = TrackDecoder::open(&path, 44100).expect("opening the wav");
    assert_eq!(dec.info.sample_rate, 44100);
    assert_eq!(dec.info.channels, 2);
    assert!(
        (dec.info.duration_secs - 1.0).abs() < 0.05,
        "duration was {}",
        dec.info.duration_secs
    );
}

#[test]
fn output_length_matches_the_target_rate() {
    let dir = temp_dir("resample");
    let path = dir.join("tone.wav");
    write_wav(&path, 44100, 2, 1.0, 440.0);

    // 44.1 kHz source into a 48 kHz device.
    let mut dec = TrackDecoder::open(&path, 48000).expect("opening the wav");
    let samples = read_all(&mut dec);
    let frames = samples.len() / 2;

    // One second in is one second out, give or take the resampler's own
    // filter delay. A loose bound here would hide the zero-padded tail that
    // the output budget exists to trim.
    assert!(
        (frames as i64 - 48000).abs() < 200,
        "expected roughly 48000 frames, got {frames}"
    );
    assert!(
        samples.iter().any(|s| s.abs() > 0.1),
        "output should not be silent"
    );
}

/// The same-rate path has no resampler, and used to drop whatever was left in
/// the staging buffer when the source ran out. That clipped the tail of every
/// track and stopped the engine ever reporting the track as finished.
#[test]
fn no_audio_is_lost_when_the_rates_already_match() {
    let dir = temp_dir("same-rate");
    let path = dir.join("tone.wav");
    write_wav(&path, 44100, 2, 1.0, 440.0);

    // Target rate equals the file's rate, so nothing is resampled.
    let mut dec = TrackDecoder::open(&path, 44100).expect("opening the wav");
    let samples = read_all(&mut dec);
    let frames = samples.len() / 2;

    assert_eq!(
        frames, 44100,
        "every frame of a same-rate file should come out"
    );
    assert!(
        dec.is_eof(),
        "the decoder should report end of file once drained"
    );
}

#[test]
fn varispeed_makes_the_track_shorter() {
    let dir = temp_dir("varispeed");
    let path = dir.join("tone.wav");
    write_wav(&path, 44100, 2, 1.0, 440.0);

    let mut normal = TrackDecoder::open(&path, 44100).expect("opening the wav");
    let baseline = read_all(&mut normal).len();

    let mut fast = TrackDecoder::open(&path, 44100).expect("opening the wav");
    // One octave up: twice the speed, so half the output.
    fast.set_speed(2.0).expect("setting the speed");
    let sped_up = read_all(&mut fast).len();

    let ratio = baseline as f64 / sped_up as f64;
    assert!(
        (ratio - 2.0).abs() < 0.15,
        "playing at 2x should halve the output; ratio was {ratio}"
    );
}

#[test]
fn mono_sources_are_widened_to_stereo() {
    let dir = temp_dir("mono");
    let path = dir.join("mono.wav");
    write_wav(&path, 44100, 1, 0.5, 440.0);

    let mut dec = TrackDecoder::open(&path, 44100).expect("opening the wav");
    assert_eq!(dec.info.channels, 1, "the file itself is mono");

    let samples = read_all(&mut dec);
    assert_eq!(samples.len() % 2, 0, "output is interleaved stereo");
    // Both channels carry the same signal.
    let mismatches = samples
        .chunks_exact(2)
        .filter(|frame| (frame[0] - frame[1]).abs() > 1e-6)
        .count();
    assert_eq!(
        mismatches, 0,
        "left and right should be identical for a mono source"
    );
}

#[test]
fn seeking_moves_the_reported_position() {
    let dir = temp_dir("seek");
    let path = dir.join("tone.wav");
    write_wav(&path, 44100, 2, 3.0, 440.0);

    let mut dec = TrackDecoder::open(&path, 44100).expect("opening the wav");
    dec.seek(2.0).expect("seeking");
    assert!(
        (dec.decoded_secs() - 2.0).abs() < 0.2,
        "position after seek was {}",
        dec.decoded_secs()
    );

    // There should still be roughly one second left to read.
    let remaining = read_all(&mut dec).len() / 2;
    assert!(
        (remaining as i64 - 44100).abs() < 6000,
        "expected about one second left, got {remaining} frames"
    );
}

#[test]
fn a_file_that_is_not_audio_fails_cleanly() {
    let dir = temp_dir("garbage");
    let path = dir.join("not-audio.wav");
    std::fs::write(&path, b"this is definitely not a wave file").expect("writing the file");

    assert!(
        TrackDecoder::open(&path, 48000).is_err(),
        "opening a non-audio file should fail rather than panic"
    );
}
