//! Core library types for the DJ module.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unique identifier for a track in the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId(pub i64);

impl From<i64> for TrackId {
    fn from(id: i64) -> Self {
        Self(id)
    }
}

impl From<TrackId> for i64 {
    fn from(id: TrackId) -> Self {
        id.0
    }
}

impl fmt::Display for TrackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Supported audio formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    Mp3,
    Wav,
    Aiff,
    Flac,
    Aac,
    Ogg,
    Unknown,
}

impl AudioFormat {
    /// Determine format from file extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "mp3" => Self::Mp3,
            "wav" => Self::Wav,
            "aiff" | "aif" => Self::Aiff,
            "flac" => Self::Flac,
            "aac" | "m4a" => Self::Aac,
            "ogg" => Self::Ogg,
            _ => Self::Unknown,
        }
    }

    /// Get the format as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
            Self::Aiff => "aiff",
            Self::Flac => "flac",
            Self::Aac => "aac",
            Self::Ogg => "ogg",
            Self::Unknown => "unknown",
        }
    }
}

/// Audio track metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Unique database ID.
    pub id: TrackId,
    /// Absolute path to the audio file.
    pub file_path: String,
    /// Track title.
    pub title: String,
    /// Artist name.
    pub artist: Option<String>,
    /// Album name.
    pub album: Option<String>,
    /// Track duration in seconds.
    pub duration_seconds: f64,
    /// Detected BPM (None if not analyzed).
    pub bpm: Option<f64>,
    /// Musical key (e.g., "Am", "C#").
    pub key: Option<String>,
    /// Audio format.
    pub format: AudioFormat,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Bit depth (e.g., 16, 24).
    pub bit_depth: u16,
    /// Number of audio channels.
    pub channels: u8,
    /// File size in bytes.
    pub file_size_bytes: u64,
    /// When the track was added to the library.
    pub date_added: DateTime<Utc>,
    /// When the track was last played.
    pub last_played: Option<DateTime<Utc>>,
    /// Number of times the track has been played.
    pub play_count: u32,
    /// User rating (0-5 stars).
    pub rating: u8,
    /// User comment.
    pub comment: Option<String>,
}

impl Track {
    /// Create a new track with required fields.
    pub fn new(
        id: TrackId,
        file_path: String,
        title: String,
        duration_seconds: f64,
        format: AudioFormat,
        sample_rate: u32,
    ) -> Self {
        Self {
            id,
            file_path,
            title,
            artist: None,
            album: None,
            duration_seconds,
            bpm: None,
            key: None,
            format,
            sample_rate,
            bit_depth: 16,
            channels: 2,
            file_size_bytes: 0,
            date_added: Utc::now(),
            last_played: None,
            play_count: 0,
            rating: 0,
            comment: None,
        }
    }

    /// Get a display string for the track (Artist - Title).
    pub fn display_name(&self) -> String {
        match &self.artist {
            Some(artist) => format!("{} - {}", artist, self.title),
            None => self.title.clone(),
        }
    }
}

/// Beat grid analysis data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatGrid {
    /// ID of the track this beat grid belongs to.
    pub track_id: TrackId,
    /// Detected BPM.
    pub bpm: f64,
    /// Time offset to the first beat in milliseconds.
    pub first_beat_offset_ms: f64,
    /// Beat positions in seconds (can be empty if only BPM/offset stored).
    pub beat_positions: Vec<f64>,
    /// Analysis confidence (0.0-1.0).
    pub confidence: f32,
    /// When the analysis was performed.
    pub analyzed_at: DateTime<Utc>,
    /// Version of the analysis algorithm.
    pub algorithm_version: String,
}

impl BeatGrid {
    /// Get the beat interval in seconds.
    pub fn beat_interval_seconds(&self) -> f64 {
        60.0 / self.bpm
    }

    /// Get the beat number at a given position.
    pub fn beat_at_position(&self, position_seconds: f64) -> f64 {
        let offset_seconds = self.first_beat_offset_ms / 1000.0;
        (position_seconds - offset_seconds) / self.beat_interval_seconds()
    }

