//! Deck module for DJ deck state and playback control.

use serde::{Deserialize, Serialize};

use crate::library::{BeatGrid, HotCue, MasterTempoMode, TempoRange, Track};

/// Deck identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeckId {
    A,
    B,
}

impl DeckId {
    /// Get the deck as a numeric index (0 for A, 1 for B).
    pub fn index(&self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }

    /// Get the deck as a u8 (0 for A, 1 for B).
    pub fn as_u8(&self) -> u8 {
        self.index() as u8
    }

    /// Get the deck from a numeric index.
    pub fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::A),
            1 => Some(Self::B),
            _ => None,
        }
    }

    /// Get the other deck.
    pub fn other(&self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

impl std::fmt::Display for DeckId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::A => write!(f, "A"),
            Self::B => write!(f, "B"),
        }
    }
}

/// Deck playback state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DeckState {
    /// No track loaded.
    #[default]
    Empty,
    /// Track is loading.
    Loading,
    /// Track loaded but stopped at start.
    Stopped,
    /// Track is playing.
    Playing,
    /// Track is paused.
    Paused,
    /// Cue preview mode (playing while cue button held).
    Cueing,
}

impl DeckState {
    /// Returns true if the deck is actively producing audio.
    pub fn is_playing(&self) -> bool {
        matches!(self, Self::Playing | Self::Cueing)
    }

    /// Returns true if a track is loaded.
    pub fn has_track(&self) -> bool {
        !matches!(self, Self::Empty | Self::Loading)
    }
}

/// Complete deck state.
#[derive(Debug, Clone)]
pub struct Deck {
    /// Deck identifier.
    pub id: DeckId,
    /// Current playback state.
    pub state: DeckState,
    /// Currently loaded track.
    pub loaded_track: Option<Track>,
    /// Beat grid for the loaded track.
    pub beat_grid: Option<BeatGrid>,

    // Playback position
    /// Current position in seconds.
    pub position_seconds: f64,
    /// Current position in beats (from beat grid).
    pub position_beats: f64,

    // Tempo
    /// Original BPM of the loaded track.
    pub original_bpm: f64,
    /// Current adjusted BPM (after pitch adjustment).
    pub adjusted_bpm: f64,
    /// Pitch fader position (-1.0 to 1.0).
    pub pitch_percent: f64,
    /// Current tempo range setting.
    pub tempo_range: TempoRange,

    // Cue points
    /// Main cue point position in seconds.
    pub cue_point: Option<f64>,
    /// Position when cue preview started (for returning on release).
    pub cue_preview_start: Option<f64>,
    /// 4 hot cue slots.
    pub hot_cues: [Option<HotCue>; 4],

    // Sync
    /// Is this deck the tempo master?
    pub is_master: bool,
    /// Is sync mode enabled?
    pub sync_enabled: bool,

    // Master Tempo (key lock)
    /// Master Tempo mode (off = varispeed, on = time-stretch).
    pub master_tempo: MasterTempoMode,

    // Metering
    /// Current volume level (0.0-1.0) for VU meter.
    pub volume_level: f32,
    /// Peak level for VU meter.
    pub peak_level: f32,
}

impl Deck {
    /// Create a new empty deck.
    pub fn new(id: DeckId) -> Self {
        Self {
            id,
            state: DeckState::Empty,
            loaded_track: None,
            beat_grid: None,
            position_seconds: 0.0,
            position_beats: 0.0,
            original_bpm: 0.0,
            adjusted_bpm: 0.0,
            pitch_percent: 0.0,
            tempo_range: TempoRange::default(),
            cue_point: None,
            cue_preview_start: None,
            hot_cues: [None, None, None, None],
            is_master: false,
            sync_enabled: false,
            master_tempo: MasterTempoMode::Off,
            volume_level: 0.0,
            peak_level: 0.0,
        }
    }

    /// Calculate the adjusted BPM based on pitch fader position.
    pub fn calculate_adjusted_bpm(&self) -> f64 {
        self.original_bpm * self.tempo_range.pitch_to_multiplier(self.pitch_percent)
    }

    /// Update the adjusted BPM from current pitch setting.
    pub fn update_adjusted_bpm(&mut self) {
        self.adjusted_bpm = self.calculate_adjusted_bpm();
    }

