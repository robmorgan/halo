use serde::{Deserialize, Serialize};

use super::preset::{
    BeamPreset, ColorPreset, EffectPreset, IntensityPreset, PositionPreset, Preset, PresetType,
};

/// Central library for managing all presets in a show
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PresetLibrary {
    #[serde(default)]
    pub color: Vec<ColorPreset>,
    #[serde(default)]
    pub position: Vec<PositionPreset>,
    #[serde(default)]
    pub intensity: Vec<IntensityPreset>,
    #[serde(default)]
    pub beam: Vec<BeamPreset>,
    #[serde(default)]
    pub effect: Vec<EffectPreset>,
}

impl PresetLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a preset to the library
    pub fn add_preset(&mut self, preset: Preset) {
        match preset {
            Preset::Color(p) => self.color.push(p),
            Preset::Position(p) => self.position.push(p),
            Preset::Intensity(p) => self.intensity.push(p),
            Preset::Beam(p) => self.beam.push(p),
            Preset::Effect(p) => self.effect.push(p),
        }
    }

    /// Get a preset by ID and type
    pub fn get_preset(&self, preset_type: &PresetType, id: usize) -> Option<Preset> {
        match preset_type {
            PresetType::Color => self
                .color
                .iter()
                .find(|p| p.id == id)
                .cloned()
                .map(Preset::Color),
            PresetType::Position => self
                .position
                .iter()
                .find(|p| p.id == id)
                .cloned()
                .map(Preset::Position),
            PresetType::Intensity => self
                .intensity
                .iter()
                .find(|p| p.id == id)
                .cloned()
                .map(Preset::Intensity),
            PresetType::Beam => self
                .beam
                .iter()
                .find(|p| p.id == id)
                .cloned()
                .map(Preset::Beam),
            PresetType::Effect => self
                .effect
                .iter()
                .find(|p| p.id == id)
                .cloned()
                .map(Preset::Effect),
        }
    }

    /// Update an existing preset
    pub fn update_preset(&mut self, preset: Preset) -> bool {
        match preset {
            Preset::Color(new_preset) => {
                if let Some(existing) = self.color.iter_mut().find(|p| p.id == new_preset.id) {
                    *existing = new_preset;
                    true
                } else {
                    false
                }
            }
            Preset::Position(new_preset) => {
                if let Some(existing) = self.position.iter_mut().find(|p| p.id == new_preset.id) {
                    *existing = new_preset;
                    true
                } else {
                    false
                }
            }
            Preset::Intensity(new_preset) => {
                if let Some(existing) = self.intensity.iter_mut().find(|p| p.id == new_preset.id) {
                    *existing = new_preset;
                    true
                } else {
                    false
                }
            }
            Preset::Beam(new_preset) => {
                if let Some(existing) = self.beam.iter_mut().find(|p| p.id == new_preset.id) {
                    *existing = new_preset;
                    true
                } else {
                    false
                }
            }
            Preset::Effect(new_preset) => {
                if let Some(existing) = self.effect.iter_mut().find(|p| p.id == new_preset.id) {
                    *existing = new_preset;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Delete a preset by ID and type
    pub fn delete_preset(&mut self, preset_type: &PresetType, id: usize) -> bool {
        match preset_type {
            PresetType::Color => {
                let len_before = self.color.len();
                self.color.retain(|p| p.id != id);
                self.color.len() < len_before
            }
            PresetType::Position => {
                let len_before = self.position.len();
                self.position.retain(|p| p.id != id);
                self.position.len() < len_before
            }
            PresetType::Intensity => {
                let len_before = self.intensity.len();
                self.intensity.retain(|p| p.id != id);
                self.intensity.len() < len_before
            }
            PresetType::Beam => {
                let len_before = self.beam.len();
                self.beam.retain(|p| p.id != id);
                self.beam.len() < len_before
            }
            PresetType::Effect => {
                let len_before = self.effect.len();
                self.effect.retain(|p| p.id != id);
                self.effect.len() < len_before
            }
        }
    }

    /// Get all presets of a specific type
    pub fn get_presets_by_type(&self, preset_type: &PresetType) -> Vec<Preset> {
        match preset_type {
            PresetType::Color => self.color.iter().cloned().map(Preset::Color).collect(),
            PresetType::Position => self
                .position
                .iter()
                .cloned()
                .map(Preset::Position)
                .collect(),
            PresetType::Intensity => self
                .intensity
                .iter()
                .cloned()
                .map(Preset::Intensity)
                .collect(),
            PresetType::Beam => self.beam.iter().cloned().map(Preset::Beam).collect(),
            PresetType::Effect => self.effect.iter().cloned().map(Preset::Effect).collect(),
        }
    }

    /// Get all presets
    pub fn get_all_presets(&self) -> Vec<Preset> {
        let mut presets = Vec::new();
        presets.extend(self.color.iter().cloned().map(Preset::Color));
        presets.extend(self.position.iter().cloned().map(Preset::Position));
        presets.extend(self.intensity.iter().cloned().map(Preset::Intensity));
        presets.extend(self.beam.iter().cloned().map(Preset::Beam));
        presets.extend(self.effect.iter().cloned().map(Preset::Effect));
        presets
    }

    /// Get next available ID for a preset type
    pub fn next_id(&self, preset_type: &PresetType) -> usize {
        let max_id = match preset_type {
            PresetType::Color => self.color.iter().map(|p| p.id).max().unwrap_or(0),
            PresetType::Position => self.position.iter().map(|p| p.id).max().unwrap_or(0),
            PresetType::Intensity => self.intensity.iter().map(|p| p.id).max().unwrap_or(0),
            PresetType::Beam => self.beam.iter().map(|p| p.id).max().unwrap_or(0),
            PresetType::Effect => self.effect.iter().map(|p| p.id).max().unwrap_or(0),
        };
        max_id + 1
    }

    /// Get presets that apply to a specific fixture group
    pub fn get_presets_for_group(&self, group_id: usize) -> Vec<Preset> {
        self.get_all_presets()
            .into_iter()
            .filter(|preset| preset.fixture_groups().contains(&group_id))
            .collect()
    }
}