    /// Get the phase (0.0-1.0) within the current beat.
    pub fn beat_phase_at_position(&self, position_seconds: f64) -> f64 {
        let beat = self.beat_at_position(position_seconds);
        beat - beat.floor()
    }

    /// Get the bar phase (0.0-1.0) assuming 4/4 time.
    pub fn bar_phase_at_position(&self, position_seconds: f64) -> f64 {
        let beat = self.beat_at_position(position_seconds);
        let bar = beat / 4.0;
        bar - bar.floor()
    }

    /// Get the phrase phase (0.0-1.0) assuming 8-bar phrases.
    pub fn phrase_phase_at_position(&self, position_seconds: f64) -> f64 {
        let beat = self.beat_at_position(position_seconds);
        let phrase = beat / 32.0; // 8 bars * 4 beats
        phrase - phrase.floor()
    }

    /// Find the nearest beat position to the given time (seconds).
    ///
    /// Returns the time in seconds of the beat closest to the given position.
    /// Used for quantizing loop IN points to beat boundaries.
    pub fn nearest_beat(&self, position_seconds: f64) -> f64 {
        let beat_number = self.beat_at_position(position_seconds);
        let quantized_beat = beat_number.round();
        let offset_seconds = self.first_beat_offset_ms / 1000.0;
        offset_seconds + (quantized_beat * self.beat_interval_seconds())
    }

    /// Get the position N beats after a given position (seconds).
    ///
    /// Used for calculating loop OUT points from loop IN.
    pub fn beat_position_after(&self, position_seconds: f64, beat_count: f64) -> f64 {
        position_seconds + (beat_count * self.beat_interval_seconds())
    }
}

/// 3-band frequency data for colored waveform visualization.
///
/// Each band represents the energy in a frequency range:
/// - Low: 20-250 Hz (bass, kick drums)
/// - Mid: 250-4000 Hz (vocals, instruments)
/// - High: 4000-20000 Hz (hi-hats, cymbals)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct FrequencyBands {
    /// Low frequency energy (0.0-1.0) - bass, kick drums.
    pub low: f32,
    /// Mid frequency energy (0.0-1.0) - vocals, instruments.
    pub mid: f32,
    /// High frequency energy (0.0-1.0) - hi-hats, cymbals.
    pub high: f32,
}

impl FrequencyBands {
    /// Create new frequency bands.
    pub fn new(low: f32, mid: f32, high: f32) -> Self {
        Self { low, mid, high }
    }

    /// Convert to RGB color (Red=low, Green=mid, Blue=high).
    pub fn to_rgb(&self) -> (u8, u8, u8) {
        // Scale and clamp values for better visibility
        let r = (self.low.clamp(0.0, 1.0) * 255.0) as u8;
        let g = (self.mid.clamp(0.0, 1.0) * 255.0) as u8;
        let b = (self.high.clamp(0.0, 1.0) * 255.0) as u8;
        (r, g, b)
    }

    /// Convert to tuple for serialization.
    pub fn as_tuple(&self) -> (f32, f32, f32) {
        (self.low, self.mid, self.high)
    }

    /// Create from tuple.
    pub fn from_tuple(t: (f32, f32, f32)) -> Self {
        Self {
            low: t.0,
            mid: t.1,
            high: t.2,
        }
    }
}

/// Waveform data version.
pub const WAVEFORM_VERSION_LEGACY: u8 = 1;
pub const WAVEFORM_VERSION_COLORED: u8 = 2;

/// Waveform data for UI visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackWaveform {
    /// ID of the track this waveform belongs to.
    pub track_id: TrackId,
    /// Downsampled waveform peaks (absolute values, 0.0-1.0).
    pub samples: Vec<f32>,
    /// 3-band frequency data for coloring (parallel to samples).
    /// None for legacy waveforms (pre-colored analysis).
    pub frequency_bands: Option<Vec<FrequencyBands>>,
    /// Number of samples in the waveform.
    pub sample_count: usize,
    /// Duration of the track in seconds.
    pub duration_seconds: f64,
    /// Waveform format version (1=legacy amplitude only, 2=colored with frequency bands).
    pub version: u8,
}

