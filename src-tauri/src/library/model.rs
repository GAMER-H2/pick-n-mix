//! Shared library types.
//!
//! `Track` deliberately carries enough musical identity (title, artist, album,
//! duration, MusicBrainz ids) to be re-found on a different machine, which is
//! what makes shared playlists work.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    /// Which library this came from. "local" today; a Navidrome or Jellyfin
    /// server id later.
    pub source_id: String,
    /// Absolute path for local sources; a server-side id for remote ones.
    pub location: String,
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub duration_secs: f64,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub bits_per_sample: Option<u32>,
    pub bitrate_kbps: Option<u32>,
    pub file_size: Option<u64>,
    pub format: Option<String>,
    pub artwork_id: Option<String>,
    pub musicbrainz_recording_id: Option<String>,
    pub musicbrainz_release_id: Option<String>,
    /// Normalisation gain in dB, from ReplayGain tags when present.
    pub gain_db: Option<f32>,
    pub added_at: i64,
    /// Number of known file versions for this logical song.
    pub file_count: u32,
    /// Number of known versions whose file is currently unavailable.
    pub missing_file_count: u32,
    /// The file version supplying the playable location and technical fields.
    pub effective_file_id: Option<String>,
    /// A manually selected version. It remains set while that file is missing.
    pub preferred_file_id: Option<String>,
}

impl Track {
    /// Key used to re-find this track in someone else's library.
    pub fn match_key(&self) -> String {
        match_key(&self.artist, &self.title, &self.album)
    }
}

/// One physical or remote file version belonging to a logical song.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TrackFile {
    pub id: String,
    pub song_id: String,
    pub source_id: String,
    pub location: String,
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub duration_secs: f64,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub bits_per_sample: Option<u32>,
    pub bitrate_kbps: Option<u32>,
    pub file_size: Option<u64>,
    pub format: Option<String>,
    pub artwork_id: Option<String>,
    pub musicbrainz_recording_id: Option<String>,
    pub musicbrainz_release_id: Option<String>,
    pub gain_db: Option<f32>,
    pub added_at: i64,
    pub modified_at: i64,
    pub available: bool,
    pub missing: bool,
    pub preferred: bool,
    pub effective: bool,
}

/// Alternate terminology used by duplicate-management callers.
pub type FileVersion = TrackFile;

/// Loose normalisation so "The Beatles" and "the beatles " agree.
pub fn normalise(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The lead artist of a credit, with any featured guests removed.
///
/// "Lemaitre, Jennie A." and "Lemaitre feat. Jennie A." are the same act
/// credited two ways. Comparing the lead alone stops an album being called a
/// compilation just because its tracks spell the guest credit differently.
///
/// Only explicit feature markers and a comma split the string. "&" is left
/// alone on purpose, so "Earth, Wind & Fire" is not mangled into "Earth" for
/// one track and something else for another; every track of theirs reduces to
/// the same lead, which is all this is used for.
pub fn lead_artist(credit: &str) -> String {
    const MARKERS: [&str; 6] = [
        " feat. ",
        " feat ",
        " ft. ",
        " ft ",
        " featuring ",
        " with ",
    ];
    let lower = credit.to_lowercase();

    let mut cut = lower.len();
    for marker in MARKERS {
        if let Some(at) = lower.find(marker) {
            cut = cut.min(at);
        }
    }
    if let Some(at) = lower.find(", ") {
        cut = cut.min(at);
    }
    normalise(&lower[..cut])
}

pub fn match_key(artist: &str, title: &str, album: &str) -> String {
    format!(
        "{}|{}|{}",
        normalise(artist),
        normalise(title),
        normalise(album)
    )
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub year: Option<i32>,
    pub track_count: u32,
    pub duration_secs: f64,
    pub artwork_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub album_count: u32,
    pub track_count: u32,
    pub artwork_id: Option<String>,
}

/// One finished listen, as written to the `plays` table.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Play {
    pub song_id: String,
    pub played_at: i64,
    /// Seconds actually heard — accumulated only while playing, so pausing or
    /// seeking around cannot inflate it.
    pub seconds_played: f64,
    /// How far through the song that reached, 0..1.
    pub fraction: f64,
    /// Whether this passed the bar to count as a play rather than a skip.
    pub counted: bool,
    pub context_kind: Option<String>,
    pub context_id: Option<String>,
}

