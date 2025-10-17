use std::f64::consts::PI;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{Interval, RhythmState};

/// Effect release behavior - controls what happens to effects when cues change
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EffectRelease {
    /// Continue running indefinitely (default for tracking consoles)
    Hold,
    /// Remove when cue changes
    Remove,
    /// Fade out over time (future enhancement)
    FadeOut(Duration),
}

impl Default for EffectRelease {
    fn default() -> Self {
        EffectRelease::Hold
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Effect {
    pub effect_type: EffectType,
    pub min: u8,
    pub max: u8,
    pub amplitude: f32,
    pub frequency: f32,
    pub offset: f32,
    pub params: EffectParams,
    // pub value: f64,
    // pub loop: bool,
    // pub paused: bool,
}

impl Effect {
    // Takes a phase (0.0 to 1.0) and returns a value (0.0 to 1.0)
    pub fn apply(&self, phase: f64) -> f64 {
        // Apply based on the effect type
        let apply_fn = match self.effect_type {
            EffectType::Sine => sine_effect,
            EffectType::Square => square_effect,
            EffectType::Sawtooth => sawtooth_effect,
            EffectType::Triangle => |phase| {
                if phase < 0.5 {
                    phase * 2.0
                } else {
                    2.0 - phase * 2.0
                }
            },
            _ => sine_effect, // Default
        };
        (apply_fn)(phase)
    }
}

impl Default for Effect {
    fn default() -> Self {
        Self {
            effect_type: EffectType::Sine,
            min: 0,
            max: 255,
            amplitude: 1.0,
            frequency: 1.0,
            offset: 0.0,
            params: EffectParams::default(),
        }
    }
}

// Effect types
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize)]
pub enum EffectType {
    Sine,
    Sawtooth,
    Square,
    Triangle,
    Pulse,
    Random,
}

impl EffectType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EffectType::Sine => "Sine",
            EffectType::Sawtooth => "Sawtooth",
            EffectType::Square => "Square",
            EffectType::Triangle => "Triangle",
            EffectType::Pulse => "Pulse",
            EffectType::Random => "Random",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectParams {
    pub interval: Interval,
    pub interval_ratio: f64,
    pub phase: f64,
}

impl Default for EffectParams {
    fn default() -> Self {
        EffectParams {
            interval: Interval::Beat,
            interval_ratio: 1.0,
            phase: 0.0,
        }
    }
}

pub fn get_effect_phase(rhythm: &RhythmState, params: &EffectParams) -> f64 {
    let base_phase = match params.interval {
        Interval::Beat => rhythm.beat_phase,
        Interval::Bar => rhythm.bar_phase,
        Interval::Phrase => rhythm.phrase_phase,
    };

    (base_phase * params.interval_ratio + params.phase) % 1.0
}

pub fn sine_effect(phase: f64) -> f64 {
    (phase * 2.0 * PI).sin() * 0.5 + 0.5
}

pub fn square_effect(phase: f64) -> f64 {
    if phase < 0.5 {
        1.0
    } else {
        0.0
    }
}

pub fn sawtooth_effect(phase: f64) -> f64 {
    phase
}
