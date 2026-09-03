//! Offline bounce of a one-block mix through the same timeline the editor uses.

use std::f32::consts::TAU;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use pick_n_mix_lib::audio::ambience::Bank;
use pick_n_mix_lib::audio::bounce::{render, BounceFormat, BounceOptions};
use pick_n_mix_lib::audio::decode::decode_whole;
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
        // No block in this mix asks for an atmosphere, so an empty bank is
        // everything the render needs.
        Arc::new(Bank::new()),
        None,
        &|_| {},
    )
    .expect("bounce");

    let bytes = std::fs::metadata(&dest).unwrap().len();
    assert!(
        bytes > 44 + 1000,
        "bounced wav should contain more than a header, got {bytes} bytes"
    );
}

/// A one-by-one PNG: the smallest thing lofty will accept as a picture.
fn write_png(path: &PathBuf) {
    const PIXEL: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    std::fs::write(path, PIXEL).unwrap();
}

/// A playlist with its own picture bounces to a file carrying that picture,
/// so the mix arrives in another player looking like the playlist it came from.
#[test]
fn a_cover_is_embedded_in_the_bounced_file() {
    use lofty::file::TaggedFileExt;
    use lofty::picture::PictureType;
    use lofty::probe::Probe;

    let dir = temp_dir("cover");
    let source = dir.join("tone.wav");
    write_wav(&source, 44_100, 0.2, 440.0);
    let cover = dir.join("cover.png");
    write_png(&cover);
    let dest = dir.join("mix-with-cover.wav");

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
        Arc::new(Bank::new()),
        Some(cover.as_path()),
        &|_| {},
    )
    .expect("bounce");

    let tagged = Probe::open(&dest).unwrap().read().unwrap();
    let tag = tagged
        .primary_tag()
        .or_else(|| tagged.first_tag())
        .expect("the bounced file should carry a tag");
    assert!(
        tag.pictures()
            .iter()
            .any(|picture| picture.pic_type() == PictureType::CoverFront),
        "the playlist's image should be the front cover"
    );
}

fn one_block_plan(source: &PathBuf, seconds: f64) -> Plan {
    Plan::new(vec![PlanBlock {
        path: source.clone(),
        block: Block {
            source: BlockSource::Asset {
                file: "tone.wav".into(),
            },
            start_secs: 0.0,
            duration_secs: seconds,
            ..Default::default()
        },
        lane_gain: 1.0,
        settings: Arc::new(Resolved::default()),
        track_gain_db: 0.0,
    }])
}

fn options(format: BounceFormat) -> BounceOptions {
    BounceOptions {
        format,
        sample_rate: 44_100,
        wav_bit_depth: 24,
        flac_compression: 5,
        mp3_bitrate: 320,
    }
}

/// The metadata blocks of a FLAC, as (type, is_last) in file order.
fn flac_metadata_chain(path: &PathBuf) -> Vec<(u8, bool)> {
    let data = std::fs::read(path).unwrap();
    assert_eq!(&data[0..4], b"fLaC", "not a FLAC file");
    let mut out = Vec::new();
    let mut at = 4;
    loop {
        let header = data[at];
        let length = u32::from_be_bytes([0, data[at + 1], data[at + 2], data[at + 3]]) as usize;
        out.push((header & 0x7f, header & 0x80 != 0));
        at += 4 + length;
        if header & 0x80 != 0 || out.len() > 16 {
            break;
        }
    }
    out
}

/// A cover in a FLAC goes in as a metadata block, and the block before it has
/// to stop claiming to be the last one.
///
/// Getting that wrong produces a file every decoder reads the picture out of
/// as though it were audio: a burst of noise, then a stream it has to
/// resynchronise into. That is what a bounced mix with artwork used to be.
#[test]
fn a_flac_cover_leaves_the_audio_decodable() {
    let dir = temp_dir("flac-cover");
    let source = dir.join("tone.wav");
    write_wav(&source, 44_100, 1.0, 440.0);
    let cover = dir.join("cover.png");
    write_png(&cover);

    let plain = dir.join("plain.flac");
    render(
        one_block_plan(&source, 1.0),
        &plain,
        &options(BounceFormat::Flac),
        &Normalisation::default(),
        Arc::new(Bank::new()),
        None,
        &|_| {},
    )
    .expect("bounce without a cover");

    let with_cover = dir.join("cover.flac");
    render(
        one_block_plan(&source, 1.0),
        &with_cover,
        &options(BounceFormat::Flac),
        &Normalisation::default(),
        Arc::new(Bank::new()),
        Some(cover.as_path()),
        &|_| {},
    )
    .expect("bounce with a cover");

    let chain = flac_metadata_chain(&with_cover);
    assert_eq!(
        chain,
        vec![(0u8, false), (6u8, true)],
        "STREAMINFO then PICTURE, and only the picture is flagged last"
    );

    // The proof that matters: the audio is the same audio, byte for byte,
    // whether or not a picture is sitting in front of it.
    let bare = decode_whole(&plain, 44_100).expect("decode the plain flac");
    let covered = decode_whole(&with_cover, 44_100).expect("decode the covered flac");
    assert_eq!(bare.len(), covered.len());
    assert!(
        bare.iter().zip(covered.iter()).all(|(a, b)| a == b),
        "a cover must not change a single sample"
    );
    assert!(
        bare.iter().any(|s| s.abs() > 0.1),
        "the tone should be audible, not silence"
    );
}

/// A bounce reports how far along it is, monotonically, and finishes at one.
#[test]
fn a_bounce_reports_its_progress() {
    let dir = temp_dir("progress");
    let source = dir.join("tone.wav");
    write_wav(&source, 44_100, 3.0, 440.0);
    let dest = dir.join("progress.wav");

    let seen = std::sync::Mutex::new(Vec::new());
    render(
        one_block_plan(&source, 3.0),
        &dest,
        &options(BounceFormat::Wav),
        &Normalisation::default(),
        Arc::new(Bank::new()),
        None,
        &|fraction| seen.lock().unwrap().push(fraction),
    )
    .expect("bounce");

    let seen = seen.into_inner().unwrap();
    assert!(seen.len() > 1, "a three-second render should report more than once");
    assert!(
        seen.windows(2).all(|w| w[1] >= w[0]),
        "progress must not go backwards: {seen:?}"
    );
    assert_eq!(seen.last().copied(), Some(1.0), "it ends at the end");
}
