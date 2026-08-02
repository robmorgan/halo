//! SQLite track library: imported tracks with tag metadata, the playlist
//! tree, and the analysis cache (one `PreAnalysisArtifact` per track, stored
//! at the file's native sample rate and rescaled per device on load).
//!
//! `rusqlite::Connection` is not `Sync`, so each thread that touches the DB
//! (UI, analysis worker, folder importer) opens its own `Library`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use timestretch::PreAnalysisArtifact;

/// File extensions Halo can decode (must stay in sync with the symphonia
/// features in Cargo.toml).
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "wav"];

#[derive(Debug, Clone)]
pub struct TrackRow {
    pub id: i64,
    pub path: PathBuf,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub key: Option<String>,
    pub duration_secs: Option<f64>,
    pub bpm: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct PlaylistRow {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub is_folder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SortColumn {
    Title,
    Artist,
    Album,
    Bpm,
    Key,
    Duration,
}

impl SortColumn {
    fn sql(self) -> &'static str {
        match self {
            SortColumn::Title => "title COLLATE NOCASE",
            SortColumn::Artist => "artist COLLATE NOCASE",
            SortColumn::Album => "album COLLATE NOCASE",
            SortColumn::Bpm => "bpm",
            SortColumn::Key => "key COLLATE NOCASE",
            SortColumn::Duration => "duration_secs",
        }
    }
}

/// Tag metadata for one file, ready to insert.
#[derive(Debug, Clone, Default)]
pub struct TrackMeta {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub key: Option<String>,
    pub duration_secs: Option<f64>,
}

pub struct Library {
    conn: Connection,
}

impl Library {
    /// Default database location; `HALO_DB` overrides it (dev/test hook).
    pub fn default_path() -> PathBuf {
        if let Some(p) = std::env::var_os("HALO_DB") {
            return PathBuf::from(p);
        }
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Halo")
            .join("halo.db")
    }

    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("create {dir:?}: {e}"))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("open {path:?}: {e}"))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS tracks (
                 id INTEGER PRIMARY KEY,
                 path TEXT NOT NULL UNIQUE,
                 title TEXT NOT NULL,
                 artist TEXT,
                 album TEXT,
                 key TEXT,
                 duration_secs REAL,
                 bpm REAL,
                 native_sample_rate INTEGER,
                 analysis_json TEXT,
                 added_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS playlists (
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL,
                 parent_id INTEGER REFERENCES playlists(id) ON DELETE CASCADE,
                 is_folder INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS playlist_tracks (
                 playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                 track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                 position INTEGER NOT NULL,
                 PRIMARY KEY (playlist_id, track_id)
             );
             CREATE INDEX IF NOT EXISTS idx_tracks_bpm ON tracks(bpm);
             CREATE TABLE IF NOT EXISTS lighting_cues (
                 track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
                 cues_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS settings (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );",
        )
        .map_err(|e| format!("schema: {e}"))?;
        Ok(Self { conn })
    }

    /// Insert (or find) a track. Existing rows keep their analysis; tags are
    /// refreshed. Returns the track id.
    pub fn upsert_track(&self, path: &Path, meta: &TrackMeta) -> Result<i64, String> {
        let title = meta.title.clone().unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        });
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.conn
            .execute(
                "INSERT INTO tracks (path, title, artist, album, key, duration_secs, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(path) DO UPDATE SET
                     title = excluded.title,
                     artist = excluded.artist,
                     album = excluded.album,
                     key = excluded.key,
                     duration_secs = COALESCE(excluded.duration_secs, tracks.duration_secs)",
                params![
                    path.to_string_lossy(),
                    title,
                    meta.artist,
                    meta.album,
                    meta.key,
                    meta.duration_secs,
                    now
                ],
            )
            .map_err(|e| format!("upsert track: {e}"))?;
        self.conn
            .query_row(
                "SELECT id FROM tracks WHERE path = ?1",
                params![path.to_string_lossy()],
                |r| r.get(0),
            )
            .map_err(|e| format!("track id: {e}"))
    }

    /// Import one audio file: read tags, upsert, and adopt a Phase-2 sidecar
    /// as the analysis if the track has none yet (one-time migration; no new
    /// sidecars are ever written).
    pub fn import_file(&self, path: &Path) -> Result<i64, String> {
        let id = self.upsert_track(path, &read_meta(path))?;
        if self.analysis_json(id)?.is_none() {
            let sidecar = sidecar_path(path);
            if sidecar.exists()
                && let Ok(artifact) = timestretch::read_preanalysis_json(&sidecar)
            {
                log::info!("Importing sidecar {}", sidecar.display());
                self.store_analysis(id, &artifact)?;
            }
        }
        Ok(id)
    }

    /// Recursively import a folder. Returns the number of audio files seen.
    pub fn import_folder(&self, dir: &Path) -> Result<usize, String> {
        let mut count = 0;
        let entries = std::fs::read_dir(dir).map_err(|e| format!("read {dir:?}: {e}"))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += self.import_folder(&path).unwrap_or(0);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
            {
                match self.import_file(&path) {
                    Ok(_) => count += 1,
                    Err(e) => log::warn!("import {}: {e}", path.display()),
                }
            }
        }
        Ok(count)
    }

    pub fn store_analysis(
        &self,
        track_id: i64,
        artifact: &PreAnalysisArtifact,
    ) -> Result<(), String> {
        let json = serde_json::to_string(artifact).map_err(|e| format!("serialize: {e}"))?;
        self.conn
            .execute(
                "UPDATE tracks SET analysis_json = ?1, bpm = ?2, native_sample_rate = ?3
                 WHERE id = ?4",
                params![json, artifact.bpm, artifact.sample_rate, track_id],
            )
            .map_err(|e| format!("store analysis: {e}"))?;
        Ok(())
    }

    fn analysis_json(&self, track_id: i64) -> Result<Option<String>, String> {
        self.conn
            .query_row(
                "SELECT analysis_json FROM tracks WHERE id = ?1",
                params![track_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("analysis: {e}"))
            .map(Option::flatten)
    }

    /// The stored artifact at its native rate, if analyzed. An empty string
    /// is the "analysis failed" marker and reads as `None`.
    pub fn analysis(&self, track_id: i64) -> Result<Option<PreAnalysisArtifact>, String> {
        match self.analysis_json(track_id)? {
            Some(json) if !json.is_empty() => serde_json::from_str(&json)
                .map(Some)
                .map_err(|e| format!("parse analysis: {e}")),
            _ => Ok(None),
        }
    }

    /// Persist a track's lighting cues (JSON, times in seconds).
    pub fn store_cues(
        &self,
        track_id: i64,
        file: &halo_light::cues::CueFile,
    ) -> Result<(), String> {
        let json = serde_json::to_string(file).map_err(|e| format!("serialize cues: {e}"))?;
        self.conn
            .execute(
                "INSERT INTO lighting_cues (track_id, cues_json) VALUES (?1, ?2)
                 ON CONFLICT(track_id) DO UPDATE SET cues_json = excluded.cues_json",
                params![track_id, json],
            )
            .map_err(|e| format!("store cues: {e}"))?;
        Ok(())
    }

    /// The stored lighting cues for a track, if any were authored.
    pub fn cues(&self, track_id: i64) -> Result<Option<halo_light::cues::CueFile>, String> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT cues_json FROM lighting_cues WHERE track_id = ?1",
                params![track_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("cues: {e}"))?;
        match json {
            Some(json) => serde_json::from_str(&json)
                .map(Some)
                .map_err(|e| format!("parse cues: {e}")),
            None => Ok(None),
        }
    }

    /// App-level setting (rig patch, Art-Net config, …), stored as JSON.
    pub fn setting(&self, key: &str) -> Result<Option<String>, String> {
        self.conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("setting {key}: {e}"))
    }

    pub fn store_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(|e| format!("store setting {key}: {e}"))?;
        Ok(())
    }

    /// Mark a track as failed analysis (empty JSON) so the queue moves on
    /// instead of retrying an undecodable file forever.
    pub fn store_analysis_failure(&self, track_id: i64) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE tracks SET analysis_json = '' WHERE id = ?1",
                params![track_id],
            )
            .map_err(|e| format!("store failure: {e}"))?;
        Ok(())
    }

    /// Queue a track for re-analysis: clearing the stored analysis re-arms
    /// `next_unanalyzed`. Keeps the `bpm` column so the browser readout
    /// doesn't blink to "—" while the worker runs.
    pub fn clear_analysis(&self, track_id: i64) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE tracks SET analysis_json = NULL WHERE id = ?1",
                params![track_id],
            )
            .map_err(|e| format!("clear analysis: {e}"))?;
        Ok(())
    }

    /// Tracks still waiting for analysis (for the status readout).
    pub fn unanalyzed_count(&self) -> Result<i64, String> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM tracks WHERE analysis_json IS NULL",
                [],
                |r| r.get(0),
            )
            .map_err(|e| format!("count: {e}"))
    }

    /// Next track without analysis, oldest first.
    pub fn next_unanalyzed(&self) -> Result<Option<(i64, PathBuf)>, String> {
        self.conn
            .query_row(
                "SELECT id, path FROM tracks WHERE analysis_json IS NULL
                 ORDER BY added_at, id LIMIT 1",
                [],
                |r| Ok((r.get::<_, i64>(0)?, PathBuf::from(r.get::<_, String>(1)?))),
            )
            .optional()
            .map_err(|e| format!("next unanalyzed: {e}"))
    }

    pub fn track(&self, track_id: i64) -> Result<Option<TrackRow>, String> {
        self.conn
            .query_row(
                "SELECT id, path, title, artist, album, key, duration_secs, bpm
                 FROM tracks WHERE id = ?1",
                params![track_id],
                row_to_track,
            )
            .optional()
            .map_err(|e| format!("track: {e}"))
    }

    /// Tracks matching a search filter, optionally restricted to a playlist,
    /// sorted by `sort`.
    pub fn tracks(
        &self,
        playlist: Option<i64>,
        search: &str,
        sort: SortColumn,
        ascending: bool,
    ) -> Result<Vec<TrackRow>, String> {
        let dir = if ascending { "ASC" } else { "DESC" };
        let base = "SELECT t.id, t.path, t.title, t.artist, t.album, t.key,
                           t.duration_secs, t.bpm FROM tracks t";
        let (join, where_pl) = match playlist {
            Some(_) => (
                " JOIN playlist_tracks pt ON pt.track_id = t.id",
                " AND pt.playlist_id = ?2",
            ),
            None => ("", ""),
        };
        let sql = format!(
            "{base}{join} WHERE (t.title LIKE ?1 OR t.artist LIKE ?1 OR t.album LIKE ?1){where_pl}
             ORDER BY {} {dir} NULLS LAST, t.title COLLATE NOCASE ASC",
            sort.sql()
        );
        let pattern = format!("%{search}%");
        let mut stmt = self.conn.prepare(&sql).map_err(|e| format!("query: {e}"))?;
        let rows = match playlist {
            Some(pl) => stmt
                .query_map(params![pattern, pl], row_to_track)
                .map_err(|e| format!("query: {e}"))?
                .collect::<Result<Vec<_>, _>>(),
            None => stmt
                .query_map(params![pattern], row_to_track)
                .map_err(|e| format!("query: {e}"))?
                .collect::<Result<Vec<_>, _>>(),
        };
        rows.map_err(|e| format!("rows: {e}"))
    }

    // ---- Playlists ----

    pub fn playlists(&self) -> Result<Vec<PlaylistRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, name, parent_id, is_folder FROM playlists ORDER BY name COLLATE NOCASE",
            )
            .map_err(|e| format!("playlists: {e}"))?;
        stmt.query_map([], |r| {
            Ok(PlaylistRow {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                is_folder: r.get::<_, i64>(3)? != 0,
            })
        })
        .map_err(|e| format!("playlists: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("playlists: {e}"))
    }

    pub fn create_playlist(
        &self,
        name: &str,
        parent: Option<i64>,
        is_folder: bool,
    ) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO playlists (name, parent_id, is_folder) VALUES (?1, ?2, ?3)",
                params![name, parent, is_folder as i64],
            )
            .map_err(|e| format!("create playlist: {e}"))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE playlists SET name = ?1 WHERE id = ?2",
                params![name, id],
            )
            .map_err(|e| format!("rename playlist: {e}"))?;
        Ok(())
    }

    pub fn delete_playlist(&self, id: i64) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM playlists WHERE id = ?1", params![id])
            .map_err(|e| format!("delete playlist: {e}"))?;
        Ok(())
    }

    pub fn add_to_playlist(&self, playlist: i64, track: i64) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position)
                 VALUES (?1, ?2,
                         (SELECT COALESCE(MAX(position), 0) + 1 FROM playlist_tracks
                          WHERE playlist_id = ?1))",
                params![playlist, track],
            )
            .map_err(|e| format!("add to playlist: {e}"))?;
        Ok(())
    }

    pub fn remove_from_playlist(&self, playlist: i64, track: i64) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2",
                params![playlist, track],
            )
            .map_err(|e| format!("remove from playlist: {e}"))?;
        Ok(())
    }

    /// Remove a track row; foreign keys cascade playlist membership and
    /// lighting cues. The audio file on disk is untouched.
    pub fn delete_track(&self, track_id: i64) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM tracks WHERE id = ?1", params![track_id])
            .map_err(|e| format!("delete track: {e}"))?;
        Ok(())
    }
}

