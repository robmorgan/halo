use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

use halo_fixtures::{Fixture, FixtureLibrary};

use crate::artnet::network_config::NetworkConfig;
use crate::cue::cue_manager::{CueManager, PlaybackState};
use crate::modules::{
    ModuleManager, ModuleEvent, ModuleId, ModuleMessage,
    AudioModule, DmxModule, MidiModule, SmpteModule,
};
use crate::midi::midi::{MidiMessage, MidiOverride, MidiAction};
use crate::programmer::Programmer;
use crate::show::show_manager::ShowManager;
use crate::{ableton_link, CueList, StaticValue, RhythmState};

pub struct AsyncLightingConsole {
    // Core components
    show_name: String,
    tempo: f64,
    fixture_library: FixtureLibrary,
    pub fixtures: Arc<RwLock<Vec<Fixture>>>,
    pub link_state: ableton_link::State,
    pub cue_manager: Arc<RwLock<CueManager>>,
    pub programmer: Arc<RwLock<Programmer>>,
    pub show_manager: Arc<RwLock<ShowManager>>,
    
    // Async module system
    module_manager: ModuleManager,
    message_handler: Option<JoinHandle<()>>,
    
    // MIDI overrides
    midi_overrides: HashMap<u8, MidiOverride>,
    active_overrides: HashMap<u8, (bool, u8)>,
    
    // Rhythm state
    rhythm_state: Arc<RwLock<RhythmState>>,
    
    // System state
    is_running: bool,
}

impl AsyncLightingConsole {
    pub fn new(bpm: f64, network_config: NetworkConfig) -> Result<Self, anyhow::Error> {
        let link_state = ableton_link::State::new(bpm);
        link_state.link.enable(true);
        
        let mut module_manager = ModuleManager::new();
        
        // Register async modules
        module_manager.register_module(Box::new(DmxModule::new(network_config)));
        module_manager.register_module(Box::new(AudioModule::new()));
        module_manager.register_module(Box::new(SmpteModule::new(30))); // 30fps default
        module_manager.register_module(Box::new(MidiModule::new("MPK49".to_string())));
        
        let show_manager = ShowManager::new()?;
        
        Ok(Self {
            show_name: "Untitled Show".to_string(),
            tempo: bpm,
            fixture_library: FixtureLibrary::new(),
            fixtures: Arc::new(RwLock::new(Vec::new())),
            link_state,
            cue_manager: Arc::new(RwLock::new(CueManager::new(Vec::new()))),
            programmer: Arc::new(RwLock::new(Programmer::new())),
            show_manager: Arc::new(RwLock::new(show_manager)),
            module_manager,
            message_handler: None,
            midi_overrides: HashMap::new(),
            active_overrides: HashMap::new(),
            rhythm_state: Arc::new(RwLock::new(RhythmState {
                beat_phase: 0.0,
                bar_phase: 0.0,
                phrase_phase: 0.0,
                beats_per_bar: 4,
                bars_per_phrase: 4,
                last_tap_time: None,
                tap_count: 0,
            })),
            is_running: false,
        })
    }

    /// Initialize the async console and all modules
    pub async fn initialize(&mut self) -> Result<(), anyhow::Error> {
        log::info!("Initializing async lighting console...");
        
        // Initialize all modules
        self.module_manager.initialize().await?;
        
        // Start all modules
        self.module_manager.start().await?;
        
        // Start message handling
        if let Some(mut message_rx) = self.module_manager.take_message_receiver() {
            let rhythm_state = Arc::clone(&self.rhythm_state);
            let cue_manager = Arc::clone(&self.cue_manager);
            let fixtures = Arc::clone(&self.fixtures);
            
            let handle = tokio::spawn(async move {
                while let Some(message) = message_rx.recv().await {
                    Self::handle_module_message(
                        message, 
                        &rhythm_state, 
                        &cue_manager, 
                        &fixtures
                    ).await;
                }
            });
            
            self.message_handler = Some(handle);
        }
        
        self.is_running = true;
        log::info!("Async lighting console initialized successfully");
        Ok(())
    }

    async fn handle_module_message(
        message: ModuleMessage,
        rhythm_state: &Arc<RwLock<RhythmState>>,
        cue_manager: &Arc<RwLock<CueManager>>,
        fixtures: &Arc<RwLock<Vec<Fixture>>>,
    ) {
        match message {
            ModuleMessage::Event(event) => {
                match event {
                    ModuleEvent::MidiInput(midi_msg) => {
                        Self::handle_midi_input(midi_msg, rhythm_state, cue_manager).await;
                    }
                    _ => {
                        // Handle other inter-module events as needed
                    }
                }
            }
            ModuleMessage::Status(status) => {
                log::info!("Module status: {}", status);
            }
            ModuleMessage::Error(error) => {
                log::error!("Module error: {}", error);
            }
        }
    }

    async fn handle_midi_input(
        midi_msg: MidiMessage,
        rhythm_state: &Arc<RwLock<RhythmState>>,
        cue_manager: &Arc<RwLock<CueManager>>,
    ) {
        match midi_msg {
            MidiMessage::Clock => {
                // Handle MIDI clock for tempo sync
                log::debug!("MIDI Clock received");
            }
            MidiMessage::NoteOn(note, velocity) => {
                log::info!("MIDI Note On: {} velocity: {}", note, velocity);
                // Handle MIDI note on for cue triggers, etc.
            }
            MidiMessage::NoteOff(note) => {
                log::info!("MIDI Note Off: {}", note);
                // Handle MIDI note off
            }
            MidiMessage::ControlChange(cc, value) => {
                log::info!("MIDI CC: {} value: {}", cc, value);
                
                // Handle specific control changes
                match cc {
                    116 if value > 64 => {
                        // Go button
                        let mut cue_mgr = cue_manager.write().await;
                        if let Err(e) = cue_mgr.go() {
                            log::error!("Error advancing cue: {}", e);
                        }
                    }
                    22 => {
                        // BPM control
                        let bpm = 60.0 + (value as f64 / 127.0) * (187.0 - 60.0);
                        log::info!("Setting BPM to {}", bpm);
                        // Update tempo via rhythm state
                    }
                    _ => {}
                }
            }
        }
    }

