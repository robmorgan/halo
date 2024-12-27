use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::collections::HashMap;
use std::io::{self, stdout, Read, Write};
use std::sync::mpsc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;
use std::time::Instant;

use crate::ableton_link::ClockState;
use crate::artnet::{self, ArtNet, ArtNetMode};
use crate::cue::{Chase, ChaseStep, Cue, EffectDistribution, EffectMapping, StaticValue};
use crate::effect::{Effect, EffectParams};
use crate::fixture::{Channel, ChannelType, Fixture, FixtureLibrary};
use crate::midi::{MidiMessage, MidiOverride};
use crate::rhythm::RhythmState;
use crate::{ableton_link, effect};

const TARGET_FREQUENCY: f64 = 40.0; // 40Hz DMX Spec (every 25ms)
const TARGET_DELTA: f64 = 1.0 / TARGET_FREQUENCY;
const TARGET_DURATION: f64 = 1.0 / TARGET_FREQUENCY;

// TODO - bounding box for Rals dancefloor.
// One for spots and one for washes.

// Key commands
enum KeyCommand {
    Go,
    IncreaseBPM,
    DecreaseBPM,
}

pub struct LightingConsole {
    tempo: f64,
    fixture_library: FixtureLibrary,
    fixtures: Vec<Fixture>,
    link_state: ableton_link::State,
    dmx_output: artnet::ArtNet,
    cues: Vec<Cue>,
    current_cue: usize,
    show_start_time: Instant,
    midi_overrides: HashMap<u8, MidiOverride>, // Key is MIDI note number
    active_overrides: HashMap<u8, (bool, u8)>, // Stores (active, velocity)
    override_tx: Sender<MidiMessage>,
    override_rx: Receiver<MidiMessage>,
    _midi_connection: Option<MidiInputConnection<()>>, // Keep connection alive
    _midi_output: Option<MidiOutputConnection>,
    rhythm_state: RhythmState,
}

impl LightingConsole {
    pub fn new(bpm: f64) -> Result<Self, anyhow::Error> {
        let link_state = ableton_link::State::new(bpm);
        link_state.link.enable(true);
        let dmx_output = ArtNet::new(ArtNetMode::Broadcast)?;

        let (override_tx, override_rx) = channel();

        Ok(LightingConsole {
            tempo: bpm,
            fixture_library: FixtureLibrary::new(),
            fixtures: Vec::new(),
            cues: Vec::new(),
            current_cue: 0,
            show_start_time: Instant::now(),
            link_state: link_state,
            dmx_output: dmx_output,
            midi_overrides: HashMap::new(),
            active_overrides: HashMap::new(),
            override_tx,
            override_rx,
            _midi_connection: None,
            _midi_output: None,
            rhythm_state: RhythmState {
                beat_phase: 0.0,
                bar_phase: 0.0,
                phrase_phase: 0.0,
                beats_per_bar: 4,   // Default to 4/4 time
                bars_per_phrase: 4, // Default 4-bar phrase
                last_tap_time: Option::None,
                tap_count: 0,
            },
        })
    }

    pub fn load_fixture_library(&mut self) {
        let fixture_library = FixtureLibrary::new();
        self.fixture_library = fixture_library;
    }

    pub fn patch_fixture(
        &mut self,
        name: &str,
        profile_name: &str,
        universe: u8,
        address: u16,
    ) -> Result<(), String> {
        let profile = self
            .fixture_library
            .profiles
            .get(&profile_name.to_string())
            .ok_or_else(|| format!("Profile {} not found", profile_name))?;

        let fixture = Fixture {
            name: name.to_string(),
            profile_name: profile_name.to_string(),
            channels: profile.channel_layout.clone(),
            universe: universe,
            start_address: address,
        };

        self.fixtures.push(fixture);
        Ok(())
    }

    pub fn set_cues(&mut self, cues: Vec<Cue>) {
        self.cues = cues;
    }

    pub fn add_cue(&mut self, cue: Cue) {
        self.cues.push(cue);
    }

