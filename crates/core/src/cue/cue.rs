use std::time::Duration;

use halo_fixtures::ChannelType;

use crate::Effect;

#[derive(Clone, Debug)]
pub struct CueList {
    pub name: String,
    pub cues: Vec<Cue>,
    pub audio_file: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Cue {
    pub name: String,
    pub duration: Duration,
    pub start_time: Duration,
    pub fade_time: f64,
    pub timecode: String,
    pub static_values: Vec<StaticValue>,
    pub effects: Vec<EffectMapping>,
    pub is_playing: bool,
    pub progress: f32,
    // A cue's "time" is a measure of how long it takes the cue to complete, once it has been
    // executed. Depending upon the console, time(s), entered in minutes and seconds, can be
    // entered for the cue as a whole or, individually, for transitions in focus, intensity (up
    // and/or down), and color, as well as for individual channels. Time (or delay) applied to
    // individual channels is called, "discrete" timing. FadeTime time.Time
    // The (optional) length of time (in seconds, after pressing the "Go" button) after which a cue
    // parameter will begin its fade. WaitTime time.Duration
    // A blocking cue prevents level changes from tracking through it and successive cues.
    //Block bool
}

impl Default for Cue {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            duration: Duration::ZERO,
            start_time: Duration::ZERO,
            fade_time: 0.0,
            timecode: "".to_string(),
            static_values: vec![],
            effects: vec![],
            is_playing: false,
            progress: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StaticValue {
    pub fixture_id: usize,
    pub channel_type: ChannelType,
    pub value: u8,
}

#[derive(Clone, Debug)]
pub struct EffectMapping {
    pub effect: Effect,
    pub fixture_ids: Vec<usize>,
    pub channel_type: ChannelType,
    pub distribution: EffectDistribution,
}

#[derive(Clone, Debug)]
pub enum EffectDistribution {
    All,
    Step(usize),
    Wave(f64), // Phase offset between fixtures
}
