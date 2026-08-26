//! SQLite index of the music library.
//!
//! The database is a cache, not the source of truth: local files and playlist
//! files on disk are. That means it can always be deleted and rebuilt.

use std::path::Path;

use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::library::model::{match_key, stable_id, Album, Artist, Track};

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening library database at {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Db {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    /// An in-memory library, used by tests and useful for a scratch session.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Db {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS tracks (
                id                       TEXT PRIMARY KEY,
                source_id                TEXT NOT NULL DEFAULT 'local',
                location                 TEXT NOT NULL,
                title                    TEXT NOT NULL DEFAULT '',
                artist                   TEXT NOT NULL DEFAULT '',
                album_artist             TEXT NOT NULL DEFAULT '',
                album                    TEXT NOT NULL DEFAULT '',
                track_number             INTEGER,
                disc_number              INTEGER,
                year                     INTEGER,
                genre                    TEXT,
                duration_secs            REAL NOT NULL DEFAULT 0,
                sample_rate              INTEGER,
                channels                 INTEGER,
                bits_per_sample          INTEGER,
                bitrate_kbps             INTEGER,
                file_size                INTEGER,
                format                   TEXT,
                artwork_id               TEXT,
                musicbrainz_recording_id TEXT,
                musicbrainz_release_id   TEXT,
                gain_db                  REAL,
                added_at                 INTEGER NOT NULL DEFAULT 0,
                modified_at              INTEGER NOT NULL DEFAULT 0,
                match_key                TEXT NOT NULL DEFAULT '',
                UNIQUE (source_id, location)
            );

            CREATE INDEX IF NOT EXISTS idx_tracks_album     ON tracks (album, disc_number, track_number);
            CREATE INDEX IF NOT EXISTS idx_tracks_artist    ON tracks (album_artist, album);
            CREATE INDEX IF NOT EXISTS idx_tracks_matchkey  ON tracks (match_key);
            CREATE INDEX IF NOT EXISTS idx_tracks_mbid      ON tracks (musicbrainz_recording_id);

            CREATE TABLE IF NOT EXISTS folders (
                path        TEXT PRIMARY KEY,
                added_at    INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    // -- folders ---------------------------------------------------------

    pub fn add_folder(&self, path: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR IGNORE INTO folders (path, added_at) VALUES (?1, ?2)",
            params![path, now()],
        )?;
        Ok(())
    }

    pub fn remove_folder(&self, path: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM folders WHERE path = ?1", params![path])?;
        // Drop the tracks that lived under it.
        conn.execute(
            "DELETE FROM tracks WHERE source_id = 'local' AND location LIKE ?1 || '%'",
            params![path],
        )?;
        Ok(())
    }

    pub fn folders(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT path FROM folders ORDER BY path")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    // -- settings --------------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock();
        let v = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        Ok(v)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // -- tracks ----------------------------------------------------------

    /// Insert or refresh a track. Returns true when the row is new.
    pub fn upsert_track(&self, track: &Track) -> Result<bool> {
        let conn = self.conn.lock();
        let existed: bool = conn
            .query_row(
                "SELECT 1 FROM tracks WHERE source_id = ?1 AND location = ?2",
                params![track.source_id, track.location],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);

        conn.execute(
            r#"
            INSERT INTO tracks (
                id, source_id, location, title, artist, album_artist, album,
                track_number, disc_number, year, genre, duration_secs,
                sample_rate, channels, bits_per_sample, bitrate_kbps, file_size,
                format, artwork_id, musicbrainz_recording_id, musicbrainz_release_id,
                gain_db, added_at, modified_at, match_key
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25
            )
            ON CONFLICT (source_id, location) DO UPDATE SET
                title = excluded.title,
                artist = excluded.artist,
                album_artist = excluded.album_artist,
                album = excluded.album,
                track_number = excluded.track_number,
                disc_number = excluded.disc_number,
                year = excluded.year,
                genre = excluded.genre,
                duration_secs = excluded.duration_secs,
                sample_rate = excluded.sample_rate,
                channels = excluded.channels,
                bits_per_sample = excluded.bits_per_sample,
                bitrate_kbps = excluded.bitrate_kbps,
                file_size = excluded.file_size,
                format = excluded.format,
                artwork_id = COALESCE(excluded.artwork_id, tracks.artwork_id),
                musicbrainz_recording_id =
                    COALESCE(excluded.musicbrainz_recording_id, tracks.musicbrainz_recording_id),
                musicbrainz_release_id =
                    COALESCE(excluded.musicbrainz_release_id, tracks.musicbrainz_release_id),
                gain_db = COALESCE(excluded.gain_db, tracks.gain_db),
                modified_at = excluded.modified_at,
                match_key = excluded.match_key
            "#,
            params![
                track.id,
                track.source_id,
                track.location,
                track.title,
                track.artist,
                track.album_artist,
                track.album,
                track.track_number,
                track.disc_number,
                track.year,
                track.genre,
                track.duration_secs,
                track.sample_rate,
                track.channels,
                track.bits_per_sample,
                track.bitrate_kbps,
                track.file_size.map(|v| v as i64),
                track.format,
                track.artwork_id,
                track.musicbrainz_recording_id,
                track.musicbrainz_release_id,
                track.gain_db,
                track.added_at,
                now(),
                track.match_key(),
            ],
        )?;
        Ok(!existed)
    }

    pub fn get_track(&self, id: &str) -> Result<Option<Track>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!("{TRACK_SELECT} WHERE id = ?1"))?;
        let t = stmt.query_row(params![id], row_to_track).optional()?;
        Ok(t)
    }

    pub fn all_tracks(&self) -> Result<Vec<Track>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "{TRACK_SELECT} ORDER BY album_artist COLLATE NOCASE, album COLLATE NOCASE, \
             disc_number, track_number, title COLLATE NOCASE"
        ))?;
        let rows = stmt.query_map([], row_to_track)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<Track>> {
        let conn = self.conn.lock();
        let like = format!("%{}%", query.trim());
        let mut stmt = conn.prepare(&format!(
            "{TRACK_SELECT} WHERE title LIKE ?1 COLLATE NOCASE
                              OR artist LIKE ?1 COLLATE NOCASE
                              OR album LIKE ?1 COLLATE NOCASE
             ORDER BY title COLLATE NOCASE LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![like, limit], row_to_track)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn tracks_by_album(&self, album_id: &str) -> Result<Vec<Track>> {
        let all = self.all_tracks()?;
        Ok(all
            .into_iter()
            .filter(|t| album_id_for(t) == album_id)
            .collect())
    }

    pub fn tracks_by_artist(&self, artist_id: &str) -> Result<Vec<Track>> {
        let all = self.all_tracks()?;
        Ok(all
            .into_iter()
            .filter(|t| {
                stable_id("ar", &crate::library::model::normalise(&t.album_artist)) == artist_id
                    || stable_id("ar", &crate::library::model::normalise(&t.artist)) == artist_id
            })
            .collect())
    }

    /// Albums are derived from track rows rather than stored, so an edit to a
    /// tag can never leave a stale album behind.
    pub fn albums(&self) -> Result<Vec<Album>> {
        let mut out: Vec<Album> = Vec::new();
        // Album id -> the distinct artists seen on its tracks.
        let mut contributors: Vec<(String, Vec<String>)> = Vec::new();

        for track in self.all_tracks()? {
            // A track with no album tag is a single, not a member of an album.
            if track.album.trim().is_empty() {
                continue;
            }
            let id = album_id_for(&track);
            let who = album_artist_of(&track).to_string();
            // Compared by lead artist, so a differently written guest credit
            // does not turn a single-artist album into a compilation.
            let lead = crate::library::model::lead_artist(&who);
            match contributors.iter_mut().find(|(k, _)| *k == id) {
                Some((_, leads)) => {
                    if !leads.contains(&lead) {
                        leads.push(lead);
                    }
                }
                None => contributors.push((id.clone(), vec![lead])),
            }

            match out.iter_mut().find(|a| a.id == id) {
                Some(album) => {
                    album.track_count += 1;
                    album.duration_secs += track.duration_secs;
                    if album.artwork_id.is_none() {
                        album.artwork_id = track.artwork_id.clone();
                    }
                    if album.year.is_none() {
                        album.year = track.year;
                    }
                }
                None => out.push(Album {
                    id,
                    name: track.album.clone(),
                    artist: who,
                    year: track.year,
                    track_count: 1,
                    duration_secs: track.duration_secs,
                    artwork_id: track.artwork_id.clone(),
                }),
            }
        }
        // An album whose tracks are by several artists is a compilation.
        for album in out.iter_mut() {
            if let Some((_, leads)) = contributors.iter().find(|(k, _)| *k == album.id) {
                if leads.len() > 1 {
                    album.artist = VARIOUS_ARTISTS.to_string();
                }
            }
        }

        out.sort_by(|a, b| {
            a.artist
                .to_lowercase()
                .cmp(&b.artist.to_lowercase())
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(out)
    }

    pub fn artists(&self) -> Result<Vec<Artist>> {
        let mut out: Vec<Artist> = Vec::new();
        let mut seen_albums: Vec<(String, String)> = Vec::new();
        for track in self.all_tracks()? {
            let name = album_artist_of(&track).to_string();
            let id = stable_id("ar", &crate::library::model::normalise(&name));
            let has_album = !track.album.trim().is_empty();
            let album_id = album_id_for(&track);

            match out.iter_mut().find(|a| a.id == id) {
                Some(artist) => {
                    artist.track_count += 1;
                    if has_album
                        && !seen_albums
                            .iter()
                            .any(|(ar, al)| ar == &id && al == &album_id)
                    {
                        artist.album_count += 1;
                        seen_albums.push((id.clone(), album_id));
                    }
                    if artist.artwork_id.is_none() {
                        artist.artwork_id = track.artwork_id.clone();
                    }
                }
                None => {
                    seen_albums.push((id.clone(), album_id));
                    out.push(Artist {
                        id,
                        name,
                        album_count: u32::from(has_album),
                        track_count: 1,
                        artwork_id: track.artwork_id.clone(),
                    });
                }
            }
        }
        out.sort_by_key(|a| a.name.to_lowercase());
        Ok(out)
    }

    /// Resolve a shared playlist entry against this library.
    ///
    /// Tried in descending order of confidence: MusicBrainz id, then the exact
    /// artist/title/album key, then artist and title alone.
    pub fn resolve(
        &self,
        mbid: Option<&str>,
        artist: &str,
        title: &str,
        album: &str,
    ) -> Result<Option<Track>> {
        let conn = self.conn.lock();

        if let Some(mbid) = mbid.filter(|m| !m.is_empty()) {
            let mut stmt = conn.prepare(&format!(
                "{TRACK_SELECT} WHERE musicbrainz_recording_id = ?1"
            ))?;
            if let Some(t) = stmt.query_row(params![mbid], row_to_track).optional()? {
                return Ok(Some(t));
            }
        }

        let key = match_key(artist, title, album);
        let mut stmt = conn.prepare(&format!("{TRACK_SELECT} WHERE match_key = ?1"))?;
        if let Some(t) = stmt.query_row(params![key], row_to_track).optional()? {
            return Ok(Some(t));
        }

        // Same song from a different release still counts as a match.
        let prefix = format!(
            "{}|{}|%",
            crate::library::model::normalise(artist),
            crate::library::model::normalise(title)
        );
        let mut stmt = conn.prepare(&format!("{TRACK_SELECT} WHERE match_key LIKE ?1 LIMIT 1"))?;
        Ok(stmt.query_row(params![prefix], row_to_track).optional()?)
    }

    /// Locations currently indexed for a source, so the scanner can spot
    /// files that have been deleted.
    pub fn locations(&self, source_id: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT location FROM tracks WHERE source_id = ?1")?;
        let rows = stmt.query_map(params![source_id], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn delete_track_at(&self, source_id: &str, location: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM tracks WHERE source_id = ?1 AND location = ?2",
            params![source_id, location],
        )?;
        Ok(())
    }

    pub fn track_count(&self) -> Result<u32> {
        let conn = self.conn.lock();
        Ok(conn.query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get::<_, u32>(0))?)
    }
}

/// Who to show for a track, falling back to the performing artist.
pub fn album_artist_of(track: &Track) -> &str {
    if track.album_artist.trim().is_empty() {
        &track.artist
    } else {
        &track.album_artist
    }
}

/// Identity of the album a track belongs to.
///
/// When the file carries a real album-artist tag, that plus the album name
/// identifies it. When it does not, the album name alone does: a compilation
/// has a different artist on every track, and keying on the performing artist
/// would split it into one album per song.
///
/// The trade-off is that two untagged albums that share a title would merge.
/// That is far rarer than the split it prevents.
pub fn album_id_for(track: &Track) -> String {
    let album = crate::library::model::normalise(&track.album);
    let tagged_artist = track.album_artist.trim();
    if tagged_artist.is_empty() {
        stable_id("al", &album)
    } else {
        stable_id(
            "al",
            &format!(
                "{}|{}",
                crate::library::model::normalise(tagged_artist),
                album
            ),
        )
    }
}

/// Shown when an album's tracks are by more than one artist.
pub const VARIOUS_ARTISTS: &str = "Various Artists";

const TRACK_SELECT: &str = r#"
SELECT id, source_id, location, title, artist, album_artist, album, track_number,
       disc_number, year, genre, duration_secs, sample_rate, channels,
       bits_per_sample, bitrate_kbps, file_size, format, artwork_id,
       musicbrainz_recording_id, musicbrainz_release_id, gain_db, added_at
FROM tracks
"#;

fn row_to_track(row: &Row<'_>) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get(0)?,
        source_id: row.get(1)?,
        location: row.get(2)?,
        title: row.get(3)?,
        artist: row.get(4)?,
        album_artist: row.get(5)?,
        album: row.get(6)?,
        track_number: row.get(7)?,
        disc_number: row.get(8)?,
        year: row.get(9)?,
        genre: row.get(10)?,
        duration_secs: row.get(11)?,
        sample_rate: row.get(12)?,
        channels: row.get(13)?,
        bits_per_sample: row.get(14)?,
        bitrate_kbps: row.get(15)?,
        file_size: row.get::<_, Option<i64>>(16)?.map(|v| v as u64),
        format: row.get(17)?,
        artwork_id: row.get(18)?,
        musicbrainz_recording_id: row.get(19)?,
        musicbrainz_release_id: row.get(20)?,
        gain_db: row.get(21)?,
        added_at: row.get(22)?,
    })
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str, artist: &str, album: &str, path: &str) -> Track {
        Track {
            id: stable_id("t", path),
            source_id: "local".into(),
            location: path.into(),
            title: title.into(),
            artist: artist.into(),
            album_artist: artist.into(),
            album: album.into(),
            duration_secs: 180.0,
            added_at: now(),
            ..Default::default()
        }
    }

    #[test]
    fn upsert_reports_new_rows_then_updates() {
        let db = Db::open_in_memory().unwrap();
        let mut t = track("Song", "Artist", "Album", "/m/a.flac");
        assert!(db.upsert_track(&t).unwrap());
        t.title = "Song (Remaster)".into();
        assert!(!db.upsert_track(&t).unwrap());
        assert_eq!(db.track_count().unwrap(), 1);
        assert_eq!(
            db.get_track(&t.id).unwrap().unwrap().title,
            "Song (Remaster)"
        );
    }

    #[test]
    fn resolve_finds_tracks_despite_formatting_differences() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track(
            "Come Together",
            "The Beatles",
            "Abbey Road",
            "/m/1.flac",
        ))
        .unwrap();

        let hit = db
            .resolve(None, "the beatles", "COME TOGETHER", "abbey  road")
            .unwrap();
        assert!(hit.is_some(), "should match on the normalised key");

        // Different album still resolves through the artist/title fallback.
        let hit = db
            .resolve(None, "The Beatles", "Come Together", "Some Compilation")
            .unwrap();
        assert!(hit.is_some(), "should fall back to artist and title");

        assert!(db
            .resolve(None, "Nobody", "Nothing", "Nowhere")
            .unwrap()
            .is_none());
    }

    #[test]
    fn resolve_prefers_the_musicbrainz_id() {
        let db = Db::open_in_memory().unwrap();
        let mut a = track("Wrong Name", "Wrong Artist", "Wrong Album", "/m/1.flac");
        a.musicbrainz_recording_id = Some("mbid-123".into());
        db.upsert_track(&a).unwrap();
        db.upsert_track(&track("Right", "Right", "Right", "/m/2.flac"))
            .unwrap();

        let hit = db
            .resolve(Some("mbid-123"), "Right", "Right", "Right")
            .unwrap()
            .unwrap();
        assert_eq!(hit.location, "/m/1.flac");
    }

    #[test]
    fn albums_and_artists_are_grouped_from_tracks() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track("A", "Artist", "Album One", "/m/1.flac"))
            .unwrap();
        db.upsert_track(&track("B", "Artist", "Album One", "/m/2.flac"))
            .unwrap();
        db.upsert_track(&track("C", "Artist", "Album Two", "/m/3.flac"))
            .unwrap();

        let albums = db.albums().unwrap();
        assert_eq!(albums.len(), 2);
        assert_eq!(
            albums
                .iter()
                .find(|a| a.name == "Album One")
                .unwrap()
                .track_count,
            2
        );

        let artists = db.artists().unwrap();
        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0].album_count, 2);
        assert_eq!(artists[0].track_count, 3);
    }

    #[test]
    fn removing_a_folder_removes_its_tracks() {
        let db = Db::open_in_memory().unwrap();
        db.add_folder("/m").unwrap();
        db.upsert_track(&track("A", "Artist", "Album", "/m/1.flac"))
            .unwrap();
        db.upsert_track(&track("B", "Artist", "Album", "/other/2.flac"))
            .unwrap();
        db.remove_folder("/m").unwrap();
        assert_eq!(db.track_count().unwrap(), 1);
    }
}

