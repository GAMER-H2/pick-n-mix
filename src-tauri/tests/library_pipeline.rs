//! Scans real audio files, indexes them, and round-trips a shareable playlist.

use std::f32::consts::TAU;
use std::io::Write;
use std::path::{Path, PathBuf};

use pick_n_mix_lib::audio::params::{MixerSettings, Reverb};
use pick_n_mix_lib::library::db::Db;
use pick_n_mix_lib::library::scan;
use pick_n_mix_lib::playlist::Playlist;

fn write_wav(path: &Path, sample_rate: u32, channels: u16, seconds: f32, freq: f32) {
    let frames = (sample_rate as f32 * seconds) as u32;
    let data_len = frames * channels as u32 * 2;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * channels as u32 * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for frame in 0..frames {
        let t = frame as f32 / sample_rate as f32;
        let pcm = ((TAU * freq * t).sin() * 0.4 * i16::MAX as f32) as i16;
        for _ in 0..channels {
            out.extend_from_slice(&pcm.to_le_bytes());
        }
    }
    let mut file = std::fs::File::create(path).expect("creating wav");
    file.write_all(&out).expect("writing wav");
}

/// A music folder plus somewhere to put artwork and playlists.
fn fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("pnm-lib-test-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    let music = root.join("music");
    let artwork = root.join("artwork");
    let playlists = root.join("playlists");
    for dir in [&music, &artwork, &playlists] {
        std::fs::create_dir_all(dir).expect("creating fixture dirs");
    }
    write_wav(&music.join("01 First Light.wav"), 44100, 2, 2.0, 220.0);
    write_wav(&music.join("02 Slow Drift.wav"), 44100, 2, 1.5, 196.0);
    write_wav(&music.join("03 Night Bus.wav"), 48000, 2, 1.0, 174.6);
    // Something that is not music, to prove the filter works.
    std::fs::write(music.join("cover.jpg"), b"not audio").expect("writing decoy");
    (music, artwork, playlists)
}

#[test]
fn scanning_indexes_audio_and_ignores_everything_else() {
    let (music, artwork, _) = fixture("scan");
    let db = Db::open_in_memory().expect("opening db");

    let report = scan::scan_folders(&db, &artwork, &[music.display().to_string()], |_, _| {})
        .expect("scanning");

    assert_eq!(report.scanned, 3, "the jpg should not be scanned");
    assert_eq!(report.added, 3);
    assert!(
        report.errors.is_empty(),
        "unexpected errors: {:?}",
        report.errors
    );

    let tracks = db.all_tracks().expect("listing tracks");
    assert_eq!(tracks.len(), 3);

    // Titles fall back to the file name when there are no tags.
    assert!(tracks.iter().any(|t| t.title == "01 First Light"));

    let night_bus = tracks
        .iter()
        .find(|t| t.title == "03 Night Bus")
        .expect("finding track");
    assert_eq!(night_bus.sample_rate, Some(48000));
    assert_eq!(night_bus.channels, Some(2));
    assert!((night_bus.duration_secs - 1.0).abs() < 0.1);
    assert_eq!(night_bus.format.as_deref(), Some("WAV"));
    assert!(night_bus.file_size.unwrap_or(0) > 0);
}

#[test]
fn rescanning_updates_rather_than_duplicates() {
    let (music, artwork, _) = fixture("rescan");
    let db = Db::open_in_memory().expect("opening db");
    let folders = vec![music.display().to_string()];

    scan::scan_folders(&db, &artwork, &folders, |_, _| {}).expect("first scan");
    let second = scan::scan_folders(&db, &artwork, &folders, |_, _| {}).expect("second scan");

    assert_eq!(second.added, 0);
    assert_eq!(second.updated, 3);
    assert_eq!(db.track_count().expect("counting"), 3);
}

#[test]
fn deleted_files_drop_out_of_the_index() {
    let (music, artwork, _) = fixture("deleted");
    let db = Db::open_in_memory().expect("opening db");
    let folders = vec![music.display().to_string()];

    scan::scan_folders(&db, &artwork, &folders, |_, _| {}).expect("first scan");
    std::fs::remove_file(music.join("02 Slow Drift.wav")).expect("removing a file");
    scan::scan_folders(&db, &artwork, &folders, |_, _| {}).expect("second scan");

    assert_eq!(db.track_count().expect("counting"), 2);
    assert!(db
        .all_tracks()
        .unwrap()
        .iter()
        .all(|t| t.title != "02 Slow Drift"));
}

#[test]
fn a_playlist_survives_being_shared_with_a_different_library() {
    let (music, artwork, playlists) = fixture("share");
    let db = Db::open_in_memory().expect("opening db");
    scan::scan_folders(&db, &artwork, &[music.display().to_string()], |_, _| {}).expect("scanning");

    // Build a playlist on "their" machine.
    let mut playlist = Playlist {
        name: "Late Night".into(),
        ..Default::default()
    };
    playlist.mixer = Some(MixerSettings {
        reverb: Some(Reverb {
            enabled: true,
            mix: 0.4,
            ..Default::default()
        }),
        ..Default::default()
    });
    for track in db.all_tracks().expect("listing") {
        playlist.add_track(&track);
    }
    let file = playlists.join("late-night.pnmx");
    playlist.save(&file).expect("saving playlist");

    // "Our" machine: same music, completely different paths.
    let (our_music, our_artwork, _) = fixture("share-receiver");
    let our_db = Db::open_in_memory().expect("opening db");
    scan::scan_folders(
        &our_db,
        &our_artwork,
        &[our_music.display().to_string()],
        |_, _| {},
    )
    .expect("scanning");

    let received = Playlist::load(&file).expect("loading the shared playlist");
    assert_ne!(
        received.tracks[0].local_path.as_deref().unwrap(),
        our_music.join("01 First Light.wav").display().to_string(),
        "the stored path must genuinely differ for this test to mean anything"
    );

    let resolved = received.resolve(&our_db).expect("resolving");
    assert_eq!(
        resolved.missing_count, 0,
        "every entry should re-match by identity"
    );
    assert_eq!(resolved.items.len(), 3);
    assert_eq!(
        resolved.playlist.mixer.unwrap().reverb.unwrap().mix,
        0.4,
        "the playlist's mixer travelled with it"
    );
}

#[test]
fn artwork_is_not_written_for_files_that_have_none() {
    let (music, artwork, _) = fixture("artwork");
    let db = Db::open_in_memory().expect("opening db");
    scan::scan_folders(&db, &artwork, &[music.display().to_string()], |_, _| {}).expect("scanning");

    assert!(db
        .all_tracks()
        .unwrap()
        .iter()
        .all(|t| t.artwork_id.is_none()));
    let written = std::fs::read_dir(&artwork).map(|d| d.count()).unwrap_or(0);
    assert_eq!(written, 0, "no artwork should have been cached");
}
