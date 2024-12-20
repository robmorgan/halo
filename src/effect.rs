use std::f64::consts::PI;

use crate::rhythm::{Interval, RhythmState};

#[derive(Clone, Debug)]
pub struct Effect {
    pub name: String,
    pub apply: fn(f64) -> f64, // Takes a phase (0.0 to 1.0) and returns a value (0.0 to 1.0)
    pub min: u16,
    pub max: u16,
    pub params: EffectParams,
}

#[derive(Clone, Debug)]
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

pub fn cosine_effect(phase: f64) -> f64 {
    (phase * 2.0 * PI).cos() * 0.5 + 0.5
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