#[cfg(test)]
mod albumless_tests {
    use super::*;

    fn single(title: &str, artist: &str, path: &str) -> Track {
        Track {
            id: stable_id("t", path),
            source_id: "local".into(),
            location: path.into(),
            title: title.into(),
            artist: artist.into(),
            album_artist: artist.into(),
            album: String::new(),
            duration_secs: 180.0,
            ..Default::default()
        }
    }

    #[test]
    fn a_track_with_no_album_tag_produces_no_album() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&single("Loose Track", "Someone", "/m/loose.mp3"))
            .unwrap();

        assert!(
            db.albums().unwrap().is_empty(),
            "an albumless track invented an album"
        );
        // The track itself is still in the library.
        assert_eq!(db.all_tracks().unwrap().len(), 1);
    }

    #[test]
    fn albumless_tracks_do_not_inflate_an_artists_album_count() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&single("Loose One", "Someone", "/m/a.mp3"))
            .unwrap();
        db.upsert_track(&single("Loose Two", "Someone", "/m/b.mp3"))
            .unwrap();

        let artists = db.artists().unwrap();
        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0].track_count, 2);
        assert_eq!(artists[0].album_count, 0, "singles are not albums");
    }

    #[test]
    fn albumless_tracks_from_different_artists_are_not_lumped_together() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&single("One", "Artist A", "/m/a.mp3"))
            .unwrap();
        db.upsert_track(&single("Two", "Artist B", "/m/b.mp3"))
            .unwrap();

        // The old "Unknown Album" behaviour would have merged these.
        assert!(db.albums().unwrap().is_empty());
        assert_eq!(db.artists().unwrap().len(), 2);
    }

    #[test]
    fn an_artist_with_both_an_album_and_singles_counts_only_the_album() {
        let db = Db::open_in_memory().unwrap();
        let mut on_album = single("On Album", "Someone", "/m/album.flac");
        on_album.album = "Real Album".into();
        db.upsert_track(&on_album).unwrap();
        db.upsert_track(&single("Single", "Someone", "/m/single.mp3"))
            .unwrap();

        let artists = db.artists().unwrap();
        assert_eq!(artists[0].album_count, 1);
        assert_eq!(artists[0].track_count, 2);
        assert_eq!(db.albums().unwrap().len(), 1);
    }
}

