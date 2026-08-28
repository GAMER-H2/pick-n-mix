//! SQLite index of the music library.
//!
//! Logical songs are stable public records. Physical files are version rows that
//! can be ranked, preferred, marked missing, relinked, or forgotten independently.

use std::cmp::Ordering;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use crate::library::model::{match_key, normalise, stable_id, Album, Artist, Track, TrackFile};

const SCHEMA_VERSION: u32 = 1;

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
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Db {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let mut conn = self.conn.lock();
        let version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            bail!(
                "library database schema {version} is newer than supported version {SCHEMA_VERSION}"
            );
        }
        if version == SCHEMA_VERSION {
            return Ok(());
        }

        let tx = conn.transaction()?;
        tx.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS folders (
                path     TEXT PRIMARY KEY,
                added_at INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;

        let has_legacy_tracks = table_exists(&tx, "tracks")?;
        if has_legacy_tracks {
            tx.execute_batch("ALTER TABLE tracks RENAME TO legacy_tracks;")?;
        }
        create_current_schema(&tx)?;

        if has_legacy_tracks {
            let legacy = {
                let mut stmt = tx.prepare(&format!(
                    "{LEGACY_TRACK_SELECT} FROM legacy_tracks ORDER BY id"
                ))?;
                let rows = stmt.query_map([], row_to_legacy_track)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            for track in legacy {
                upsert_track_tx(&tx, &track)?;
            }
            tx.execute_batch("DROP TABLE legacy_tracks;")?;
        }

        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
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
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM folders WHERE path = ?1", params![path])?;

        let root = Path::new(path);
        let versions = {
            let mut stmt = tx.prepare(
                "SELECT id, song_id, location FROM track_files WHERE source_id = 'local'",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut affected = Vec::new();
        for (file_id, song_id, location) in versions {
            if !Path::new(&location).starts_with(root) {
                continue;
            }
            tx.execute(
                "DELETE FROM track_aliases WHERE alias_id = ?1",
                params![file_id],
            )?;
            tx.execute(
                "UPDATE songs SET preferred_file_id = NULL
                 WHERE id = ?1 AND preferred_file_id = ?2",
                params![song_id, file_id],
            )?;
            tx.execute("DELETE FROM track_files WHERE id = ?1", params![file_id])?;
            if !affected.contains(&song_id) {
                affected.push(song_id);
            }
        }
        for song_id in affected {
            recompute_song(&tx, &song_id)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn folders(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT path FROM folders ORDER BY path")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.filter_map(|row| row.ok()).collect())
    }

    // -- settings --------------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock();
        Ok(conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
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

    // -- collapsed songs -------------------------------------------------

    /// Insert or refresh one file candidate. Returns true when the file row is new.
    /// A known source/location always keeps its existing song membership.
    pub fn upsert_track(&self, track: &Track) -> Result<bool> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let added = upsert_track_tx(&tx, track)?;
        tx.commit()?;
        Ok(added)
    }

    /// Fetch a logical song by its stable song id or any historical file id alias.
    pub fn get_track(&self, id: &str) -> Result<Option<Track>> {
        let conn = self.conn.lock();
        let Some(song_id) = resolve_song_id(&conn, id)? else {
            return Ok(None);
        };
        let mut stmt = conn.prepare(&format!("{TRACK_SELECT} WHERE s.id = ?1"))?;
        Ok(stmt.query_row(params![song_id], row_to_track).optional()?)
    }

    pub fn all_tracks(&self) -> Result<Vec<Track>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "{TRACK_SELECT} ORDER BY s.album_artist COLLATE NOCASE, s.album COLLATE NOCASE, \
             s.disc_number, s.track_number, s.title COLLATE NOCASE, s.id"
        ))?;
        let rows = stmt.query_map([], row_to_track)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<Track>> {
        let conn = self.conn.lock();
        let like = format!("%{}%", query.trim());
        let mut stmt = conn.prepare(&format!(
            "{TRACK_SELECT} WHERE s.title LIKE ?1 COLLATE NOCASE
                              OR s.artist LIKE ?1 COLLATE NOCASE
                              OR s.album LIKE ?1 COLLATE NOCASE
             ORDER BY s.title COLLATE NOCASE, s.id LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![like, limit], row_to_track)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn tracks_by_album(&self, album_id: &str) -> Result<Vec<Track>> {
        Ok(self
            .all_tracks()?
            .into_iter()
            .filter(|track| album_id_for(track) == album_id)
            .collect())
    }

    pub fn tracks_by_artist(&self, artist_id: &str) -> Result<Vec<Track>> {
        Ok(self
            .all_tracks()?
            .into_iter()
            .filter(|track| {
                stable_id("ar", &normalise(&track.album_artist)) == artist_id
                    || stable_id("ar", &normalise(&track.artist)) == artist_id
            })
            .collect())
    }

    /// Albums are derived from collapsed songs so file duplicates do not inflate counts.
    pub fn albums(&self) -> Result<Vec<Album>> {
        let mut out: Vec<Album> = Vec::new();
        let mut contributors: Vec<(String, Vec<String>)> = Vec::new();

        for track in self.all_tracks()? {
            if track.album.trim().is_empty() {
                continue;
            }
            let id = album_id_for(&track);
            let who = album_artist_of(&track).to_string();
            let lead = crate::library::model::lead_artist(&who);
            match contributors.iter_mut().find(|(key, _)| *key == id) {
                Some((_, leads)) => {
                    if !leads.contains(&lead) {
                        leads.push(lead);
                    }
                }
                None => contributors.push((id.clone(), vec![lead])),
            }

            match out.iter_mut().find(|album| album.id == id) {
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
        for album in &mut out {
            if contributors
                .iter()
                .find(|(key, _)| key == &album.id)
                .map(|(_, leads)| leads.len() > 1)
                .unwrap_or(false)
            {
                album.artist = VARIOUS_ARTISTS.to_string();
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
            let id = stable_id("ar", &normalise(&name));
            let has_album = !track.album.trim().is_empty();
            let album_id = album_id_for(&track);
            match out.iter_mut().find(|artist| artist.id == id) {
                Some(artist) => {
                    artist.track_count += 1;
                    if has_album
                        && !seen_albums
                            .iter()
                            .any(|(artist_id, seen)| artist_id == &id && seen == &album_id)
                    {
                        artist.album_count += 1;
                        seen_albums.push((id.clone(), album_id));
                    }
                    if artist.artwork_id.is_none() {
                        artist.artwork_id = track.artwork_id.clone();
                    }
                }
                None => {
                    if has_album {
                        seen_albums.push((id.clone(), album_id));
                    }
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
        out.sort_by_key(|artist| artist.name.to_lowercase());
        Ok(out)
    }

    /// Resolve a playlist identity without ever crossing album boundaries.
    pub fn resolve(
        &self,
        mbid: Option<&str>,
        artist: &str,
        title: &str,
        album: &str,
    ) -> Result<Option<Track>> {
        let album_key = normalise(album);
        let wanted_mbid = mbid.map(str::trim).filter(|value| !value.is_empty());
        let wanted_key = match_key(artist, title, album);
        let mut tracks = self.all_tracks()?;
        tracks.sort_by(|a, b| a.id.cmp(&b.id));

        if let Some(wanted_mbid) = wanted_mbid {
            if let Some(track) = tracks.iter().find(|track| {
                track
                    .musicbrainz_recording_id
                    .as_deref()
                    .map(str::trim)
                    .map(|value| value.eq_ignore_ascii_case(wanted_mbid))
                    .unwrap_or(false)
                    && normalise(&track.album) == album_key
            }) {
                return Ok(Some(track.clone()));
            }
        }
        Ok(tracks
            .into_iter()
            .find(|track| track.match_key() == wanted_key))
    }

    // -- file versions ---------------------------------------------------

    /// All versions for a song, accepting a song id or file alias.
    pub fn files_for_song(&self, song_or_alias_id: &str) -> Result<Vec<TrackFile>> {
        let conn = self.conn.lock();
        let Some(song_id) = resolve_song_id(&conn, song_or_alias_id)? else {
            return Ok(Vec::new());
        };
        files_for_song_conn(&conn, &song_id)
    }

    /// Available versions in automatic quality order (best first).
    pub fn ranked_available_files(&self, song_or_alias_id: &str) -> Result<Vec<TrackFile>> {
        let mut files: Vec<TrackFile> = self
            .files_for_song(song_or_alias_id)?
            .into_iter()
            .filter(|file| file.available)
            .collect();
        files.sort_by(compare_file_rank);
        Ok(files)
    }

    /// The currently effective playable version, if any.
    pub fn effective_file_for_song(&self, song_or_alias_id: &str) -> Result<Option<TrackFile>> {
        Ok(self
            .files_for_song(song_or_alias_id)?
            .into_iter()
            .find(|file| file.effective && file.available))
    }

    /// Compatibility song DTO only when it has an effective playable version.
    pub fn playable_track(&self, song_or_alias_id: &str) -> Result<Option<Track>> {
        Ok(self
            .get_track(song_or_alias_id)?
            .filter(|track| track.effective_file_id.is_some()))
    }

    /// Set a manual preferred version, or `None` for automatic ranking.
    /// Missing versions are valid preferences and remain stored for reappearance.
    pub fn set_preferred_file(&self, song_or_alias_id: &str, file_id: Option<&str>) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let song_id = resolve_song_id(&tx, song_or_alias_id)?
            .ok_or_else(|| anyhow!("song not found: {song_or_alias_id}"))?;
        if let Some(file_id) = file_id {
            let belongs: bool = tx
                .query_row(
                    "SELECT 1 FROM track_files WHERE id = ?1 AND song_id = ?2",
                    params![file_id, song_id],
                    |_| Ok(true),
                )
                .optional()?
                .unwrap_or(false);
            if !belongs {
                bail!("file {file_id} does not belong to song {song_id}");
            }
        }
        tx.execute(
            "UPDATE songs SET preferred_file_id = ?2 WHERE id = ?1",
            params![song_id, file_id],
        )?;
        recompute_song(&tx, &song_id)?;
        tx.commit()?;
        Ok(())
    }

    /// Find one exact physical version, including a missing version.
    pub fn file_by_id(&self, file_id: &str) -> Result<Option<TrackFile>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!("{FILE_SELECT} WHERE f.id = ?1"))?;
        Ok(stmt
            .query_row(params![file_id], row_to_track_file)
            .optional()?)
    }

    /// Find a version by its source-specific location, including missing versions.
    pub fn file_by_location(&self, source_id: &str, location: &str) -> Result<Option<TrackFile>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "{FILE_SELECT} WHERE f.source_id = ?1 AND f.location = ?2"
        ))?;
        Ok(stmt
            .query_row(params![source_id, location], row_to_track_file)
            .optional()?)
    }

    /// Mark one known version unavailable. Returns false when the location is unknown.
    pub fn mark_file_missing(&self, source_id: &str, location: &str) -> Result<bool> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let song_id: Option<String> = tx
            .query_row(
                "SELECT song_id FROM track_files WHERE source_id = ?1 AND location = ?2",
                params![source_id, location],
                |row| row.get(0),
            )
            .optional()?;
        let Some(song_id) = song_id else {
            tx.commit()?;
            return Ok(false);
        };
        tx.execute(
            "UPDATE track_files SET available = 0, modified_at = ?3
             WHERE source_id = ?1 AND location = ?2",
            params![source_id, location, now()],
        )?;
        recompute_song(&tx, &song_id)?;
        tx.commit()?;
        Ok(true)
    }

    /// Relink and refresh an existing version without changing its file id or song.
    pub fn relink_file(&self, file_id: &str, replacement: &Track) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let song_id: String = tx
            .query_row(
                "SELECT song_id FROM track_files WHERE id = ?1",
                params![file_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| anyhow!("file not found: {file_id}"))?;
        write_file_fields(&tx, file_id, replacement, true)?;
        if !replacement.id.trim().is_empty() {
            tx.execute(
                "INSERT OR IGNORE INTO track_aliases (alias_id, song_id) VALUES (?1, ?2)",
                params![replacement.id, song_id],
            )?;
        }
        recompute_song(&tx, &song_id)?;
        tx.commit()?;
        Ok(())
    }

    /// Permanently forget a version record. Returns false when it did not exist.
    pub fn forget_file(&self, file_id: &str) -> Result<bool> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let song_id: Option<String> = tx
            .query_row(
                "SELECT song_id FROM track_files WHERE id = ?1",
                params![file_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(song_id) = song_id else {
            tx.commit()?;
            return Ok(false);
        };
        tx.execute(
            "DELETE FROM track_aliases WHERE alias_id = ?1",
            params![file_id],
        )?;
        tx.execute(
            "UPDATE songs SET preferred_file_id = NULL
             WHERE id = ?1 AND preferred_file_id = ?2",
            params![song_id, file_id],
        )?;
        tx.execute("DELETE FROM track_files WHERE id = ?1", params![file_id])?;
        recompute_song(&tx, &song_id)?;
        tx.commit()?;
        Ok(true)
    }

    /// Locations currently indexed for a source, including missing versions.
    pub fn locations(&self, source_id: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT location FROM track_files WHERE source_id = ?1 ORDER BY location")?;
        let rows = stmt.query_map(params![source_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Legacy hard-delete API. New scanner code should use `mark_file_missing`.
    pub fn delete_track_at(&self, source_id: &str, location: &str) -> Result<()> {
        if let Some(file) = self.file_by_location(source_id, location)? {
            self.forget_file(&file.id)?;
        }
        Ok(())
    }

    /// Compatibility count: public logical songs, not physical files.
    pub fn track_count(&self) -> Result<u32> {
        self.song_count()
    }

    pub fn song_count(&self) -> Result<u32> {
        let conn = self.conn.lock();
        Ok(conn.query_row("SELECT COUNT(*) FROM songs", [], |row| row.get(0))?)
    }

    pub fn file_count(&self) -> Result<u32> {
        let conn = self.conn.lock();
        Ok(conn.query_row("SELECT COUNT(*) FROM track_files", [], |row| row.get(0))?)
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
pub fn album_id_for(track: &Track) -> String {
    let album = normalise(&track.album);
    let tagged_artist = track.album_artist.trim();
    if tagged_artist.is_empty() {
        stable_id("al", &album)
    } else {
        stable_id("al", &format!("{}|{}", normalise(tagged_artist), album))
    }
}

pub const VARIOUS_ARTISTS: &str = "Various Artists";

fn create_current_schema(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TABLE songs (
            id                       TEXT PRIMARY KEY,
            source_id                TEXT NOT NULL DEFAULT 'local',
            title                    TEXT NOT NULL DEFAULT '',
            artist                   TEXT NOT NULL DEFAULT '',
            album_artist             TEXT NOT NULL DEFAULT '',
            album                    TEXT NOT NULL DEFAULT '',
            track_number             INTEGER,
            disc_number              INTEGER,
            year                     INTEGER,
            genre                    TEXT,
            artwork_id               TEXT,
            musicbrainz_recording_id TEXT,
            musicbrainz_release_id   TEXT,
            added_at                 INTEGER NOT NULL DEFAULT 0,
            match_key                TEXT NOT NULL DEFAULT '',
            preferred_file_id        TEXT,
            effective_file_id        TEXT
        );

        CREATE TABLE track_files (
            id                       TEXT PRIMARY KEY,
            song_id                  TEXT NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
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
            available                INTEGER NOT NULL DEFAULT 1,
            UNIQUE (source_id, location)
        );

        CREATE TABLE track_aliases (
            alias_id TEXT PRIMARY KEY,
            song_id  TEXT NOT NULL REFERENCES songs(id) ON DELETE CASCADE
        );

        CREATE INDEX idx_songs_album    ON songs (album, disc_number, track_number);
        CREATE INDEX idx_songs_artist   ON songs (album_artist, album);
        CREATE INDEX idx_songs_matchkey ON songs (match_key);
        CREATE INDEX idx_songs_mbid     ON songs (musicbrainz_recording_id);
        CREATE INDEX idx_files_song     ON track_files (song_id);
        CREATE INDEX idx_files_mbid     ON track_files (musicbrainz_recording_id);
        CREATE INDEX idx_aliases_song   ON track_aliases (song_id);
        "#,
    )?;
    Ok(())
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false))
}

fn resolve_song_id(conn: &Connection, id: &str) -> Result<Option<String>> {
    if conn
        .query_row("SELECT 1 FROM songs WHERE id = ?1", params![id], |_| {
            Ok(true)
        })
        .optional()?
        .unwrap_or(false)
    {
        return Ok(Some(id.to_string()));
    }
    Ok(conn
        .query_row(
            "SELECT song_id FROM track_aliases WHERE alias_id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?)
}

fn upsert_track_tx(tx: &Transaction<'_>, track: &Track) -> Result<bool> {
    let known: Option<(String, String)> = tx
        .query_row(
            "SELECT id, song_id FROM track_files WHERE source_id = ?1 AND location = ?2",
            params![track.source_id, track.location],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    if let Some((file_id, song_id)) = known {
        write_file_fields(tx, &file_id, track, true)?;
        recompute_song(tx, &song_id)?;
        return Ok(false);
    }

    let file_id = if track.id.trim().is_empty() {
        stable_id("t", &format!("{}\0{}", track.source_id, track.location))
    } else {
        track.id.clone()
    };
    if tx
        .query_row(
            "SELECT 1 FROM track_files WHERE id = ?1",
            params![file_id],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false)
    {
        bail!("file id collision for {file_id}");
    }

    let song_id = match find_duplicate_song(tx, track)? {
        Some(song_id) => song_id,
        None => {
            if tx
                .query_row(
                    "SELECT 1 FROM songs WHERE id = ?1",
                    params![file_id],
                    |_| Ok(true),
                )
                .optional()?
                .unwrap_or(false)
            {
                bail!("song id collision for {file_id}");
            }
            tx.execute(
                "INSERT INTO songs (id, source_id, added_at) VALUES (?1, ?2, ?3)",
                params![file_id, track.source_id, track.added_at],
            )?;
            file_id.clone()
        }
    };

    tx.execute(
        r#"
        INSERT INTO track_files (
            id, song_id, source_id, location, title, artist, album_artist, album,
            track_number, disc_number, year, genre, duration_secs, sample_rate,
            channels, bits_per_sample, bitrate_kbps, file_size, format, artwork_id,
            musicbrainz_recording_id, musicbrainz_release_id, gain_db, added_at,
            modified_at, available
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, 1
        )
        "#,
        params![
            file_id,
            song_id,
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
            track.file_size.map(|value| value as i64),
            track.format,
            track.artwork_id,
            track.musicbrainz_recording_id,
            track.musicbrainz_release_id,
            track.gain_db,
            track.added_at,
            now(),
        ],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO track_aliases (alias_id, song_id) VALUES (?1, ?2)",
        params![file_id, song_id],
    )?;
    recompute_song(tx, &song_id)?;
    Ok(true)
}

fn write_file_fields(
    tx: &Transaction<'_>,
    file_id: &str,
    track: &Track,
    available: bool,
) -> Result<()> {
    tx.execute(
        r#"
        UPDATE track_files SET
            source_id = ?2,
            location = ?3,
            title = ?4,
            artist = ?5,
            album_artist = ?6,
            album = ?7,
            track_number = ?8,
            disc_number = ?9,
            year = ?10,
            genre = ?11,
            duration_secs = ?12,
            sample_rate = ?13,
            channels = ?14,
            bits_per_sample = ?15,
            bitrate_kbps = ?16,
            file_size = ?17,
            format = ?18,
            artwork_id = ?19,
            musicbrainz_recording_id = ?20,
            musicbrainz_release_id = ?21,
            gain_db = ?22,
            modified_at = ?23,
            available = ?24
        WHERE id = ?1
        "#,
        params![
            file_id,
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
            track.file_size.map(|value| value as i64),
            track.format,
            track.artwork_id,
            track.musicbrainz_recording_id,
            track.musicbrainz_release_id,
            track.gain_db,
            now(),
            available,
        ],
    )?;
    Ok(())
}

fn find_duplicate_song(tx: &Transaction<'_>, track: &Track) -> Result<Option<String>> {
    let candidates = load_raw_files(tx, None)?;
    let mut song_ids: Vec<String> = candidates
        .iter()
        .filter(|file| duplicate_match(track, file))
        .map(|file| file.song_id.clone())
        .collect();
    song_ids.sort();
    song_ids.dedup();

    // Requiring agreement with every version prevents duration-tolerance chains
    // from creating a group whose endpoints are more than two seconds apart.
    Ok(song_ids.into_iter().find(|song_id| {
        candidates
            .iter()
            .filter(|file| file.song_id == *song_id)
            .all(|file| duplicate_match(track, file))
    }))
}

fn duplicate_match(track: &Track, file: &TrackFile) -> bool {
    if conflicting_positions(
        track.disc_number,
        track.track_number,
        file.disc_number,
        file.track_number,
    ) {
        return false;
    }

    let album = normalise(&track.album);
    let other_album = normalise(&file.album);
    if album != other_album {
        return false;
    }

    let mbids_equal = nonempty_equal(
        track.musicbrainz_recording_id.as_deref(),
        file.musicbrainz_recording_id.as_deref(),
    );
    if mbids_equal {
        return true;
    }
    if album.is_empty() {
        return false;
    }

    normalise(&track.artist) == normalise(&file.artist)
        && normalise(&track.title) == normalise(&file.title)
        && durations_within_tolerance(track.duration_secs, file.duration_secs)
}

fn conflicting_positions(
    disc_a: Option<u32>,
    track_a: Option<u32>,
    disc_b: Option<u32>,
    track_b: Option<u32>,
) -> bool {
    matches!((disc_a, disc_b), (Some(a), Some(b)) if a != b)
        || matches!((track_a, track_b), (Some(a), Some(b)) if a != b)
}

fn nonempty_equal(a: Option<&str>, b: Option<&str>) -> bool {
    match (a.map(str::trim), b.map(str::trim)) {
        (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => a.eq_ignore_ascii_case(b),
        _ => false,
    }
}

fn durations_within_tolerance(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 2.0
}

fn recompute_song(tx: &Transaction<'_>, song_id: &str) -> Result<()> {
    let mut files = load_raw_files(tx, Some(song_id))?;
    if files.is_empty() {
        tx.execute("DELETE FROM songs WHERE id = ?1", params![song_id])?;
        return Ok(());
    }

    let preferred: Option<String> = tx.query_row(
        "SELECT preferred_file_id FROM songs WHERE id = ?1",
        params![song_id],
        |row| row.get(0),
    )?;
    let effective = preferred
        .as_deref()
        .and_then(|id| files.iter().find(|file| file.id == id && file.available))
        .map(|file| file.id.clone())
        .or_else(|| {
            let mut available: Vec<&TrackFile> =
                files.iter().filter(|file| file.available).collect();
            available.sort_by(|a, b| compare_file_rank(a, b));
            available.first().map(|file| file.id.clone())
        });

    files.sort_by(|a, b| {
        metadata_completeness(b)
            .cmp(&metadata_completeness(a))
            .then(a.id.cmp(&b.id))
    });
    let mut merged = MergedMetadata::default();
    for file in &files {
        fill_string(&mut merged.title, &file.title);
        fill_string(&mut merged.artist, &file.artist);
        fill_string(&mut merged.album_artist, &file.album_artist);
        fill_string(&mut merged.album, &file.album);
        fill_option(&mut merged.track_number, file.track_number);
        fill_option(&mut merged.disc_number, file.disc_number);
        fill_option(&mut merged.year, file.year);
        fill_optional_string(&mut merged.genre, file.genre.as_deref());
        fill_optional_string(&mut merged.artwork_id, file.artwork_id.as_deref());
        fill_optional_string(
            &mut merged.musicbrainz_recording_id,
            file.musicbrainz_recording_id.as_deref(),
        );
        fill_optional_string(
            &mut merged.musicbrainz_release_id,
            file.musicbrainz_release_id.as_deref(),
        );
    }
    let added_at = files.iter().map(|file| file.added_at).min().unwrap_or(0);
    let source_id = files
        .first()
        .map(|file| file.source_id.as_str())
        .unwrap_or("local");
    let key = match_key(&merged.artist, &merged.title, &merged.album);

    tx.execute(
        r#"
        UPDATE songs SET
            source_id = ?2,
            title = ?3,
            artist = ?4,
            album_artist = ?5,
            album = ?6,
            track_number = ?7,
            disc_number = ?8,
            year = ?9,
            genre = ?10,
            artwork_id = ?11,
            musicbrainz_recording_id = ?12,
            musicbrainz_release_id = ?13,
            added_at = ?14,
            match_key = ?15,
            effective_file_id = ?16
        WHERE id = ?1
        "#,
        params![
            song_id,
            source_id,
            merged.title,
            merged.artist,
            merged.album_artist,
            merged.album,
            merged.track_number,
            merged.disc_number,
            merged.year,
            merged.genre,
            merged.artwork_id,
            merged.musicbrainz_recording_id,
            merged.musicbrainz_release_id,
            added_at,
            key,
            effective,
        ],
    )?;
    Ok(())
}

#[derive(Default)]
struct MergedMetadata {
    title: String,
    artist: String,
    album_artist: String,
    album: String,
    track_number: Option<u32>,
    disc_number: Option<u32>,
    year: Option<i32>,
    genre: Option<String>,
    artwork_id: Option<String>,
    musicbrainz_recording_id: Option<String>,
    musicbrainz_release_id: Option<String>,
}

fn metadata_completeness(file: &TrackFile) -> u32 {
    let strings = [&file.title, &file.artist, &file.album_artist, &file.album]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .count() as u32;
    strings
        + u32::from(file.track_number.is_some())
        + u32::from(file.disc_number.is_some())
        + u32::from(file.year.is_some())
        + u32::from(nonempty(file.genre.as_deref()))
        + u32::from(nonempty(file.artwork_id.as_deref()))
        + u32::from(nonempty(file.musicbrainz_recording_id.as_deref()))
        + u32::from(nonempty(file.musicbrainz_release_id.as_deref()))
}

fn nonempty(value: Option<&str>) -> bool {
    value
        .map(str::trim)
        .map(|value| !value.is_empty())
        .unwrap_or(false)
}

fn fill_string(target: &mut String, value: &str) {
    if target.trim().is_empty() && !value.trim().is_empty() {
        *target = value.to_string();
    }
}

fn fill_option<T: Copy>(target: &mut Option<T>, value: Option<T>) {
    if target.is_none() {
        *target = value;
    }
}

fn fill_optional_string(target: &mut Option<String>, value: Option<&str>) {
    if target.is_none() {
        *target = value
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
    }
}

fn compare_file_rank(a: &TrackFile, b: &TrackFile) -> Ordering {
    is_lossless(b)
        .cmp(&is_lossless(a))
        .then(technical_score(b).cmp(&technical_score(a)))
        .then(
            b.bitrate_kbps
                .unwrap_or(0)
                .cmp(&a.bitrate_kbps.unwrap_or(0)),
        )
        .then(b.file_size.unwrap_or(0).cmp(&a.file_size.unwrap_or(0)))
        .then(a.id.cmp(&b.id))
}

fn is_lossless(file: &TrackFile) -> bool {
    file.format
        .as_deref()
        .map(str::trim)
        .map(|format| {
            ["FLAC", "ALAC", "WAV", "AIFF", "AIF", "WV", "APE"]
                .iter()
                .any(|lossless| format.eq_ignore_ascii_case(lossless))
        })
        .unwrap_or(false)
}

fn technical_score(file: &TrackFile) -> u64 {
    u64::from(file.bits_per_sample.unwrap_or(0)) * u64::from(file.sample_rate.unwrap_or(0))
}

fn files_for_song_conn(conn: &Connection, song_id: &str) -> Result<Vec<TrackFile>> {
    let mut stmt = conn.prepare(&format!("{FILE_SELECT} WHERE f.song_id = ?1 ORDER BY f.id"))?;
    let rows = stmt.query_map(params![song_id], row_to_track_file)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_raw_files(conn: &Connection, song_id: Option<&str>) -> Result<Vec<TrackFile>> {
    let sql = match song_id {
        Some(_) => format!("{RAW_FILE_SELECT} WHERE song_id = ?1 ORDER BY id"),
        None => format!("{RAW_FILE_SELECT} ORDER BY id"),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = match song_id {
        Some(song_id) => stmt.query_map(params![song_id], row_to_raw_file)?,
        None => stmt.query_map([], row_to_raw_file)?,
    };
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

const TRACK_SELECT: &str = r#"
SELECT s.id, s.source_id, COALESCE(f.location, ''), s.title, s.artist,
       s.album_artist, s.album, s.track_number, s.disc_number, s.year, s.genre,
       COALESCE(f.duration_secs, 0), f.sample_rate, f.channels, f.bits_per_sample,
       f.bitrate_kbps, f.file_size, f.format, s.artwork_id,
       s.musicbrainz_recording_id, s.musicbrainz_release_id, f.gain_db, s.added_at,
       (SELECT COUNT(*) FROM track_files count_files WHERE count_files.song_id = s.id),
       (SELECT COUNT(*) FROM track_files missing_files
        WHERE missing_files.song_id = s.id AND missing_files.available = 0),
       s.effective_file_id, s.preferred_file_id
FROM songs s
LEFT JOIN track_files f ON f.id = s.effective_file_id
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
        file_size: row.get::<_, Option<i64>>(16)?.map(|value| value as u64),
        format: row.get(17)?,
        artwork_id: row.get(18)?,
        musicbrainz_recording_id: row.get(19)?,
        musicbrainz_release_id: row.get(20)?,
        gain_db: row.get(21)?,
        added_at: row.get(22)?,
        file_count: row.get(23)?,
        missing_file_count: row.get(24)?,
        effective_file_id: row.get(25)?,
        preferred_file_id: row.get(26)?,
    })
}

const RAW_FILE_SELECT: &str = r#"
SELECT id, song_id, source_id, location, title, artist, album_artist, album,
       track_number, disc_number, year, genre, duration_secs, sample_rate,
       channels, bits_per_sample, bitrate_kbps, file_size, format, artwork_id,
       musicbrainz_recording_id, musicbrainz_release_id, gain_db, added_at,
       modified_at, available
FROM track_files
"#;

const FILE_SELECT: &str = r#"
SELECT f.id, f.song_id, f.source_id, f.location, f.title, f.artist,
       f.album_artist, f.album, f.track_number, f.disc_number, f.year, f.genre,
       f.duration_secs, f.sample_rate, f.channels, f.bits_per_sample,
       f.bitrate_kbps, f.file_size, f.format, f.artwork_id,
       f.musicbrainz_recording_id, f.musicbrainz_release_id, f.gain_db,
       f.added_at, f.modified_at, f.available,
       CASE WHEN s.preferred_file_id = f.id THEN 1 ELSE 0 END,
       CASE WHEN s.effective_file_id = f.id THEN 1 ELSE 0 END
FROM track_files f
JOIN songs s ON s.id = f.song_id
"#;

fn row_to_raw_file(row: &Row<'_>) -> rusqlite::Result<TrackFile> {
    row_to_file(row, false)
}

fn row_to_track_file(row: &Row<'_>) -> rusqlite::Result<TrackFile> {
    row_to_file(row, true)
}

fn row_to_file(row: &Row<'_>, has_flags: bool) -> rusqlite::Result<TrackFile> {
    let available: bool = row.get(25)?;
    Ok(TrackFile {
        id: row.get(0)?,
        song_id: row.get(1)?,
        source_id: row.get(2)?,
        location: row.get(3)?,
        title: row.get(4)?,
        artist: row.get(5)?,
        album_artist: row.get(6)?,
        album: row.get(7)?,
        track_number: row.get(8)?,
        disc_number: row.get(9)?,
        year: row.get(10)?,
        genre: row.get(11)?,
        duration_secs: row.get(12)?,
        sample_rate: row.get(13)?,
        channels: row.get(14)?,
        bits_per_sample: row.get(15)?,
        bitrate_kbps: row.get(16)?,
        file_size: row.get::<_, Option<i64>>(17)?.map(|value| value as u64),
        format: row.get(18)?,
        artwork_id: row.get(19)?,
        musicbrainz_recording_id: row.get(20)?,
        musicbrainz_release_id: row.get(21)?,
        gain_db: row.get(22)?,
        added_at: row.get(23)?,
        modified_at: row.get(24)?,
        available,
        missing: !available,
        preferred: has_flags && row.get(26)?,
        effective: has_flags && row.get(27)?,
    })
}

const LEGACY_TRACK_SELECT: &str = r#"
SELECT id, source_id, location, title, artist, album_artist, album, track_number,
       disc_number, year, genre, duration_secs, sample_rate, channels,
       bits_per_sample, bitrate_kbps, file_size, format, artwork_id,
       musicbrainz_recording_id, musicbrainz_release_id, gain_db, added_at
"#;

fn row_to_legacy_track(row: &Row<'_>) -> rusqlite::Result<Track> {
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
        file_size: row.get::<_, Option<i64>>(16)?.map(|value| value as u64),
        format: row.get(17)?,
        artwork_id: row.get(18)?,
        musicbrainz_recording_id: row.get(19)?,
        musicbrainz_release_id: row.get(20)?,
        gain_db: row.get(21)?,
        added_at: row.get(22)?,
        ..Default::default()
    })
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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

    fn version(path: &str, format: &str, bits: u32, rate: u32, bitrate: u32) -> Track {
        let mut value = track("Song", "Artist", "Album", path);
        value.format = Some(format.into());
        value.bits_per_sample = Some(bits);
        value.sample_rate = Some(rate);
        value.bitrate_kbps = Some(bitrate);
        value.file_size = Some(u64::from(bitrate) * 1000);
        value
    }

    #[test]
    fn duplicate_album_and_duration_rules_are_conservative() {
        let db = Db::open_in_memory().unwrap();
        let a = track("Song", "Artist", "Album", "/m/a.flac");
        let mut within = track(" song ", "ARTIST", "album", "/m/b.mp3");
        within.duration_secs = 182.0;
        let different_album = track("Song", "Artist", "Other", "/m/c.flac");
        let mut too_long = track("Song", "Artist", "Album", "/m/d.flac");
        too_long.duration_secs = 182.01;
        db.upsert_track(&a).unwrap();
        db.upsert_track(&within).unwrap();
        db.upsert_track(&different_album).unwrap();
        db.upsert_track(&too_long).unwrap();
        assert_eq!(db.song_count().unwrap(), 3);
        assert_eq!(db.file_count().unwrap(), 4);
        assert_eq!(db.get_track(&a.id).unwrap().unwrap().file_count, 2);
    }

    #[test]
    fn mbid_groups_only_when_albums_agree_including_both_albumless() {
        let db = Db::open_in_memory().unwrap();
        let mut a = track("One", "A", "Album", "/m/a.flac");
        a.musicbrainz_recording_id = Some("MBID".into());
        let mut same_album = track("Different", "B", "album", "/m/b.mp3");
        same_album.musicbrainz_recording_id = Some("mbid".into());
        same_album.duration_secs = 999.0;
        let mut other_album = same_album.clone();
        other_album.id = stable_id("t", "/m/c.mp3");
        other_album.location = "/m/c.mp3".into();
        other_album.album = "Other".into();
        let mut loose_a = track("Loose A", "A", "", "/m/d.flac");
        loose_a.musicbrainz_recording_id = Some("loose".into());
        let mut loose_b = track("Loose B", "B", "", "/m/e.mp3");
        loose_b.musicbrainz_recording_id = Some("LOOSE".into());
        db.upsert_track(&a).unwrap();
        db.upsert_track(&same_album).unwrap();
        db.upsert_track(&other_album).unwrap();
        db.upsert_track(&loose_a).unwrap();
        db.upsert_track(&loose_b).unwrap();
        assert_eq!(db.song_count().unwrap(), 3);
        assert_eq!(db.get_track(&a.id).unwrap().unwrap().file_count, 2);
        assert_eq!(db.get_track(&loose_a.id).unwrap().unwrap().file_count, 2);
    }

    #[test]
    fn albumless_metadata_and_conflicting_positions_do_not_group() {
        let db = Db::open_in_memory().unwrap();
        let loose_a = track("Song", "Artist", "", "/m/a.flac");
        let loose_b = track("Song", "Artist", "", "/m/b.mp3");
        let mut pos_a = track("Other", "Artist", "Album", "/m/c.flac");
        pos_a.track_number = Some(1);
        let mut pos_b = track("Other", "Artist", "Album", "/m/d.mp3");
        pos_b.track_number = Some(2);
        for value in [&loose_a, &loose_b, &pos_a, &pos_b] {
            db.upsert_track(value).unwrap();
        }
        assert_eq!(db.song_count().unwrap(), 4);
    }

    #[test]
    fn automatic_ranking_is_lossless_then_technical_bitrate_size_and_id() {
        let db = Db::open_in_memory().unwrap();
        let mp3 = version("/m/a.mp3", "MP3", 32, 192_000, 9999);
        let flac_cd = version("/m/b.flac", "FLAC", 16, 44_100, 900);
        let flac_hi = version("/m/c.flac", "FLAC", 24, 96_000, 700);
        db.upsert_track(&mp3).unwrap();
        db.upsert_track(&flac_cd).unwrap();
        db.upsert_track(&flac_hi).unwrap();
        let song = db.get_track(&mp3.id).unwrap().unwrap();
        assert_eq!(song.location, flac_hi.location);
        assert_eq!(song.effective_file_id.as_deref(), Some(flac_hi.id.as_str()));
        let ranked = db.ranked_available_files(&song.id).unwrap();
        assert_eq!(ranked[0].id, flac_hi.id);
        assert_eq!(ranked[1].id, flac_cd.id);
        assert_eq!(ranked[2].id, mp3.id);
    }

    #[test]
    fn metadata_comes_from_most_complete_version_not_playback_preference() {
        let db = Db::open_in_memory().unwrap();
        let mut rich = version("/m/rich.mp3", "MP3", 16, 44_100, 320);
        rich.album_artist = "Album Artist".into();
        rich.year = Some(2024);
        rich.genre = Some("Rock".into());
        rich.artwork_id = Some("cover.jpg".into());
        let mut sparse = version("/m/sparse.flac", "FLAC", 24, 96_000, 1000);
        sparse.album_artist.clear();
        db.upsert_track(&rich).unwrap();
        db.upsert_track(&sparse).unwrap();
        let song = db.get_track(&rich.id).unwrap().unwrap();
        assert_eq!(song.location, sparse.location);
        assert_eq!(song.album_artist, "Album Artist");
        assert_eq!(song.year, Some(2024));
        assert_eq!(song.genre.as_deref(), Some("Rock"));
        assert_eq!(song.artwork_id.as_deref(), Some("cover.jpg"));
    }

    #[test]
    fn missing_preference_falls_back_and_reappearing_file_becomes_effective() {
        let db = Db::open_in_memory().unwrap();
        let preferred = version("/m/preferred.mp3", "MP3", 16, 44_100, 320);
        let fallback = version("/m/fallback.flac", "FLAC", 24, 96_000, 900);
        db.upsert_track(&preferred).unwrap();
        db.upsert_track(&fallback).unwrap();
        let song_id = db.get_track(&preferred.id).unwrap().unwrap().id;
        db.set_preferred_file(&song_id, Some(&preferred.id))
            .unwrap();
        assert_eq!(
            db.get_track(&song_id).unwrap().unwrap().location,
            preferred.location
        );
        assert!(db.mark_file_missing("local", &preferred.location).unwrap());
        let missing = db.get_track(&song_id).unwrap().unwrap();
        assert_eq!(missing.location, fallback.location);
        assert_eq!(
            missing.preferred_file_id.as_deref(),
            Some(preferred.id.as_str())
        );
        assert_eq!(missing.missing_file_count, 1);
        db.upsert_track(&preferred).unwrap();
        let returned = db.get_track(&song_id).unwrap().unwrap();
        assert_eq!(returned.location, preferred.location);
        assert_eq!(returned.missing_file_count, 0);
        db.set_preferred_file(&song_id, None).unwrap();
        assert_eq!(
            db.get_track(&song_id).unwrap().unwrap().location,
            fallback.location
        );
    }

    #[test]
    fn every_file_id_alias_resolves_to_the_collapsed_song() {
        let db = Db::open_in_memory().unwrap();
        let a = track("Song", "Artist", "Album", "/m/a.flac");
        let b = track("Song", "Artist", "Album", "/m/b.mp3");
        db.upsert_track(&a).unwrap();
        db.upsert_track(&b).unwrap();
        let by_a = db.get_track(&a.id).unwrap().unwrap();
        let by_b = db.get_track(&b.id).unwrap().unwrap();
        assert_eq!(by_a.id, by_b.id);
        assert_eq!(by_a.file_count, 2);
    }

    #[test]
    fn resolve_never_falls_back_across_albums_even_for_mbid() {
        let db = Db::open_in_memory().unwrap();
        let mut value = track("Song", "Artist", "Album", "/m/a.flac");
        value.musicbrainz_recording_id = Some("mbid".into());
        db.upsert_track(&value).unwrap();
        assert!(db
            .resolve(None, "artist", "song", "album")
            .unwrap()
            .is_some());
        assert!(db
            .resolve(None, "artist", "song", "Other")
            .unwrap()
            .is_none());
        assert!(db
            .resolve(Some("mbid"), "artist", "song", "Other")
            .unwrap()
            .is_none());
    }

    #[test]
    fn folder_removal_uses_path_components_and_cleans_orphans() {
        let db = Db::open_in_memory().unwrap();
        db.add_folder("/music").unwrap();
        let inside = track("A", "Artist", "Album", "/music/a.flac");
        let prefix_only = track("B", "Artist", "Album", "/music-other/b.flac");
        db.upsert_track(&inside).unwrap();
        db.upsert_track(&prefix_only).unwrap();
        db.remove_folder("/music").unwrap();
        assert_eq!(db.song_count().unwrap(), 1);
        assert!(db
            .file_by_location("local", "/music/a.flac")
            .unwrap()
            .is_none());
        assert!(db
            .file_by_location("local", "/music-other/b.flac")
            .unwrap()
            .is_some());
    }

    #[test]
    fn relink_and_forget_preserve_membership_then_clean_orphans() {
        let db = Db::open_in_memory().unwrap();
        let original = track("Song", "Artist", "Album", "/old/a.flac");
        db.upsert_track(&original).unwrap();
        let mut moved = original.clone();
        moved.id = stable_id("t", "/new/a.flac");
        moved.location = "/new/a.flac".into();
        db.relink_file(&original.id, &moved).unwrap();
        assert!(db
            .file_by_location("local", "/old/a.flac")
            .unwrap()
            .is_none());
        assert_eq!(db.get_track(&moved.id).unwrap().unwrap().id, original.id);
        assert!(db.forget_file(&original.id).unwrap());
        assert_eq!(db.song_count().unwrap(), 0);
        assert_eq!(db.file_count().unwrap(), 0);
    }

    #[test]
    fn versioned_migration_preserves_data_groups_files_and_chooses_minimum_id() {
        let path = temp_db_path("migration");
        let conn = Connection::open(&path).unwrap();
        create_legacy_schema(&conn);
        conn.execute(
            "INSERT INTO folders (path, added_at) VALUES ('/music', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('volume', '0.5')",
            [],
        )
        .unwrap();
        let a = track("Song", "Artist", "Album", "/music/a.flac");
        let b = track("Song", "Artist", "Album", "/music/b.mp3");
        insert_legacy(&conn, &b);
        insert_legacy(&conn, &a);
        drop(conn);

        let db = Db::open(&path).unwrap();
        let expected_id = a.id.clone().min(b.id.clone());
        assert_eq!(db.song_count().unwrap(), 1);
        assert_eq!(db.file_count().unwrap(), 2);
        assert_eq!(db.folders().unwrap(), vec!["/music"]);
        assert_eq!(db.get_setting("volume").unwrap().as_deref(), Some("0.5"));
        assert_eq!(db.get_track(&a.id).unwrap().unwrap().id, expected_id);
        assert_eq!(db.get_track(&b.id).unwrap().unwrap().id, expected_id);
        assert_eq!(
            db.conn
                .lock()
                .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        drop(db);
        remove_db_files(&path);
    }

    #[test]
    fn albums_and_artists_count_collapsed_songs_not_versions() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track("A", "Artist", "Album", "/m/a.flac"))
            .unwrap();
        db.upsert_track(&track("A", "Artist", "Album", "/m/a.mp3"))
            .unwrap();
        db.upsert_track(&track("B", "Artist", "Album", "/m/b.flac"))
            .unwrap();
        assert_eq!(db.albums().unwrap()[0].track_count, 2);
        assert_eq!(db.artists().unwrap()[0].track_count, 2);
    }

    fn create_legacy_schema(conn: &Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE tracks (
                id TEXT PRIMARY KEY, source_id TEXT NOT NULL DEFAULT 'local',
                location TEXT NOT NULL, title TEXT NOT NULL DEFAULT '',
                artist TEXT NOT NULL DEFAULT '', album_artist TEXT NOT NULL DEFAULT '',
                album TEXT NOT NULL DEFAULT '', track_number INTEGER, disc_number INTEGER,
                year INTEGER, genre TEXT, duration_secs REAL NOT NULL DEFAULT 0,
                sample_rate INTEGER, channels INTEGER, bits_per_sample INTEGER,
                bitrate_kbps INTEGER, file_size INTEGER, format TEXT, artwork_id TEXT,
                musicbrainz_recording_id TEXT, musicbrainz_release_id TEXT, gain_db REAL,
                added_at INTEGER NOT NULL DEFAULT 0, modified_at INTEGER NOT NULL DEFAULT 0,
                match_key TEXT NOT NULL DEFAULT '', UNIQUE(source_id, location)
            );
            CREATE TABLE folders (path TEXT PRIMARY KEY, added_at INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            "#,
        )
        .unwrap();
    }

    fn insert_legacy(conn: &Connection, track: &Track) {
        conn.execute(
            r#"
            INSERT INTO tracks (
                id, source_id, location, title, artist, album_artist, album,
                duration_secs, format, added_at, modified_at, match_key
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11)
            "#,
            params![
                track.id,
                track.source_id,
                track.location,
                track.title,
                track.artist,
                track.album_artist,
                track.album,
                track.duration_secs,
                track.format,
                track.added_at,
                track.match_key(),
            ],
        )
        .unwrap();
    }

    fn temp_db_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "pick-n-mix-{label}-{}.sqlite",
            stable_id("db", &format!("{:?}", std::time::Instant::now()))
        ))
    }

    fn remove_db_files(path: &Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }
}