    pub fn init_mpk49_midi(&mut self) -> Result<(), anyhow::Error> {
        let midi_in = MidiInput::new("halo_controller")?;
        let midi_out = MidiOutput::new("halo_controller")?;

        // Find the MPK49 port
        let port = midi_in
            .ports()
            .into_iter()
            .find(|port| {
                midi_in
                    .port_name(port)
                    .map(|name| name.contains("MPK49"))
                    .unwrap_or(false)
            })
            .ok_or_else(|| anyhow::Error::msg("MPK49 not found"))?;

        let tx = self.override_tx.clone();

        let connection = midi_in.connect(
            &port,
            "midi-override",
            move |_timestamp, message, _| {
                if message.len() >= 3 {
                    match message[0] & 0xF0 {
                        0xF8 => {
                            // MIDI Clock message
                            println!("midi clock on: {} {}", message[1], message[2]);
                            tx.send(MidiMessage::Clock).unwrap();
                        }
                        0x90 => {
                            println!("midi note on: {} {}", message[1], message[2]);
                            // Note On
                            if message[2] > 0 {
                                tx.send(MidiMessage::NoteOn(message[1], message[2]))
                                    .unwrap();
                            } else {
                                tx.send(MidiMessage::NoteOff(message[1])).unwrap();
                            }
                        }
                        0x80 => {
                            // Note Off
                            tx.send(MidiMessage::NoteOff(message[1])).unwrap();
                        }
                        0xB0 => {
                            // Control Change
                            println!("midi control change on: {} {}", message[1], message[2]);
                            tx.send(MidiMessage::ControlChange(message[1], message[2]))
                                .unwrap();
                        }
                        _ => (),
                    }
                }
            },
            (),
        )?;

        let out_port = midi_out
            .ports()
            .into_iter()
            .find(|port| {
                midi_out
                    .port_name(port)
                    .map(|name| name.contains("MPK49"))
                    .unwrap_or(false)
            })
            .ok_or_else(|| anyhow::Error::msg("MPK49 output not found"))?;

        let output_connection = midi_out.connect(&out_port, "midi-display")?;

        self._midi_connection = Some(connection);
        self._midi_output = Some(output_connection);
        Ok(())
    }

    // pub fn set_midi_output(&mut self, midi_output: MidiOutputConnection) {
    //     self._midi_output = Some(midi_output);
    // }

    // Send a message to the MIDI LCD
    pub fn send_to_midi_lcd(&mut self, text: &str) {
        if let Some(output) = &mut self._midi_output {
            // Format SysEx message
            let message = format_lcd_message(text);
            output.send(&message).unwrap();
        }
    }

    // Add a new MIDI override configuration
    pub fn add_midi_override(&mut self, note: u8, override_config: MidiOverride) {
        self.midi_overrides.insert(note, override_config);
        self.active_overrides.insert(note, (false, 0));
    }

    pub fn run(&mut self) {
        let fps = TARGET_FREQUENCY;
        let frame_duration = Duration::from_secs_f64(1.0 / fps as f64);

        // Keyboard input handling
        let (tx, rx) = mpsc::channel();

        // Enable raw mode before spawning input thread
        enable_raw_mode().unwrap();

        thread::spawn(move || loop {
            if let Ok(Event::Key(KeyEvent {
                code, modifiers, ..
            })) = event::read()
            {
                match (code, modifiers) {
                    (KeyCode::Char('g'), _) => tx.send(KeyCommand::Go).unwrap(),
                    (KeyCode::Char('['), _) => tx.send(KeyCommand::DecreaseBPM).unwrap(),
                    (KeyCode::Char(']'), _) => tx.send(KeyCommand::IncreaseBPM).unwrap(),
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        disable_raw_mode().unwrap();
                        std::process::exit(0);
                    }
                    _ => {}
                }
            }
        });

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
            if let Ok(cmd) = rx.try_recv() {
                match cmd {
                    KeyCommand::Go => {
                        self.current_cue = (self.current_cue + 1) % self.cues.len();
                        cue_time = 0.0;
                        println!("Advanced to cue: {}", self.cues[self.current_cue].name);
                    }
                    KeyCommand::IncreaseBPM => {
                        self.tempo += 1.0;
                        self.link_state.set_tempo(self.tempo);
                    }
                    KeyCommand::DecreaseBPM => {
                        self.tempo = (self.tempo - 1.0).max(1.0);
                        self.link_state.set_tempo(self.tempo);
                    }
                }
            }

