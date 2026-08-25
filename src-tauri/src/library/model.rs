//! Shared library types.
//!
//! `Track` deliberately carries enough musical identity (title, artist, album,
//! duration, MusicBrainz ids) to be re-found on a different machine, which is
//! what makes shared playlists work.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
}

impl Track {
    /// Key used to re-find this track in someone else's library.
    pub fn match_key(&self) -> String {
        match_key(&self.artist, &self.title, &self.album)
    }
}

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

pub fn match_key(artist: &str, title: &str, album: &str) -> String {
    format!("{}|{}|{}", normalise(artist), normalise(title), normalise(album))
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
        assert_eq!(stable_id("t", "/music/a.flac"), stable_id("t", "/music/a.flac"));
        assert_ne!(stable_id("t", "/music/a.flac"), stable_id("t", "/music/b.flac"));
        assert!(stable_id("t", "x").starts_with("t_"));
    }
}