#[cfg(test)]
mod compilation_tests {
    use super::*;

    fn comp_track(title: &str, artist: &str, album: &str, album_artist: &str, path: &str) -> Track {
        Track {
            id: stable_id("t", path),
            source_id: "local".into(),
            location: path.into(),
            title: title.into(),
            artist: artist.into(),
            album_artist: album_artist.into(),
            album: album.into(),
            duration_secs: 200.0,
            ..Default::default()
        }
    }

    /// A soundtrack with a different artist on every track and no album-artist
    /// tag used to become one album per song.
    #[test]
    fn a_compilation_with_no_album_artist_tag_stays_one_album() {
        let db = Db::open_in_memory().unwrap();
        for (i, artist) in ["Mike Shinoda", "Freya Ridings", "Marcus King"]
            .iter()
            .enumerate()
        {
            db.upsert_track(&comp_track(
                &format!("Track {i}"),
                artist,
                "Arcane Season Two",
                "",
                &format!("/m/arcane/{i}.flac"),
            ))
            .unwrap();
        }

        let albums = db.albums().unwrap();
        assert_eq!(albums.len(), 1, "the soundtrack split into separate albums");
        assert_eq!(albums[0].track_count, 3);
        assert_eq!(albums[0].artist, VARIOUS_ARTISTS);
    }

