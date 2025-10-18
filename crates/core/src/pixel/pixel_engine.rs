use std::collections::HashMap;

use halo_fixtures::{Fixture, FixtureType};

use super::pixel_effects::PixelEffect;
use crate::rhythm::rhythm::RhythmState;
use crate::EffectDistribution;

/// Global pixel engine managing all pixel bar fixtures
pub struct PixelEngine {
    /// Configuration
    enabled: bool,
    /// Mapping of fixture ID to universe
    universe_mapping: HashMap<usize, u8>,
    /// Active pixel effects mapped by a unique key
    active_effects: HashMap<String, (Vec<usize>, PixelEffect, EffectDistribution)>,
    /// Sequential packing mode enabled
    sequential_packing: bool,
    /// Fixture mapping: fixture_id -> (universe, start_address, channels_needed)
    fixture_mapping: HashMap<usize, (u8, u16, usize)>,
}

impl PixelEngine {
    pub fn new() -> Self {
        Self {
            enabled: true,
            universe_mapping: HashMap::new(),
            active_effects: HashMap::new(),
            sequential_packing: false,
            fixture_mapping: HashMap::new(),
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

    /// Enable sequential packing mode and calculate fixture mappings
    pub fn enable_sequential_packing(&mut self, fixtures: &[Fixture]) {
        self.sequential_packing = true;
        self.fixture_mapping = self.calculate_sequential_mapping(fixtures);

        log::info!(
            "Sequential packing enabled for {} pixel fixtures",
            self.fixture_mapping.len()
        );
        for (fixture_id, (universe, start_address, channels)) in &self.fixture_mapping {
            log::info!(
                "  Fixture {}: Universe {}, Address {}-{} ({} channels)",
                fixture_id,
                universe,
                start_address,
                start_address + *channels as u16 - 1,
                channels
            );
        }
    }

    /// Disable sequential packing mode
    pub fn disable_sequential_packing(&mut self) {
        self.sequential_packing = false;
        self.fixture_mapping.clear();
        log::info!("Sequential packing disabled");
    }

    /// Calculate sequential mapping for pixel bar fixtures
    /// Returns: HashMap<fixture_id, (universe, start_address, channels_needed)>
    /// Ensures all addresses are pixel-aligned (address-1 must be divisible by 3)
    fn calculate_sequential_mapping(
        &self,
        fixtures: &[Fixture],
    ) -> HashMap<usize, (u8, u16, usize)> {
        let mut mapping = HashMap::new();

        // Find all pixel bar fixtures sorted by ID
        let mut pixel_fixtures: Vec<&Fixture> = fixtures
            .iter()
            .filter(|f| f.profile.fixture_type == FixtureType::PixelBar)
            .collect();
        pixel_fixtures.sort_by_key(|f| f.id);

        let mut current_universe: u8 = 1;
        let mut current_address: u16 = 1;

        for fixture in pixel_fixtures {
            let pixel_count = self.get_pixel_count_from_channels(&fixture.channels);
            if pixel_count == 0 {
                continue;
            }

            let channels_needed = pixel_count * 3; // RGB per pixel

            mapping.insert(
                fixture.id,
                (current_universe, current_address, channels_needed),
            );

            // Update address for next fixture, handling universe overflow with pixel alignment
            let next_address = current_address + channels_needed as u16;

            if next_address > 512 {
                // Calculate how many channels actually fit in current universe
                let available_in_universe = 512 - current_address + 1;

                // Only complete pixels (groups of 3 channels) can be written
                let complete_pixels_in_universe = available_in_universe / 3;
                let channels_written = complete_pixels_in_universe * 3;

                // Remaining channels go to next universe
                let remaining_channels = channels_needed as u16 - channels_written;

                // Next fixture starts after the spillover
                current_universe += 1;
                current_address = 1 + remaining_channels;
            } else {
                current_address = next_address;
            }
        }

        mapping
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
            let channels_needed = pixel_count * 3; // RGB per pixel

            // Determine universe and start address (use sequential mapping if enabled)
            let (start_universe, start_address) = if self.sequential_packing {
                if let Some((universe, address, _)) = self.fixture_mapping.get(&fixture.id) {
                    (*universe, *address)
                } else {
                    // Fallback if fixture not in mapping
                    (
                        self.get_fixture_universe(fixture.id, fixture.universe),
                        fixture.start_address,
                    )
                }
            } else {
                (
                    self.get_fixture_universe(fixture.id, fixture.universe),
                    fixture.start_address,
                )
            };

            log::info!(
                "Pixel Engine - Fixture {} ({}): pixel_count={}, channels.len()={}, start_address={}, universe={}, channels_needed={}",
                fixture.id,
                fixture.name,
                pixel_count,
                fixture.channels.len(),
                start_address,
                start_universe,
                channels_needed
            );

            // Write pixel data with spillover support
            self.write_with_spillover(
                &mut universe_data,
                &pixel_data,
                start_universe,
                start_address,
                channels_needed,
                fixture.id,
                &fixture.name,
            );
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
            for (effect, distribution, fixture_idx, _total_fixtures) in &applicable_effects {
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

    /// Write pixel data with automatic spillover across universe boundaries
    /// Ensures splits happen only on pixel boundaries (multiples of 3 channels)
    fn write_with_spillover(
        &self,
        universe_data: &mut HashMap<u8, Vec<u8>>,
        pixel_data: &[u8],
        start_universe: u8,
        start_address: u16,
        channels_needed: usize,
        fixture_id: usize,
        fixture_name: &str,
    ) {
        let mut remaining_channels = channels_needed;
        let mut source_offset = 0;
        let mut current_universe = start_universe;
        let mut current_address = start_address;

        while remaining_channels > 0 {
            // Calculate how many channels we can write in the current universe
            let available_in_universe = (512 - current_address as usize + 1).min(512);
            let mut to_write = remaining_channels.min(available_in_universe);

            // CRITICAL: Ensure we only split on pixel boundaries (RGB = 3 channels)
            // If we would split in the middle of a pixel, write fewer channels
            if to_write < remaining_channels {
                // We're going to split - make sure it's on a pixel boundary
                to_write = (to_write / 3) * 3;

                // If we can't write any complete pixels, something is wrong with addressing
                if to_write == 0 {
                    log::error!(
                        "Pixel fixture {} ({}) cannot write complete pixels: only {} channels available at Universe {} address {}",
                        fixture_id,
                        fixture_name,
                        available_in_universe,
                        current_universe,
                        current_address
                    );
                    break;
                }
            }

            // Initialize universe buffer if needed
            let universe_buffer = universe_data
                .entry(current_universe)
                .or_insert_with(|| vec![0; 512]);

            // Write channels to this universe
            let start_idx = (current_address - 1) as usize; // DMX addresses are 1-based
            let end_idx = start_idx + to_write;

            if end_idx <= 512 {
                universe_buffer[start_idx..end_idx]
                    .copy_from_slice(&pixel_data[source_offset..source_offset + to_write]);

                if remaining_channels > to_write {
                    log::info!(
                        "  Fixture {} ({}): Wrote {} channels ({} pixels) to Universe {} (addresses {}-{}), {} channels remaining",
                        fixture_id,
                        fixture_name,
                        to_write,
                        to_write / 3,
                        current_universe,
                        current_address,
                        current_address + to_write as u16 - 1,
                        remaining_channels - to_write
                    );
                }
            } else {
                log::error!(
                    "Pixel fixture {} ({}) write overflow: trying to write to {}-{} in universe {}",
                    fixture_id,
                    fixture_name,
                    start_idx,
                    end_idx,
                    current_universe
                );
            }

            // Update for next iteration
            remaining_channels -= to_write;
            source_offset += to_write;
            current_universe += 1;
            current_address = 1; // Next universe starts at address 1
        }
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
