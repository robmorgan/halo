use std::collections::HashMap;
use std::io::{stdout, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use halo_fixtures::{Fixture, FixtureLibrary};
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use parking_lot::Mutex;

use crate::ableton_link::ClockState;
use crate::artnet::artnet::ArtNet;
use crate::artnet::network_config::NetworkConfig;
use crate::cue::cue_manager::{CueManager, PlaybackState};
use crate::effect::effect::get_effect_phase;
use crate::midi::midi::{MidiMessage, MidiOverride};
use crate::programmer::Programmer;
use crate::show::show::Show;
use crate::{
    ableton_link, artnet, Cue, CueList, Effect, EffectDistribution, EffectMapping, RhythmState,
    ShowManager, StaticValue, TimeCode, TimeCodeManager,
};

const TARGET_FREQUENCY: f64 = 44.0; // 44Hz DMX Spec (every 25ms)
const TARGET_DELTA: f64 = 1.0 / TARGET_FREQUENCY;
const TARGET_DURATION: f64 = 1.0 / TARGET_FREQUENCY;

// TODO - bounding box for Rals dancefloor.
// One for spots and one for washes.

pub struct LightingConsole {
    // is the event loop running?
    is_running: bool,
    show_name: String,
    tempo: f64,
    fixture_library: FixtureLibrary,
    pub fixtures: Vec<Fixture>,
    pub link_state: ableton_link::State,
    pub cue_manager: CueManager,
    pub programmer: Programmer,
    pub show_manager: ShowManager,
    dmx_output: artnet::artnet::ArtNet,
    midi_overrides: HashMap<u8, MidiOverride>, // Key is MIDI note number
    active_overrides: HashMap<u8, (bool, u8)>, // Stores (active, velocity)
    override_tx: Sender<MidiMessage>,
    override_rx: Receiver<MidiMessage>,
    _midi_connection: Option<MidiInputConnection<()>>, // Keep connection alive
    _midi_output: Option<MidiOutputConnection>,
    rhythm_state: RhythmState,
}

impl LightingConsole {
    pub fn new(bpm: f64, network_config: NetworkConfig) -> Result<Self, anyhow::Error> {
        let link_state = ableton_link::State::new(bpm);
        link_state.link.enable(true);
        let dmx_output = ArtNet::new(network_config.mode)?;

        let (override_tx, override_rx) = channel();

        let show_manager = ShowManager::new()?;

        Ok(LightingConsole {
            is_running: true,
            show_name: "Untitled Show".to_string(),
            tempo: bpm,
            fixture_library: FixtureLibrary::new(),
            fixtures: Vec::new(),
            cue_manager: CueManager::new(Vec::new()),
            programmer: Programmer::new(),
            show_manager,
            link_state,
            dmx_output,
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
    ) -> Result<usize, String> {
        let profile = self
            .fixture_library
            .profiles
            .get(profile_name)
            .ok_or_else(|| format!("Profile {} not found", profile_name))?;

        // Assign a new ID to the fixture
        let id = self.fixtures.len();

        let fixture = Fixture {
            id,
            name: name.to_string(),
            profile_id: profile.id.clone(),
            profile: profile.clone(),
            channels: profile.channel_layout.clone(),
            universe,
            start_address: address,
        };

        self.fixtures.push(fixture);
        Ok(id)
    }

    pub fn set_cue_lists(&mut self, cue_lists: Vec<CueList>) {
        self.cue_manager.set_cue_lists(cue_lists);
    }

    pub fn init_mpk49_midi(&mut self) -> anyhow::Result<()> {
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

        let connection = midi_in
            .connect(
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
            )
            .map_err(|_| anyhow::anyhow!("opening input failed"))?; // workaround: https://github.com/Boddlnagg/midir/issues/55

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

        let output_connection = midi_out
            .connect(&out_port, "midi-display")
            .map_err(|_| anyhow::anyhow!("opening output failed"))?; // workaround: https://github.com/Boddlnagg/midir/issues/55

        self._midi_connection = Some(connection);
        self._midi_output = Some(output_connection);
        Ok(())
    }

    // pub fn set_midi_output(&mut self, midi_output: MidiOutputConnection) {
    //     self._midi_output = Some(midi_output);
    // }

    // Add a new MIDI override configuration
    pub fn add_midi_override(&mut self, note: u8, override_config: MidiOverride) {
        self.midi_overrides.insert(note, override_config);
        self.active_overrides.insert(note, (false, 0));
    }

    pub fn set_bpm(&mut self, bpm: f64) {
        // set the tempo using ableton's boundary
        self.tempo = bpm.min(999.0).max(20.0);
        self.link_state.set_tempo(self.tempo);
    }

    pub fn run(&mut self) -> Result<(), anyhow::Error> {
        // Render loop
        loop {
            // Process any pending MIDI messages
            while let Ok(midi_msg) = self.override_rx.try_recv() {
                match midi_msg {
                    MidiMessage::Clock => {
                        println!("MIDI Clock msg recv");
                        let now = Instant::now();
                        if let Some(last_time) = self.rhythm_state.last_tap_time {
                            let interval = now.duration_since(last_time).as_secs_f64();
                            let new_bpm = 60.0 / interval;
                            let _ = self.link_state.set_tempo(new_bpm);
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

                        // Go Button: Advance the cue
                        if cc == 116 && value > 64 {
                            // Example: CC #116 when value goes above 64
                            if let Err(err) = self.cue_manager.go() {
                                println!("Error advancing cue: {}", err);
                            }
                        }

                        // K1 Knob: Set the BPM
                        if cc == 22 {
                            // Control Encoder 22
                            // Scale 0-127 to 60-187 BPM range
                            let bpm = 60.0 + (value as f64 / 127.0) * (187.0 - 60.0);
                            self.set_bpm(bpm);
                        }
                    }
                }
            }

            self.update();
            self.render();
        }
    }

    pub fn update(&mut self) {
        // TODO - wrap this code in is_link_enabled?
        self.link_state.capture_app_state();
        self.link_state.link.enable_start_stop_sync(true);
        self.link_state.commit_app_state();

        let clock = self.link_state.get_clock_state();
        let beat_time = clock.beats;

        self.tempo = self.link_state.session_state.tempo();
        self.update_rhythm_state(beat_time);

        // Is the console currently playing a cue?
        if self.cue_manager.get_playback_state() == PlaybackState::Playing {
            if let Some(current_cue) = self.cue_manager.get_current_cue().cloned() {
                // Apply cue-level static values first
                self.apply_values(current_cue.static_values);

                // apply effect mappings
                self.apply_effects(current_cue.effects);
            }
        }

        // Render any values from the programmer
        self.apply_programmer_values();

        // Update the Cue Manager
        self.cue_manager.update();
    }

    fn apply_values(&mut self, values: Vec<StaticValue>) {
        for value in &values {
            if let Some(fixture) = self.fixtures.iter_mut().find(|f| f.id == value.fixture_id) {
                fixture.set_channel_value(&value.channel_type, value.value);
            }
        }
    }

    fn apply_effects(&mut self, effects: Vec<EffectMapping>) {
        for mapping in &effects {
            // get all fixtures that use this mapping
            let mut affected_fixtures: Vec<&mut Fixture> = self
                .fixtures
                .iter_mut()
                .filter(|f| mapping.fixture_ids.contains(&f.id))
                .collect();

            // apply the effect to each fixture
            for (i, fixture) in affected_fixtures.iter_mut().enumerate() {
                if let Some(channel) = fixture.channels.iter_mut().find(|c| {
                    std::mem::discriminant(&c.channel_type)
                        == std::mem::discriminant(&mapping.channel_type)
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

    fn apply_programmer_values(&mut self) {
        if self.programmer.get_preview_mode() {
            // apply programmer static values
            self.apply_values(self.programmer.get_values().clone());

            // apply programmer effects
            self.apply_effects(self.programmer.get_effects().clone());
        }
    }

    pub fn render(&self) {
        // Send DMX data
        let dmx_data = self.generate_dmx_data();
        self.dmx_output.send_data(1, dmx_data);
    }

    pub fn go(&mut self) -> Result<&Cue, String> {
        self.cue_manager.go()
    }

    pub fn record_cue(&mut self, cue_name: String, fade_time: f32) {
        let cue_list_idx = self.cue_manager.get_current_cue_list_idx();
        let programmer_values = self.programmer.get_values();
        let effects = self.programmer.get_effects();
        self.cue_manager.record(
            cue_name,
            cue_list_idx,
            fade_time,
            programmer_values.clone(),
            effects.clone(),
        );
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

    pub fn new_show(&mut self, name: String) -> Result<(), anyhow::Error> {
        let _ = self.show_manager.new_show(name);
        Ok(())
    }

    pub fn save_show(&mut self) -> Result<PathBuf, anyhow::Error> {
        let result = self.show_manager.save_show(&self.get_show().clone())?;
        Ok(result)
    }

    pub fn save_show_as(&mut self, name: String, path: PathBuf) -> Result<PathBuf, anyhow::Error> {
        self.show_name = name;
        let result = self
            .show_manager
            .save_show_as(&self.get_show().clone(), path)?;

        Ok(result)
    }

    pub fn load_show(&mut self, path: &Path) -> Result<(), anyhow::Error> {
        let show = self.show_manager.load_show(path)?;
        // TODO - we'll probably want to lookup fixtures here
        self.fixtures = show.fixtures.clone();
        self.cue_manager.set_cue_lists(show.cue_lists.clone());
        Ok(())
    }

    pub fn get_show(&self) -> Show {
        let mut show = Show::new(self.show_name.clone());
        show.fixtures = self.fixtures.clone();
        show.cue_lists = self.cue_manager.get_cue_lists().clone();
        show.modified_at = SystemTime::now();
        show
    }

    fn display_status(
        &self,
        clock: &ClockState,
        frames_sent: u64,
        current_cue: &str,
        cue_time: f64,
        beat_time: f64,
    ) {
        let bpm = clock.tempo;
        let num_peers = clock.num_peers;

        print!("\r"); // Move cursor to the beginning of the line
        print!(
            "Frames: {:8} | BPM: {:6.2} | Peers: {:3} | Current Cue: {:3} | Cue Time: {:6.2}s | Beat: {:6.2}",
            frames_sent, bpm, num_peers, current_cue, cue_time, beat_time
        );

        // TODO - I'd love an ascii progress bar here with the current cue progress

        stdout().flush().unwrap();
    }
}

fn apply_effect(effect: &Effect, rhythm: &RhythmState, current_value: u8) -> u8 {
    let phase = get_effect_phase(rhythm, &effect.params);
    let target_value = effect.apply(phase);
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

/// EventLoop handles the main event loop and communication with the hardware.
pub struct EventLoop {
    console: Arc<Mutex<LightingConsole>>,
    frequency: f64,
    frames_sent: i128,
}

impl EventLoop {
    pub fn new(console: Arc<Mutex<LightingConsole>>, frequency: f64) -> Self {
        Self {
            console,
            frequency,
            frames_sent: 0,
        }
    }

    pub fn run(&mut self) {
        let target_cycle_time = Duration::from_secs_f64(1.0 / self.frequency);

        loop {
            let is_running = {
                // Scope the mutex guard to release it as soon as possible
                let console = self.console.lock();
                if !console.is_running {
                    break;
                }
                true
            };

            if !is_running {
                break;
            }

            let cycle_start = Instant::now();

            // Perform update operations
            self.update();

            // Sleep to maintain target frequency
            let elapsed = cycle_start.elapsed();
            if elapsed < target_cycle_time {
                thread::sleep(target_cycle_time - elapsed);
            }
        }
    }

    pub fn update(&mut self) {
        {
            // Scope the mutex guard to minimize lock time
            let mut console = self.console.lock();
            console.update();
            console.render();
            self.frames_sent += 1;
        }
    }
}
