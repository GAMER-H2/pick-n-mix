//! Online metadata enrichment.
//!
//! Nothing here runs unless the user explicitly asks for it. MusicBrainz is
//! used because it needs no API key and its data is freely licensed; cover art
//! comes from the Cover Art Archive, which is keyed by the same release ids.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::library::model::{stable_id, Track};

/// MusicBrainz asks that applications identify themselves and stay under one
/// request per second. Both are honoured here.
const USER_AGENT: &str = "PickNMix/0.1.0 ( https://github.com/picknmix )";
const RATE_LIMIT: Duration = Duration::from_millis(1100);
const MUSICBRAINZ: &str = "https://musicbrainz.org/ws/2";
const COVER_ART: &str = "https://coverartarchive.org";

/// What an online lookup managed to find. Every field is optional so a partial
/// match can still fill in the gaps.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Enrichment {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<i32>,
    pub track_number: Option<u32>,
    pub musicbrainz_recording_id: Option<String>,
    pub musicbrainz_release_id: Option<String>,
    pub artwork_id: Option<String>,
    /// How confident the match is, 0..1, as reported by MusicBrainz.
    pub confidence: f32,
}

/// Where metadata can come from. Local tags are the default; this trait is what
/// a Navidrome or Jellyfin provider would implement later.
pub trait MetadataProvider: Send + Sync {
    fn name(&self) -> &str;
    fn lookup(&self, track: &Track) -> Result<Option<Enrichment>>;
}

pub struct MusicBrainz {
    client: reqwest::blocking::Client,
    artwork_dir: std::path::PathBuf,
    last_request: parking_lot::Mutex<Option<std::time::Instant>>,
}

impl MusicBrainz {
    pub fn new(artwork_dir: &Path) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(15))
            .build()
            .context("building the MusicBrainz HTTP client")?;
        Ok(MusicBrainz {
            client,
            artwork_dir: artwork_dir.to_path_buf(),
            last_request: parking_lot::Mutex::new(None),
        })
    }

    /// Block until at least `RATE_LIMIT` has passed since the last call.
    fn throttle(&self) {
        let mut last = self.last_request.lock();
        if let Some(previous) = *last {
            let elapsed = previous.elapsed();
            if elapsed < RATE_LIMIT {
                std::thread::sleep(RATE_LIMIT - elapsed);
            }
        }
        *last = Some(std::time::Instant::now());
    }

    fn search_recording(&self, track: &Track) -> Result<Option<RecordingMatch>> {
        let mut terms = vec![format!("recording:\"{}\"", escape(&track.title))];
        if !track.artist.is_empty() && track.artist != "Unknown Artist" {
            terms.push(format!("artist:\"{}\"", escape(&track.artist)));
        }
        if !track.album.is_empty() && track.album != "Unknown Album" {
            terms.push(format!("release:\"{}\"", escape(&track.album)));
        }
        let query = terms.join(" AND ");

        let url = format!(
            "{MUSICBRAINZ}/recording?query={}&fmt=json&limit=5",
            urlencoding::encode(&query)
        );

        self.throttle();
        let response = self.client.get(&url).send().context("querying MusicBrainz")?;
        if !response.status().is_success() {
            return Err(anyhow!("MusicBrainz returned {}", response.status()));
        }
        let body: RecordingSearch = response.json().context("parsing the MusicBrainz response")?;

        // Prefer a candidate whose duration is close to the file's, which
        // reliably separates a studio cut from a live or extended version.
        let mut best: Option<(f64, RecordingMatch)> = None;
        for candidate in body.recordings {
            let score = score_candidate(&candidate, track);
            if best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
                best = Some((score, candidate));
            }
        }
        Ok(best.map(|(_, c)| c))
    }

    /// Fetch the front cover for a release and store it in the artwork cache.
    fn fetch_cover(&self, release_id: &str) -> Result<Option<String>> {
        let url = format!("{COVER_ART}/release/{release_id}/front-500");
        self.throttle();
        let response = self.client.get(&url).send().context("fetching cover art")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(anyhow!("Cover Art Archive returned {}", response.status()));
        }
        let bytes = response.bytes().context("reading cover art")?;
        if bytes.is_empty() {
            return Ok(None);
        }

        let id = format!("{}.jpg", stable_id("art", release_id));
        std::fs::create_dir_all(&self.artwork_dir).ok();
        std::fs::write(self.artwork_dir.join(&id), &bytes).context("saving cover art")?;
        Ok(Some(id))
    }
}

impl MetadataProvider for MusicBrainz {
    fn name(&self) -> &str {
        "MusicBrainz"
    }

