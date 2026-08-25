//! Drives the real audio engine against a real output device.
//!
//! Volume is set to zero throughout, so this is silent, but every other part
//! of the path is live: cpal opens the device, the worker decodes and runs the
//! DSP chain, and the callback drains the ring.
//!
//! Skipped rather than failed when no output device is available, so the suite
//! still passes on a machine or CI runner with no sound card.

use std::f32::consts::TAU;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pick_n_mix_lib::audio::params::{Lofi, MixerSettings, Pitch, Reverb};
use pick_n_mix_lib::audio::{AudioEngine, EngineEvent};

fn write_wav(path: &Path, sample_rate: u32, seconds: f32, freq: f32) {
    let frames = (sample_rate as f32 * seconds) as u32;
    let data_len = frames * 2 * 2;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for frame in 0..frames {
        let t = frame as f32 / sample_rate as f32;
        let pcm = ((TAU * freq * t).sin() * 0.4 * i16::MAX as f32) as i16;
        out.extend_from_slice(&pcm.to_le_bytes());
        out.extend_from_slice(&pcm.to_le_bytes());
    }
    let mut file = std::fs::File::create(path).expect("creating wav");
    file.write_all(&out).expect("writing wav");
}

fn fixture(name: &str, seconds: f32) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pnm-playback-{name}"));
    std::fs::create_dir_all(&dir).expect("creating dir");
    let path = dir.join("tone.wav");
    write_wav(&path, 44100, seconds, 330.0);
    path
}

/// Returns None when the machine has no usable output device.
fn engine() -> Option<(AudioEngine, crossbeam_channel::Receiver<EngineEvent>)> {
    let (tx, rx) = crossbeam_channel::unbounded();
    match AudioEngine::new(tx) {
        Ok(engine) => {
            // Everything here runs silently.
            engine.set_volume(0.0);
            Some((engine, rx))
        }
        Err(e) => {
            eprintln!("skipping: no audio output device available ({e})");
            None
        }
    }
}

/// Poll until `check` passes or the deadline expires.
fn wait_for(timeout: Duration, mut check: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

#[test]
fn playing_a_track_advances_the_position() {
    let Some((engine, _rx)) = engine() else { return };
    let path = fixture("advance", 5.0);

    let info = engine.load(path, 0.0, 0.0).expect("loading the track");
    assert!((info.duration_secs - 5.0).abs() < 0.2);

    engine.play();
    assert!(
        wait_for(Duration::from_secs(3), || engine.snapshot().position_secs > 0.3),
        "position never advanced; last was {}",
        engine.snapshot().position_secs
    );
    assert!(engine.is_playing());
}

#[test]
fn pausing_holds_the_position_still() {
    let Some((engine, _rx)) = engine() else { return };
    engine.load(fixture("pause", 6.0), 0.0, 0.0).expect("loading");
    engine.play();

    assert!(
        wait_for(Duration::from_secs(3), || engine.snapshot().position_secs > 0.3),
        "playback never started"
    );
    engine.pause();

    // Let anything already in flight settle before sampling.
    std::thread::sleep(Duration::from_millis(300));
    let at_pause = engine.snapshot().position_secs;
    std::thread::sleep(Duration::from_millis(600));
    let later = engine.snapshot().position_secs;

    assert!(
        (later - at_pause).abs() < 0.1,
        "position moved while paused: {at_pause} then {later}"
    );
    assert!(!engine.is_playing());
}

#[test]
fn seeking_jumps_the_position() {
    let Some((engine, _rx)) = engine() else { return };
    engine.load(fixture("seek", 10.0), 0.0, 0.0).expect("loading");
    engine.play();
    assert!(wait_for(Duration::from_secs(3), || engine.snapshot().position_secs > 0.2));

    engine.seek(6.0);
    assert!(
        wait_for(Duration::from_secs(3), || {
            let p = engine.snapshot().position_secs;
            (5.5..7.5).contains(&p)
        }),
        "seek did not land; position is {}",
        engine.snapshot().position_secs
    );
}

#[test]
fn reaching_the_end_reports_the_track_as_finished() {
    let Some((engine, rx)) = engine() else { return };
    // Short enough that the test does not drag.
    engine.load(fixture("finish", 1.0), 0.0, 0.0).expect("loading");
    engine.play();

    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut finished = false;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(EngineEvent::TrackFinished) => {
                finished = true;
                break;
            }
            Ok(EngineEvent::Error { message }) => seen.push(message),
            Err(_) => {}
        }
    }
    assert!(finished, "never reported as finished; errors seen: {seen:?}");
    // Running off the end of a file is not an error, and must not be
    // reported as one: it would surface as a toast after every track.
    assert!(seen.is_empty(), "clean end of file raised errors: {seen:?}");
}

#[test]
fn pitch_changes_the_reported_playback_speed() {
    let Some((engine, _rx)) = engine() else { return };
    engine.load(fixture("pitch", 8.0), 0.0, 0.0).expect("loading");

    // An octave up is exactly double speed under varispeed.
    let settings = MixerSettings {
        enabled: Some(true),
        pitch: Some(Pitch { semitones: 12.0, cents: 0.0 }),
        ..Default::default()
    };
    engine.set_settings(MixerSettings::resolve(&[&settings]));
    engine.play();

    assert!(
        wait_for(Duration::from_secs(3), || {
            (engine.snapshot().speed - 2.0).abs() < 0.01
        }),
        "speed was {}",
        engine.snapshot().speed
    );

    // And the track really is consumed twice as fast.
    let start = engine.snapshot().position_secs;
    std::thread::sleep(Duration::from_millis(700));
    let travelled = engine.snapshot().position_secs - start;
    assert!(
        travelled > 0.9,
        "at 2x, 0.7s of wall clock should cover well over 0.9s of track; covered {travelled}"
    );
}

#[test]
fn the_effect_chain_runs_without_starving_playback() {
    let Some((engine, _rx)) = engine() else { return };
    engine.load(fixture("effects", 6.0), 0.0, 0.0).expect("loading");

    // Everything on at once: the heaviest the chain gets.
    let settings = MixerSettings {
        enabled: Some(true),
        reverb: Some(Reverb { enabled: true, size: 0.9, mix: 0.5, ..Default::default() }),
        delay: Some(pick_n_mix_lib::audio::params::Delay {
            enabled: true,
            ..Default::default()
        }),
        lofi: Some(Lofi { enabled: true, sample_rate_hz: 8000.0, bit_depth: 8.0, mix: 1.0 }),
        pitch: Some(Pitch { semitones: -3.0, cents: 0.0 }),
        ..Default::default()
    };
    engine.set_settings(MixerSettings::resolve(&[&settings]));
    engine.play();

    assert!(
        wait_for(Duration::from_secs(4), || engine.snapshot().position_secs > 0.5),
        "playback stalled with the full effect chain engaged"
    );
}
