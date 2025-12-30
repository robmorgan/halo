//! SQLite database for the DJ library.

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result as SqliteResult};

use super::types::{AudioFormat, BeatGrid, HotCue, Track, TrackId, TrackWaveform};

/// Database connection wrapper for the DJ library.
pub struct LibraryDatabase {
    conn: Connection,
}

impl LibraryDatabase {
    /// Open or create a database at the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;

        // Enable WAL mode for better concurrent access
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        let db = Self { conn };
        db.create_tables()?;

        Ok(db)
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.create_tables()?;
        Ok(db)
    }

    /// Create the database tables if they don't exist.
    fn create_tables(&self) -> SqliteResult<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS tracks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                artist TEXT,
                album TEXT,
                duration_seconds REAL NOT NULL,
                bpm REAL,
                musical_key TEXT,
                format TEXT NOT NULL,
                sample_rate INTEGER NOT NULL,
                bit_depth INTEGER NOT NULL,
                channels INTEGER NOT NULL,
                file_size_bytes INTEGER NOT NULL,
                date_added TEXT NOT NULL,
                last_played TEXT,
                play_count INTEGER DEFAULT 0,
                rating INTEGER DEFAULT 0,
                comment TEXT
            );

            CREATE TABLE IF NOT EXISTS beat_grids (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                track_id INTEGER NOT NULL UNIQUE,
                bpm REAL NOT NULL,
                first_beat_offset_ms REAL NOT NULL,
                beat_positions BLOB,
                confidence REAL NOT NULL,
                analyzed_at TEXT NOT NULL,
                algorithm_version TEXT NOT NULL,
                FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS waveforms (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                track_id INTEGER NOT NULL UNIQUE,
                samples BLOB NOT NULL,
                sample_count INTEGER NOT NULL,
                duration_seconds REAL NOT NULL,
                FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS hot_cues (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                track_id INTEGER NOT NULL,
                slot INTEGER NOT NULL,
                position_seconds REAL NOT NULL,
                name TEXT,
                color_r INTEGER,
                color_g INTEGER,
                color_b INTEGER,
                created_at TEXT NOT NULL,
                FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE,
                UNIQUE(track_id, slot)
            );

            CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
            CREATE INDEX IF NOT EXISTS idx_tracks_bpm ON tracks(bpm);
            CREATE INDEX IF NOT EXISTS idx_tracks_date_added ON tracks(date_added);
            "#,
        )?;
        Ok(())
    }

    /// Insert a new track into the database.
    pub fn insert_track(&self, track: &Track) -> SqliteResult<TrackId> {
        self.conn.execute(
            r#"
            INSERT INTO tracks (
                file_path, title, artist, album, duration_seconds, bpm, musical_key,
                format, sample_rate, bit_depth, channels, file_size_bytes,
                date_added, last_played, play_count, rating, comment
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            "#,
            params![
                track.file_path,
                track.title,
                track.artist,
                track.album,
                track.duration_seconds,
                track.bpm,
                track.key,
                track.format.as_str(),
                track.sample_rate,
                track.bit_depth,
                track.channels,
                track.file_size_bytes,
                track.date_added.to_rfc3339(),
                track.last_played.map(|dt| dt.to_rfc3339()),
                track.play_count,
                track.rating,
                track.comment,
            ],
        )?;

        Ok(TrackId(self.conn.last_insert_rowid()))
    }

    /// Get a track by ID.
    pub fn get_track(&self, id: TrackId) -> SqliteResult<Option<Track>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, file_path, title, artist, album, duration_seconds, bpm, musical_key,
                   format, sample_rate, bit_depth, channels, file_size_bytes,
                   date_added, last_played, play_count, rating, comment
            FROM tracks WHERE id = ?1
            "#,
        )?;

        let mut rows = stmt.query(params![id.0])?;

        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_track(row)?))
        } else {
            Ok(None)
        }
    }

    /// Get a track by file path.
    pub fn get_track_by_path(&self, path: &str) -> SqliteResult<Option<Track>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, file_path, title, artist, album, duration_seconds, bpm, musical_key,
                   format, sample_rate, bit_depth, channels, file_size_bytes,
                   date_added, last_played, play_count, rating, comment
            FROM tracks WHERE file_path = ?1
            "#,
        )?;

        let mut rows = stmt.query(params![path])?;

        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_track(row)?))
        } else {
            Ok(None)
        }
    }

    /// Get all tracks in the library.
    pub fn get_all_tracks(&self) -> SqliteResult<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, file_path, title, artist, album, duration_seconds, bpm, musical_key,
                   format, sample_rate, bit_depth, channels, file_size_bytes,
                   date_added, last_played, play_count, rating, comment
            FROM tracks ORDER BY date_added DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| Self::row_to_track(row))?;

        rows.collect()
    }

    /// Search tracks by title or artist.
    pub fn search_tracks(&self, query: &str) -> SqliteResult<Vec<Track>> {
        let search_pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, file_path, title, artist, album, duration_seconds, bpm, musical_key,
                   format, sample_rate, bit_depth, channels, file_size_bytes,
                   date_added, last_played, play_count, rating, comment
            FROM tracks
            WHERE title LIKE ?1 OR artist LIKE ?1 OR album LIKE ?1
            ORDER BY title
            "#,
        )?;

        let rows = stmt.query_map(params![search_pattern], |row| Self::row_to_track(row))?;

        rows.collect()
    }

    /// Update the BPM for a track.
    pub fn update_track_bpm(&self, id: TrackId, bpm: f64) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE tracks SET bpm = ?1 WHERE id = ?2",
            params![bpm, id.0],
        )?;
        Ok(())
    }

    /// Update the play count and last played time.
    pub fn update_track_played(&self, id: TrackId) -> SqliteResult<()> {
        self.conn.execute(
            r#"
            UPDATE tracks
            SET play_count = play_count + 1, last_played = ?1
            WHERE id = ?2
            "#,
            params![Utc::now().to_rfc3339(), id.0],
        )?;
        Ok(())
    }

    /// Delete a track from the database.
    pub fn delete_track(&self, id: TrackId) -> SqliteResult<()> {
        self.conn
            .execute("DELETE FROM tracks WHERE id = ?1", params![id.0])?;
        Ok(())
    }

    /// Get the total number of tracks.
    pub fn track_count(&self) -> SqliteResult<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    // Beat grid operations

    /// Save a beat grid.
    pub fn save_beat_grid(&self, beat_grid: &BeatGrid) -> SqliteResult<()> {
        // Serialize beat positions as JSON blob
        let positions_blob = serde_json::to_vec(&beat_grid.beat_positions).unwrap_or_default();

        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO beat_grids (
                track_id, bpm, first_beat_offset_ms, beat_positions,
                confidence, analyzed_at, algorithm_version
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                beat_grid.track_id.0,
                beat_grid.bpm,
                beat_grid.first_beat_offset_ms,
                positions_blob,
                beat_grid.confidence,
                beat_grid.analyzed_at.to_rfc3339(),
                beat_grid.algorithm_version,
            ],
        )?;
        Ok(())
    }

    /// Get the beat grid for a track.
    pub fn get_beat_grid(&self, track_id: TrackId) -> SqliteResult<Option<BeatGrid>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT track_id, bpm, first_beat_offset_ms, beat_positions,
                   confidence, analyzed_at, algorithm_version
            FROM beat_grids WHERE track_id = ?1
            "#,
        )?;

        let mut rows = stmt.query(params![track_id.0])?;

        if let Some(row) = rows.next()? {
            let positions_blob: Vec<u8> = row.get(3)?;
            let beat_positions: Vec<f64> =
                serde_json::from_slice(&positions_blob).unwrap_or_default();
            let analyzed_at_str: String = row.get(5)?;

            Ok(Some(BeatGrid {
                track_id: TrackId(row.get(0)?),
                bpm: row.get(1)?,
                first_beat_offset_ms: row.get(2)?,
                beat_positions,
                confidence: row.get(4)?,
                analyzed_at: DateTime::parse_from_rfc3339(&analyzed_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                algorithm_version: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    // Hot cue operations

    /// Save a hot cue.
    pub fn save_hot_cue(&self, hot_cue: &HotCue) -> SqliteResult<i64> {
        let (color_r, color_g, color_b) = hot_cue.color.unwrap_or((255, 255, 255));

        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO hot_cues (
                track_id, slot, position_seconds, name,
                color_r, color_g, color_b, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                hot_cue.track_id.0,
                hot_cue.slot,
                hot_cue.position_seconds,
                hot_cue.name,
                color_r,
                color_g,
                color_b,
                hot_cue.created_at.to_rfc3339(),
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Get all hot cues for a track.
    pub fn get_hot_cues(&self, track_id: TrackId) -> SqliteResult<Vec<HotCue>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, track_id, slot, position_seconds, name,
                   color_r, color_g, color_b, created_at
            FROM hot_cues WHERE track_id = ?1 ORDER BY slot
            "#,
        )?;

        let rows = stmt.query_map(params![track_id.0], |row| {
            let created_at_str: String = row.get(8)?;
            let color_r: Option<u8> = row.get(5)?;
            let color_g: Option<u8> = row.get(6)?;
            let color_b: Option<u8> = row.get(7)?;

            Ok(HotCue {
                id: row.get(0)?,
                track_id: TrackId(row.get(1)?),
                slot: row.get(2)?,
                position_seconds: row.get(3)?,
                name: row.get(4)?,
                color: color_r
                    .zip(color_g)
                    .zip(color_b)
                    .map(|((r, g), b)| (r, g, b)),
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;

        rows.collect()
    }

    /// Delete a hot cue.
    pub fn delete_hot_cue(&self, track_id: TrackId, slot: u8) -> SqliteResult<()> {
        self.conn.execute(
            "DELETE FROM hot_cues WHERE track_id = ?1 AND slot = ?2",
            params![track_id.0, slot],
        )?;
        Ok(())
    }

    // Waveform operations

    /// Save a waveform.
    pub fn save_waveform(&self, waveform: &TrackWaveform) -> SqliteResult<()> {
        // Convert f32 samples to bytes
        let samples_bytes: Vec<u8> = waveform
            .samples
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();

        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO waveforms (
                track_id, samples, sample_count, duration_seconds
            ) VALUES (?1, ?2, ?3, ?4)
            "#,
            params![
                waveform.track_id.0,
                samples_bytes,
                waveform.sample_count,
                waveform.duration_seconds,
            ],
        )?;
        Ok(())
    }

    /// Get the waveform for a track.
    pub fn get_waveform(&self, track_id: TrackId) -> SqliteResult<Option<TrackWaveform>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT track_id, samples, sample_count, duration_seconds
            FROM waveforms WHERE track_id = ?1
            "#,
        )?;

        let mut rows = stmt.query(params![track_id.0])?;

        if let Some(row) = rows.next()? {
            let samples_bytes: Vec<u8> = row.get(1)?;
            let samples: Vec<f32> = samples_bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            Ok(Some(TrackWaveform {
                track_id: TrackId(row.get(0)?),
                samples,
                sample_count: row.get(2)?,
                duration_seconds: row.get(3)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Convert a database row to a Track.
    fn row_to_track(row: &rusqlite::Row) -> SqliteResult<Track> {
        let format_str: String = row.get(8)?;
        let date_added_str: String = row.get(13)?;
        let last_played_str: Option<String> = row.get(14)?;

        Ok(Track {
            id: TrackId(row.get(0)?),
            file_path: row.get(1)?,
            title: row.get(2)?,
            artist: row.get(3)?,
            album: row.get(4)?,
            duration_seconds: row.get(5)?,
            bpm: row.get(6)?,
            key: row.get(7)?,
            format: AudioFormat::from_extension(&format_str),
            sample_rate: row.get(9)?,
            bit_depth: row.get(10)?,
            channels: row.get(11)?,
            file_size_bytes: row.get(12)?,
            date_added: DateTime::parse_from_rfc3339(&date_added_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            last_played: last_played_str.and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            }),
            play_count: row.get(15)?,
            rating: row.get(16)?,
            comment: row.get(17)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_database() {
        let db = LibraryDatabase::open_in_memory().unwrap();
        assert_eq!(db.track_count().unwrap(), 0);
    }

    #[test]
    fn test_insert_and_get_track() {
        let db = LibraryDatabase::open_in_memory().unwrap();

        let track = Track::new(
            TrackId(0),
            "/path/to/song.mp3".to_string(),
            "Test Song".to_string(),
            120.0,
            AudioFormat::Mp3,
            44100,
        );

        let id = db.insert_track(&track).unwrap();
        assert_eq!(id.0, 1);

        let loaded = db.get_track(id).unwrap().unwrap();
        assert_eq!(loaded.title, "Test Song");
        assert_eq!(loaded.file_path, "/path/to/song.mp3");
    }

    #[test]
    fn test_search_tracks() {
        let db = LibraryDatabase::open_in_memory().unwrap();

        let mut track1 = Track::new(
            TrackId(0),
            "/path/to/song1.mp3".to_string(),
            "Hello World".to_string(),
            120.0,
            AudioFormat::Mp3,
            44100,
        );
        track1.artist = Some("Test Artist".to_string());

        let mut track2 = Track::new(
            TrackId(0),
            "/path/to/song2.mp3".to_string(),
            "Goodbye".to_string(),
            130.0,
            AudioFormat::Mp3,
            44100,
        );
        track2.artist = Some("Other Artist".to_string());

        db.insert_track(&track1).unwrap();
        db.insert_track(&track2).unwrap();

        let results = db.search_tracks("Hello").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Hello World");

        let results = db.search_tracks("Artist").unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_hot_cues() {
        let db = LibraryDatabase::open_in_memory().unwrap();

        let track = Track::new(
            TrackId(0),
            "/path/to/song.mp3".to_string(),
            "Test".to_string(),
            180.0,
            AudioFormat::Mp3,
            44100,
        );
        let track_id = db.insert_track(&track).unwrap();

        let cue = HotCue::new(track_id, 0, 30.5);
        db.save_hot_cue(&cue).unwrap();

        let cues = db.get_hot_cues(track_id).unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].slot, 0);
        assert!((cues[0].position_seconds - 30.5).abs() < 0.001);
    }
}