impl TrackWaveform {
    /// Get the sample index for a given position in seconds.
    pub fn sample_at_position(&self, position_seconds: f64) -> usize {
        let ratio = position_seconds / self.duration_seconds;
        ((ratio * self.sample_count as f64) as usize).min(self.sample_count.saturating_sub(1))
    }

    /// Get a slice of samples for a time range.
    pub fn samples_in_range(&self, start_seconds: f64, end_seconds: f64) -> &[f32] {
        let start_idx = self.sample_at_position(start_seconds);
        let end_idx = self.sample_at_position(end_seconds);
        &self.samples[start_idx..=end_idx.min(self.samples.len().saturating_sub(1))]
    }
}

/// Hot cue point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotCue {
    /// Database ID.
    pub id: i64,
    /// ID of the track this hot cue belongs to.
    pub track_id: TrackId,
    /// Slot number (0-3 for 4 hot cues).
    pub slot: u8,
    /// Position in seconds.
    pub position_seconds: f64,
    /// Optional name for the cue.
    pub name: Option<String>,
    /// Optional RGB color.
    pub color: Option<(u8, u8, u8)>,
    /// When the cue was created.
    pub created_at: DateTime<Utc>,
}

impl HotCue {
    /// Create a new hot cue.
    pub fn new(track_id: TrackId, slot: u8, position_seconds: f64) -> Self {
        Self {
            id: 0, // Will be set by database
            track_id,
            slot,
            position_seconds,
            name: None,
            color: None,
            created_at: Utc::now(),
        }
    }

    /// Get the default color for a slot.
    pub fn default_color_for_slot(slot: u8) -> (u8, u8, u8) {
        match slot {
            0 => (255, 0, 0),   // Red
            1 => (0, 255, 0),   // Green
            2 => (0, 0, 255),   // Blue
            3 => (255, 255, 0), // Yellow
            _ => (255, 255, 255),
        }
    }
}

/// Master Tempo (key lock) mode.
///
/// When enabled, tempo changes via pitch fader don't affect the audio pitch.
/// Uses time-stretching (WSOLA algorithm) to decouple tempo from pitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MasterTempoMode {
    /// Varispeed mode - pitch changes with tempo (default, lowest latency).
    #[default]
    Off,
    /// Master Tempo - pitch locked, tempo changes via time-stretching.
    On,
}

/// Tempo adjustment range preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TempoRange {
    /// +/- 6%
    Range6,
    /// +/- 10%
    #[default]
    Range10,
    /// +/- 16%
    Range16,
    /// +/- 100% (wide - full range, allows near-stop to double speed)
    Wide,
}

impl TempoRange {
    /// Get the range as a percentage (e.g., 0.10 for +/- 10%).
    pub fn as_fraction(&self) -> f64 {
        match self {
            Self::Range6 => 0.06,
            Self::Range10 => 0.10,
            Self::Range16 => 0.16,
            Self::Wide => 1.00,
        }
    }

    /// Convert a pitch fader value (-1.0 to 1.0) to a tempo multiplier.
    pub fn pitch_to_multiplier(&self, pitch: f64) -> f64 {
        1.0 + (pitch * self.as_fraction())
    }

