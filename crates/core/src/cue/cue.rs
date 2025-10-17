use std::time::Duration;

use halo_fixtures::ChannelType;
use serde::{Deserialize, Serialize};

use crate::{Effect, EffectRelease, PixelEffect};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CueList {
    pub name: String,
    pub cues: Vec<Cue>,
    pub audio_file: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cue {
    pub id: usize,
    pub name: String,
    // Time to fade to the new values
    pub fade_time: Duration,
    // TODO - Wait before starting the fade
    //pub delay_time: Duration,
    pub static_values: Vec<StaticValue>,
    pub effects: Vec<EffectMapping>,
    pub pixel_effects: Vec<PixelEffectMapping>,
    pub timecode: Option<String>,
    // A blocking cue prevents level changes from tracking through it and successive cues.
    pub is_blocking: bool,
}

impl Default for Cue {
    fn default() -> Self {
        Self {
            id: 0,
            name: "".to_string(),
            fade_time: Duration::ZERO,
            //delay_time: Duration::ZERO,
            timecode: None,
            static_values: vec![],
            effects: vec![],
            pixel_effects: vec![],
            is_blocking: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StaticValue {
    pub fixture_id: usize,
    pub channel_type: ChannelType,
    pub value: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct EffectMapping {
    pub name: String,
    pub effect: Effect,
    pub fixture_ids: Vec<usize>,
    pub channel_types: Vec<ChannelType>,
    pub distribution: EffectDistribution,
    #[serde(default)]
    pub release: EffectRelease,
}

impl<'de> Deserialize<'de> for EffectMapping {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct EffectMappingHelper {
            name: String,
            effect: Effect,
            fixture_ids: Vec<usize>,
            #[serde(flatten)]
            channel_data: ChannelData,
            distribution: EffectDistribution,
            #[serde(default)]
            release: EffectRelease,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ChannelData {
            Old { channel_type: ChannelType },
            New { channel_types: Vec<ChannelType> },
        }

        let helper = EffectMappingHelper::deserialize(deserializer)?;

        let channel_types = match helper.channel_data {
            ChannelData::Old { channel_type } => vec![channel_type],
            ChannelData::New { channel_types } => channel_types,
        };

        Ok(EffectMapping {
            name: helper.name,
            effect: helper.effect,
            fixture_ids: helper.fixture_ids,
            channel_types,
            distribution: helper.distribution,
            release: helper.release,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EffectDistribution {
    All,
    Step(usize),
    Wave(f64), // Phase offset between fixtures
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PixelEffectMapping {
    pub name: String,
    pub effect: PixelEffect,
    pub fixture_ids: Vec<usize>,
    pub distribution: EffectDistribution,
    #[serde(default)]
    pub release: EffectRelease,
}