    /// Get the playback rate multiplier (for audio engine).
    pub fn playback_rate(&self) -> f64 {
        self.tempo_range.pitch_to_multiplier(self.pitch_percent)
    }

    /// Set a hot cue at the given slot.
    pub fn set_hot_cue(&mut self, slot: u8, position_seconds: f64) {
        if let Some(track) = &self.loaded_track {
            let hot_cue = HotCue::new(track.id, slot, position_seconds);
            if (slot as usize) < self.hot_cues.len() {
                self.hot_cues[slot as usize] = Some(hot_cue);
            }
        }
    }

    /// Clear a hot cue at the given slot.
    pub fn clear_hot_cue(&mut self, slot: u8) {
        if (slot as usize) < self.hot_cues.len() {
            self.hot_cues[slot as usize] = None;
        }
    }

    /// Load hot cues from a list.
    pub fn load_hot_cues(&mut self, hot_cues: Vec<HotCue>) {
        self.hot_cues = [None, None, None, None];
        for cue in hot_cues {
            let slot = cue.slot as usize;
            if slot < self.hot_cues.len() {
                self.hot_cues[slot] = Some(cue);
            }
        }
    }

    /// Reset deck to empty state.
    pub fn eject(&mut self) {
        self.state = DeckState::Empty;
        self.loaded_track = None;
        self.beat_grid = None;
        self.position_seconds = 0.0;
        self.position_beats = 0.0;
        self.original_bpm = 0.0;
        self.adjusted_bpm = 0.0;
        self.cue_point = None;
        self.cue_preview_start = None;
        self.hot_cues = [None, None, None, None];
        self.master_tempo = MasterTempoMode::Off;
        self.volume_level = 0.0;
        self.peak_level = 0.0;
    }

    /// Update beat position from current time position.
    pub fn update_beat_position(&mut self) {
        if let Some(beat_grid) = &self.beat_grid {
            self.position_beats = beat_grid.beat_at_position(self.position_seconds);
        }
    }

    /// Get the current beat phase (0.0-1.0).
    pub fn beat_phase(&self) -> f64 {
        if let Some(beat_grid) = &self.beat_grid {
            beat_grid.beat_phase_at_position(self.position_seconds)
        } else {
            0.0
        }
    }

    /// Get the current bar phase (0.0-1.0).
    pub fn bar_phase(&self) -> f64 {
        if let Some(beat_grid) = &self.beat_grid {
            beat_grid.bar_phase_at_position(self.position_seconds)
        } else {
            0.0
        }
    }

    /// Get the current phrase phase (0.0-1.0).
    pub fn phrase_phase(&self) -> f64 {
        if let Some(beat_grid) = &self.beat_grid {
            beat_grid.phrase_phase_at_position(self.position_seconds)
        } else {
            0.0
        }
    }
}

impl Default for Deck {
    fn default() -> Self {
        Self::new(DeckId::A)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deck_id() {
        assert_eq!(DeckId::A.index(), 0);
        assert_eq!(DeckId::B.index(), 1);
        assert_eq!(DeckId::A.other(), DeckId::B);
        assert_eq!(DeckId::B.other(), DeckId::A);
        assert_eq!(DeckId::from_index(0), Some(DeckId::A));
        assert_eq!(DeckId::from_index(2), None);
    }

    #[test]
    fn test_deck_state() {
        assert!(DeckState::Playing.is_playing());
        assert!(DeckState::Cueing.is_playing());
        assert!(!DeckState::Paused.is_playing());
        assert!(!DeckState::Empty.has_track());
        assert!(DeckState::Stopped.has_track());
    }

    #[test]
    fn test_deck_playback_rate() {
        let mut deck = Deck::new(DeckId::A);
        deck.tempo_range = TempoRange::Range10;

        // No pitch adjustment
        deck.pitch_percent = 0.0;
        assert!((deck.playback_rate() - 1.0).abs() < 0.001);

        // +10% pitch
        deck.pitch_percent = 1.0;
        assert!((deck.playback_rate() - 1.1).abs() < 0.001);

        // -10% pitch
        deck.pitch_percent = -1.0;
        assert!((deck.playback_rate() - 0.9).abs() < 0.001);
    }
}