    fn lookup(&self, track: &Track) -> Result<Option<Enrichment>> {
        let Some(found) = self.search_recording(track)? else {
            return Ok(None);
        };

        let release = found.releases.first();
        let release_id = release.map(|r| r.id.clone());
        let artwork_id = match release_id.as_deref() {
            // A missing cover is normal, not an error worth failing the lookup for.
            Some(id) => self.fetch_cover(id).unwrap_or(None),
            None => None,
        };

        Ok(Some(Enrichment {
            title: Some(found.title.clone()),
            artist: found.artist_credit.first().map(|a| a.name.clone()),
            album: release.map(|r| r.title.clone()),
            album_artist: found.artist_credit.first().map(|a| a.name.clone()),
            year: release.and_then(|r| r.date.as_deref()).and_then(year_of),
            track_number: release
                .and_then(|r| r.media.first())
                .and_then(|m| m.track.first())
                .and_then(|t| t.number.as_deref())
                .and_then(|n| n.parse().ok()),
            musicbrainz_recording_id: Some(found.id.clone()),
            musicbrainz_release_id: release_id,
            artwork_id,
            confidence: (found.score as f32 / 100.0).clamp(0.0, 1.0),
        }))
    }
}

/// Apply an enrichment without discarding anything already known.
pub fn apply(track: &mut Track, e: &Enrichment) {
    if let Some(v) = e.title.as_ref().filter(|v| !v.trim().is_empty()) {
        track.title = v.clone();
    }
    if let Some(v) = e.artist.as_ref().filter(|v| !v.trim().is_empty()) {
        track.artist = v.clone();
    }
    if let Some(v) = e.album.as_ref().filter(|v| !v.trim().is_empty()) {
        track.album = v.clone();
    }
    if let Some(v) = e.album_artist.as_ref().filter(|v| !v.trim().is_empty()) {
        track.album_artist = v.clone();
    }
    if track.year.is_none() {
        track.year = e.year;
    }
    if track.track_number.is_none() {
        track.track_number = e.track_number;
    }
    if e.musicbrainz_recording_id.is_some() {
        track.musicbrainz_recording_id = e.musicbrainz_recording_id.clone();
    }
    if e.musicbrainz_release_id.is_some() {
        track.musicbrainz_release_id = e.musicbrainz_release_id.clone();
    }
    // Embedded artwork is usually what the user wants; only fill a gap.
    if track.artwork_id.is_none() {
        track.artwork_id = e.artwork_id.clone();
    }
}

fn score_candidate(candidate: &RecordingMatch, track: &Track) -> f64 {
    let mut score = candidate.score as f64;
    if let Some(length_ms) = candidate.length {
        let theirs = length_ms as f64 / 1000.0;
        let delta = (theirs - track.duration_secs).abs();
        // Within three seconds is effectively the same recording.
        if delta <= 3.0 {
            score += 40.0;
        } else if delta <= 10.0 {
            score += 10.0;
        } else {
            score -= delta.min(60.0);
        }
    }
    score
}

fn year_of(date: &str) -> Option<i32> {
    date.get(..4).and_then(|y| y.parse().ok())
}

/// Lucene special characters would otherwise break the query.
fn escape(s: &str) -> String {
    s.replace('\\', "").replace('"', "").replace(['(', ')', '[', ']', ':', '^', '~'], " ")
}

// -- MusicBrainz response shapes --------------------------------------------

#[derive(Debug, Deserialize)]
struct RecordingSearch {
    #[serde(default)]
    recordings: Vec<RecordingMatch>,
}

#[derive(Debug, Clone, Deserialize)]
struct RecordingMatch {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    score: u32,
    #[serde(default)]
    length: Option<u64>,
    #[serde(default, rename = "artist-credit")]
    artist_credit: Vec<ArtistCredit>,
    #[serde(default)]
    releases: Vec<ReleaseRef>,
}

#[derive(Debug, Clone, Deserialize)]
struct ArtistCredit {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseRef {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    media: Vec<Media>,
}

#[derive(Debug, Clone, Deserialize)]
struct Media {
    #[serde(default)]
    track: Vec<TrackRef>,
}

#[derive(Debug, Clone, Deserialize)]
struct TrackRef {
    #[serde(default)]
    number: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lucene_specials_are_stripped() {
        assert_eq!(escape("Song (Live) [Remix]"), "Song  Live   Remix ");
        assert!(!escape("a\"b").contains('"'));
    }

    #[test]
    fn years_come_off_the_front_of_dates() {
        assert_eq!(year_of("1999-04-01"), Some(1999));
        assert_eq!(year_of("1999"), Some(1999));
        assert_eq!(year_of("??"), None);
    }

    #[test]
    fn a_close_duration_beats_a_higher_raw_score() {
        let track = Track { duration_secs: 200.0, ..Default::default() };
        let close = RecordingMatch {
            id: "a".into(),
            title: String::new(),
            score: 80,
            length: Some(200_000),
            artist_credit: vec![],
            releases: vec![],
        };
        let far = RecordingMatch {
            id: "b".into(),
            title: String::new(),
            score: 95,
            length: Some(400_000),
            artist_credit: vec![],
            releases: vec![],
        };
        assert!(score_candidate(&close, &track) > score_candidate(&far, &track));
    }

    #[test]
    fn enrichment_never_clears_existing_values() {
        let mut track = Track {
            title: "Known Title".into(),
            artwork_id: Some("art_local.jpg".into()),
            ..Default::default()
        };
        apply(
            &mut track,
            &Enrichment {
                title: Some("  ".into()),
                artwork_id: Some("art_remote.jpg".into()),
                ..Default::default()
            },
        );
        assert_eq!(track.title, "Known Title");
        assert_eq!(track.artwork_id.as_deref(), Some("art_local.jpg"));
    }
}
