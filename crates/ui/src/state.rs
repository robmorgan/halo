use std::collections::HashMap;
use std::time::SystemTime;

use halo_core::{
    AudioDeviceInfo, ConsoleCommand, CueList, PlaybackState, RhythmState, Settings, Show, TimeCode,
};
use halo_fixtures::{Fixture, FixtureLibrary};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct ConsoleState {
    pub fixtures: HashMap<String, Fixture>,
    pub cue_lists: Vec<CueList>,
    pub current_cue_list_index: usize,
    pub current_cue_index: usize,
    pub current_cue_progress: f32,
    pub playback_state: PlaybackState,
    pub bpm: f64,
    pub current_time: SystemTime,
    pub link_peers: u32,
    pub link_quantum: f64,
    pub link_tempo: f64,
    pub link_start_stop_sync: bool,
    pub link_enabled: bool,
    pub rhythm_state: RhythmState,
    pub show: Option<Show>,
    pub timecode: Option<TimeCode>,
    pub programmer_preview_mode: bool,
    pub programmer_collapsed: bool,
    pub selected_fixtures: Vec<usize>,
    pub programmer_values: HashMap<(usize, String), u8>, // (fixture_id, channel) -> value
    pub programmer_effects: Vec<(String, halo_core::EffectType, Vec<usize>)>, /* (name, effect_type, fixture_ids) */
    pub settings: Settings,
    pub audio_devices: Vec<AudioDeviceInfo>,
    pub fixture_library: FixtureLibrary,
    pub active_effects_count: usize,
    pub last_error: Option<String>,
}

impl Default for ConsoleState {
    fn default() -> Self {
        Self {
            fixtures: HashMap::new(),
            cue_lists: Vec::new(),
            current_cue_list_index: 0,
            current_cue_index: 0,
            current_cue_progress: 0.0,
            playback_state: PlaybackState::Stopped,
            bpm: 120.0,
            current_time: SystemTime::now(),
            link_peers: 0,
            link_quantum: 4.0,
            link_tempo: 120.0,
            link_start_stop_sync: false,
            link_enabled: false,
            rhythm_state: RhythmState {
                beat_phase: 0.0,
                bar_phase: 0.0,
                phrase_phase: 0.0,
                beats_per_bar: 4,
                bars_per_phrase: 4,
                last_tap_time: None,
                tap_count: 0,
            },
            show: None,
            timecode: None,
            programmer_preview_mode: false,
            programmer_collapsed: false,
            selected_fixtures: Vec::new(),
            programmer_values: HashMap::new(),
            programmer_effects: Vec::new(),
            settings: Settings::default(),
            audio_devices: Vec::new(),
            fixture_library: FixtureLibrary::new(),
            active_effects_count: 0,
            last_error: None,
        }
    }
}

