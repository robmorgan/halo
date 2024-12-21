use std::io::{self, stdout, Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use crate::ableton_link::ClockState;
use crate::artnet::{self, ArtNet, ArtNetMode};
use crate::cue::{Chase, ChaseStep, Cue, EffectDistribution, EffectMapping, StaticValue};
use crate::effect::{Effect, EffectParams};
use crate::fixture::{Channel, ChannelType, Fixture};
use crate::rhythm::RhythmState;
use crate::{ableton_link, effect};

use std::{
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    time::SystemTime,
};

const TARGET_FREQUENCY: f64 = 40.0; // 40Hz DMX Spec (every 25ms)
const TARGET_DELTA: f64 = 1.0 / TARGET_FREQUENCY;
const TARGET_DURATION: f64 = 1.0 / TARGET_FREQUENCY;

pub struct PlaybackState {
    pub cue_index: usize,
    pub cue_time: f64,
    pub beat_time: f64,
    pub elapsed_time: Duration,
    pub frames_sent: u64,
    pub current_cue: usize,
    pub show_start_time: Instant,
}

pub struct LightingConsole {
    tempo: f64,
    fixtures: Vec<Fixture>,
    link_state: ableton_link::State,
    dmx_output: artnet::ArtNet,
    cues: Vec<Cue>,
    //current_cue: usize,
    //show_start_time: Instant,
    rhythm_state: RhythmState,
    playback_state: PlaybackState,
}

impl LightingConsole {
    pub fn new(bpm: f64) -> Result<Self, anyhow::Error> {
        let link_state = ableton_link::State::new(bpm);
        link_state.link.enable(true);

        // Broadcast
        let dmx_output = ArtNet::new(ArtNetMode::Broadcast)?;

        // Unicast
        // let src = ("0.0.0.0", 6453).to_socket_addrs()?.next().unwrap();
        // let dest = ("192.168.1.78", 6454).to_socket_addrs()?.next().unwrap();
        // let dmx_output = ArtNet::new(ArtNetMode::Unicast(src, dest))?;

        Ok(LightingConsole {
            tempo: bpm,
            fixtures: Vec::new(),
            cues: Vec::new(),
            //current_cue: 0,
            //show_start_time: Instant::now(),
            link_state: link_state,
            dmx_output: dmx_output,
            playback_state: PlaybackState {
                cue_index: 0,
                cue_time: 0.0,
                beat_time: 0.0,
                elapsed_time: Duration::from_secs(0),
                frames_sent: 0,
                current_cue: 0,
                show_start_time: Instant::now(),
            },
            rhythm_state: RhythmState {
                beat_phase: 0.0,
                bar_phase: 0.0,
                phrase_phase: 0.0,
                beats_per_bar: 4,   // Default to 4/4 time
                bars_per_phrase: 4, // Default 4-bar phrase
            },
        })
    }

    // TODO - implement show loading and saving
    //
    // pub fn save_show(&self, path: &str) -> Result<(), Error> {
    //     let file = File::create(path)?;
    //     serde_json::to_writer_pretty(file, &self.cues)?;
    //     Ok(())
    // }

    // pub fn load_show(&mut self, path: &str) -> Result<(), Error> {
    //     let file = File::open(path)?;
    //     self.cues = serde_json::from_reader(file)?;
    //     Ok(())
    // }

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
                match buffer[0] {
                    b'G' | b'g' => tx.send(KeyCommand::Go).unwrap(),
                    b'[' => tx.send(KeyCommand::DecreaseBPM).unwrap(),
                    b']' => tx.send(KeyCommand::IncreaseBPM).unwrap(),
                    _ => {}
                }
            }
        });

        // Add enum for key commands
        enum KeyCommand {
            Go,
            IncreaseBPM,
            DecreaseBPM,
        }

        let mut frames_sent = 0;
        let mut last_update = Instant::now();
        let mut cue_time = 0.0; // TODO - I think cue time needs to be Instant::now()

        // Render loop
        loop {
            let frame_start = Instant::now();
            let elapsed_time = last_update.elapsed(); // TODO - rename to delta time?
                                                      //let elapsed_time = frame_start.duration_since(last_update);
            last_update = frame_start;

            // check for keyboard input
            // if rx.try_recv().is_ok() {
            //     self.playback_state.current_cue =
            //         (self.playback_state.current_cue + 1) % self.cues.len();
            //     cue_time = 0.0;
            //     println!(
            //         "Advanced to cue: {}",
            //         self.cues[self.playback_state.current_cue].name
            //     );
            // }

            if let Ok(cmd) = rx.try_recv() {
                match cmd {
                    KeyCommand::Go => {
                        self.playback_state.current_cue =
                            (self.playback_state.current_cue + 1) % self.cues.len();
                        cue_time = 0.0;
                        println!(
                            "Advanced to cue: {}",
                            self.cues[self.playback_state.current_cue].name
                        );
                    }
                    KeyCommand::IncreaseBPM => {
                        self.tempo += 1.0;
                        self.link_state.set_tempo(self.tempo);
                        self.link_state.session_state.set_tempo(bpm, at_time);
                    }
                    KeyCommand::DecreaseBPM => {
                        self.tempo = (self.tempo - 1.0).max(1.0);
                        self.link_state.set_tempo(self.tempo);
                    }
                }
            }

            self.link_state.capture_app_state();
            self.link_state.link.enable_start_stop_sync(true);
            self.link_state.commit_app_state();

            self.update(elapsed_time);
            self.render();

            frames_sent += 1;

            // TODO - what is this doing and do we need it?
            // if beat_time - cue_start_time >= cues[current_cue].duration {
            //     cue_start_time = beat_time;
            // }

            // reset cue time if it's greater than cue duration (loop cue)
            if cue_time >= self.cues[self.playback_state.current_cue].duration {
                cue_time = 0.0; // Reset cue time but don't change the cue
            }

            // Display status information
            let clock = &self.link_state.get_clock_state();
            self.display_status(
                clock,
                frames_sent,
                &self.cues[self.playback_state.current_cue].name,
                self.playback_state.show_start_time.elapsed(),
                cue_time,
                self.rhythm_state.beat_phase,
            );

            let elapsed = frame_start.elapsed();
            if elapsed < frame_duration {
                thread::sleep(frame_duration - elapsed);
            }
        }
    }

    pub fn update(&mut self, elapsed: Duration) {
        let clock = self.link_state.get_clock_state();
        let beat_time = clock.beats;

        self.tempo = self.link_state.session_state.tempo();
        self.update_rhythm_state(beat_time);

        if let Some(current_cue) = self.cues.get_mut(self.playback_state.current_cue) {
            // Apply cue-level static values first
            for static_value in &current_cue.static_values {
                if let Some(fixture) = self
                    .fixtures
                    .iter_mut()
                    .find(|f| f.name == static_value.fixture_name)
                {
                    fixture.set_channel_value(&static_value.channel_name, static_value.value);
                }
            }

            for chase in &mut current_cue.chases {
                chase.update(elapsed);

                // Apply chase-level static values
                for static_value in chase.get_current_static_values() {
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
                            channel.value =
                                (current_value + (target_value - current_value)).round() as u16;
                        }
                    }
                }

                // Apply chase-level effect mappings
                for mapping in chase.get_current_effect_mappings() {
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

                                channel.value = apply_effect(
                                    &mapping.effect,
                                    &self.rhythm_state,
                                    channel.value,
                                );
                            }
                        }
                    }
                }

                // Apply chase-level steps
                // TODO - implement
                // We probably need to determine the current chase step and apply the corresponding step values
            }
        }
    }

    pub fn render(&self) {
        // Send DMX data
        let dmx_data = self.generate_dmx_data();
        self.dmx_output.send_data(1, dmx_data);
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
        // Only render a single universe for now
        let mut dmx_data = vec![0; 512];
        for fixture in &self.fixtures {
            let start_channel = (fixture.start_address - 1) as usize;
            let end_channel = (start_channel + fixture.channels.len()).min(dmx_data.len());
            dmx_data[start_channel..end_channel].copy_from_slice(&fixture.get_dmx_values());
        }
        dmx_data
    }

    fn display_status(
        &self,
        clock: &ClockState,
        frames_sent: u64,
        current_cue: &str,
        elapsed: Duration,
        cue_time: f64,
        beat_time: f64,
    ) {
        let bpm = clock.tempo;
        let num_peers = clock.num_peers;
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
