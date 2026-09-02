//! The packaged atmospheres, checked against the machine they will actually
//! run on.
//!
//! These exist because a silent atmosphere is indistinguishable from a broken
//! one. If a bundled file is missing, unreadable, or simply so slow to become
//! audible that nobody waits for it, the only symptom is a button that lights
//! up and produces nothing.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use pick_n_mix_lib::audio::ambience::{load_bed, BUILT_IN};
use pick_n_mix_lib::audio::decode::TrackDecoder;

/// Generous ceiling for one bed to decode.
///
/// Not a performance target — it is the point past which an atmosphere reads
/// as broken rather than slow. Measured worst case is around eight seconds in
/// a debug build, for the one asset that needs resampling; the original
/// unoptimised-dependency build took eighty-six, which is what this guards
/// against coming back.
const SLOW: Duration = Duration::from_secs(30);

fn asset(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .join("audio_assets")
        .join(file)
}

#[test]
fn every_packaged_atmosphere_is_present_and_becomes_audible_promptly() {
    let mut problems = Vec::new();

    for (id, _, file) in BUILT_IN {
        let path = asset(file);
        assert!(path.is_file(), "{file} is missing from audio_assets/");

        // Reported alongside the timing because a bed whose rate does not match
        // the device goes through the resampler, which costs far more than the
        // decode itself and is otherwise invisible.
        let rate = TrackDecoder::open(&path, 48_000)
            .map(|decoder| decoder.info.sample_rate)
            .unwrap_or(0);

        let started = Instant::now();
        match load_bed(&path, 48_000) {
            Ok(samples) => {
                let elapsed = started.elapsed();
                let secs = samples.len() as f64 / 2.0 / 48_000.0;
                let resampled = if rate == 48_000 { "" } else { " (resampled)" };
                println!("{id}: {secs:.0}s at {rate} Hz, decoded in {elapsed:?}{resampled}");

                if samples.is_empty() {
                    problems.push(format!("{id} decoded to no audio at all"));
                } else if elapsed > SLOW {
                    problems.push(format!(
                        "{id} took {elapsed:?} to decode, which is long enough to look broken"
                    ));
                }
            }
            Err(e) => problems.push(format!("{id} ({file}) failed to decode: {e}")),
        }
    }

    assert!(
        problems.is_empty(),
        "packaged atmospheres that would not be heard:\n{}",
        problems.join("\n")
    );
}