impl ConsoleState {
    pub fn update(&mut self, event: halo_core::ConsoleEvent) {
        match event {
            halo_core::ConsoleEvent::FixturesUpdated { fixtures } => {
                self.fixtures.clear();
                for fixture in fixtures {
                    self.fixtures.insert(fixture.id.to_string(), fixture);
                }
            }
            halo_core::ConsoleEvent::CueListsUpdated { cue_lists } => {
                self.cue_lists = cue_lists;
            }
            halo_core::ConsoleEvent::CueListSelected { list_index } => {
                self.current_cue_list_index = list_index;
            }
            halo_core::ConsoleEvent::CurrentCueChanged {
                cue_index,
                progress,
            } => {
                self.current_cue_index = cue_index;
                self.current_cue_progress = progress;
            }
            halo_core::ConsoleEvent::PlaybackStateChanged { state } => {
                self.playback_state = state;
            }
            halo_core::ConsoleEvent::BpmChanged { bpm } => {
                self.bpm = bpm;
            }
            halo_core::ConsoleEvent::TimecodeUpdated { timecode } => {
                self.timecode = Some(timecode);
            }
            halo_core::ConsoleEvent::LinkStateChanged { enabled, num_peers } => {
                self.link_peers = num_peers as u32;
                self.link_enabled = enabled;
            }
            halo_core::ConsoleEvent::FixturePatched {
                fixture_id,
                fixture,
            } => {
                self.fixtures.insert(fixture_id.to_string(), fixture);
            }
            halo_core::ConsoleEvent::FixtureUnpatched { fixture_id } => {
                self.fixtures.remove(&fixture_id.to_string());
            }
            halo_core::ConsoleEvent::FixtureUpdated {
                fixture_id,
                fixture,
            } => {
                self.fixtures.insert(fixture_id.to_string(), fixture);
            }
            halo_core::ConsoleEvent::FixtureLibraryList { profiles } => {
                // Update the fixture library with the profiles from the console
                for (id, _display_name) in profiles {
                    // The library is already initialized with all profiles, so we don't need to do
                    // anything here This event is mainly for UI updates
                    // We could potentially use this to populate a cache if needed in the future
                    let _ = id; // Suppress unused warning
                }
            }
            halo_core::ConsoleEvent::ShowLoaded { show } => {
                self.fixtures.clear();
                for fixture in &show.fixtures {
                    self.fixtures
                        .insert(fixture.id.to_string(), fixture.clone());
                }
                self.cue_lists = show.cue_lists.clone();
                self.current_cue_list_index = 0; // Reset to first cue list when show is loaded
                self.show = Some(show);
            }
            halo_core::ConsoleEvent::RhythmStateUpdated { state } => {
                self.rhythm_state = state;
            }
            halo_core::ConsoleEvent::ProgrammerStateUpdated {
                preview_mode,
                collapsed,
                selected_fixtures,
            } => {
                self.programmer_preview_mode = preview_mode;
                self.programmer_collapsed = collapsed;
                self.selected_fixtures = selected_fixtures;
            }
            halo_core::ConsoleEvent::ProgrammerValuesUpdated { values } => {
                self.programmer_values.clear();
                for (fixture_id, channel, value) in values {
                    self.programmer_values.insert((fixture_id, channel), value);
                }
            }
            halo_core::ConsoleEvent::ProgrammerEffectsUpdated { effects } => {
                self.programmer_effects = effects;
            }
            // Handle query responses
            halo_core::ConsoleEvent::FixturesList { fixtures } => {
                self.fixtures.clear();
                for fixture in fixtures {
                    self.fixtures.insert(fixture.id.to_string(), fixture);
                }
            }
            halo_core::ConsoleEvent::CueListsList { cue_lists } => {
                self.cue_lists = cue_lists;
                // Reset to first cue list when cue lists are loaded
                self.current_cue_list_index = 0;
            }
            halo_core::ConsoleEvent::CurrentCueListIndex { index } => {
                self.current_cue_list_index = index;
            }
            halo_core::ConsoleEvent::CurrentCue {
                cue_index,
                progress,
            } => {
                self.current_cue_index = cue_index;
                self.current_cue_progress = progress;
            }
            halo_core::ConsoleEvent::CurrentPlaybackState { state } => {
                self.playback_state = state;
            }
            halo_core::ConsoleEvent::CurrentRhythmState { state } => {
                self.rhythm_state = state;
            }
            halo_core::ConsoleEvent::CurrentShow { show } => {
                self.fixtures.clear();
                for fixture in &show.fixtures {
                    self.fixtures
                        .insert(fixture.id.to_string(), fixture.clone());
                }
                self.cue_lists = show.cue_lists.clone();
                self.current_cue_list_index = 0; // Reset to first cue list when show is loaded
                self.show = Some(show);
            }
            halo_core::ConsoleEvent::SettingsUpdated { settings } => {
                self.settings = settings;
            }
            halo_core::ConsoleEvent::CurrentSettings { settings } => {
                self.settings = settings;
            }
            halo_core::ConsoleEvent::AudioDevicesList { devices } => {
                self.audio_devices = devices;
            }
            halo_core::ConsoleEvent::TrackingStateUpdated {
                active_effect_count,
            } => {
                self.active_effects_count = active_effect_count;
            }
            halo_core::ConsoleEvent::Error { message } => {
                self.last_error = Some(message);
            }
            _ => {
                // Handle other events as needed
            }
        }
    }
}

/// Context struct that combines console state and command sender
/// This reduces parameter passing and provides a cleaner interface for UI components
pub struct ConsoleContext<'a> {
    pub state: &'a ConsoleState,
    pub console_tx: &'a mpsc::UnboundedSender<ConsoleCommand>,
}

impl<'a> ConsoleContext<'a> {
    pub fn new(
        state: &'a ConsoleState,
        console_tx: &'a mpsc::UnboundedSender<ConsoleCommand>,
    ) -> Self {
        Self { state, console_tx }
    }

    /// Convenience method to send a command
    pub fn send_command(&self, command: ConsoleCommand) {
        let _ = self.console_tx.send(command);
    }
}
