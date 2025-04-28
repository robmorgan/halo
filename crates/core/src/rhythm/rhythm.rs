use std::time::Instant;

use serde::{Deserialize, Serialize};

// Assuming we have access to these from our rhythm engine
pub struct RhythmState {
    pub beat_phase: f64,   // 0.0 to 1.0, resets each beat
    pub bar_phase: f64,    // 0.0 to 1.0, resets each bar
    pub phrase_phase: f64, // 0.0 to 1.0, resets each phrase
    pub beats_per_bar: u32,
    pub bars_per_phrase: u32,
    pub last_tap_time: Option<Instant>,
    pub tap_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Interval {
    Beat,
    Bar,
    Phrase,
}