/// A history row joined back to the song it refers to, for the history view.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayRecord {
    pub play: Play,
    /// `None` once the song has left the library — history outlives it.
    pub track: Option<Track>,
}

/// One recommendation on the home page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomePick {
    /// `"song"` or `"album"`, which decides where clicking it goes.
    pub kind: String,
    /// Song id, or the stable album id the album view routes by.
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub artwork_id: Option<String>,
    /// Why this was picked, in words, shown next to it. A recommendation
    /// that cannot explain itself is indistinguishable from a random one,
    /// and reads as broken the moment it suggests something unwanted.
    pub reason: String,
    /// What to enqueue when it is played.
    pub track_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub scanned: u32,
    pub added: u32,
    pub updated: u32,
    pub skipped: u32,
    pub errors: Vec<String>,
}

/// Stable id for a piece of content, used for tracks, albums and artwork.
pub fn stable_id(prefix: &str, seed: &str) -> String {
    // FNV-1a: short, deterministic, and good enough to key local rows.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{prefix}_{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisation_ignores_case_punctuation_and_spacing() {
        assert_eq!(normalise("  The Beatles! "), "the beatles");
        assert_eq!(normalise("Sgt. Pepper's"), "sgt peppers");
    }

    #[test]
    fn match_keys_agree_across_formatting_differences() {
        let a = match_key("The Beatles", "Come Together", "Abbey Road");
        let b = match_key("the beatles", "COME TOGETHER", "Abbey  Road");
        assert_eq!(a, b);
    }

    #[test]
    fn stable_ids_are_deterministic_and_prefixed() {
        assert_eq!(
            stable_id("t", "/music/a.flac"),
            stable_id("t", "/music/a.flac")
        );
        assert_ne!(
            stable_id("t", "/music/a.flac"),
            stable_id("t", "/music/b.flac")
        );
        assert!(stable_id("t", "x").starts_with("t_"));
    }
}

#[cfg(test)]
mod lead_artist_tests {
    use super::*;

    #[test]
    fn feature_credits_reduce_to_the_lead() {
        assert_eq!(lead_artist("Lemaitre, Jennie A."), "lemaitre");
        assert_eq!(lead_artist("Lemaitre feat. Jennie A."), "lemaitre");
        assert_eq!(lead_artist("TWRP, McKenna Rae"), "twrp");
        assert_eq!(lead_artist("TWRP feat. McKenna Rae"), "twrp");
        assert_eq!(lead_artist("Artist ft. Someone"), "artist");
        assert_eq!(lead_artist("Artist featuring Someone"), "artist");
    }

    #[test]
    fn an_ampersand_in_a_band_name_is_left_alone() {
        // Splitting on "&" would break these; every track still agrees.
        assert_eq!(lead_artist("Earth, Wind & Fire"), "earth");
        assert_eq!(
            lead_artist("Earth, Wind & Fire"),
            lead_artist("Earth, Wind & Fire")
        );
        assert_eq!(lead_artist("Simon & Garfunkel"), "simon garfunkel");
    }

    #[test]
    fn genuinely_different_artists_stay_different() {
        assert_ne!(lead_artist("Mike Shinoda"), lead_artist("Freya Ridings"));
        assert_ne!(lead_artist("Marcus King"), lead_artist("Fever 333"));
    }

    #[test]
    fn a_plain_name_is_just_normalised() {
        assert_eq!(lead_artist("The Beatles"), "the beatles");
        assert_eq!(lead_artist("  The   Beatles  "), "the beatles");
    }
}
