use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use crate::{Interval, RhythmState};

/// Pixel-specific effect types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PixelEffectType {
    Chase,
    Wave,
    Strobe,
    ColorCycle,
}

impl PixelEffectType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PixelEffectType::Chase => "Chase",
            PixelEffectType::Wave => "Wave",
            PixelEffectType::Strobe => "Strobe",
            PixelEffectType::ColorCycle => "ColorCycle",
        }
    }

    pub fn all() -> Vec<PixelEffectType> {
        vec![
            PixelEffectType::Chase,
            PixelEffectType::Wave,
            PixelEffectType::Strobe,
            PixelEffectType::ColorCycle,
        ]
    }
}

/// Scope of pixel effect application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelEffectScope {
    /// Apply effect to all pixels in bar uniformly
    Bar,
    /// Apply effect to individual pixels
    Individual,
}

/// Pixel effect parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PixelEffectParams {
    pub interval: Interval,
    pub interval_ratio: f64,
    pub phase: f64,
    pub speed: f64,
}

impl Default for PixelEffectParams {
    fn default() -> Self {
        PixelEffectParams {
            interval: Interval::Beat,
            interval_ratio: 1.0,
            phase: 0.0,
            speed: 1.0,
        }
    }
}

/// Complete pixel effect definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PixelEffect {
    pub effect_type: PixelEffectType,
    pub scope: PixelEffectScope,
    pub color: (u8, u8, u8),
    pub params: PixelEffectParams,
}

impl Default for PixelEffect {
    fn default() -> Self {
        Self {
            effect_type: PixelEffectType::Chase,
            scope: PixelEffectScope::Individual,
            color: (255, 255, 255),
            params: PixelEffectParams::default(),
        }
    }
}

impl PixelEffect {
    /// Render effect for a single pixel at given position
    /// position: 0.0 to 1.0 representing position in bar (0 = start, 1 = end)
    /// phase: 0.0 to 1.0 representing effect phase from rhythm
    /// Returns RGB tuple
    pub fn render_pixel(&self, position: f64, phase: f64) -> (u8, u8, u8) {
        // ColorCycle needs special handling - it generates colors dynamically
        if self.effect_type == PixelEffectType::ColorCycle {
            return self.render_color_cycle(position, phase);
        }

        let intensity = match self.scope {
            PixelEffectScope::Bar => {
                // All pixels get same intensity based on phase
                self.calculate_intensity(phase)
            }
            PixelEffectScope::Individual => {
                // Each pixel gets intensity based on its position and phase
                self.calculate_intensity_individual(position, phase)
            }
        };

        // Apply intensity to color
        (
            ((self.color.0 as f64 * intensity) as u8),
            ((self.color.1 as f64 * intensity) as u8),
            ((self.color.2 as f64 * intensity) as u8),
        )
    }

    /// Render color cycle effect with actual color changes
    fn render_color_cycle(&self, position: f64, phase: f64) -> (u8, u8, u8) {
        // Neon purple and electric blue
        let neon_purple = (191.0, 0.0, 255.0); // RGB
        let electric_blue = (125.0, 249.0, 255.0); // RGB

        let t = match self.scope {
            PixelEffectScope::Bar => {
                // All pixels cycle through colors together based on phase
                // Use sine wave for smooth alternation
                (phase * std::f64::consts::PI * 2.0).sin() * 0.5 + 0.5
            }
            PixelEffectScope::Individual => {
                // Each pixel has different color based on position + phase
                let combined = (position + phase) % 1.0;
                (combined * std::f64::consts::PI * 2.0).sin() * 0.5 + 0.5
            }
        };

        // Interpolate between neon purple and electric blue
        let r = (neon_purple.0 * (1.0 - t) + electric_blue.0 * t) as u8;
        let g = (neon_purple.1 * (1.0 - t) + electric_blue.1 * t) as u8;
        let b = (neon_purple.2 * (1.0 - t) + electric_blue.2 * t) as u8;

        (r, g, b)
    }

    fn calculate_intensity(&self, phase: f64) -> f64 {
        match self.effect_type {
            PixelEffectType::Chase => {
                // Simple on/off based on phase
                if phase < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            PixelEffectType::Wave => {
                // Sine wave
                (phase * 2.0 * PI).sin() * 0.5 + 0.5
            }
            PixelEffectType::Strobe => {
                // Fast on/off
                if (phase * 10.0) % 1.0 < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            PixelEffectType::ColorCycle => {
                // Always on for color cycle (color changes in bar mode)
                1.0
            }
        }
    }

    fn calculate_intensity_individual(&self, position: f64, phase: f64) -> f64 {
        match self.effect_type {
            PixelEffectType::Chase => {
                // Chase effect: light travels down the bar
                let chase_pos = phase;
                let distance = (position - chase_pos).abs();
                if distance < 0.1 {
                    1.0
                } else {
                    0.0
                }
            }
            PixelEffectType::Wave => {
                // Wave effect: sine wave travels down the bar
                let wave_phase = phase + position;
                (wave_phase * 2.0 * PI).sin() * 0.5 + 0.5
            }
            PixelEffectType::Strobe => {
                // All pixels strobe together in individual mode
                if (phase * 10.0) % 1.0 < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            PixelEffectType::ColorCycle => {
                // Always full intensity for color cycle (color changes, not intensity)
                1.0
            }
        }
    }

    /// Get effect phase from rhythm state
    pub fn get_phase(&self, rhythm: &RhythmState) -> f64 {
        let base_phase = match self.params.interval {
            Interval::Beat => rhythm.beat_phase,
            Interval::Bar => rhythm.bar_phase,
            Interval::Phrase => rhythm.phrase_phase,
        };

        // Calculate phase and ensure it's in valid range [0.0, 1.0)
        let phase =
            (base_phase * self.params.interval_ratio * self.params.speed + self.params.phase) % 1.0;

        // Clamp to prevent any floating point edge cases
        phase.max(0.0).min(0.9999999)
    }
}

/// Apply distribution to pixel effects across multiple fixtures
#[allow(dead_code)]
pub fn apply_pixel_distribution(
    _effect: &PixelEffect,
    fixture_index: usize,
    total_fixtures: usize,
    base_phase: f64,
) -> f64 {
    // For pixel effects, we can apply distribution to offset the phase
    // This makes effects spread across multiple pixel bars
    match total_fixtures {
        0 | 1 => base_phase,
        _ => {
            let fixture_offset = fixture_index as f64 / total_fixtures as f64;
            (base_phase + fixture_offset) % 1.0
        }
    }
}
