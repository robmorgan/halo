use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Source for tempo/rhythm synchronization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TempoSource {
    /// Internal tempo (tap tempo, manual BPM setting).
    #[default]
    Internal,
    /// Ableton Link network sync.
    AbletonLink,
    /// DJ module master deck.
    DjMaster,
}

impl TempoSource {
    /// Get a display name for the tempo source.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Internal => "Internal",
            Self::AbletonLink => "Ableton Link",
            Self::DjMaster => "DJ Master",
        }
    }
}

// Assuming we have access to these from our rhythm engine
#[derive(Debug, Clone)]
pub struct RhythmState {
    pub beat_phase: f64,   // 0.0 to 1.0, resets each beat
    pub bar_phase: f64,    // 0.0 to 1.0, resets each bar
    pub phrase_phase: f64, // 0.0 to 1.0, resets each phrase
    pub beats_per_bar: u32,
    pub bars_per_phrase: u32,
    pub last_tap_time: Option<Instant>,
    pub tap_count: u32,
    pub bpm: f64,
    pub tempo_source: TempoSource,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Interval {
    Beat,
    Bar,
    Phrase,
}
