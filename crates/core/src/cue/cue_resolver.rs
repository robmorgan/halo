use halo_fixtures::ChannelType;

use crate::{
    Cue, EffectDistribution, EffectMapping, FixtureGroup, PixelEffectMapping, Preset,
    PresetLibrary, StaticValue,
};

/// Resolves cue preset references into concrete static values and effects
pub struct CueResolver<'a> {
    preset_library: &'a PresetLibrary,
    fixture_groups: &'a [FixtureGroup],
}

impl<'a> CueResolver<'a> {
    pub fn new(preset_library: &'a PresetLibrary, fixture_groups: &'a [FixtureGroup]) -> Self {
        Self {
            preset_library,
            fixture_groups,
        }
    }

    /// Resolve all preset references in a cue to static values and effects
    pub fn resolve_cue(&self, cue: &Cue) -> ResolvedCue {
        let mut static_values = Vec::new();
        let mut effects = Vec::new();
        let mut pixel_effects = Vec::new();

        // Process each preset reference
        for preset_ref in &cue.preset_references {
            if let Some(preset) = self
                .preset_library
                .get_preset(&preset_ref.preset_type, preset_ref.preset_id)
            {
                let resolved = self.resolve_preset_reference(preset_ref, &preset);
                static_values.extend(resolved.static_values);
                effects.extend(resolved.effects);
                pixel_effects.extend(resolved.pixel_effects);
            }
        }

        // Add direct static values (these take precedence over preset values)
        static_values.extend(cue.static_values.clone());

        // Add direct effects
        effects.extend(cue.effects.clone());

        // Add direct pixel effects
        pixel_effects.extend(cue.pixel_effects.clone());

        // Deduplicate static values - last write wins for same fixture/channel
        static_values = Self::deduplicate_static_values(static_values);

        ResolvedCue {
            static_values,
            effects,
            pixel_effects,
        }
    }

    /// Resolve a single preset reference
    fn resolve_preset_reference(
        &self,
        preset_ref: &crate::cue::cue::PresetReference,
        preset: &Preset,
    ) -> ResolvedCue {
        let mut static_values = Vec::new();
        let mut effects = Vec::new();
        let mut pixel_effects = Vec::new();

        // Get the fixtures to apply this preset to
        let target_fixtures = self.get_target_fixtures(preset, preset_ref.fixture_group_id);

        // Resolve based on preset type
        match preset {
            Preset::Color(color_preset) => {
                for fixture_id in &target_fixtures {
                    for color_value in &color_preset.values {
                        static_values.push(StaticValue {
                            fixture_id: *fixture_id,
                            channel_type: color_value.channel_type.clone(),
                            value: color_value.value,
                        });
                    }
                }
            }
            Preset::Position(pos_preset) => {
                for fixture_id in &target_fixtures {
                    if let Some(pan) = pos_preset.pan {
                        static_values.push(StaticValue {
                            fixture_id: *fixture_id,
                            channel_type: ChannelType::Pan,
                            value: pan,
                        });
                    }
                    if let Some(tilt) = pos_preset.tilt {
                        static_values.push(StaticValue {
                            fixture_id: *fixture_id,
                            channel_type: ChannelType::Tilt,
                            value: tilt,
                        });
                    }
                }
            }
            Preset::Intensity(intensity_preset) => {
                for fixture_id in &target_fixtures {
                    static_values.push(StaticValue {
                        fixture_id: *fixture_id,
                        channel_type: ChannelType::Dimmer,
                        value: intensity_preset.dimmer,
                    });
                }
            }
            Preset::Beam(beam_preset) => {
                for fixture_id in &target_fixtures {
                    for beam_value in &beam_preset.values {
                        static_values.push(StaticValue {
                            fixture_id: *fixture_id,
                            channel_type: beam_value.channel_type.clone(),
                            value: beam_value.value,
                        });
                    }
                }
            }
            Preset::Effect(effect_preset) => {
                // For effect presets, create effect mappings for target fixtures
                match &effect_preset.effect {
                    crate::preset::preset::EffectPresetType::Standard(effect) => {
                        // Get all relevant channel types from the effect
                        // For now, we'll apply to Dimmer as a default
                        // This could be expanded based on effect configuration
                        effects.push(EffectMapping {
                            name: format!("Preset: {}", effect_preset.name),
                            effect: effect.clone(),
                            fixture_ids: target_fixtures.clone(),
                            channel_types: vec![ChannelType::Dimmer],
                            distribution: EffectDistribution::All,
                            release: crate::EffectRelease::Hold,
                        });
                    }
                    crate::preset::preset::EffectPresetType::Pixel(pixel_effect) => {
                        pixel_effects.push(PixelEffectMapping {
                            name: format!("Preset: {}", effect_preset.name),
                            effect: pixel_effect.clone(),
                            fixture_ids: target_fixtures.clone(),
                            distribution: EffectDistribution::All,
                            release: crate::EffectRelease::Hold,
                        });
                    }
                }
            }
        }

        // Apply overrides
        for override_val in &preset_ref.overrides {
            // Find and replace the static value for this fixture/channel
            if let Some(existing) = static_values.iter_mut().find(|sv| {
                sv.fixture_id == override_val.fixture_id
                    && sv.channel_type == override_val.channel_type
            }) {
                existing.value = override_val.value;
            } else {
                // Add the override if it doesn't exist
                static_values.push(override_val.clone());
            }
        }

        ResolvedCue {
            static_values,
            effects,
            pixel_effects,
        }
    }

    /// Get the target fixtures for a preset, considering fixture groups and optional restrictions
    fn get_target_fixtures(&self, preset: &Preset, filter_group_id: Option<usize>) -> Vec<usize> {
        let mut fixtures = Vec::new();

        let preset_groups = preset.fixture_groups();

        for &group_id in preset_groups {
            // If filter_group_id is specified and doesn't match, skip this group
            if let Some(filter_id) = filter_group_id {
                if filter_id != group_id {
                    continue;
                }
            }

            // Find the fixture group and add its fixtures
            if let Some(group) = self.fixture_groups.iter().find(|g| g.id == group_id) {
                fixtures.extend_from_slice(&group.fixture_ids);
            }
        }

        // Deduplicate fixtures
        fixtures.sort_unstable();
        fixtures.dedup();
        fixtures
    }

    /// Deduplicate static values - last write wins for the same fixture/channel combination
    fn deduplicate_static_values(values: Vec<StaticValue>) -> Vec<StaticValue> {
        let mut result = Vec::new();

        for value in values {
            // Find if we already have this fixture/channel combination
            if let Some(existing) = result.iter_mut().find(|v: &&mut StaticValue| {
                v.fixture_id == value.fixture_id && v.channel_type == value.channel_type
            }) {
                // Update the existing value (last write wins)
                existing.value = value.value;
            } else {
                // Add new value
                result.push(value);
            }
        }

        result
    }
}

/// A cue with all preset references resolved to concrete values
#[derive(Clone, Debug)]
pub struct ResolvedCue {
    pub static_values: Vec<StaticValue>,
    pub effects: Vec<EffectMapping>,
    pub pixel_effects: Vec<PixelEffectMapping>,
}
