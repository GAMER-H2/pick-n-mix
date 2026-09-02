//! Offline bounce of a one-block mix through the same timeline the editor uses.

use std::f32::consts::TAU;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use pick_n_mix_lib::audio::bounce::{render, BounceFormat, BounceOptions};
use pick_n_mix_lib::audio::params::{Normalisation, Resolved};
use pick_n_mix_lib::audio::timeline::{Plan, PlanBlock};
use pick_n_mix_lib::master_mix::{Block, BlockSource};

fn write_wav(path: &PathBuf, sample_rate: u32, seconds: f32, freq: f32) {
    let frames = (sample_rate as f32 * seconds) as u32;
    let data_len = frames * 4;
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
        let sample = (TAU * freq * t).sin() * 0.5;
        let pcm = (sample * i16::MAX as f32) as i16;
        out.extend_from_slice(&pcm.to_le_bytes());
        out.extend_from_slice(&pcm.to_le_bytes());
    }
    std::fs::File::create(path)
        .unwrap()
        .write_all(&out)
        .unwrap();
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pnm-bounce-test-{name}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn bouncing_a_one_block_mix_writes_a_wav() {
    let dir = temp_dir("wav");
    let source = dir.join("tone.wav");
    write_wav(&source, 44_100, 0.2, 440.0);
    let dest = dir.join("mix.wav");

    let plan = Plan::new(vec![PlanBlock {
        path: source,
        block: Block {
            source: BlockSource::Asset {
                file: "tone.wav".into(),
            },
            start_secs: 0.0,
            duration_secs: 0.2,
            ..Default::default()
        },
        lane_gain: 1.0,
        settings: Arc::new(Resolved::default()),
        track_gain_db: 0.0,
    }]);

    render(
        plan,
        &dest,
        &BounceOptions {
            format: BounceFormat::Wav,
            sample_rate: 44_100,
            wav_bit_depth: 16,
            flac_compression: 5,
            mp3_bitrate: 320,
        },
        &Normalisation::default(),
    )
    .expect("bounce");

    let bytes = std::fs::metadata(&dest).unwrap().len();
    assert!(
        bytes > 44 + 1000,
        "bounced wav should contain more than a header, got {bytes} bytes"
    );
}
