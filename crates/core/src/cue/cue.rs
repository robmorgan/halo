use std::time::{Duration, Instant};

use halo_fixtures::ChannelType;

use crate::Effect;

#[derive(Clone, Debug)]
pub struct Cue {
    pub name: String,
    pub duration: Duration,
    pub start_time: Duration,
    pub fade_time: f64,
    pub timecode: String,
    pub static_values: Vec<StaticValue>,
    pub chases: Vec<Chase>,
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
            static_values: Vec::new(),
            chases: Vec::new(),
            is_playing: false,
            progress: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CueList {
    pub name: String,
    pub cues: Vec<Cue>,
    pub audio_file: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StaticValue {
    pub fixture_name: String,
    pub channel_name: String,
    pub value: u8,
}

#[derive(Clone, Debug)]
pub struct Chase {
    pub name: String,
    pub steps: Vec<ChaseStep>,
    pub current_step: usize, /* TODO - current step should probably be stored by the player/cue
                              * master/console */
    pub current_step_elapsed: f64,
    // TODO - reenable this is we want to make it beat driven
    pub accumulated_beats: f64,
    pub last_step_change: Instant,
    pub loop_count: Option<usize>, // None for infinite loop
}

impl Chase {
    pub fn new(name: String, steps: Vec<ChaseStep>, loop_count: Option<usize>) -> Self {
        Chase {
            name,
            steps,
            current_step: 0,
            current_step_elapsed: 0.0,
            accumulated_beats: 0.0,
            loop_count,
            last_step_change: Instant::now(),
        }
    }

    pub fn update(&mut self, elapsed: Duration) {
        if self.steps.is_empty() {
            return;
        }

        //self.accumulated_beats += beat_time;
        self.current_step_elapsed += elapsed.as_secs_f64();

        let current_step_duration = self.steps[self.current_step].duration;
        if self.current_step_elapsed >= current_step_duration.as_secs_f64() {
            self.current_step = (self.current_step + 1) % self.steps.len();
            self.current_step_elapsed = 0.0;
            self.last_step_change = Instant::now();
        }
        //if self.accumulated_beats >= current_step_duration {
        // self.current_step = (self.current_step + 1) % self.steps.len();
        // self.accumulated_beats = 0.0;
        // self.last_step_change = Instant::now();
        //}
        //if elapsed >= current_step_duration {
        //     self.current_step = (self.current_step + 1) % self.steps.len();
        //     self.last_step_change = Instant::now();
        // }
    }

    pub fn get_current_step(&self) -> &ChaseStep {
        &self.steps[self.current_step]
    }

    pub fn get_current_static_values(&self) -> &Vec<StaticValue> {
        &self.steps[self.current_step].static_values
    }

    pub fn get_current_effect_mappings(&self) -> &Vec<EffectMapping> {
        &self.steps[self.current_step].effect_mappings
    }

    pub fn set_current_step(&mut self, step: usize) {
        self.current_step = step;
    }
}

#[derive(Clone, Debug)]
pub struct ChaseStep {
    // TODO - uncomment if we want to make it beat driven
    //pub duration: f64,
    pub duration: Duration,
    //pub duration: Duration,
    pub effect_mappings: Vec<EffectMapping>,
    pub static_values: Vec<StaticValue>,
}

// TODO - one day we'll make this apply to multiple fixtures and channels
// TODO - this might be the case now
#[derive(Clone, Debug)]
pub struct EffectMapping {
    pub effect: Effect,
    pub fixture_names: Vec<String>,
    pub channel_types: Vec<ChannelType>,
    pub distribution: EffectDistribution,
}

#[derive(Clone, Debug)]
pub enum EffectDistribution {
    All,
    Step(usize),
    Wave(f64), // Phase offset between fixtures
}
