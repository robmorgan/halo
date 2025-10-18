use std::collections::HashMap;

use halo_fixtures::{Fixture, FixtureType};

use super::pixel_effects::{PixelEffect, PixelEffectScope};
use crate::rhythm::rhythm::RhythmState;
use crate::{EffectDistribution, EffectMapping};

/// Global pixel engine managing all pixel bar fixtures
pub struct PixelEngine {
    /// Configuration
    enabled: bool,
    /// Mapping of fixture ID to universe
    universe_mapping: HashMap<usize, u8>,
    /// Active pixel effects mapped by a unique key
    active_effects: HashMap<String, (Vec<usize>, PixelEffect, EffectDistribution)>,
}

impl PixelEngine {
    pub fn new() -> Self {
        Self {
            enabled: true,
            universe_mapping: HashMap::new(),
            active_effects: HashMap::new(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Set universe mapping for a fixture
    pub fn set_fixture_universe(&mut self, fixture_id: usize, universe: u8) {
        self.universe_mapping.insert(fixture_id, universe);
    }

    /// Get universe for a fixture (falls back to fixture's own universe if not mapped)
    pub fn get_fixture_universe(&self, fixture_id: usize, default_universe: u8) -> u8 {
        *self
            .universe_mapping
            .get(&fixture_id)
            .unwrap_or(&default_universe)
    }

    /// Clear all universe mappings
    pub fn clear_universe_mappings(&mut self) {
        self.universe_mapping.clear();
    }

    /// Set active pixel effects from effect mappings
    pub fn set_effects(
        &mut self,
        effects: Vec<(String, Vec<usize>, PixelEffect, EffectDistribution)>,
    ) {
        self.active_effects.clear();
        for (name, fixture_ids, effect, distribution) in effects {
            self.active_effects
                .insert(name, (fixture_ids, effect, distribution));
        }
    }

    /// Add a single pixel effect
    pub fn add_effect(
        &mut self,
        name: String,
        fixture_ids: Vec<usize>,
        effect: PixelEffect,
        distribution: EffectDistribution,
    ) {
        self.active_effects
            .insert(name, (fixture_ids, effect, distribution));
    }

    /// Remove a pixel effect by name
    pub fn remove_effect(&mut self, name: &str) {
        self.active_effects.remove(name);
    }

    /// Clear all active effects
    pub fn clear_effects(&mut self) {
        self.active_effects.clear();
    }

    /// Render all pixel fixtures and return DMX data per universe
    pub fn render(&self, fixtures: &[Fixture], rhythm_state: &RhythmState) -> HashMap<u8, Vec<u8>> {
        if !self.enabled {
            return HashMap::new();
        }

        let mut universe_data: HashMap<u8, Vec<u8>> = HashMap::new();

        // Find all pixel bar fixtures
        let pixel_fixtures: Vec<&Fixture> = fixtures
            .iter()
            .filter(|f| f.profile.fixture_type == FixtureType::PixelBar)
            .collect();

        if pixel_fixtures.is_empty() {
            return universe_data;
        }

        // Render each pixel fixture
        for fixture in pixel_fixtures {
            let pixel_count = self.get_pixel_count_from_channels(&fixture.channels);
            if pixel_count == 0 {
                continue;
            }

            // Calculate RGB values for each pixel
            let pixel_data = self.render_fixture(fixture, pixel_count, rhythm_state);

            // Get universe for this fixture
            let universe = self.get_fixture_universe(fixture.id, fixture.universe);

            // Initialize universe buffer if needed (512 channels)
            let universe_buffer = universe_data
                .entry(universe)
                .or_insert_with(|| vec![0; 512]);

            // Write pixel data to universe at fixture's start address
            let start_idx = (fixture.start_address - 1) as usize; // DMX addresses are 1-based
            let channels_needed = pixel_count * 3; // RGB per pixel

            if start_idx + channels_needed <= 512 {
                universe_buffer[start_idx..start_idx + channels_needed]
                    .copy_from_slice(&pixel_data);
            }
        }

        universe_data
    }

    /// Render a single pixel fixture
    fn render_fixture(
        &self,
        fixture: &Fixture,
        pixel_count: usize,
        rhythm_state: &RhythmState,
    ) -> Vec<u8> {
        let mut pixel_data = vec![0u8; pixel_count * 3]; // RGB per pixel

        // Find effects that apply to this fixture
        let applicable_effects: Vec<(&PixelEffect, &EffectDistribution, usize, usize)> = self
            .active_effects
            .values()
            .filter_map(|(fixture_ids, effect, distribution)| {
                fixture_ids
                    .iter()
                    .position(|&id| id == fixture.id)
                    .map(|idx| (effect, distribution, idx, fixture_ids.len()))
            })
            .collect();

        if applicable_effects.is_empty() {
            // No effects, return black (all zeros)
            return pixel_data;
        }

        // Render each pixel
        for pixel_idx in 0..pixel_count {
            let position = (pixel_idx as f64 + 0.5) / pixel_count as f64;
            let mut r = 0u16;
            let mut g = 0u16;
            let mut b = 0u16;

            // Accumulate all applicable effects
            for (effect, distribution, fixture_idx, total_fixtures) in &applicable_effects {
                let base_phase = effect.get_phase(rhythm_state);

                // Apply distribution to offset phase across fixtures
                let phase = match distribution {
                    EffectDistribution::All => base_phase,
                    EffectDistribution::Step(step) => {
                        let step_offset = (fixture_idx % step) as f64 / (*step).max(1) as f64;
                        (base_phase + step_offset) % 1.0
                    }
                    EffectDistribution::Wave(offset) => {
                        let wave_offset = *fixture_idx as f64 * offset;
                        (base_phase + wave_offset) % 1.0
                    }
                };

                let (pr, pg, pb) = effect.render_pixel(position, phase);
                r += pr as u16;
                g += pg as u16;
                b += pb as u16;
            }

            // Clamp to 255
            let base = pixel_idx * 3;
            pixel_data[base] = r.min(255) as u8;
            pixel_data[base + 1] = g.min(255) as u8;
            pixel_data[base + 2] = b.min(255) as u8;
        }

        pixel_data
    }

    /// Extract pixel count from channel layout
    /// Assumes RGB layout: 3 channels per pixel
    fn get_pixel_count_from_channels(&self, channels: &[halo_fixtures::Channel]) -> usize {
        channels.len() / 3
    }

    /// Get current universe mapping
    pub fn get_universe_mapping(&self) -> &HashMap<usize, u8> {
        &self.universe_mapping
    }
}

impl Default for PixelEngine {
    fn default() -> Self {
        Self::new()
    }
}
