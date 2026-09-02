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
    let Some((engine, _rx)) = engine() else {
        return;
    };
    let path = fixture("advance", 5.0);

    let info = engine.load(path, 0.0, 0.0).expect("loading the track");
    assert!((info.duration_secs - 5.0).abs() < 0.2);

    engine.play();
    assert!(
        wait_for(Duration::from_secs(3), || engine.snapshot().position_secs
            > 0.3),
        "position never advanced; last was {}",
        engine.snapshot().position_secs
    );
    assert!(engine.is_playing());
}

#[test]
fn pausing_holds_the_position_still() {
    let Some((engine, _rx)) = engine() else {
        return;
    };
    engine
        .load(fixture("pause", 6.0), 0.0, 0.0)
        .expect("loading");
    engine.play();

    assert!(
        wait_for(Duration::from_secs(3), || engine.snapshot().position_secs
            > 0.3),
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
    let Some((engine, _rx)) = engine() else {
        return;
    };
    engine
        .load(fixture("seek", 10.0), 0.0, 0.0)
        .expect("loading");
    engine.play();
    assert!(wait_for(Duration::from_secs(3), || engine
        .snapshot()
        .position_secs
        > 0.2));

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
    engine
        .load(fixture("finish", 1.0), 0.0, 0.0)
        .expect("loading");
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
    assert!(
        finished,
        "never reported as finished; errors seen: {seen:?}"
    );
    // Running off the end of a file is not an error, and must not be
    // reported as one: it would surface as a toast after every track.
    assert!(seen.is_empty(), "clean end of file raised errors: {seen:?}");
}

#[test]
fn pitch_changes_the_reported_playback_speed() {
    let Some((engine, _rx)) = engine() else {
        return;
    };
    engine
        .load(fixture("pitch", 8.0), 0.0, 0.0)
        .expect("loading");

    // An octave up is exactly double speed under varispeed.
    let settings = MixerSettings {
        enabled: Some(true),
        pitch: Some(Pitch {
            semitones: 12.0,
            cents: 0.0,
        }),
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
    let Some((engine, _rx)) = engine() else {
        return;
    };
    engine
        .load(fixture("effects", 6.0), 0.0, 0.0)
        .expect("loading");

    // Everything on at once: the heaviest the chain gets.
    let settings = MixerSettings {
        enabled: Some(true),
        reverb: Some(Reverb {
            enabled: true,
            size: 0.9,
            mix: 0.5,
            ..Default::default()
        }),
        delay: Some(pick_n_mix_lib::audio::params::Delay {
            enabled: true,
            ..Default::default()
        }),
        lofi: Some(Lofi {
            enabled: true,
            sample_rate_hz: 8000.0,
            bit_depth: 8.0,
            mix: 1.0,
        }),
        pitch: Some(Pitch {
            semitones: -3.0,
            cents: 0.0,
        }),
        ..Default::default()
    };
    engine.set_settings(MixerSettings::resolve(&[&settings]));
    engine.play();

    assert!(
        wait_for(Duration::from_secs(4), || engine.snapshot().position_secs
            > 0.5),
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
        let info = engine
            .load(fixture("cf-a", 6.0), 0.0, 0.0)
            .expect("loading the first track");
        engine.set_crossfade(CrossfadeSettings::new(1.5));
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
            !before_promotion
                .iter()
                .any(|e| matches!(e, EngineEvent::TrackFinished)),
            "the outgoing track must not be reported finished before the prepared \
             voice is promoted, or the queue would skip a track: {events:?}"
        );
        let advanced = match &events[promotion_at] {
            EngineEvent::TrackAdvanced {
                order_index,
                track_id,
            } => (*order_index, track_id.clone()),
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

    /// Declining a request must stop the engine asking again. Answering
    /// "there is nothing to play next" by merely clearing the pending token
    /// let the trigger re-fire on the very next block, spraying hundreds of
    /// events a second for the rest of the track.
    #[test]
    fn declining_stops_the_engine_asking_again() {
        let Some((engine, rx)) = engine() else { return };

        engine
            .load(fixture("cf-decline", 8.0), 0.0, 0.0)
            .expect("loading");
        engine.set_crossfade(CrossfadeSettings::new(2.0));
        engine.play();

        let token = wait_for_need_next(&rx, Duration::from_secs(8))
            .expect("the engine never asked to prepare a crossfade");
        engine.decline_next(token);

        // Well over a hundred worker blocks pass in this window.
        let events = collect_events(&rx, Duration::from_secs(2));
        let asks = events
            .iter()
            .filter(|e| matches!(e, EngineEvent::NeedNext { .. }))
            .count();
        assert_eq!(
            asks, 0,
            "a declined request should not be repeated; got {asks} more in two seconds"
        );
    }

    /// The next voice is prepared early on purpose, so that a slow decoder
    /// open cannot stall the transition. But it must not start *playing*
    /// early: reading from it while its gain is still zero would advance its
    /// decoder silently, and the incoming track would begin partway in with
    /// its opening thrown away.
    #[test]
    fn the_incoming_track_does_not_lose_its_opening() {
        let Some((engine, rx)) = engine() else { return };

        const LENGTH: f32 = 1.0;
        engine
            .load(fixture("cf-opening-a", 6.0), 0.0, 0.0)
            .expect("loading the first track");
        engine.set_crossfade(CrossfadeSettings::new(LENGTH));
        engine.play();

        let token = wait_for_need_next(&rx, Duration::from_secs(8))
            .expect("the engine never asked to prepare a crossfade");
        let decoder =
            TrackDecoder::open(&fixture("cf-opening-b", 6.0), engine.device_sample_rate())
                .expect("opening voice B");
        engine.prepare_next(decoder, Resolved::default(), 0.0, token, 0, "b".into());

        // Wait for the handover, then look at how far into the incoming track
        // playback actually is.
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut promoted = false;
        while Instant::now() < deadline {
            if let Ok(EngineEvent::TrackAdvanced { .. }) =
                rx.recv_timeout(Duration::from_millis(100))
            {
                promoted = true;
                break;
            }
        }
        assert!(promoted, "the prepared voice was never promoted");

        // Overlapping by LENGTH means the incoming track is legitimately
        // LENGTH seconds in at the handover. The trigger fires a further
        // TRIGGER_HEADROOM (2 s) earlier than that, so a voice being read too
        // soon would land at roughly LENGTH + 2 instead.
        let position = engine.snapshot().position_secs;
        assert!(
            position < (LENGTH as f64) + 1.0,
            "the incoming track was already {position:.2}s in at the handover, so it \
             was being read before its fade began and lost its opening"
        );
    }

    /// `CancelNext` must leave the engine able to fall back to the ordinary,
    /// un-crossfaded ending — it must not get stuck waiting for a reply that
    /// will never come.
    #[test]
    fn cancelling_a_pending_next_falls_back_to_a_normal_finish() {
        let Some((engine, rx)) = engine() else { return };

        engine
            .load(fixture("cf-cancel", 2.0), 0.0, 0.0)
            .expect("loading");
        engine.set_crossfade(CrossfadeSettings::new(1.5));
        engine.play();

        wait_for_need_next(&rx, Duration::from_secs(4))
            .expect("the engine never asked to prepare a crossfade");
        engine.cancel_next();

        let finished = (0..50).any(|_| match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(EngineEvent::TrackFinished) => true,
            _ => false,
        });
        assert!(
            finished,
            "cancelling a pending crossfade should not prevent a normal finish"
        );
    }

    /// A stale reply — answering a request that has already been superseded
    /// or cancelled — must be ignored rather than corrupting playback.
    #[test]
    fn a_reply_with_the_wrong_token_is_ignored() {
        let Some((engine, rx)) = engine() else { return };

        engine
            .load(fixture("cf-stale", 2.0), 0.0, 0.0)
            .expect("loading");
        engine.set_crossfade(CrossfadeSettings::new(1.5));
        engine.play();

        let real_token = wait_for_need_next(&rx, Duration::from_secs(4))
            .expect("the engine never asked to prepare a crossfade");

        let path = fixture("cf-stale-b", 2.0);
        let decoder =
            TrackDecoder::open(&path, engine.device_sample_rate()).expect("opening a decoder");
        // Answer with a token that cannot possibly be the one just issued.
        engine.prepare_next(
            decoder,
            Resolved::default(),
            0.0,
            real_token.wrapping_add(1),
            0,
            "x".into(),
        );

        // Playback must still reach an ordinary, un-promoted finish: the
        // bogus reply must not have been accepted as the pending voice.
        let finished = (0..50).any(|_| match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(EngineEvent::TrackFinished) => true,
            Ok(EngineEvent::TrackAdvanced { .. }) => {
                panic!("a stale-token reply must never be promoted")
            }
            _ => false,
        });
        assert!(
            finished,
            "a stale reply should leave the track free to finish normally"
        );
    }

    /// With crossfading off (the default), behaviour is byte-for-byte the
    /// same as before this feature existed: no `NeedNext`, straight to
    /// `TrackFinished`. This is the regression the whole design leans on.
    #[test]
    fn crossfading_off_never_asks_for_a_next_track() {
        let Some((engine, rx)) = engine() else { return };
        engine
            .load(fixture("cf-off", 1.0), 0.0, 0.0)
            .expect("loading");
        // Default settings: length_secs == 0, i.e. disabled.
        engine.play();

        let events = collect_events(&rx, Duration::from_secs(4));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, EngineEvent::NeedNext { .. })),
            "crossfading must stay off unless explicitly enabled: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, EngineEvent::TrackFinished)),
            "should still reach an ordinary finish: {events:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Mixer settings reaching the audio
// ---------------------------------------------------------------------------

/// The DJ Mixer's controls all funnel through `Chain`, which only picks up new
/// values when `Chain::update` is called. A refactor once dropped that call
/// for the playing voice, and the result was subtle in exactly the worst way:
/// pitch kept working (it goes through the decoder, not the chain) and so did
/// crossfading (it reads its curve directly), while EQ, reverb, delay, lo-fi
/// and the normalisation gain all silently did nothing.
///
/// Nothing here inspects DSP internals — these drive the real engine and
/// observe the master limiter's gain-reduction meter, which only moves if a
/// setting genuinely reached the audio.
mod mixer_reaches_audio {
    use super::*;
    use pick_n_mix_lib::audio::params::{Eq, Normalisation};

    fn boosted(settings: MixerSettings) -> pick_n_mix_lib::audio::params::Resolved {
        MixerSettings::resolve(&[&settings])
    }

    /// Waits for the limiter to report it is pulling the signal down, which
    /// can only happen if something upstream made the signal louder.
    fn limiter_engages(engine: &AudioEngine) -> bool {
        wait_for(Duration::from_secs(4), || {
            engine.snapshot().limiter_reduction_db > 1.0
        })
    }

    #[test]
    fn normalisation_gain_reaches_the_audio() {
        let Some((engine, _rx)) = engine() else {
            return;
        };
        engine
            .load(fixture("mixer-gain", 8.0), 0.0, 0.0)
            .expect("loading");

        // +24 dB against a 0.4-amplitude tone is far past the ceiling, so the
        // limiter must respond. Left at unity, it has nothing to do.
        engine.set_settings(boosted(MixerSettings {
            enabled: Some(true),
            normalisation: Some(Normalisation {
                enabled: true,
                gain_db: 24.0,
                limiter_enabled: true,
                ..Default::default()
            }),
            ..Default::default()
        }));
        engine.play();

        assert!(
            limiter_engages(&engine),
            "a +24 dB normalisation gain never reached the audio; reduction stayed at {}",
            engine.snapshot().limiter_reduction_db
        );
    }

    #[test]
    fn eq_gain_reaches_the_audio() {
        let Some((engine, _rx)) = engine() else {
            return;
        };
        engine
            .load(fixture("mixer-eq", 8.0), 0.0, 0.0)
            .expect("loading");

        let mut eq = Eq::default();
        for band in eq.bands.iter_mut() {
            band.gain_db = 12.0;
        }
        engine.set_settings(boosted(MixerSettings {
            enabled: Some(true),
            eq: Some(eq),
            normalisation: Some(Normalisation {
                limiter_enabled: true,
                ..Default::default()
            }),
            ..Default::default()
        }));
        engine.play();

        assert!(
            limiter_engages(&engine),
            "a +12 dB boost on every EQ band never reached the audio; reduction stayed at {}",
            engine.snapshot().limiter_reduction_db
        );
    }

    /// The mirror image: with everything flat, the limiter should stay idle.
    /// Guards against the test above passing for the wrong reason.
    #[test]
    fn a_flat_chain_leaves_the_limiter_idle() {
        let Some((engine, _rx)) = engine() else {
            return;
        };
        engine
            .load(fixture("mixer-flat", 4.0), 0.0, 0.0)
            .expect("loading");
        engine.set_settings(boosted(MixerSettings {
            enabled: Some(true),
            ..Default::default()
        }));
        engine.play();

        assert!(
            wait_for(Duration::from_secs(2), || engine.snapshot().position_secs
                > 0.5),
            "playback never started"
        );
        assert!(
            engine.snapshot().limiter_reduction_db < 0.5,
            "an untouched mixer should not be driving the limiter"
        );
    }

    /// Settings changed mid-playback must take effect, not just those present
    /// when the track was loaded.
    #[test]
    fn a_setting_changed_during_playback_takes_effect() {
        let Some((engine, _rx)) = engine() else {
            return;
        };
        engine
            .load(fixture("mixer-live", 10.0), 0.0, 0.0)
            .expect("loading");
        engine.set_settings(boosted(MixerSettings {
            enabled: Some(true),
            ..Default::default()
        }));
        engine.play();

        assert!(
            wait_for(Duration::from_secs(3), || engine.snapshot().position_secs
                > 0.4),
            "playback never started"
        );
        assert!(
            engine.snapshot().limiter_reduction_db < 0.5,
            "should start idle"
        );

        engine.set_settings(boosted(MixerSettings {
            enabled: Some(true),
            normalisation: Some(Normalisation {
                enabled: true,
                gain_db: 24.0,
                limiter_enabled: true,
                ..Default::default()
            }),
            ..Default::default()
        }));

        assert!(
            limiter_engages(&engine),
            "turning a control up mid-track had no effect on the audio"
        );
    }
}

/// Pausing with a reverb tail must not cost the listener their place.
///
/// The tail is produced by discarding the audio already queued for the output
/// and winding the decoder back to what was actually heard, so the interesting
/// property is that resume continues from there rather than skipping the
/// discarded stretch or repeating it.
mod reverb_tail {
    use super::*;

    fn reverb_on() -> MixerSettings {
        MixerSettings {
            reverb: Some(Reverb {
                enabled: true,
                mix: 0.6,
                size: 0.8,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn started(engine: &AudioEngine) -> bool {
        wait_for(Duration::from_secs(3), || {
            engine.snapshot().position_secs > 0.4
        })
    }

    #[test]
    fn a_tail_keeps_the_position_where_the_listener_heard_it() {
        let Some((engine, _rx)) = engine() else {
            return;
        };
        engine.set_settings(MixerSettings::resolve(&[&reverb_on()]));
        engine.set_keep_tail(true);
        engine
            .load(fixture("tail-position", 8.0), 0.0, 0.0)
            .expect("loading");
        engine.play();
        assert!(started(&engine), "playback never started");

        engine.pause();
        // Long enough for the tail to have rendered and finished.
        std::thread::sleep(Duration::from_millis(700));
        let at_pause = engine.snapshot().position_secs;
        std::thread::sleep(Duration::from_millis(500));
        let later = engine.snapshot().position_secs;

        assert!(
            (later - at_pause).abs() < 0.15,
            "the position drifted while the tail rang out: {at_pause} then {later}"
        );
        assert!(!engine.is_playing());
    }

    #[test]
    fn playback_resumes_from_where_it_was_paused() {
        let Some((engine, _rx)) = engine() else {
            return;
        };
        engine.set_settings(MixerSettings::resolve(&[&reverb_on()]));
        engine.set_keep_tail(true);
        engine
            .load(fixture("tail-resume", 8.0), 0.0, 0.0)
            .expect("loading");
        engine.play();
        assert!(started(&engine), "playback never started");

        engine.pause();
        std::thread::sleep(Duration::from_millis(700));
        let at_pause = engine.snapshot().position_secs;

        engine.play();
        std::thread::sleep(Duration::from_millis(250));
        let resumed = engine.snapshot().position_secs;

        // Forwards, but not by a jump: winding the decoder back for the tail
        // must not have lost or repeated a chunk of the track.
        assert!(
            resumed >= at_pause - 0.15,
            "resume went backwards: paused at {at_pause}, resumed at {resumed}"
        );
        assert!(
            resumed - at_pause < 0.9,
            "resume skipped ahead: paused at {at_pause}, resumed at {resumed}"
        );
    }

    /// With the setting off, pause stays exactly as instant as it always was.
    #[test]
    fn without_the_setting_pause_is_unchanged() {
        let Some((engine, _rx)) = engine() else {
            return;
        };
        engine.set_settings(MixerSettings::resolve(&[&reverb_on()]));
        engine.set_keep_tail(false);
        engine
            .load(fixture("tail-off", 8.0), 0.0, 0.0)
            .expect("loading");
        engine.play();
        assert!(started(&engine), "playback never started");

        engine.pause();
        std::thread::sleep(Duration::from_millis(300));
        let at_pause = engine.snapshot().position_secs;
        std::thread::sleep(Duration::from_millis(500));

        assert!(
            (engine.snapshot().position_secs - at_pause).abs() < 0.1,
            "position moved while paused with the tail disabled"
        );
    }

    /// A dry chain has nothing to ring out, so the tail path must not engage
    /// and hold the output open for no reason.
    #[test]
    fn a_dry_chain_pauses_immediately_even_with_the_setting_on() {
        let Some((engine, _rx)) = engine() else {
            return;
        };
        engine.set_keep_tail(true);
        engine
            .load(fixture("tail-dry", 8.0), 0.0, 0.0)
            .expect("loading");
        engine.play();
        assert!(started(&engine), "playback never started");

        engine.pause();
        std::thread::sleep(Duration::from_millis(300));
        let at_pause = engine.snapshot().position_secs;
        std::thread::sleep(Duration::from_millis(500));

        assert!(
            (engine.snapshot().position_secs - at_pause).abs() < 0.1,
            "position moved while paused on a dry chain"
        );
    }
}

mod output_device {
    use super::*;

    #[test]
    fn the_machines_output_devices_can_be_listed() {
        if engine().is_none() {
            return;
        }
        // The default device the engine just opened has to appear in the list,
        // or the picker would not be able to show what is already selected.
        assert!(
            !AudioEngine::output_devices().is_empty(),
            "no output devices were listed on a machine that has one"
        );
    }

    #[test]
    fn switching_back_to_the_default_keeps_playing() {
        let Some((engine, _rx)) = engine() else {
            return;
        };
        engine
            .load(fixture("device-default", 8.0), 0.0, 0.0)
            .expect("loading");
        engine.play();
        assert!(
            wait_for(Duration::from_secs(3), || engine.snapshot().position_secs
                > 0.3),
            "playback never started"
        );

        let rate = engine
            .set_output_device(None)
            .expect("reopening the default device");
        assert!(rate >= 8000, "implausible sample rate reported: {rate}");
    }

    #[test]
    fn moving_to_each_available_device_reports_its_rate() {
        let Some((engine, _rx)) = engine() else {
            return;
        };
        for name in AudioEngine::output_devices() {
            // A device can be listed and still refuse to open — exclusive
            // mode, a disconnected interface — so a failure here is only
            // interesting if it leaves the engine unusable, which the
            // fallback to the default prevents.
            if let Ok(rate) = engine.set_output_device(Some(&name)) {
                assert!(rate >= 8000, "{name} reported an implausible rate: {rate}");
            }
        }
        // Whatever happened above, the engine still works.
        assert!(engine.set_output_device(None).is_ok());
    }

    #[test]
    fn an_unknown_device_is_refused_and_the_default_still_works() {
        let Some((engine, _rx)) = engine() else {
            return;
        };
        assert!(
            engine
                .set_output_device(Some("a device that does not exist"))
                .is_err(),
            "a made-up device name was accepted"
        );
        // The output thread falls back rather than leaving the app silent.
        assert!(
            wait_for(Duration::from_secs(3), || engine
                .set_output_device(None)
                .is_ok()),
            "the engine did not recover after a failed device switch"
        );
    }
}