    /// Convert to u8 for UI/event serialization.
    pub fn to_u8(&self) -> u8 {
        match self {
            Self::Range6 => 0,
            Self::Range10 => 1,
            Self::Range16 => 2,
            Self::Wide => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_format_from_extension() {
        assert_eq!(AudioFormat::from_extension("mp3"), AudioFormat::Mp3);
        assert_eq!(AudioFormat::from_extension("MP3"), AudioFormat::Mp3);
        assert_eq!(AudioFormat::from_extension("wav"), AudioFormat::Wav);
        assert_eq!(AudioFormat::from_extension("aiff"), AudioFormat::Aiff);
        assert_eq!(AudioFormat::from_extension("aif"), AudioFormat::Aiff);
        assert_eq!(AudioFormat::from_extension("xyz"), AudioFormat::Unknown);
    }

    #[test]
    fn test_beat_grid_calculations() {
        let grid = BeatGrid {
            track_id: TrackId(1),
            bpm: 120.0,
            first_beat_offset_ms: 500.0, // 0.5 seconds
            beat_positions: vec![],
            confidence: 0.95,
            analyzed_at: Utc::now(),
            algorithm_version: "1.0".to_string(),
        };

        // At 120 BPM, beat interval is 0.5 seconds
        assert!((grid.beat_interval_seconds() - 0.5).abs() < 0.001);

        // At position 1.0 seconds (0.5s after first beat), should be beat 1.0
        assert!((grid.beat_at_position(1.0) - 1.0).abs() < 0.001);

        // At position 0.75 seconds (0.25s into first beat), phase should be 0.5
        assert!((grid.beat_phase_at_position(0.75) - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_nearest_beat() {
        let grid = BeatGrid {
            track_id: TrackId(1),
            bpm: 120.0,
            first_beat_offset_ms: 0.0,
            beat_positions: vec![],
            confidence: 0.95,
            analyzed_at: Utc::now(),
            algorithm_version: "1.0".to_string(),
        };

        // At 120 BPM, beats are at 0.0, 0.5, 1.0, 1.5, etc.
        // Position 0.2 should snap to 0.0
        assert!((grid.nearest_beat(0.2) - 0.0).abs() < 0.001);
        // Position 0.3 should snap to 0.5
        assert!((grid.nearest_beat(0.3) - 0.5).abs() < 0.001);
        // Position 0.75 should snap to 1.0
        assert!((grid.nearest_beat(0.75) - 1.0).abs() < 0.001);
        // Position 1.24 should snap to 1.0
        assert!((grid.nearest_beat(1.24) - 1.0).abs() < 0.001);
        // Position 1.26 should snap to 1.5
        assert!((grid.nearest_beat(1.26) - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_beat_position_after() {
        let grid = BeatGrid {
            track_id: TrackId(1),
            bpm: 120.0,
            first_beat_offset_ms: 0.0,
            beat_positions: vec![],
            confidence: 0.95,
            analyzed_at: Utc::now(),
            algorithm_version: "1.0".to_string(),
        };

        // At 120 BPM, beat interval is 0.5 seconds
        // 4 beats after 0.0 should be 2.0 seconds
        assert!((grid.beat_position_after(0.0, 4.0) - 2.0).abs() < 0.001);
        // 8 beats after 0.0 should be 4.0 seconds
        assert!((grid.beat_position_after(0.0, 8.0) - 4.0).abs() < 0.001);
        // 4 beats after 1.0 should be 3.0 seconds
        assert!((grid.beat_position_after(1.0, 4.0) - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_tempo_range() {
        let range = TempoRange::Range10;

        // At pitch 0.0, multiplier should be 1.0
        assert!((range.pitch_to_multiplier(0.0) - 1.0).abs() < 0.001);

        // At pitch 1.0, multiplier should be 1.10
        assert!((range.pitch_to_multiplier(1.0) - 1.10).abs() < 0.001);

        // At pitch -1.0, multiplier should be 0.90
        assert!((range.pitch_to_multiplier(-1.0) - 0.90).abs() < 0.001);
    }

    #[test]
    fn test_tempo_range_wide() {
        let range = TempoRange::Wide;

        // Wide is ±100%
        assert!((range.as_fraction() - 1.0).abs() < 0.001);

        // At pitch 0.0, multiplier should be 1.0
        assert!((range.pitch_to_multiplier(0.0) - 1.0).abs() < 0.001);

        // At pitch 1.0, multiplier should be 2.0 (double speed)
        assert!((range.pitch_to_multiplier(1.0) - 2.0).abs() < 0.001);

        // At pitch -1.0, multiplier should be 0.0 (stopped)
        assert!((range.pitch_to_multiplier(-1.0) - 0.0).abs() < 0.001);
    }
}
