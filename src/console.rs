use std::io::{self, stdout, Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use crate::artnet::{self, ArtNet, ArtNetMode};
use crate::cue::{Chase, ChaseStep, Cue, EffectDistribution, EffectMapping, StaticValue};
use crate::effect::{Effect, EffectParams};
use crate::fixture::{Channel, ChannelType, Fixture};
use crate::rhythm::{Interval, RhythmState};
use crate::{ableton_link, effect};

const TARGET_FREQUENCY: f64 = 40.0; // 40Hz DMX Spec (every 25ms)
const TARGET_DELTA: f64 = 1.0 / TARGET_FREQUENCY;
const TARGET_DURATION: f64 = 1.0 / TARGET_FREQUENCY;

pub struct LightingConsole {
    tempo: f64,
    fixtures: Vec<Fixture>,
    link_state: ableton_link::State,
    dmx_output: artnet::ArtNet,
    cues: Vec<Cue>,
    current_cue: usize,
    current_chase_step: usize,
    rhythm_state: RhythmState,
}

impl LightingConsole {
    pub fn new(bpm: f64) -> Result<Self, anyhow::Error> {
        let link_state = ableton_link::State::new(bpm);
        link_state.link.enable(true);

        let dmx_output = ArtNet::new(ArtNetMode::Broadcast)?;

        Ok(LightingConsole {
            tempo: bpm,
            fixtures: Vec::new(),
            cues: Vec::new(),
            current_cue: 0,
            current_chase_step: 0,
            link_state: link_state,
            dmx_output: dmx_output,
            rhythm_state: RhythmState {
                beat_phase: 0.0,
                bar_phase: 0.0,
                phrase_phase: 0.0,
                beats_per_bar: 4,   // Default to 4/4 time
                bars_per_phrase: 4, // Default 4-bar phrase
            },
        })
    }

    pub fn set_fixtures(&mut self, fixtures: Vec<Fixture>) {
        self.fixtures = fixtures;
    }

    pub fn add_fixture(&mut self, fixture: Fixture) {
        self.fixtures.push(fixture);
    }

    pub fn set_cues(&mut self, cues: Vec<Cue>) {
        self.cues = cues;
    }

    pub fn add_cue(&mut self, cue: Cue) {
        self.cues.push(cue);
    }

    pub fn run(&mut self) {
        let fps = TARGET_FREQUENCY;
        let frame_duration = Duration::from_secs_f64(1.0 / fps as f64);

        // Keyboard input handling
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || loop {
            let mut buffer = [0; 1];
            if io::stdin().read_exact(&mut buffer).is_ok() {
                if buffer[0] == b'G' || buffer[0] == b'g' {
                    tx.send(()).unwrap();
                }
            }
        });

        let mut frames_sent = 0;
        let mut elapsed_time = Duration::new(0, 0);
        let mut last_update = Instant::now();
        let mut cue_time = 0.0;

        // Render loop
        loop {
            let frame_start = Instant::now();
            elapsed_time += frame_start.duration_since(last_update);
            last_update = frame_start;

            // check for keyboard input
            if rx.try_recv().is_ok() {
                self.current_cue = (self.current_cue + 1) % self.cues.len();
                cue_time = 0.0;
                println!("Advanced to cue: {}", self.cues[self.current_cue].name);
            }

            self.update();
            self.render();

            frames_sent += 1;

            // TODO - what is this doing and do we need it?
            // if beat_time - cue_start_time >= cues[current_cue].duration {
            //     cue_start_time = beat_time;
            // }

            // reset cue time if it's greater than cue duration (loop cue)
            if cue_time >= self.cues[self.current_cue].duration {
                cue_time = 0.0; // Reset cue time but don't change the cue
            }

            // Display status information
            self.display_status(
                frames_sent,
                &self.cues[self.current_cue].name,
                elapsed_time,
                cue_time,
                self.rhythm_state.beat_phase,
            );

            let elapsed = frame_start.elapsed();
            if elapsed < frame_duration {
                thread::sleep(frame_duration - elapsed);
            }
        }
    }

    pub fn update(&mut self) {
        // Update effects based on beat_time
        // This is where you'd implement your effect calculations
        self.link_state.capture_app_state();
        let time = self.link_state.link.clock_micros();
        let beat_time = self
            .link_state
            .session_state
            .beat_at_time(time, self.link_state.quantum);

        self.tempo = self.link_state.session_state.tempo();
        self.update_rhythm_state(beat_time);

        // Apply effects
        for chase in self.cues[self.current_cue].chases.clone() {
            for step in chase.steps {
                self.apply_chase_step(&step);
            }
        }
    }

    pub fn render(&self) {
        // let mut dmx_data = vec![0u8; 512]; // Single universe

        // // Apply fixture data to DMX buffer
        // for fixture in &self.fixtures {
        //     for (i, &channel) in fixture.channels.iter().enumerate() {
        //         dmx_data[channel as usize] = i as u8; // Placeholder value
        //     }
        // }

        // // Send DMX data
        // self.dmx_output.send(1, &dmx_data);

        let dmx_data = self.generate_dmx_data();
        self.dmx_output.send_data(dmx_data);
    }

    // TODO - we might be able to push these down into a cuemaster at some point
    // pub fn apply_cue(
    //     &self,
    //     fixtures: &mut [Fixture],
    //     step: &ChaseStep,
    //     rhythm: &RhythmState,
    // ) {
    //     // Apply cue-level static values
    //     for static_value in &self.cues[self.current_cue].static_values {
    //         if let Some(fixture) = fixtures
    //             .iter_mut()
    //             .find(|f| f.name == static_value.fixture_name)
    //         {
    //             fixture.set_channel_value(&static_value.channel_name, static_value.value);
    //         }
    //     }

    //     // Apply the current chase step if any
    // }

    pub fn apply_chase_step(&mut self, step: &ChaseStep) {
        // Apply static values first
        for static_value in &step.static_values {
            if let Some(fixture) = self
                .fixtures
                .iter_mut()
                .find(|f| f.name == static_value.fixture_name)
            {
                if let Some(channel) = fixture
                    .channels
                    .iter_mut()
                    .find(|c| c.name == static_value.channel_name)
                {
                    // Smooth transition for static values as well
                    let current_value = channel.value as f64;
                    let target_value = static_value.value as f64;
                    channel.value = (current_value + (target_value - current_value)).round() as u16;
                }
            }
        }

        // Then apply effect mappings
        for mapping in &step.effect_mappings {
            let mut affected_fixtures: Vec<&mut Fixture> = self
                .fixtures
                .iter_mut()
                .filter(|f| mapping.fixture_names.contains(&f.name))
                .collect();

            for (i, fixture) in affected_fixtures.iter_mut().enumerate() {
                for channel_type in &mapping.channel_types {
                    if let Some(channel) = fixture.channels.iter_mut().find(|c| {
                        std::mem::discriminant(&c.channel_type)
                            == std::mem::discriminant(channel_type)
                    }) {
                        let mut effect_params = mapping.effect.params.clone();

                        // Apply distribution adjustments
                        match mapping.distribution {
                            EffectDistribution::All => {}
                            EffectDistribution::Step(step) => {
                                if i % step != 0 {
                                    continue;
                                }
                            }
                            EffectDistribution::Wave(phase_offset) => {
                                effect_params.phase += phase_offset * i as f64;
                            }
                        }

                        channel.value =
                            apply_effect(&mapping.effect, &self.rhythm_state, channel.value);
                    }
                }
            }
        }
    }

    fn update_rhythm_state(&mut self, beat_time: f64) {
        // Calculate phases
        self.rhythm_state.beat_phase = beat_time.fract();
        self.rhythm_state.bar_phase = (beat_time / self.rhythm_state.beats_per_bar as f64).fract();
        self.rhythm_state.phrase_phase = (beat_time
            / (self.rhythm_state.beats_per_bar * self.rhythm_state.bars_per_phrase) as f64)
            .fract();

        // Optionally update beats_per_bar and bars_per_phrase if needed
        // This could be based on user input or a predefined configuration
    }

    fn generate_dmx_data(&self) -> Vec<u8> {
        let mut dmx_data = vec![0; 512]; // Full DMX universe
        for fixture in &self.fixtures {
            let start_channel = (fixture.start_address - 1) as usize;
            let end_channel = (start_channel + fixture.channels.len()).min(dmx_data.len());
            dmx_data[start_channel..end_channel].copy_from_slice(&fixture.get_dmx_values());
        }
        dmx_data
    }

    fn display_status(
        &self,
        frames_sent: u64,
        current_cue: &str,
        elapsed: Duration,
        cue_time: f64,
        beat_time: f64,
    ) {
        let bpm = self.link_state.session_state.tempo();
        let num_peers = self.link_state.link.num_peers();
        let elapsed_secs = elapsed.as_secs_f64();

        print!("\r"); // Move cursor to the beginning of the line
        print!(
            "Frames: {:8} | BPM: {:6.2} | Peers: {:3} | Current Cue: {:3} | Elapsed: {} | Cue Time: {:6.2}s | Beat: {:6.2} | FPS: {:5.2}",
            frames_sent, bpm, num_peers, current_cue, format_duration(elapsed), cue_time, beat_time, frames_sent as f64 / elapsed_secs
        );
        stdout().flush().unwrap();
    }
}

fn apply_effect(effect: &Effect, rhythm: &RhythmState, current_value: u16) -> u16 {
    let phase = effect::get_effect_phase(rhythm, &effect.params);
    let target_value = (effect.apply)(phase);
    let target_dmx = (target_value * (effect.max - effect.min) as f64 + effect.min as f64) as f64;

    // Smooth transition
    let current_dmx = current_value as f64;
    let new_dmx = current_dmx + (target_dmx - current_dmx);

    // println!(
    //     "\nEffect: {}, Phase: {:.2}, Value: {}\n",
    //     effect.name,
    //     phase,
    //     new_dmx / 255.0
    // );

    new_dmx.round() as u16
}

fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    let milliseconds = duration.subsec_millis();

    format!(
        "{:02}:{:02}:{:02}:{:03}",
        hours, minutes, seconds, milliseconds
    )
}