    /// Main update loop - call this regularly to process lighting data
    pub async fn update(&mut self) -> Result<(), anyhow::Error> {
        // Update Ableton Link state
        self.link_state.capture_app_state();
        self.link_state.link.enable_start_stop_sync(true);
        self.link_state.commit_app_state();

        let clock = self.link_state.get_clock_state();
        let beat_time = clock.beats;
        self.tempo = self.link_state.session_state.tempo();

        // Update rhythm state
        self.update_rhythm_state(beat_time).await;

        // Process current cue if playing
        {
            let cue_manager = self.cue_manager.read().await;
            if cue_manager.get_playback_state() == PlaybackState::Playing {
                if let Some(current_cue) = cue_manager.get_current_cue() {
                    // Apply cue values and effects
                    self.apply_cue_data(current_cue.clone()).await;
                }
            }
        }

        // Apply programmer values
        self.apply_programmer_values().await;

        // Generate and send DMX data
        self.send_dmx_data().await?;

        // Update cue manager
        {
            let mut cue_manager = self.cue_manager.write().await;
            cue_manager.update();
        }

        Ok(())
    }

    async fn update_rhythm_state(&self, beat_time: f64) {
        let mut rhythm = self.rhythm_state.write().await;
        rhythm.beat_phase = beat_time.fract();
        rhythm.bar_phase = (beat_time / rhythm.beats_per_bar as f64).fract();
        rhythm.phrase_phase = (beat_time / (rhythm.beats_per_bar * rhythm.bars_per_phrase) as f64).fract();
    }

    async fn apply_cue_data(&self, cue: crate::cue::cue::Cue) {
        let mut fixtures = self.fixtures.write().await;
        
        // Apply static values
        for value in &cue.static_values {
            if let Some(fixture) = fixtures.iter_mut().find(|f| f.id == value.fixture_id) {
                fixture.set_channel_value(&value.channel_type, value.value);
            }
        }

        // Apply effects would go here - similar to the original implementation
        // but we'd need to access the rhythm state
    }

    async fn apply_programmer_values(&self) {
        let programmer = self.programmer.read().await;
        if programmer.get_preview_mode() {
            let values = programmer.get_values();
            let mut fixtures = self.fixtures.write().await;
            
            for value in values {
                if let Some(fixture) = fixtures.iter_mut().find(|f| f.id == value.fixture_id) {
                    fixture.set_channel_value(&value.channel_type, value.value);
                }
            }
        }
    }

    async fn send_dmx_data(&self) -> Result<(), anyhow::Error> {
        let fixtures = self.fixtures.read().await;
        let mut dmx_data = vec![0; 512];
        
        for fixture in fixtures.iter() {
            let start_channel = (fixture.start_address - 1) as usize;
            let end_channel = (start_channel + fixture.channels.len()).min(dmx_data.len());
            dmx_data[start_channel..end_channel].copy_from_slice(&fixture.get_dmx_values());
        }

        // Send to DMX module
        self.module_manager.send_to_module(
            ModuleId::Dmx, 
            ModuleEvent::DmxOutput(1, dmx_data)
        ).await.map_err(|e| anyhow::anyhow!(e))?;

        Ok(())
    }

    /// Load fixture library
    pub fn load_fixture_library(&mut self) {
        self.fixture_library = FixtureLibrary::new();
    }

    /// Patch a fixture
    pub async fn patch_fixture(
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

        let mut fixtures = self.fixtures.write().await;
        let id = fixtures.len();

        let fixture = Fixture {
            id,
            name: name.to_string(),
            profile_id: profile.id.clone(),
            profile: profile.clone(),
            channels: profile.channel_layout.clone(),
            universe,
            start_address: address,
        };

        fixtures.push(fixture);
        Ok(id)
    }

    /// Set cue lists
    pub async fn set_cue_lists(&self, cue_lists: Vec<CueList>) {
        let mut cue_manager = self.cue_manager.write().await;
        cue_manager.set_cue_lists(cue_lists);
    }

    /// Shutdown the async console
    pub async fn shutdown(&mut self) -> Result<(), anyhow::Error> {
        if !self.is_running {
            return Ok(());
        }

        log::info!("Shutting down async lighting console...");

        // Shutdown module manager
        self.module_manager.shutdown().await?;

        // Cancel message handler
        if let Some(handle) = self.message_handler.take() {
            handle.abort();
        }

        self.is_running = false;
        log::info!("Async lighting console shutdown complete");
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Play audio file through audio module
    pub async fn play_audio(&self, file_path: String) -> Result<(), anyhow::Error> {
        self.module_manager.send_to_module(
            ModuleId::Audio, 
            ModuleEvent::AudioPlay { file_path }
        ).await.map_err(|e| anyhow::anyhow!(e))?;
        Ok(())
    }

    /// Set audio volume
    pub async fn set_audio_volume(&self, volume: f32) -> Result<(), anyhow::Error> {
        self.module_manager.send_to_module(
            ModuleId::Audio, 
            ModuleEvent::AudioSetVolume(volume)
        ).await.map_err(|e| anyhow::anyhow!(e))?;
        Ok(())
    }
}