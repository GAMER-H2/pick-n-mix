//! The shareable playlist file.
//!
//! Design goals, in order:
//!
//! 1. **Portable.** Entries identify music by what it *is* (title, artist,
//!    album, duration, MusicBrainz id), not by where it sits on one machine.
//!    A file path is stored only as a hint and is ignored if it is not there.
//! 2. **Forward compatible.** Every field has a default, so a file written by
//!    an older version still loads. Unknown fields are kept and written back
//!    untouched, so a file that has been through a newer version and back
//!    loses nothing.
//! 3. **Readable.** Plain pretty-printed JSON, so it diffs and can be hand-fixed.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::audio::params::MixerSettings;
use crate::library::db::Db;
use crate::library::model::{normalise, stable_id, Track};

/// Marker so a stray `.json` is not mistaken for a playlist.
pub const FORMAT_TAG: &str = "pick-n-mix.playlist";
/// Bumped only for changes that older versions genuinely cannot read.
pub const SCHEMA_VERSION: u32 = 1;
pub const EXTENSION: &str = "pnmx";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Playlist {
    pub format: String,
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    /// Artwork id in the local cache, or a URL. Optional; the UI falls back to
    /// a collage of the first few tracks' covers.
    pub artwork: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// Ignore the stored order: every time this playlist is played, shuffle it.
    /// Starting on a chosen track still honours that choice, and only the songs
    /// after it are shuffled.
    pub shuffle_only: bool,
    /// Mixer override applied to everything played from this playlist.
    pub mixer: Option<MixerSettings>,
    pub tracks: Vec<Entry>,
    /// Anything a newer version wrote, preserved verbatim.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Playlist {
    fn default() -> Self {
        let now = crate::library::db::now();
        Playlist {
            format: FORMAT_TAG.to_string(),
            schema_version: SCHEMA_VERSION,
            id: stable_id("pl", &format!("{now}")),
            name: "New Playlist".to_string(),
            description: String::new(),
            artwork: None,
            created_at: now,
            updated_at: now,
            shuffle_only: false,
            mixer: None,
            tracks: Vec::new(),
            extra: Map::new(),
        }
    }
}

/// One entry. Enough identity to be re-found in someone else's library.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Entry {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub duration_secs: f64,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<i32>,
    pub musicbrainz_recording_id: Option<String>,
    /// Where this was on the machine that wrote the file. A hint only.
    pub local_path: Option<String>,
    /// Mixer override for this entry alone, the innermost cascade layer.
    pub mixer: Option<MixerSettings>,
    pub added_at: i64,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Entry {
    pub fn from_track(track: &Track) -> Self {
        Entry {
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            album_artist: track.album_artist.clone(),
            duration_secs: track.duration_secs,
            track_number: track.track_number,
            disc_number: track.disc_number,
            year: track.year,
            musicbrainz_recording_id: track.musicbrainz_recording_id.clone(),
            local_path: Some(track.location.clone()),
            mixer: None,
            added_at: crate::library::db::now(),
            extra: Map::new(),
        }
    }
}

/// A playlist joined against the local library, ready for the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    #[serde(flatten)]
    pub playlist: Playlist,
    pub items: Vec<ResolvedEntry>,
    /// How many entries could not be matched to anything local.
    pub missing_count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedEntry {
    /// Index into `playlist.tracks`, so edits address the right entry.
    pub index: usize,
    pub entry: Entry,
    /// The local track this matched, if any.
    pub track: Option<Track>,
}

impl Playlist {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading playlist {}", path.display()))?;
        let mut playlist: Playlist = serde_json::from_str(&raw)
            .with_context(|| format!("parsing playlist {}", path.display()))?;

