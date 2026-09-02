//! Local folder scanning.
//!
//! Reads tags and embedded artwork straight out of the files. Nothing here
//! touches the network; online enrichment is a separate, opt-in step.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
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

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "flac", "mp3", "m4a", "mp4", "aac", "ogg", "oga", "opus", "wav", "aiff", "aif", "wv", "ape",
];

/// Walk `folders`, index everything found, and mark absent versions missing.
/// A root is swept only when its complete walk succeeded, avoiding destructive
/// conclusions from disconnected drives or permission errors.
pub fn scan_folders(
    db: &Db,
    artwork_dir: &Path,
    folders: &[String],
    mut progress: impl FnMut(u32, &str),
) -> Result<ScanReport> {
    std::fs::create_dir_all(artwork_dir).ok();
    let mut report = ScanReport::default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut readable_roots: Vec<PathBuf> = Vec::new();

    for folder in folders {
        if let Err(error) = std::fs::read_dir(folder) {
            report.errors.push(format!("{folder}: {error}"));
            continue;
        }

        let mut complete_walk = true;
        for item in WalkDir::new(folder).follow_links(true) {
            let entry = match item {
                Ok(entry) => entry,
                Err(error) => {
                    complete_walk = false;
                    report.errors.push(format!("{folder}: {error}"));
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !is_supported(path) {
                continue;
            }

            let location = path.display().to_string();
            // A physically present but temporarily unreadable/invalid audio file
            // is not missing. Only tag refresh is skipped in that case.
            seen.insert(location.clone());
            report.scanned += 1;
            progress(report.scanned, &location);

            match read_track(path, artwork_dir) {
                Ok(track) => match db.upsert_track(&track) {
                    Ok(true) => report.added += 1,
                    Ok(false) => report.updated += 1,
                    Err(error) => report.errors.push(format!("{}: {error}", path.display())),
                },
                Err(error) => {
                    report.skipped += 1;
                    report.errors.push(format!("{}: {error}", path.display()));
                }
            }
        }
        if complete_walk {
            readable_roots.push(PathBuf::from(folder));
        }
    }

    for location in db.locations(SOURCE_LOCAL)? {
        if seen.contains(&location) {
            continue;
        }
        if readable_roots
            .iter()
            .any(|root| Path::new(&location).starts_with(root))
        {
            db.mark_file_missing(SOURCE_LOCAL, &location)?;
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
        bitrate_kbps: properties
            .audio_bitrate()
            .or_else(|| properties.overall_bitrate()),
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
                Err(e) => eprintln!(
                    "library: could not save artwork for {}: {e}",
                    track.location
                ),
            }
        }
    }

    // Deliberately not filled in from `artist`: an album-artist we invented is
    // indistinguishable from a real one, and guessing it splits every
    // compilation into one album per track.
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
    if let Some(v) = tag
        .get_string(&ItemKey::AlbumArtist)
        .filter(|v| !v.trim().is_empty())
    {
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
    track.musicbrainz_recording_id = tag
        .get_string(&ItemKey::MusicBrainzRecordingId)
        .map(|s| s.to_string());
    track.musicbrainz_release_id = tag
        .get_string(&ItemKey::MusicBrainzReleaseId)
        .map(|s| s.to_string());
    track.gain_db = tag
        .get_string(&ItemKey::ReplayGainTrackGain)
        .and_then(parse_replay_gain);
}

/// ReplayGain values look like "-7.53 dB"; take the number.
fn parse_replay_gain(raw: &str) -> Option<f32> {
    raw.split_whitespace()
        .next()?
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
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

/// Copy an image the user picked into the artwork cache, keyed by its contents
/// the same way embedded covers are. The copy is what everything refers to
/// afterwards, so moving or deleting the original cannot break the reference.
pub fn store_artwork_file(dir: &Path, source: &Path) -> Result<String> {
    let data =
        std::fs::read(source).with_context(|| format!("reading image {}", source.display()))?;

    let ext = match source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "png",
        "gif" => "gif",
        "webp" => "webp",
        _ => "jpg",
    };

    let id = format!("{}.{ext}", stable_id("art", &fingerprint(&data)));
    let path = dir.join(&id);
    if !path.exists() {
        std::fs::create_dir_all(dir).ok();
        std::fs::write(&path, &data)
            .with_context(|| format!("writing artwork {}", path.display()))?;
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

    #[test]
    fn a_chosen_image_is_copied_into_the_cache_and_survives_the_original() {
        let dir = std::env::temp_dir().join(format!(
            "pnm-art-{}",
            stable_id("d", &format!("{:?}", std::time::Instant::now()))
        ));
        let cache = dir.join("artwork");
        std::fs::create_dir_all(&dir).unwrap();

        let source = dir.join("cover.png");
        std::fs::write(&source, b"pretend png bytes").unwrap();

        let id = store_artwork_file(&cache, &source).unwrap();
        assert!(id.ends_with(".png"), "the source extension is kept: {id}");

        // The point of copying: deleting what the user picked changes nothing.
        std::fs::remove_file(&source).unwrap();
        assert_eq!(
            std::fs::read(cache.join(&id)).unwrap(),
            b"pretend png bytes"
        );

        // Content-addressed, so the same image chosen twice is stored once.
        let again = dir.join("copy-of-cover.png");
        std::fs::write(&again, b"pretend png bytes").unwrap();
        assert_eq!(store_artwork_file(&cache, &again).unwrap(), id);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unreadable_image_is_reported_rather_than_stored() {
        let cache = std::env::temp_dir().join("pnm-art-missing");
        assert!(store_artwork_file(&cache, Path::new("/nope/not-here.png")).is_err());
    }

    #[test]
    fn only_successfully_readable_roots_are_swept() {
        let base = std::env::temp_dir().join(stable_id(
            "scan",
            &format!("{:?}", std::time::Instant::now()),
        ));
        let readable = base.join("readable");
        let unavailable = base.join("unavailable");
        let artwork = base.join("artwork");
        std::fs::create_dir_all(&readable).unwrap();

        let db = Db::open_in_memory().unwrap();
        let make_track = |path: &Path| Track {
            id: stable_id("t", &path.display().to_string()),
            source_id: SOURCE_LOCAL.into(),
            location: path.display().to_string(),
            title: "Song".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            duration_secs: 180.0,
            ..Default::default()
        };
        let under_readable = make_track(&readable.join("gone.flac"));
        let under_unavailable = make_track(&unavailable.join("gone.flac"));
        db.upsert_track(&under_readable).unwrap();
        db.upsert_track(&under_unavailable).unwrap();

        let report = scan_folders(
            &db,
            &artwork,
            &[
                readable.display().to_string(),
                unavailable.display().to_string(),
            ],
            |_, _| {},
        )
        .unwrap();
        assert!(
            !report.errors.is_empty(),
            "the unavailable root is reported"
        );
        assert!(
            db.file_by_location(SOURCE_LOCAL, &under_readable.location)
                .unwrap()
                .unwrap()
                .missing
        );
        assert!(
            db.file_by_location(SOURCE_LOCAL, &under_unavailable.location)
                .unwrap()
                .unwrap()
                .available
        );

        let _ = std::fs::remove_dir_all(base);
    }
}
