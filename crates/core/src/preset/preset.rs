use halo_fixtures::ChannelType;
use serde::{Deserialize, Serialize};

use crate::{Effect, PixelEffect};

/// Represents different types of presets
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PresetType {
    Color,
    Position,
    Intensity,
    Beam,
    Effect,
}

/// A generic preset that can be one of several types
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Preset {
    Color(ColorPreset),
    Position(PositionPreset),
    Intensity(IntensityPreset),
    Beam(BeamPreset),
    Effect(EffectPreset),
}

impl Preset {
    pub fn id(&self) -> usize {
        match self {
            Preset::Color(p) => p.id,
            Preset::Position(p) => p.id,
            Preset::Intensity(p) => p.id,
            Preset::Beam(p) => p.id,
            Preset::Effect(p) => p.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Preset::Color(p) => &p.name,
            Preset::Position(p) => &p.name,
            Preset::Intensity(p) => &p.name,
            Preset::Beam(p) => &p.name,
            Preset::Effect(p) => &p.name,
        }
    }

    pub fn preset_type(&self) -> PresetType {
        match self {
            Preset::Color(_) => PresetType::Color,
            Preset::Position(_) => PresetType::Position,
            Preset::Intensity(_) => PresetType::Intensity,
            Preset::Beam(_) => PresetType::Beam,
            Preset::Effect(_) => PresetType::Effect,
        }
    }

    pub fn fixture_groups(&self) -> &[usize] {
        match self {
            Preset::Color(p) => &p.fixture_groups,
            Preset::Position(p) => &p.fixture_groups,
            Preset::Intensity(p) => &p.fixture_groups,
            Preset::Beam(p) => &p.fixture_groups,
            Preset::Effect(p) => &p.fixture_groups,
        }
    }
}

/// A preset for color values (RGB, RGBW, color wheels, etc.)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColorPreset {
    pub id: usize,
    pub name: String,
    pub fixture_groups: Vec<usize>,
    pub values: Vec<ColorValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColorValue {
    pub channel_type: ChannelType,
    pub value: u8,
}

impl ColorPreset {
    pub fn new(id: usize, name: String, fixture_groups: Vec<usize>) -> Self {
        Self {
            id,
            name,
            fixture_groups,
            values: Vec::new(),
        }
    }

    pub fn add_value(&mut self, channel_type: ChannelType, value: u8) {
        // Remove existing value for this channel type
        self.values.retain(|v| v.channel_type != channel_type);
        self.values.push(ColorValue {
            channel_type,
            value,
        });
    }
}

/// A preset for position values (Pan, Tilt)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionPreset {
    pub id: usize,
    pub name: String,
    pub fixture_groups: Vec<usize>,
    pub pan: Option<u8>,
    pub tilt: Option<u8>,
}

impl PositionPreset {
    pub fn new(id: usize, name: String, fixture_groups: Vec<usize>) -> Self {
        Self {
            id,
            name,
            fixture_groups,
            pan: None,
            tilt: None,
        }
    }

    pub fn with_pan(mut self, pan: u8) -> Self {
        self.pan = Some(pan);
        self
    }

    pub fn with_tilt(mut self, tilt: u8) -> Self {
        self.tilt = Some(tilt);
        self
    }
}

/// A preset for intensity values (Dimmer)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IntensityPreset {
    pub id: usize,
    pub name: String,
    pub fixture_groups: Vec<usize>,
    pub dimmer: u8,
}

impl IntensityPreset {
    pub fn new(id: usize, name: String, fixture_groups: Vec<usize>, dimmer: u8) -> Self {
        Self {
            id,
            name,
            fixture_groups,
            dimmer,
        }
    }
}

/// A preset for beam attributes (Focus, Zoom, Iris, Gobo, Prism, etc.)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BeamPreset {
    pub id: usize,
    pub name: String,
    pub fixture_groups: Vec<usize>,
    pub values: Vec<BeamValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BeamValue {
    pub channel_type: ChannelType,
    pub value: u8,
}

impl BeamPreset {
    pub fn new(id: usize, name: String, fixture_groups: Vec<usize>) -> Self {
        Self {
            id,
            name,
            fixture_groups,
            values: Vec::new(),
        }
    }

    pub fn add_value(&mut self, channel_type: ChannelType, value: u8) {
        // Remove existing value for this channel type
        self.values.retain(|v| v.channel_type != channel_type);
        self.values.push(BeamValue {
            channel_type,
            value,
        });
    }
}

/// A preset for effects
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectPreset {
    pub id: usize,
    pub name: String,
    pub fixture_groups: Vec<usize>,
    pub effect: EffectPresetType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EffectPresetType {
    Standard(Effect),
    Pixel(PixelEffect),
}

impl EffectPreset {
    pub fn new_standard(
        id: usize,
        name: String,
        fixture_groups: Vec<usize>,
        effect: Effect,
    ) -> Self {
        Self {
            id,
            name,
            fixture_groups,
            effect: EffectPresetType::Standard(effect),
        }
    }

    pub fn new_pixel(
        id: usize,
        name: String,
        fixture_groups: Vec<usize>,
        effect: PixelEffect,
    ) -> Self {
        Self {
            id,
            name,
            fixture_groups,
            effect: EffectPresetType::Pixel(effect),
        }
    }
}
