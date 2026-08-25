//! Local folder scanning.
//!
//! Reads tags and embedded artwork straight out of the files. Nothing here
//! touches the network; online enrichment is a separate, opt-in step.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{Picture, PictureType};
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;
use lofty::tag::Tag;
use walkdir::WalkDir;

use crate::library::db::{now, Db};
use crate::library::model::{stable_id, ScanReport, Track};

pub const SOURCE_LOCAL: &str = "local";

pub const SUPPORTED_EXTENSIONS: &[&str] =
    &["flac", "mp3", "m4a", "mp4", "aac", "ogg", "oga", "opus", "wav", "aiff", "aif", "wv", "ape"];

/// Walk `folders`, index everything found, and forget anything that has gone.
pub fn scan_folders(
    db: &Db,
    artwork_dir: &Path,
    folders: &[String],
    mut progress: impl FnMut(u32, &str),
) -> Result<ScanReport> {
    std::fs::create_dir_all(artwork_dir).ok();
    let mut report = ScanReport::default();
    let mut seen: HashSet<String> = HashSet::new();

    for folder in folders {
        for entry in WalkDir::new(folder).follow_links(true).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !is_supported(path) {
                continue;
            }

            report.scanned += 1;
            progress(report.scanned, &path.display().to_string());

            match read_track(path, artwork_dir) {
                Ok(track) => {
                    seen.insert(track.location.clone());
                    match db.upsert_track(&track) {
                        Ok(true) => report.added += 1,
                        Ok(false) => report.updated += 1,
                        Err(e) => report.errors.push(format!("{}: {e}", path.display())),
                    }
                }
                Err(e) => {
                    report.skipped += 1;
                    report.errors.push(format!("{}: {e}", path.display()));
                }
            }
        }
    }

    // Drop rows whose files are no longer under any watched folder.
    for location in db.locations(SOURCE_LOCAL)? {
        if seen.contains(&location) {
            continue;
        }
        let under_watch = folders.iter().any(|f| location.starts_with(f.as_str()));
        if under_watch || !Path::new(&location).exists() {
            db.delete_track_at(SOURCE_LOCAL, &location)?;
        }
    }

    Ok(report)
}

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Read one file into a `Track`, extracting artwork to `artwork_dir`.
pub fn read_track(path: &Path, artwork_dir: &Path) -> Result<Track> {
    let tagged = Probe::open(path)?.options(ParseOptions::new()).read()?;
    let properties = tagged.properties();
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let location = path.display().to_string();
    let file_size = std::fs::metadata(path).map(|m| m.len()).ok();

    // Fall back to the file name when a file has no usable title tag.
    let fallback_title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown Track")
        .to_string();

    let mut track = Track {
        id: stable_id("t", &location),
        source_id: SOURCE_LOCAL.into(),
        location,
        title: fallback_title,
        artist: "Unknown Artist".into(),
        // Left empty on purpose: a file with no album tag belongs to no album,
        // rather than to a fictional "Unknown Album" that collects strangers.
        album: String::new(),
        duration_secs: properties.duration().as_secs_f64(),
        sample_rate: properties.sample_rate(),
        channels: properties.channels().map(|c| c as u32),
        bits_per_sample: properties.bit_depth().map(|b| b as u32),
        bitrate_kbps: properties.audio_bitrate().or_else(|| properties.overall_bitrate()),
        file_size,
        format: path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_uppercase()),
        added_at: now(),
        ..Default::default()
    };

    if let Some(tag) = tag {
        apply_tag(&mut track, tag);
        if let Some(picture) = pick_cover(tag) {
            match store_artwork(artwork_dir, picture) {
                Ok(id) => track.artwork_id = Some(id),
                // Bad artwork is not a reason to skip an otherwise fine track.
                Err(e) => eprintln!("library: could not save artwork for {}: {e}", track.location),
            }
        }
    }

    if track.album_artist.trim().is_empty() {
        track.album_artist = track.artist.clone();
    }
    Ok(track)
}