        if playlist.format.is_empty() {
            playlist.format = FORMAT_TAG.to_string();
        }
        if playlist.name.trim().is_empty() {
            playlist.name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled Playlist")
                .to_string();
        }
        // A file written by a future version still loads; the fields we do not
        // understand ride along in `extra`.
        Ok(playlist)
    }

    pub fn save(&mut self, path: &Path) -> Result<()> {
        self.updated_at = crate::library::db::now();
        self.format = FORMAT_TAG.to_string();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let json = serde_json::to_string_pretty(self).context("serialising playlist")?;
        // Write to a temporary file first so an interrupted save cannot
        // truncate a good playlist.
        let temp = path.with_extension("tmp");
        std::fs::write(&temp, json)
            .with_context(|| format!("writing playlist {}", temp.display()))?;
        std::fs::rename(&temp, path)
            .with_context(|| format!("replacing playlist {}", path.display()))?;
        Ok(())
    }

    pub fn add_track(&mut self, track: &Track) {
        self.tracks.push(Entry::from_track(track));
    }

    /// Join every entry against the library.
    pub fn resolve(self, db: &Db) -> Result<Resolved> {
        let mut items = Vec::with_capacity(self.tracks.len());
        let mut missing = 0;

        for (index, entry) in self.tracks.iter().enumerate() {
            // The stored path is tried first as a fast path, then we fall back
            // to matching on musical identity.
            let by_path = if let Some(path) = entry.local_path.as_deref() {
                let direct = match db.file_by_location("local", path)? {
                    Some(file) => db.get_track(&file.song_id)?,
                    None => None,
                };
                direct
                    .or(db.get_track(&stable_id("t", path))?)
                    .filter(|track| normalise(&track.album) == normalise(&entry.album))
            } else {
                None
            };

            let track = match by_path {
                Some(t) => Some(t),
                None => db.resolve(
                    entry.musicbrainz_recording_id.as_deref(),
                    &entry.artist,
                    &entry.title,
                    &entry.album,
                )?,
            };

            if track.is_none() {
                missing += 1;
            }
            items.push(ResolvedEntry {
                index,
                entry: entry.clone(),
                track,
            });
        }

        Ok(Resolved {
            playlist: self,
            items,
            missing_count: missing,
        })
    }
}

/// Every playlist file in `dir`, sorted by name.
pub fn list(dir: &Path) -> Vec<(PathBuf, Playlist)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let looks_right = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case(EXTENSION) || e.eq_ignore_ascii_case("json"))
            .unwrap_or(false);
        if !looks_right {
            continue;
        }
        match Playlist::load(&path) {
            Ok(p) if p.format == FORMAT_TAG => out.push((path, p)),
            // Quietly skip unrelated JSON sitting in the folder.
            Ok(_) => {}
            Err(e) => eprintln!("playlists: skipping {}: {e}", path.display()),
        }
    }
    out.sort_by(|a, b| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()));
    out
}