    #[test]
    fn a_real_album_artist_tag_still_separates_same_titled_albums() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&comp_track(
            "A",
            "Band One",
            "Greatest Hits",
            "Band One",
            "/m/1.flac",
        ))
        .unwrap();
        db.upsert_track(&comp_track(
            "B",
            "Band Two",
            "Greatest Hits",
            "Band Two",
            "/m/2.flac",
        ))
        .unwrap();

        let albums = db.albums().unwrap();
        assert_eq!(
            albums.len(),
            2,
            "tagged album artists must keep albums apart"
        );
    }

    #[test]
    fn an_album_by_one_artist_keeps_that_artists_name() {
        let db = Db::open_in_memory().unwrap();
        for i in 0..3 {
            db.upsert_track(&comp_track(
                &format!("Track {i}"),
                "One Band",
                "Their Album",
                "",
                &format!("/m/one/{i}.flac"),
            ))
            .unwrap();
        }

        let albums = db.albums().unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(
            albums[0].artist, "One Band",
            "not a compilation, so not Various Artists"
        );
    }

    #[test]
    fn every_track_of_a_compilation_resolves_to_the_same_album_page() {
        let db = Db::open_in_memory().unwrap();
        for (i, artist) in ["A", "B", "C"].iter().enumerate() {
            db.upsert_track(&comp_track(
                &format!("T{i}"),
                artist,
                "Comp",
                "",
                &format!("/m/comp/{i}.flac"),
            ))
            .unwrap();
        }
        let album_id = db.albums().unwrap()[0].id.clone();
        assert_eq!(db.tracks_by_album(&album_id).unwrap().len(), 3);
    }

    #[test]
    fn a_guest_feature_does_not_fragment_an_album() {
        let db = Db::open_in_memory().unwrap();
        // Same album artist, different performing artists: one album.
        db.upsert_track(&comp_track(
            "Solo",
            "TWRP",
            "A Human's Touch",
            "TWRP",
            "/m/t1.flac",
        ))
        .unwrap();
        db.upsert_track(&comp_track(
            "Duet",
            "TWRP feat. McKenna Rae",
            "A Human's Touch",
            "TWRP",
            "/m/t2.flac",
        ))
        .unwrap();

        let albums = db.albums().unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].artist, "TWRP");
    }
}