            // Process any pending MIDI messages
            while let Ok(midi_msg) = self.override_rx.try_recv() {
                match midi_msg {
                    MidiMessage::Clock => {
                        println!("MIDI Clock msg recv");
                        let now = Instant::now();
                        if let Some(last_time) = self.rhythm_state.last_tap_time {
                            let interval = now.duration_since(last_time).as_secs_f64();
                            let new_bpm = 60.0 / interval;
                            self.link_state.set_tempo(new_bpm);
                        }
                        self.rhythm_state.last_tap_time = Some(now);
                    }
                    MidiMessage::NoteOn(note, velocity) => {
                        if let Some(active) = self.active_overrides.get_mut(&note) {
                            *active = (true, velocity);
                        }
                    }
                    MidiMessage::NoteOff(note) => {
                        if let Some(active) = self.active_overrides.get_mut(&note) {
                            *active = (false, 0);
                        }
                    }
                    MidiMessage::ControlChange(cc, value) => {
                        // Handle CC messages (knobs/faders) here
                        // This is where you could implement continuous control
                        println!("CC: {}, Value: {}", cc, value);
                        // Advance the cue
                        if cc == 116 && value > 64 {
                            // Example: CC #116 when value goes above 64
                            self.current_cue = (self.current_cue + 1) % self.cues.len();
                            cue_time = 0.0;
                            println!("Advanced to cue: {}", self.cues[self.current_cue].name);
                        }
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
            if cue_time >= self.cues[self.current_cue].duration {
                cue_time = 0.0; // Reset cue time but don't change the cue
            }

            // Display status information
            let clock = &self.link_state.get_clock_state();
            self.display_status(
                clock,
                frames_sent,
                &self.cues[self.current_cue].name,
                self.show_start_time.elapsed(),
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

        if let Some(current_cue) = self.cues.get_mut(self.current_cue) {
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
                                (current_value + (target_value - current_value)).round() as u8;
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

                    // if affected fixtures is empty, log a warning and continue
                    if affected_fixtures.is_empty() {
                        println!(
                            "Warning: No fixtures found using mapping for fixtures: {:?}",
                            mapping.fixture_names.join(", ")
                        );
                    }

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

        // Then apply any active MIDI overrides
        for (note, (is_active, _velocity)) in &self.active_overrides {
            if *is_active {
                if let Some(override_config) = self.midi_overrides.get(note) {
                    // Apply overrides to fixtures
                    for sv in override_config.static_values.iter() {
                        if let Some(fixture) =
                            self.fixtures.iter_mut().find(|f| f.name == sv.fixture_name)
                        {
                            if let Some(channel) = fixture
                                .channels
                                .iter_mut()
                                .find(|c| c.name == sv.channel_name)
                            {
                                println!(
                            "\napplying midi override to fixture {:?} for channel {:?} with value {:?}\n",
                            sv.fixture_name, sv.channel_name, sv.value
                        );
                                channel.value = sv.value;
                            }
                        }
                    }
                }
            } else {
                // Check if any other active overrides control this channel
                let should_reset = |fixture_name: &str, channel_name: &str| -> bool {
                    !self
                        .active_overrides
                        .iter()
                        .any(|(other_note, (is_active, _))| {
                            if *is_active && other_note != note {
                                if let Some(other_config) = self.midi_overrides.get(other_note) {
                                    return other_config.static_values.iter().any(|sv| {
                                        sv.fixture_name == fixture_name
                                            && sv.channel_name == channel_name
                                    });
                                }
                            }
                            false
                        })
                };

                // Reset channels when override is inactive, but only if no other override is using them
                if let Some(override_config) = self.midi_overrides.get(note) {
                    for sv in override_config.static_values.iter() {
                        if should_reset(&sv.fixture_name, &sv.channel_name) {
                            if let Some(fixture) =
                                self.fixtures.iter_mut().find(|f| f.name == sv.fixture_name)
                            {
                                if let Some(channel) = fixture
                                    .channels
                                    .iter_mut()
                                    .find(|c| c.name == sv.channel_name)
                                {
                                    channel.value = 0;
                                }
                            }
                        }
                    }
                }
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

        // TODO - I'd love an ascii progress bar here with the current cue progress

        stdout().flush().unwrap();
    }
}

fn apply_effect(effect: &Effect, rhythm: &RhythmState, current_value: u8) -> u8 {
    let phase = effect::get_effect_phase(rhythm, &effect.params);
    let target_value = (effect.apply)(phase);
    let target_dmx = (target_value * (effect.max - effect.min) as f64 + effect.min as f64) as f64;

    // Smooth transition
    let current_dmx = current_value as f64;
    let new_dmx = current_dmx + (target_dmx - current_dmx);

    new_dmx.round() as u8
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

fn format_lcd_message(text: &str) -> Vec<u8> {
    let mut message = vec![
        0xF0, // Start of SysEx
        0x47, // Akai Manufacturer ID
        0x7F, // All channels
        0x7C, // LCD Display message
    ];

    // Add the text bytes, truncated to display width
    message.extend(text.chars().take(16).map(|c| c as u8));

    // End SysEx
    message.push(0xF7);

    message
}
