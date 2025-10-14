use std::collections::HashMap;

use crate::{Cue, EffectMapping, PixelEffectMapping, StaticValue};

/// Manages accumulated tracking state for a tracking console
/// Values and effects persist across cues until explicitly changed or cleared by blocking cues
#[derive(Clone)]
pub struct TrackingState {
    /// Accumulated fixture channel values
    accumulated_values: Vec<StaticValue>,
    /// Active effects that continue to run
    active_effects: HashMap<String, EffectMapping>,
    /// Active pixel effects that continue to run
    active_pixel_effects: HashMap<String, PixelEffectMapping>,
}

impl TrackingState {
    /// Create a new empty tracking state
    pub fn new() -> Self {
        Self {
            accumulated_values: Vec::new(),
            active_effects: HashMap::new(),
            active_pixel_effects: HashMap::new(),
        }
    }

    /// Apply a cue to the tracking state (merges values and effects)
    pub fn apply_cue(&mut self, cue: &Cue) {
        // Merge static values into accumulated state
        for value in &cue.static_values {
            // Find and update existing value or add new one
            if let Some(existing) = self
                .accumulated_values
                .iter_mut()
                .find(|v| v.fixture_id == value.fixture_id && v.channel_type == value.channel_type)
            {
                existing.value = value.value;
            } else {
                self.accumulated_values.push(value.clone());
            }
        }

        // Process effects based on release behavior
        for effect_mapping in &cue.effects {
            // Add or update the effect in tracking state
            self.active_effects
                .insert(effect_mapping.name.clone(), effect_mapping.clone());
        }

        // Process pixel effects based on release behavior
        for pixel_effect_mapping in &cue.pixel_effects {
            // Add or update the pixel effect in tracking state
            self.active_pixel_effects.insert(
                pixel_effect_mapping.name.clone(),
                pixel_effect_mapping.clone(),
            );
        }
    }

    /// Apply a blocking cue (clears tracking state, then applies the cue)
    pub fn apply_blocking_cue(&mut self, cue: &Cue) {
        // Clear all tracking state
        self.clear();

        // Apply the blocking cue's values
        self.apply_cue(cue);
    }

    /// Get all tracked static values for rendering
    pub fn get_static_values(&self) -> Vec<StaticValue> {
        self.accumulated_values.clone()
    }

    /// Get all active effects
    pub fn get_effects(&self) -> Vec<EffectMapping> {
        self.active_effects.values().cloned().collect()
    }

    /// Get all active pixel effects
    pub fn get_pixel_effects(&self) -> Vec<PixelEffectMapping> {
        self.active_pixel_effects.values().cloned().collect()
    }

    /// Clear all tracking state
    pub fn clear(&mut self) {
        self.accumulated_values.clear();
        self.active_effects.clear();
        self.active_pixel_effects.clear();
    }

    /// Check if tracking state is empty
    pub fn is_empty(&self) -> bool {
        self.accumulated_values.is_empty()
            && self.active_effects.is_empty()
            && self.active_pixel_effects.is_empty()
    }

    /// Get the number of active effects
    pub fn active_effect_count(&self) -> usize {
        self.active_effects.len() + self.active_pixel_effects.len()
    }
}

impl Default for TrackingState {
    fn default() -> Self {
        Self::new()
    }
}
