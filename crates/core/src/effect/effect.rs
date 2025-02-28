use std::f64::consts::PI;

use crate::{Interval, RhythmState};

#[derive(Clone, Debug)]
pub struct Effect {
    pub name: String,
    pub effect_type: EffectType,
    pub apply: fn(f64) -> f64, // Takes a phase (0.0 to 1.0) and returns a value (0.0 to 1.0)
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

impl Default for Effect {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            effect_type: EffectType::Sine,
            apply: |_| 0.0,
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
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
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

pub fn linear_effect(phase: f64) -> f64 {
    phase
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
