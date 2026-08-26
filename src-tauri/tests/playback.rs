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
            // Crossfading is off by default (`length_secs: 0`) for every
            // test in this file, so neither should ever fire here.
            Ok(EngineEvent::NeedNext { .. }) => seen.push("unexpected NeedNext".into()),
            Ok(EngineEvent::TrackAdvanced { .. }) => seen.push("unexpected TrackAdvanced".into()),
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

// ---------------------------------------------------------------------------
// Crossfading
// ---------------------------------------------------------------------------

mod crossfade_tests {
    use super::*;
    use pick_n_mix_lib::audio::crossfade::CrossfadeSettings;
    use pick_n_mix_lib::audio::decode::TrackDecoder;
    use pick_n_mix_lib::audio::params::Resolved;

    /// Drain events for `window`, returning everything that arrived.
    fn collect_events(
        rx: &crossbeam_channel::Receiver<EngineEvent>,
        window: Duration,
    ) -> Vec<EngineEvent> {
        let deadline = Instant::now() + window;
        let mut events = Vec::new();
        while Instant::now() < deadline {
            if let Ok(event) = rx.recv_timeout(Duration::from_millis(100)) {
                events.push(event);
            }
        }
        events
    }

    fn wait_for_need_next(
        rx: &crossbeam_channel::Receiver<EngineEvent>,
        timeout: Duration,
    ) -> Option<u64> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(EngineEvent::NeedNext { token }) => return Some(token),
                Ok(_) | Err(_) => continue,
            }
        }
        None
    }

    /// The full handshake, start to finish: enabling a crossfade makes the
    /// engine ask for the next track ahead of time, accept a prepared voice,
    /// and hand control over to it — all without ever reporting the outgoing
    /// track as merely "finished", which would mean the transition skipped a
    /// track from the queue's point of view.
    #[test]
    fn a_prepared_voice_is_promoted_instead_of_the_track_just_ending() {
        let Some((engine, rx)) = engine() else { return };

        // A 6 s track with a short crossfade: the trigger (lead + 2 s
        // headroom) fires with 3.5 s of it still to play, giving the test
        // ample time to answer before the boundary arrives.
        let info = engine.load(fixture("cf-a", 6.0), 0.0, 0.0).expect("loading the first track");
        engine.set_crossfade(CrossfadeSettings { length_secs: 1.5, ..Default::default() });
        engine.play();

        let token = wait_for_need_next(&rx, Duration::from_secs(6))
            .expect("the engine never asked to prepare a crossfade");

        let next_path = fixture("cf-b", 6.0);
        let decoder =
            TrackDecoder::open(&next_path, engine.device_sample_rate()).expect("opening voice B");
        engine.prepare_next(
            decoder,
            Resolved::default(),
            0.0,
            token,
            7, // an arbitrary queue position, just echoed back
            "track-b".into(),
        );

        // Collect everything from here to well past the boundary. `1.5 s`
        // crossfade length plus slack for scheduling jitter. Track B is also
        // 6 s long and crossfading is still enabled, so once B itself gets
        // close to its own end the whole cycle repeats: B's own `NeedNext`
        // fires, goes unanswered by this test, and B eventually reaches an
        // entirely ordinary `TrackFinished` of its own. That is correct
        // behaviour, not a second copy of the bug being tested for — so only
        // the events up to and including the promotion are examined below.
        let events = collect_events(&rx, Duration::from_secs(6));

        let promotion_at = events
            .iter()
            .position(|e| matches!(e, EngineEvent::TrackAdvanced { .. }))
            .expect(&format!("expected a TrackAdvanced somewhere in {events:?}"));
        let before_promotion = &events[..promotion_at];

        assert!(
            !before_promotion.iter().any(|e| matches!(e, EngineEvent::TrackFinished)),
            "the outgoing track must not be reported finished before the prepared \
             voice is promoted, or the queue would skip a track: {events:?}"
        );
        let advanced = match &events[promotion_at] {
            EngineEvent::TrackAdvanced { order_index, track_id } => (*order_index, track_id.clone()),
            _ => unreachable!("checked by `position` above"),
        };
        assert_eq!(
            advanced,
            (7, "track-b".to_string()),
            "expected a TrackAdvanced echoing back what was prepared; got {events:?}"
        );

        // Audio kept flowing through the handover: the engine's own duration
        // now describes track B, not the (shorter, by design) remainder of A.
        assert!(
            wait_for(Duration::from_secs(3), || {
                (engine.snapshot().duration_secs - info.duration_secs).abs() < 0.3
            }),
            "the engine should be reporting track B's duration after promotion"
        );
    }

    /// `CancelNext` must leave the engine able to fall back to the ordinary,
    /// un-crossfaded ending — it must not get stuck waiting for a reply that
    /// will never come.
    #[test]
    fn cancelling_a_pending_next_falls_back_to_a_normal_finish() {
        let Some((engine, rx)) = engine() else { return };

        engine.load(fixture("cf-cancel", 2.0), 0.0, 0.0).expect("loading");
        engine.set_crossfade(CrossfadeSettings { length_secs: 1.5, ..Default::default() });
        engine.play();

        wait_for_need_next(&rx, Duration::from_secs(4))
            .expect("the engine never asked to prepare a crossfade");
        engine.cancel_next();

        let finished = (0..50).any(|_| match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(EngineEvent::TrackFinished) => true,
            _ => false,
        });
        assert!(finished, "cancelling a pending crossfade should not prevent a normal finish");
    }

    /// A stale reply — answering a request that has already been superseded
    /// or cancelled — must be ignored rather than corrupting playback.
    #[test]
    fn a_reply_with_the_wrong_token_is_ignored() {
        let Some((engine, rx)) = engine() else { return };

        engine.load(fixture("cf-stale", 2.0), 0.0, 0.0).expect("loading");
        engine.set_crossfade(CrossfadeSettings { length_secs: 1.5, ..Default::default() });
        engine.play();

        let real_token = wait_for_need_next(&rx, Duration::from_secs(4))
            .expect("the engine never asked to prepare a crossfade");

        let path = fixture("cf-stale-b", 2.0);
        let decoder =
            TrackDecoder::open(&path, engine.device_sample_rate()).expect("opening a decoder");
        // Answer with a token that cannot possibly be the one just issued.
        engine.prepare_next(decoder, Resolved::default(), 0.0, real_token.wrapping_add(1), 0, "x".into());

        // Playback must still reach an ordinary, un-promoted finish: the
        // bogus reply must not have been accepted as the pending voice.
        let finished = (0..50).any(|_| match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(EngineEvent::TrackFinished) => true,
            Ok(EngineEvent::TrackAdvanced { .. }) => {
                panic!("a stale-token reply must never be promoted")
            }
            _ => false,
        });
        assert!(finished, "a stale reply should leave the track free to finish normally");
    }

    /// With crossfading off (the default), behaviour is byte-for-byte the
    /// same as before this feature existed: no `NeedNext`, straight to
    /// `TrackFinished`. This is the regression the whole design leans on.
    #[test]
    fn crossfading_off_never_asks_for_a_next_track() {
        let Some((engine, rx)) = engine() else { return };
        engine.load(fixture("cf-off", 1.0), 0.0, 0.0).expect("loading");
        // Default settings: length_secs == 0, i.e. disabled.
        engine.play();

        let events = collect_events(&rx, Duration::from_secs(4));
        assert!(
            !events.iter().any(|e| matches!(e, EngineEvent::NeedNext { .. })),
            "crossfading must stay off unless explicitly enabled: {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(e, EngineEvent::TrackFinished)),
            "should still reach an ordinary finish: {events:?}"
        );
    }
}