fn apply_tag(track: &mut Track, tag: &Tag) {
    if let Some(v) = tag.title().filter(|v| !v.trim().is_empty()) {
        track.title = v.to_string();
    }
    if let Some(v) = tag.artist().filter(|v| !v.trim().is_empty()) {
        track.artist = v.to_string();
    }
    if let Some(v) = tag.album().filter(|v| !v.trim().is_empty()) {
        track.album = v.to_string();
    }
    if let Some(v) = tag.get_string(&ItemKey::AlbumArtist).filter(|v| !v.trim().is_empty()) {
        track.album_artist = v.to_string();
    }
    track.track_number = tag.track();
    track.disc_number = tag.disk();
    track.year = tag.year().map(|y| y as i32).or_else(|| {
        // Some files only carry a full date; take the leading year from it.
        tag.get_string(&ItemKey::RecordingDate)
            .and_then(|d| d.get(..4).and_then(|y| y.parse().ok()))
    });
    track.genre = tag.genre().map(|g| g.to_string());
    track.musicbrainz_recording_id =
        tag.get_string(&ItemKey::MusicBrainzRecordingId).map(|s| s.to_string());
    track.musicbrainz_release_id =
        tag.get_string(&ItemKey::MusicBrainzReleaseId).map(|s| s.to_string());
    track.gain_db = tag.get_string(&ItemKey::ReplayGainTrackGain).and_then(parse_replay_gain);
}

/// ReplayGain values look like "-7.53 dB"; take the number.
fn parse_replay_gain(raw: &str) -> Option<f32> {
    raw.split_whitespace().next()?.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// Prefer the front cover, then any cover, then whatever is there.
fn pick_cover(tag: &Tag) -> Option<&Picture> {
    tag.get_picture_type(PictureType::CoverFront)
        .or_else(|| tag.get_picture_type(PictureType::Other))
        .or_else(|| tag.pictures().first())
}

/// Write artwork to the cache, keyed by a hash of its bytes so identical
/// covers across an album are stored once.
pub fn store_artwork(dir: &Path, picture: &Picture) -> Result<String> {
    let data = picture.data();
    let ext = picture
        .mime_type()
        .map(|m| match m.as_str() {
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            _ => "jpg",
        })
        .unwrap_or("jpg");

    let id = format!("{}.{ext}", stable_id("art", &fingerprint(data)));
    let path = dir.join(&id);
    if !path.exists() {
        std::fs::create_dir_all(dir).ok();
        std::fs::write(&path, data)?;
    }
    Ok(id)
}

/// Cheap content fingerprint: length plus a sample of the bytes. Artwork files
/// that collide here would have to be the same size and match at every probe.
fn fingerprint(data: &[u8]) -> String {
    let mut hash: u64 = data.len() as u64;
    for chunk in data.chunks(997) {
        for byte in chunk.iter().take(16) {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// Files under `dir` that look like music, used by drag-and-drop import.
pub fn collect_audio_files(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_supported(e.path()))
        .map(|e| e.path().to_path_buf())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_matching_is_case_insensitive() {
        assert!(is_supported(Path::new("/m/a.FLAC")));
        assert!(is_supported(Path::new("/m/a.mp3")));
        assert!(!is_supported(Path::new("/m/cover.jpg")));
        assert!(!is_supported(Path::new("/m/notes.txt")));
    }

    #[test]
    fn replay_gain_strings_are_parsed() {
        assert_eq!(parse_replay_gain("-7.53 dB"), Some(-7.53));
        assert_eq!(parse_replay_gain("+2.10 dB"), Some(2.10));
        assert_eq!(parse_replay_gain("not a number"), None);
    }

    #[test]
    fn identical_artwork_bytes_share_a_fingerprint() {
        let a = vec![1u8, 2, 3, 4, 5];
        let b = vec![1u8, 2, 3, 4, 5];
        let c = vec![9u8, 8, 7, 6, 5];
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_ne!(fingerprint(&a), fingerprint(&c));
    }
}
