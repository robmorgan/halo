use std::time::Duration;

use halo_fixtures::ChannelType;
use serde::{Deserialize, Serialize};

use crate::Effect;

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectMapping {
    pub name: String,
    pub effect: Effect,
    pub fixture_ids: Vec<usize>,
    pub channel_type: ChannelType,
    pub distribution: EffectDistribution,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EffectDistribution {
    All,
    Step(usize),
    Wave(f64), // Phase offset between fixtures
}