/// Turn a playlist name into a safe file name.
pub fn file_name_for(name: &str, id: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == ' ' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        format!("{id}.{EXTENSION}")
    } else {
        format!("{slug}-{}.{EXTENSION}", &id[id.len().saturating_sub(6)..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::params::Reverb;

    fn track(title: &str, artist: &str, album: &str, path: &str) -> Track {
        Track {
            id: stable_id("t", path),
            source_id: "local".into(),
            location: path.into(),
            title: title.into(),
            artist: artist.into(),
            album_artist: artist.into(),
            album: album.into(),
            duration_secs: 200.0,
            ..Default::default()
        }
    }

    #[test]
    fn a_file_missing_every_optional_field_still_loads() {
        // The bare minimum someone might hand-write.
        let json = r#"{ "name": "Minimal", "tracks": [ { "title": "A", "artist": "B" } ] }"#;
        let dir = tempdir();
        let path = dir.join("minimal.pnmx");
        std::fs::write(&path, json).unwrap();

        let p = Playlist::load(&path).unwrap();
        assert_eq!(p.name, "Minimal");
        assert_eq!(p.tracks.len(), 1);
        assert_eq!(p.tracks[0].title, "A");
        assert_eq!(p.tracks[0].duration_secs, 0.0);
        assert_eq!(p.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn fields_from_a_newer_version_survive_a_load_and_save() {
        let json = r#"{
            "format": "pick-n-mix.playlist",
            "name": "Future",
            "smartRules": { "genre": "jazz" },
            "tracks": [ { "title": "A", "crossfadeMs": 2500 } ]
        }"#;
        let dir = tempdir();
        let path = dir.join("future.pnmx");
        std::fs::write(&path, json).unwrap();

        let mut p = Playlist::load(&path).unwrap();
        p.save(&path).unwrap();

        let round_tripped = std::fs::read_to_string(&path).unwrap();
        assert!(
            round_tripped.contains("smartRules"),
            "playlist-level unknowns must survive"
        );
        assert!(
            round_tripped.contains("crossfadeMs"),
            "entry-level unknowns must survive"
        );
    }

    #[test]
    fn entries_resolve_by_identity_when_the_path_is_wrong() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track(
            "Come Together",
            "The Beatles",
            "Abbey Road",
            "/mine/1.flac",
        ))
        .unwrap();

        let mut playlist = Playlist::default();
        playlist.tracks.push(Entry {
            title: "Come Together".into(),
            artist: "The Beatles".into(),
            album: "Abbey Road".into(),
            // The path from whoever shared the file; it does not exist here.
            local_path: Some("/their/machine/come-together.flac".into()),
            ..Default::default()
        });

        let resolved = playlist.resolve(&db).unwrap();
        assert_eq!(resolved.missing_count, 0);
        assert_eq!(
            resolved.items[0].track.as_ref().unwrap().location,
            "/mine/1.flac"
        );
    }

    #[test]
    fn a_path_hint_never_crosses_album_boundaries() {
        let db = Db::open_in_memory().unwrap();
        let wrong = track("Song", "Artist", "Wrong Album", "/mine/wrong.flac");
        let right = track("Song", "Artist", "Right Album", "/mine/right.flac");
        db.upsert_track(&wrong).unwrap();
        db.upsert_track(&right).unwrap();

        let mut playlist = Playlist::default();
        playlist.tracks.push(Entry {
            title: "Song".into(),
            artist: "Artist".into(),
            album: "Right Album".into(),
            local_path: Some(wrong.location),
            ..Default::default()
        });

        let resolved = playlist.resolve(&db).unwrap();
        assert_eq!(resolved.items[0].track.as_ref().unwrap().id, right.id);
    }

    #[test]
    fn unmatched_entries_are_reported_rather_than_dropped() {
        let db = Db::open_in_memory().unwrap();
        let mut playlist = Playlist::default();
        playlist.tracks.push(Entry {
            title: "Not In My Library".into(),
            artist: "Someone".into(),
            ..Default::default()
        });

        let resolved = playlist.resolve(&db).unwrap();
        assert_eq!(resolved.missing_count, 1);
        assert_eq!(resolved.items.len(), 1, "the entry stays visible");
        assert!(resolved.items[0].track.is_none());
    }

    #[test]
    fn playlist_and_entry_mixer_overrides_round_trip() {
        let dir = tempdir();
        let path = dir.join("mixed.pnmx");

        let mut playlist = Playlist::default();
        playlist.mixer = Some(MixerSettings {
            reverb: Some(Reverb {
                enabled: true,
                mix: 0.4,
                ..Default::default()
            }),
            ..Default::default()
        });
        playlist.tracks.push(Entry {
            title: "A".into(),
            mixer: Some(MixerSettings {
                reverb: Some(Reverb {
                    enabled: true,
                    mix: 0.9,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        });
        playlist.save(&path).unwrap();

        let loaded = Playlist::load(&path).unwrap();
        assert_eq!(loaded.mixer.unwrap().reverb.unwrap().mix, 0.4);
        assert_eq!(
            loaded.tracks[0]
                .mixer
                .as_ref()
                .unwrap()
                .reverb
                .as_ref()
                .unwrap()
                .mix,
            0.9
        );
    }

    #[test]
    fn shuffle_only_defaults_off_and_round_trips() {
        let dir = tempdir();
        let path = dir.join("shuffled.pnmx");

        // A file written before the option existed must still load, and must
        // not suddenly start shuffling itself.
        std::fs::write(&path, r#"{ "name": "Old", "tracks": [] }"#).unwrap();
        assert!(!Playlist::load(&path).unwrap().shuffle_only);

        let mut playlist = Playlist::default();
        playlist.shuffle_only = true;
        playlist.save(&path).unwrap();
        assert!(Playlist::load(&path).unwrap().shuffle_only);
    }

    #[test]
    fn file_names_are_slugified_and_unique() {
        let name = file_name_for("My Chill / Mix!", "pl_abcdef123456");
        assert!(name.ends_with(".pnmx"));
        assert!(!name.contains('/'));
        assert!(name.starts_with("my-chill---mix-"));
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pnm-test-{}",
            crate::library::model::stable_id("d", &format!("{:?}", std::time::Instant::now()))
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