fn row_to_track(r: &rusqlite::Row<'_>) -> rusqlite::Result<TrackRow> {
    Ok(TrackRow {
        id: r.get(0)?,
        path: PathBuf::from(r.get::<_, String>(1)?),
        title: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        key: r.get(5)?,
        duration_secs: r.get(6)?,
        bpm: r.get(7)?,
    })
}

/// Best-effort tag read for import (no audio decode; duration comes from
/// the container properties).
pub fn read_meta(path: &Path) -> TrackMeta {
    use lofty::prelude::*;

    let Ok(tagged) = lofty::probe::Probe::open(path).and_then(|p| p.read()) else {
        return TrackMeta::default();
    };
    let duration_secs = Some(tagged.properties().duration().as_secs_f64());
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return TrackMeta {
            duration_secs,
            ..Default::default()
        };
    };
    TrackMeta {
        title: tag.title().map(|s| s.into_owned()),
        artist: tag.artist().map(|s| s.into_owned()),
        album: tag.album().map(|s| s.into_owned()),
        key: tag
            .get_string(&lofty::tag::ItemKey::InitialKey)
            .map(|s| s.to_string()),
        duration_secs,
    }
}

/// Phase-2 sidecar path for a track (read-only legacy cache).
fn sidecar_path(audio_path: &Path) -> PathBuf {
    let mut os = audio_path.as_os_str().to_os_string();
    os.push(".halo.tsanalysis.json");
    PathBuf::from(os)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_library() -> Library {
        let conn = Connection::open_in_memory().unwrap();
        // Reuse the schema by round-tripping through open(): not possible
        // in-memory via path, so replicate minimally.
        let lib = Library { conn };
        lib.conn
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE tracks (
                     id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE,
                     title TEXT NOT NULL, artist TEXT, album TEXT, key TEXT,
                     duration_secs REAL, bpm REAL, native_sample_rate INTEGER,
                     analysis_json TEXT, added_at INTEGER NOT NULL);
                 CREATE TABLE playlists (
                     id INTEGER PRIMARY KEY, name TEXT NOT NULL,
                     parent_id INTEGER REFERENCES playlists(id) ON DELETE CASCADE,
                     is_folder INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE playlist_tracks (
                     playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                     track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                     position INTEGER NOT NULL,
                     PRIMARY KEY (playlist_id, track_id));
                 CREATE TABLE lighting_cues (
                     track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
                     cues_json TEXT NOT NULL);
                 CREATE TABLE settings (
                     key TEXT PRIMARY KEY, value TEXT NOT NULL);",
            )
            .unwrap();
        lib
    }

    fn meta(title: &str, artist: &str, bpm: Option<f64>) -> (TrackMeta, Option<f64>) {
        (
            TrackMeta {
                title: Some(title.into()),
                artist: Some(artist.into()),
                ..Default::default()
            },
            bpm,
        )
    }

    fn insert(lib: &Library, path: &str, title: &str, artist: &str, bpm: Option<f64>) -> i64 {
        let (m, bpm) = meta(title, artist, bpm);
        let id = lib.upsert_track(Path::new(path), &m).unwrap();
        if let Some(bpm) = bpm {
            lib.conn
                .execute("UPDATE tracks SET bpm = ?1 WHERE id = ?2", params![bpm, id])
                .unwrap();
        }
        id
    }

    #[test]
    fn upsert_is_idempotent_and_refreshes_tags() {
        let lib = mem_library();
        let a = insert(&lib, "/x/a.mp3", "One", "AA", None);
        let (m2, _) = meta("One (Remix)", "AA", None);
        let b = lib.upsert_track(Path::new("/x/a.mp3"), &m2).unwrap();
        assert_eq!(a, b);
        let rows = lib.tracks(None, "", SortColumn::Title, true).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "One (Remix)");
    }

    #[test]
    fn cues_round_trip() {
        let lib = mem_library();
        let id = insert(&lib, "/x/a.mp3", "One", "AA", None);
        assert!(lib.cues(id).unwrap().is_none());

        let mut set = halo_light::cues::CueSet::empty();
        set.insert(halo_light::cues::Lane::Lighting, 44_100.0, 88_200.0, 0.9);
        lib.store_cues(id, &set.to_file(44_100)).unwrap();
        let file = lib.cues(id).unwrap().unwrap();
        assert_eq!(file.lanes[0].len(), 1);
        assert!((file.lanes[0][0].start - 1.0).abs() < 1e-9);

        // Upsert replaces, not duplicates.
        lib.store_cues(id, &halo_light::cues::CueSet::empty().to_file(44_100))
            .unwrap();
        assert!(lib.cues(id).unwrap().unwrap().lanes[0].is_empty());
    }

    #[test]
    fn settings_round_trip_and_upsert() {
        let lib = mem_library();
        assert!(lib.setting("rig_patch").unwrap().is_none());
        lib.store_setting("rig_patch", "{\"version\":1}").unwrap();
        assert_eq!(
            lib.setting("rig_patch").unwrap().as_deref(),
            Some("{\"version\":1}")
        );
        lib.store_setting("rig_patch", "{\"version\":2}").unwrap();
        assert_eq!(
            lib.setting("rig_patch").unwrap().as_deref(),
            Some("{\"version\":2}")
        );
    }

    #[test]
    fn search_and_sort() {
        let lib = mem_library();
        insert(&lib, "/x/a.mp3", "Alpha", "Zed", Some(140.0));
        insert(&lib, "/x/b.mp3", "Beta", "Ann", Some(120.0));
        insert(&lib, "/x/c.mp3", "Gamma", "Mid", None);

        let by_bpm = lib.tracks(None, "", SortColumn::Bpm, true).unwrap();
        assert_eq!(by_bpm[0].title, "Beta");
        assert_eq!(by_bpm[1].title, "Alpha");
        // NULL BPM sorts last.
        assert_eq!(by_bpm[2].title, "Gamma");

        let found = lib.tracks(None, "ann", SortColumn::Title, true).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Beta");
    }

    #[test]
    fn playlists_filter_tracks() {
        let lib = mem_library();
        let t1 = insert(&lib, "/x/a.mp3", "Alpha", "A", None);
        let t2 = insert(&lib, "/x/b.mp3", "Beta", "B", None);
        let pl = lib.create_playlist("Set", None, false).unwrap();
        lib.add_to_playlist(pl, t2).unwrap();

        let in_pl = lib.tracks(Some(pl), "", SortColumn::Title, true).unwrap();
        assert_eq!(in_pl.len(), 1);
        assert_eq!(in_pl[0].id, t2);

        lib.remove_from_playlist(pl, t2).unwrap();
        assert!(
            lib.tracks(Some(pl), "", SortColumn::Title, true)
                .unwrap()
                .is_empty()
        );
        let _ = t1;
    }

    #[test]
    fn analysis_round_trips_and_queue_drains() {
        let lib = mem_library();
        let id = insert(&lib, "/x/a.mp3", "Alpha", "A", None);
        assert_eq!(lib.next_unanalyzed().unwrap().unwrap().0, id);

        let artifact = PreAnalysisArtifact {
            version: 4,
            sample_rate: 44_100,
            bpm: 128.0,
            confidence: 0.9,
            beat_positions: vec![0, 22_050],
            ..Default::default()
        };
        lib.store_analysis(id, &artifact).unwrap();
        assert!(lib.next_unanalyzed().unwrap().is_none());

        let loaded = lib.analysis(id).unwrap().unwrap();
        assert_eq!(loaded.bpm, 128.0);
        assert_eq!(loaded.beat_positions, vec![0, 22_050]);
        // BPM column filled for the browser.
        assert_eq!(lib.track(id).unwrap().unwrap().bpm, Some(128.0));
    }

    #[test]
    fn clear_analysis_rearms_the_queue() {
        let lib = mem_library();
        let id = insert(&lib, "/x/a.mp3", "Alpha", "A", None);
        lib.store_analysis(id, &PreAnalysisArtifact::default())
            .unwrap();
        assert!(lib.next_unanalyzed().unwrap().is_none());

        lib.clear_analysis(id).unwrap();
        assert_eq!(lib.next_unanalyzed().unwrap().unwrap().0, id);
        // A failed analysis ('' marker) can be re-armed the same way.
        lib.store_analysis_failure(id).unwrap();
        assert!(lib.next_unanalyzed().unwrap().is_none());
        lib.clear_analysis(id).unwrap();
        assert_eq!(lib.next_unanalyzed().unwrap().unwrap().0, id);
    }

    #[test]
    fn delete_track_cascades_playlists_and_cues() {
        let lib = mem_library();
        let id = insert(&lib, "/x/a.mp3", "Alpha", "A", None);
        let pl = lib.create_playlist("Set", None, false).unwrap();
        lib.add_to_playlist(pl, id).unwrap();
        lib.store_cues(id, &halo_light::cues::CueSet::empty().to_file(44_100))
            .unwrap();

        lib.delete_track(id).unwrap();
        assert!(lib.track(id).unwrap().is_none());
        assert!(
            lib.tracks(Some(pl), "", SortColumn::Title, true)
                .unwrap()
                .is_empty(),
            "playlist membership cascades"
        );
        assert!(lib.cues(id).unwrap().is_none(), "lighting cues cascade");
        // The playlist itself survives.
        assert_eq!(lib.playlists().unwrap().len(), 1);
    }

    #[test]
    fn playlist_tree_and_rename() {
        let lib = mem_library();
        let folder = lib.create_playlist("House", None, true).unwrap();
        let pl = lib
            .create_playlist("Peak Time", Some(folder), false)
            .unwrap();
        lib.rename_playlist(pl, "Warmup").unwrap();
        let all = lib.playlists().unwrap();
        assert_eq!(all.len(), 2);
        let renamed = all.iter().find(|p| p.id == pl).unwrap();
        assert_eq!(renamed.name, "Warmup");
        assert_eq!(renamed.parent_id, Some(folder));
        lib.delete_playlist(folder).unwrap();
        assert!(lib.playlists().unwrap().is_empty(), "cascade delete");
    }
}
