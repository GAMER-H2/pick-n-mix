//! Library sources.
//!
//! Only the local filesystem is implemented today, but everything above this
//! trait works in terms of `Track` rather than file paths, so a Navidrome or
//! Jellyfin source can be added without touching the UI or the player.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::library::model::Track;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    Local,
    Navidrome,
    Jellyfin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceConfig {
    pub id: String,
    pub kind: SourceKind,
    pub name: String,
    /// Watched folders for a local source.
    #[serde(default)]
    pub folders: Vec<String>,
    /// Base URL for a remote source.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

/// How the player gets at the audio for a track.
pub enum Playable {
    /// A file on this machine, handed straight to the decoder.
    LocalFile(std::path::PathBuf),
    /// A URL to stream. Not wired up yet; remote sources will return this.
    Stream(String),
}

pub trait LibrarySource: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> SourceKind;

    /// Re-read everything this source offers.
    fn sync(&self) -> Result<Vec<Track>>;

    /// Turn a track into something the audio engine can open.
    fn playable(&self, track: &Track) -> Result<Playable>;
}

pub struct LocalSource {
    id: String,
}

impl LocalSource {
    pub fn new() -> Self {
        LocalSource {
            id: crate::library::scan::SOURCE_LOCAL.to_string(),
        }
    }
}

impl LibrarySource for LocalSource {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> SourceKind {
        SourceKind::Local
    }

    fn sync(&self) -> Result<Vec<Track>> {
        // Local scanning is driven by `scan::scan_folders`, which writes to the
        // database directly so it can report progress as it goes.
        Ok(Vec::new())
    }

    fn playable(&self, track: &Track) -> Result<Playable> {
        Ok(Playable::LocalFile(std::path::PathBuf::from(
            &track.location,
        )))
    }
}
