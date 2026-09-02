//! SQLite index of the music library.
//!
//! Logical songs are stable public records. Physical files are version rows that
//! can be ranked, preferred, marked missing, relinked, or forgotten independently.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use crate::library::model::{
    match_key, normalise, stable_id, Album, Artist, HomePick, Play, PlayRecord, Track, TrackFile,
};

const SCHEMA_VERSION: u32 = 3;

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

        // A database that already has the song/file tables is upgraded in
        // place; only a fresh or pre-songs database gets the schema created.
        if !table_exists(&tx, "songs")? {
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
        }

        // Schema 1 -> 2: per-file match key, so duplicate detection can look
        // candidates up through an index rather than scanning every file.
        if !column_exists(&tx, "track_files", "match_key")? {
            tx.execute_batch(
                r#"
                ALTER TABLE track_files ADD COLUMN match_key TEXT NOT NULL DEFAULT '';
                CREATE INDEX IF NOT EXISTS idx_files_matchkey ON track_files (match_key);
                "#,
            )?;
            backfill_file_match_keys(&tx)?;
        }

        // Schema 2 -> 3: listening history, which the home page's mixes and
        // recommendations are derived from.
        if !table_exists(&tx, "plays")? {
            tx.execute_batch(CREATE_PLAYS)?;
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

    // -- listening history -----------------------------------------------

    pub fn record_play(&self, play: &Play) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO plays
               (song_id, played_at, seconds_played, fraction, counted, context_kind, context_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                play.song_id,
                play.played_at,
                play.seconds_played,
                play.fraction,
                play.counted as i32,
                play.context_kind,
                play.context_id,
            ],
        )?;
        Ok(())
    }

    /// Most recent listens first, for the history screen.
    pub fn recent_plays(&self, limit: usize) -> Result<Vec<PlayRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT song_id, played_at, seconds_played, fraction, counted,
                    context_kind, context_id
             FROM plays ORDER BY played_at DESC, id DESC LIMIT ?1",
        )?;
        let plays = stmt
            .query_map(params![limit as i64], |row| {
                Ok(Play {
                    song_id: row.get(0)?,
                    played_at: row.get(1)?,
                    seconds_played: row.get(2)?,
                    fraction: row.get(3)?,
                    counted: row.get::<_, i32>(4)? != 0,
                    context_kind: row.get(5)?,
                    context_id: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        let mut track_stmt = conn.prepare(&format!("{TRACK_SELECT} WHERE s.id = ?1"))?;
        plays
            .into_iter()
            .map(|play| {
                let track = track_stmt
                    .query_row(params![play.song_id], row_to_track)
                    .optional()?;
                Ok(PlayRecord { play, track })
            })
            .collect()
    }

    /// How many counted plays exist, so the UI can tell "no history yet" from
    /// "history exists but this particular mix has nothing in it".
    pub fn counted_play_total(&self) -> Result<u32> {
        let conn = self.conn.lock();
        Ok(
            conn.query_row("SELECT COUNT(*) FROM plays WHERE counted = 1", [], |row| {
                row.get(0)
            })?,
        )
    }

    pub fn clear_history(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM plays", [])?;
        Ok(())
    }

    pub fn clear_history_for_song(&self, song_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM plays WHERE song_id = ?1", params![song_id])?;
        Ok(())
    }

    /// Songs played repeatedly in the recent past.
    pub fn replay_mix(&self, limit: usize, since: i64, min_plays: u32) -> Result<Vec<Track>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "{TRACK_SELECT}
             JOIN (
                SELECT song_id, COUNT(*) AS plays, MAX(played_at) AS last_at
                FROM plays
                WHERE counted = 1 AND played_at >= ?1
                GROUP BY song_id
                HAVING COUNT(*) >= ?3
             ) p ON p.song_id = s.id
             ORDER BY p.plays DESC, p.last_at DESC
             LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![since, limit as i64, min_plays], row_to_track)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Songs that were once played a lot but have gone quiet.
    pub fn archive_mix(
        &self,
        limit: usize,
        stale_before: i64,
        min_plays: u32,
    ) -> Result<Vec<Track>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "{TRACK_SELECT}
             JOIN (
                SELECT song_id, COUNT(*) AS plays, MAX(played_at) AS last_at
                FROM plays
                WHERE counted = 1
                GROUP BY song_id
                HAVING COUNT(*) >= ?3 AND MAX(played_at) < ?1
             ) p ON p.song_id = s.id
             ORDER BY p.plays DESC, p.last_at ASC
             LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![stale_before, limit as i64, min_plays], row_to_track)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Neglected corners of the library.
    ///
    /// Built in tiers, because the obvious query — "everything never played" —
    /// surfaces whatever stray non-music files happen to sit in the library
    /// alongside the actual albums. Every tier requires some evidence that the
    /// song is music the listener actually wanted: either they have played it
    /// themselves, or they have played its album or its artist.
    pub fn discover_mix(&self, limit: usize, max_plays: u32) -> Result<Vec<Track>> {
        let conn = self.conn.lock();
        let mut out: Vec<Track> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // Tier 1: played, but only a handful of times, longest ago first.
        let mut stmt = conn.prepare(&format!(
            "{TRACK_SELECT}
             JOIN (
                SELECT song_id, COUNT(*) AS plays, MAX(played_at) AS last_at
                FROM plays
                WHERE counted = 1
                GROUP BY song_id
                HAVING COUNT(*) BETWEEN 1 AND ?2
             ) p ON p.song_id = s.id
             ORDER BY p.last_at ASC
             LIMIT ?1"
        ))?;
        for track in stmt.query_map(params![limit as i64, max_plays], row_to_track)? {
            let track = track?;
            if seen.insert(track.id.clone()) {
                out.push(track);
            }
        }
        drop(stmt);

        // Tier 2: never played, but sitting on an album the listener has.
        // Tier 3 widens that to the artist. Both are ordered randomly, which
        // is the point of the shelf — but the mix is cached for the session,
        // so it will not reshuffle underfoot.
        for condition in [
            "EXISTS (SELECT 1 FROM songs sib
                     JOIN plays pl ON pl.song_id = sib.id AND pl.counted = 1
                     WHERE sib.album = s.album) AND s.album <> ''",
            "EXISTS (SELECT 1 FROM songs sib
                     JOIN plays pl ON pl.song_id = sib.id AND pl.counted = 1
                     WHERE COALESCE(NULLIF(sib.album_artist, ''), sib.artist)
                         = COALESCE(NULLIF(s.album_artist, ''), s.artist))",
        ] {
            if out.len() >= limit {
                break;
            }
            let mut stmt = conn.prepare(&format!(
                "{TRACK_SELECT}
                 WHERE s.id NOT IN (SELECT song_id FROM plays WHERE counted = 1)
                   AND {condition}
                 ORDER BY RANDOM() LIMIT ?1"
            ))?;
            for track in stmt.query_map(params![limit as i64], row_to_track)? {
                let track = track?;
                if out.len() >= limit {
                    break;
                }
                if seen.insert(track.id.clone()) {
                    out.push(track);
                }
            }
        }

        Ok(out)
    }

    /// Playlists ordered by when they were last played from.
    pub fn recent_playlist_ids(&self, limit: usize) -> Result<Vec<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT context_id, MAX(played_at) AS last_at
             FROM plays
             WHERE context_kind = 'playlist' AND context_id IS NOT NULL
             GROUP BY context_id
             ORDER BY last_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Explainable recommendations, drawn from three rules and interleaved so
    /// one rule with a lot of matches cannot fill the whole shelf.
    pub fn top_picks(&self, limit: usize, now_secs: i64) -> Result<Vec<HomePick>> {
        const WEEK: i64 = 7 * 86_400;
        const MONTH: i64 = 30 * 86_400;
        const YEAR: i64 = 365 * 86_400;

        let from_artists = self.picks_more_from_artist(limit, now_secs - WEEK, now_secs - MONTH)?;
        let from_albums = self.picks_finish_album(limit)?;
        let from_age = self.picks_not_played_since(limit, now_secs - YEAR)?;

        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut sources = [
            from_artists.into_iter(),
            from_albums.into_iter(),
            from_age.into_iter(),
        ];
        let mut exhausted = 0;
        while out.len() < limit && exhausted < sources.len() {
            exhausted = 0;
            for source in sources.iter_mut() {
                match source.next() {
                    Some(pick) => {
                        if out.len() < limit && seen.insert(pick.id.clone()) {
                            out.push(pick);
                        }
                    }
                    None => exhausted += 1,
                }
            }
        }
        Ok(out)
    }

    /// "More from an artist you played this week", skipping anything they have
    /// already had on recently.
    fn picks_more_from_artist(
        &self,
        limit: usize,
        played_since: i64,
        exclude_since: i64,
    ) -> Result<Vec<HomePick>> {
        let conn = self.conn.lock();
        let artists = {
            let mut stmt = conn.prepare(
                "SELECT COALESCE(NULLIF(s.album_artist, ''), s.artist) AS a, MAX(p.played_at) AS t
                 FROM plays p JOIN songs s ON s.id = p.song_id
                 WHERE p.counted = 1 AND p.played_at >= ?1 AND a <> ''
                 GROUP BY a ORDER BY t DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![played_since, limit as i64], |row| {
                row.get::<_, String>(0)
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        let mut stmt = conn.prepare(&format!(
            "{TRACK_SELECT}
             WHERE COALESCE(NULLIF(s.album_artist, ''), s.artist) = ?1
               AND s.id NOT IN
                   (SELECT song_id FROM plays WHERE counted = 1 AND played_at >= ?2)
             ORDER BY RANDOM() LIMIT 1"
        ))?;
        let mut out = Vec::new();
        for artist in artists {
            if let Some(track) = stmt
                .query_row(params![artist, exclude_since], row_to_track)
                .optional()?
            {
                out.push(HomePick {
                    kind: "song".into(),
                    id: track.id.clone(),
                    title: track.title.clone(),
                    subtitle: track.artist.clone(),
                    artwork_id: track.artwork_id.clone(),
                    reason: format!("More from {artist}"),
                    track_ids: vec![track.id],
                });
            }
        }
        Ok(out)
    }

    /// "You started this album but never finished it."
    fn picks_finish_album(&self, limit: usize) -> Result<Vec<HomePick>> {
        let conn = self.conn.lock();
        let albums = {
            let mut stmt = conn.prepare(
                "SELECT s.album,
                        COALESCE(NULLIF(s.album_artist, ''), s.artist) AS aa,
                        COUNT(*) AS total,
                        SUM(CASE WHEN EXISTS (
                            SELECT 1 FROM plays p WHERE p.song_id = s.id AND p.counted = 1
                        ) THEN 1 ELSE 0 END) AS heard
                 FROM songs s
                 WHERE s.album <> ''
                 GROUP BY s.album, aa
                 HAVING heard > 0 AND heard < total
                 ORDER BY (heard * 1.0 / total) DESC, total DESC
                 LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        let mut stmt = conn.prepare(&format!(
            "{TRACK_SELECT}
             WHERE s.album = ?1 AND COALESCE(NULLIF(s.album_artist, ''), s.artist) = ?2
             ORDER BY s.disc_number, s.track_number"
        ))?;
        let mut out = Vec::new();
        for (album, artist, total, heard) in albums {
            let tracks = stmt
                .query_map(params![album, artist], row_to_track)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let Some(first) = tracks.first() else {
                continue;
            };
            out.push(HomePick {
                kind: "album".into(),
                id: album_id_for(first),
                title: album.clone(),
                subtitle: artist,
                artwork_id: tracks.iter().find_map(|t| t.artwork_id.clone()),
                reason: format!("{} of {total} played", heard),
                track_ids: tracks.into_iter().map(|t| t.id).collect(),
            });
        }
        Ok(out)
    }

    /// "You have not heard this since <year>."
    fn picks_not_played_since(&self, limit: usize, before: i64) -> Result<Vec<HomePick>> {
        let conn = self.conn.lock();
        // The song ids and their dates are fetched first rather than joined
        // into `TRACK_SELECT`: that constant fixes its own select list, so a
        // joined column would not actually come back in the row.
        let stale = {
            let mut stmt = conn.prepare(
                "SELECT song_id, MAX(played_at) AS last_at
                 FROM plays WHERE counted = 1
                 GROUP BY song_id HAVING MAX(played_at) < ?1
                 ORDER BY last_at ASC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![before, limit as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        let mut stmt = conn.prepare(&format!("{TRACK_SELECT} WHERE s.id = ?1"))?;
        let mut out = Vec::new();
        for (song_id, last_at) in stale {
            let Some(track) = stmt.query_row(params![song_id], row_to_track).optional()? else {
                continue;
            };
            out.push(HomePick {
                kind: "song".into(),
                id: track.id.clone(),
                title: track.title.clone(),
                subtitle: track.artist.clone(),
                artwork_id: track.artwork_id.clone(),
                reason: format!("Not played since {}", year_of(last_at)),
                track_ids: vec![track.id],
            });
        }
        Ok(out)
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
            -- Normalised artist|title|album for this file specifically. Indexed
            -- so duplicate detection can find candidates instead of walking
            -- every file in the library for every file it indexes.
            match_key                TEXT NOT NULL DEFAULT '',
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
        CREATE INDEX idx_files_matchkey ON track_files (match_key);
        CREATE INDEX idx_aliases_song   ON track_aliases (song_id);
        "#,
    )?;
    tx.execute_batch(CREATE_PLAYS)?;
    Ok(())
}

/// Listening history.
///
/// Rows are kept even when they do not count as a play, because "started and
/// abandoned after four seconds" is a real signal about a song, and throwing
/// it away at write time would make it unrecoverable later. `counted` carries
/// the judgement so the shelf queries do not each have to re-derive it.
///
/// No foreign key to `songs`: history should survive a song being removed from
/// the library and re-added, and a rescan legitimately deletes and recreates
/// rows. Orphaned history is harmless — every read joins back to `songs` and
/// so simply ignores it.
const CREATE_PLAYS: &str = r#"
CREATE TABLE IF NOT EXISTS plays (
    id             INTEGER PRIMARY KEY,
    song_id        TEXT    NOT NULL,
    played_at      INTEGER NOT NULL,
    -- Seconds actually heard: accumulated while playing, so pausing and
    -- seeking cannot inflate it.
    seconds_played REAL    NOT NULL DEFAULT 0,
    -- How far through the song that got, 0..1.
    fraction       REAL    NOT NULL DEFAULT 0,
    -- Whether this passed the bar to count as a play rather than a skip.
    counted        INTEGER NOT NULL DEFAULT 0,
    -- What it was played from, so playlists can be ranked by use.
    context_kind   TEXT,
    context_id     TEXT
);

CREATE INDEX IF NOT EXISTS idx_plays_song    ON plays (song_id, played_at);
CREATE INDEX IF NOT EXISTS idx_plays_at      ON plays (played_at);
CREATE INDEX IF NOT EXISTS idx_plays_context ON plays (context_kind, context_id, played_at);
"#;

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Populate `track_files.match_key` for a database written before it existed.
/// The key is normalised in Rust, so this cannot be a single UPDATE.
fn backfill_file_match_keys(tx: &Transaction<'_>) -> Result<()> {
    let rows: Vec<(String, String, String, String)> = {
        let mut stmt = tx.prepare("SELECT id, artist, title, album FROM track_files")?;
        let mapped = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (id, artist, title, album) in rows {
        tx.execute(
            "UPDATE track_files SET match_key = ?2 WHERE id = ?1",
            params![id, match_key(&artist, &title, &album)],
        )?;
    }
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
            modified_at, available, match_key
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, 1, ?26
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
            track.match_key(),
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
            available = ?24,
            match_key = ?25
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
            track.match_key(),
        ],
    )?;
    Ok(())
}

fn find_duplicate_song(tx: &Transaction<'_>, track: &Track) -> Result<Option<String>> {
    // Narrow to songs that could possibly match before comparing anything.
    //
    // `duplicate_match` succeeds by one of exactly two routes: an equal
    // non-empty recording id, or equal normalised artist, title and album —
    // which is precisely an equal match key. Both are indexed, so this asks for
    // a handful of rows rather than reading the whole library for every file
    // being indexed, which made a first scan quadratic in the library size.
    let mut song_ids = candidate_song_ids(tx, track)?;
    song_ids.sort();
    song_ids.dedup();

    // Requiring agreement with every version prevents duration-tolerance chains
    // from creating a group whose endpoints are more than two seconds apart.
    for song_id in song_ids {
        let versions = load_raw_files(tx, Some(&song_id))?;
        if !versions.is_empty() && versions.iter().all(|file| duplicate_match(track, file)) {
            return Ok(Some(song_id));
        }
    }
    Ok(None)
}

/// Songs owning at least one file that could match `track`.
fn candidate_song_ids(tx: &Transaction<'_>, track: &Track) -> Result<Vec<String>> {
    let mut ids = Vec::new();

    {
        let mut stmt =
            tx.prepare("SELECT DISTINCT song_id FROM track_files WHERE match_key = ?1")?;
        let rows = stmt.query_map(params![track.match_key()], |row| row.get::<_, String>(0))?;
        for id in rows {
            ids.push(id?);
        }
    }

    // A shared recording id groups files even when the tags disagree, so it
    // cannot be found through the match key.
    if let Some(mbid) = track
        .musicbrainz_recording_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let mut stmt = tx.prepare(
            "SELECT DISTINCT song_id FROM track_files
             WHERE musicbrainz_recording_id IS NOT NULL
               AND musicbrainz_recording_id <> ''
               AND musicbrainz_recording_id = ?1 COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![mbid], |row| row.get::<_, String>(0))?;
        for id in rows {
            ids.push(id?);
        }
    }

    Ok(ids)
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

/// Civil year of a Unix timestamp.
///
/// Worked out arithmetically rather than by pulling in a date library: the
/// only date this codebase ever has to render is the year on a "not played
/// since" label, and `chrono` would be a large dependency for one integer.
/// Days are counted from 1970 through whole 400-year cycles, which is exactly
/// how the Gregorian leap rule repeats, so this stays correct indefinitely.
fn year_of(timestamp: i64) -> i64 {
    let mut days = timestamp.div_euclid(86_400);
    let mut year = 1970;

    let cycles = days.div_euclid(146_097);
    year += cycles * 400;
    days -= cycles * 146_097;

    loop {
        let length = if is_leap(year) { 366 } else { 365 };
        if days < length {
            return year;
        }
        days -= length;
        year += 1;
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
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

    /// Candidates are now looked up through an index rather than by reading
    /// every file, so each route into `duplicate_match` needs to still be
    /// reachable. This one cannot be found by match key: the tags disagree.
    #[test]
    fn a_shared_recording_id_still_groups_files_whose_tags_disagree() {
        let db = Db::open_in_memory().unwrap();

        let mut first = track("Song", "Artist", "Album", "/m/a.flac");
        first.musicbrainz_recording_id = Some("mbid-1".into());
        let mut second = track("Sng (Remaster)", "The Artist", "Album", "/m/b.flac");
        second.musicbrainz_recording_id = Some("MBID-1".into());

        db.upsert_track(&first).unwrap();
        db.upsert_track(&second).unwrap();

        assert_eq!(db.song_count().unwrap(), 1, "the recording id groups them");
        assert_eq!(db.file_count().unwrap(), 2);
    }

    /// The other route: no recording ids at all, matched purely on tags.
    #[test]
    fn matching_tags_still_group_without_any_recording_id() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track("Song", "Artist", "Album", "/m/a.flac"))
            .unwrap();
        db.upsert_track(&track("song", "artist", "album", "/m/b.mp3"))
            .unwrap();

        assert_eq!(db.song_count().unwrap(), 1);
        assert_eq!(db.file_count().unwrap(), 2);
    }

    /// Narrowing must not become a way of accidentally merging things: songs
    /// that share nothing indexable stay apart.
    #[test]
    fn unrelated_songs_are_not_drawn_together_by_the_lookup() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track("One", "Artist", "Album", "/m/a.flac"))
            .unwrap();
        db.upsert_track(&track("Two", "Artist", "Album", "/m/b.flac"))
            .unwrap();
        assert_eq!(db.song_count().unwrap(), 2);
    }

    /// Tags change on rescan, so the stored key has to change with them or the
    /// file becomes invisible to the next duplicate lookup.
    #[test]
    fn retagging_a_file_updates_the_key_it_is_found_by() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track("Wrong Title", "Artist", "Album", "/m/a.flac"))
            .unwrap();
        // Same location, corrected tags: an update, not a new file.
        db.upsert_track(&track("Right Title", "Artist", "Album", "/m/a.flac"))
            .unwrap();

        // A second file carrying the corrected tags must now find it.
        db.upsert_track(&track("Right Title", "Artist", "Album", "/m/b.mp3"))
            .unwrap();
        assert_eq!(db.song_count().unwrap(), 1, "the key followed the retag");
        assert_eq!(db.file_count().unwrap(), 2);
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

    // -- listening history ---------------------------------------------

    const DAY: i64 = 86_400;

    fn play(song_id: &str, at: i64, counted: bool) -> Play {
        Play {
            song_id: song_id.into(),
            played_at: at,
            seconds_played: if counted { 120.0 } else { 4.0 },
            fraction: if counted { 0.7 } else { 0.02 },
            counted,
            context_kind: None,
            context_id: None,
        }
    }

    /// Seed `count` counted plays for a song, spaced a day apart ending at `last`.
    fn seed_plays(db: &Db, song_id: &str, count: usize, last: i64) {
        for i in 0..count {
            db.record_play(&play(song_id, last - (i as i64) * DAY, true))
                .unwrap();
        }
    }

    #[test]
    fn a_skip_is_recorded_but_does_not_feed_the_mixes() {
        let db = Db::open_in_memory().unwrap();
        let song = track("Song", "Artist", "Album", "/m/a.flac");
        db.upsert_track(&song).unwrap();
        let id = db.all_tracks().unwrap()[0].id.clone();

        let at = now();
        for _ in 0..5 {
            db.record_play(&play(&id, at, false)).unwrap();
        }

        // The rows are kept — "started and abandoned" is real information —
        // but nothing that ranks by listening counts them.
        assert_eq!(db.recent_plays(50).unwrap().len(), 5);
        assert_eq!(db.counted_play_total().unwrap(), 0);
        assert!(db.replay_mix(20, at - 30 * DAY, 2).unwrap().is_empty());
    }

    #[test]
    fn the_replay_mix_wants_repeats_not_one_offs() {
        let db = Db::open_in_memory().unwrap();
        for (i, title) in ["Repeated", "Once"].iter().enumerate() {
            db.upsert_track(&track(title, "Artist", "Album", &format!("/m/{i}.flac")))
                .unwrap();
        }
        let tracks = db.all_tracks().unwrap();
        let repeated = tracks.iter().find(|t| t.title == "Repeated").unwrap();
        let once = tracks.iter().find(|t| t.title == "Once").unwrap();

        let at = now();
        seed_plays(&db, &repeated.id, 4, at);
        seed_plays(&db, &once.id, 1, at);

        let mix = db.replay_mix(20, at - 30 * DAY, 2).unwrap();
        let titles: Vec<_> = mix.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, vec!["Repeated"]);
        assert!(db.replay_mix(20, at - 30 * DAY, 5).unwrap().is_empty());
    }

    #[test]
    fn the_replay_mix_ignores_plays_outside_its_window() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track("Old", "Artist", "Album", "/m/old.flac"))
            .unwrap();
        let id = db.all_tracks().unwrap()[0].id.clone();

        let at = now();
        seed_plays(&db, &id, 5, at - 200 * DAY);

        assert!(db.replay_mix(20, at - 30 * DAY, 2).unwrap().is_empty());
        // The same song is exactly what the archive mix is for.
        let archive = db.archive_mix(20, at - 60 * DAY, 3).unwrap();
        assert_eq!(archive.len(), 1);
        assert_eq!(archive[0].title, "Old");
        assert!(db.archive_mix(20, at - 60 * DAY, 6).unwrap().is_empty());
    }

    /// A song still in rotation is not "archived", however often it was played.
    #[test]
    fn the_archive_mix_excludes_anything_played_lately() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track("Current", "Artist", "Album", "/m/cur.flac"))
            .unwrap();
        let id = db.all_tracks().unwrap()[0].id.clone();

        let at = now();
        seed_plays(&db, &id, 10, at);

        assert!(db.archive_mix(20, at - 60 * DAY, 3).unwrap().is_empty());
    }

    /// The whole point of the tiering: a stray non-music file that was never
    /// played, by an artist never played, must not be recommended.
    #[test]
    fn the_discover_mix_leaves_unplayed_strays_alone() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track("Barely", "Known", "Record", "/m/barely.flac"))
            .unwrap();
        db.upsert_track(&track("Voice Memo", "", "", "/m/memo.m4a"))
            .unwrap();
        let tracks = db.all_tracks().unwrap();
        let barely = tracks.iter().find(|t| t.title == "Barely").unwrap();

        seed_plays(&db, &barely.id, 2, now());

        let mix = db.discover_mix(20, 3).unwrap();
        let titles: Vec<_> = mix.iter().map(|t| t.title.as_str()).collect();
        assert!(titles.contains(&"Barely"), "got {titles:?}");
        assert!(!titles.contains(&"Voice Memo"), "got {titles:?}");
    }

    /// An unplayed track earns its place through the album around it.
    #[test]
    fn the_discover_mix_reaches_unplayed_tracks_on_a_known_album() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track("Heard", "Artist", "Album", "/m/1.flac"))
            .unwrap();
        db.upsert_track(&track("Unheard", "Artist", "Album", "/m/2.flac"))
            .unwrap();
        let tracks = db.all_tracks().unwrap();
        let heard = tracks.iter().find(|t| t.title == "Heard").unwrap();

        seed_plays(&db, &heard.id, 2, now());

        let titles: Vec<_> = db
            .discover_mix(20, 3)
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert!(titles.iter().any(|t| t == "Unheard"), "got {titles:?}");
    }

    #[test]
    fn a_heavily_played_song_is_not_a_discovery() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track("Favourite", "Artist", "Album", "/m/fav.flac"))
            .unwrap();
        let id = db.all_tracks().unwrap()[0].id.clone();
        seed_plays(&db, &id, 12, now());

        let titles: Vec<_> = db
            .discover_mix(20, 3)
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert!(!titles.iter().any(|t| t == "Favourite"), "got {titles:?}");
        assert!(db
            .discover_mix(20, 12)
            .unwrap()
            .iter()
            .any(|track| track.title == "Favourite"));
    }

    #[test]
    fn a_half_heard_album_becomes_a_finish_it_pick() {
        let db = Db::open_in_memory().unwrap();
        for i in 0..4 {
            db.upsert_track(&track(
                &format!("Track {i}"),
                "Artist",
                "Half Album",
                &format!("/m/{i}.flac"),
            ))
            .unwrap();
        }
        let tracks = db.all_tracks().unwrap();
        seed_plays(&db, &tracks[0].id, 2, now());

        let picks = db.top_picks(6, now()).unwrap();
        let album = picks
            .iter()
            .find(|p| p.kind == "album")
            .expect("expected an album pick");
        assert_eq!(album.title, "Half Album");
        assert_eq!(album.reason, "1 of 4 played");
        // Playing the pick should enqueue the whole album, not just the rest.
        assert_eq!(album.track_ids.len(), 4);
    }

    /// A fully-heard album has nothing left to finish.
    #[test]
    fn a_complete_album_is_not_offered_to_be_finished() {
        let db = Db::open_in_memory().unwrap();
        for i in 0..2 {
            db.upsert_track(&track(
                &format!("Track {i}"),
                "Artist",
                "Done",
                &format!("/m/{i}.flac"),
            ))
            .unwrap();
        }
        for t in db.all_tracks().unwrap() {
            seed_plays(&db, &t.id, 2, now());
        }

        let picks = db.top_picks(6, now()).unwrap();
        assert!(!picks.iter().any(|p| p.title == "Done"));
    }

    #[test]
    fn a_long_forgotten_song_is_labelled_with_its_year() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track("Forgotten", "Artist", "Album", "/m/f.flac"))
            .unwrap();
        let id = db.all_tracks().unwrap()[0].id.clone();

        // 2021-01-01, comfortably over a year before any plausible "now".
        db.record_play(&play(&id, 1_609_459_200, true)).unwrap();

        let picks = db.top_picks(6, now()).unwrap();
        let pick = picks
            .iter()
            .find(|p| p.title == "Forgotten")
            .expect("expected the stale song");
        assert_eq!(pick.reason, "Not played since 2021");
    }

    #[test]
    fn playlists_are_ranked_by_when_they_were_last_played_from() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track("Song", "Artist", "Album", "/m/a.flac"))
            .unwrap();
        let id = db.all_tracks().unwrap()[0].id.clone();

        let at = now();
        for (playlist, when) in [("older", at - 5 * DAY), ("newer", at)] {
            let mut row = play(&id, when, true);
            row.context_kind = Some("playlist".into());
            row.context_id = Some(playlist.into());
            db.record_play(&row).unwrap();
        }

        assert_eq!(db.recent_playlist_ids(10).unwrap(), vec!["newer", "older"]);
    }

    #[test]
    fn clearing_one_songs_history_leaves_other_songs_history() {
        let db = Db::open_in_memory().unwrap();
        for (title, path) in [("Clear", "/m/a.flac"), ("Keep", "/m/b.flac")] {
            db.upsert_track(&track(title, "Artist", "Album", path))
                .unwrap();
        }
        let tracks = db.all_tracks().unwrap();
        let clear = tracks.iter().find(|track| track.title == "Clear").unwrap();
        let keep = tracks.iter().find(|track| track.title == "Keep").unwrap();
        seed_plays(&db, &clear.id, 2, now());
        seed_plays(&db, &keep.id, 3, now());

        db.clear_history_for_song(&clear.id).unwrap();

        let history = db.recent_plays(10).unwrap();
        assert_eq!(history.len(), 3);
        assert!(history.iter().all(|record| record.play.song_id == keep.id));
    }

    #[test]
    fn clearing_history_leaves_the_library_alone() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&track("Song", "Artist", "Album", "/m/a.flac"))
            .unwrap();
        let id = db.all_tracks().unwrap()[0].id.clone();
        seed_plays(&db, &id, 3, now());

        db.clear_history().unwrap();

        assert_eq!(db.counted_play_total().unwrap(), 0);
        assert!(db.recent_plays(50).unwrap().is_empty());
        assert_eq!(db.all_tracks().unwrap().len(), 1);
    }

    /// History outlives the library: a rescan deletes and recreates song rows,
    /// and the read path must not choke on rows pointing at songs that are gone.
    #[test]
    fn history_survives_the_song_it_refers_to_disappearing() {
        let db = Db::open_in_memory().unwrap();
        db.record_play(&play("song-that-never-existed", now(), true))
            .unwrap();

        let history = db.recent_plays(10).unwrap();
        assert_eq!(history.len(), 1);
        assert!(history[0].track.is_none());
        assert!(db.replay_mix(10, 0, 2).unwrap().is_empty());
    }

    #[test]
    fn years_are_derived_correctly_across_leap_boundaries() {
        assert_eq!(year_of(0), 1970);
        // Precise boundaries either side of a leap day.
        assert_eq!(year_of(951_782_400), 2000); // 2000-02-29
        assert_eq!(year_of(1_609_459_199), 2020); // last second of 2020
        assert_eq!(year_of(1_609_459_200), 2021); // first second of 2021
        assert_eq!(year_of(1_704_067_200), 2024);
        assert_eq!(year_of(4_102_444_800), 2100); // not a leap year
    }
}
