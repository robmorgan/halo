use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

use halo_fixtures::{Fixture, FixtureLibrary};

use crate::artnet::network_config::NetworkConfig;
use crate::cue::cue_manager::{CueManager, PlaybackState};
use crate::midi::midi::{MidiAction, MidiMessage, MidiOverride};
use crate::modules::{
    AudioModule, DmxModule, MidiModule, ModuleEvent, ModuleId, ModuleManager, ModuleMessage,
    SmpteModule,
};
use crate::programmer::Programmer;
use crate::show::show_manager::ShowManager;
use crate::{ableton_link, CueList, RhythmState, StaticValue};

pub struct LightingConsole {
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

impl LightingConsole {
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
        self.module_manager
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("Module initialization failed: {}", e))?;

        // Start all modules
        self.module_manager
            .start()
            .await
            .map_err(|e| anyhow::anyhow!("Module start failed: {}", e))?;

        // Start message handling
        if let Some(mut message_rx) = self.module_manager.take_message_receiver() {
            let rhythm_state = Arc::clone(&self.rhythm_state);
            let cue_manager = Arc::clone(&self.cue_manager);
            let fixtures = Arc::clone(&self.fixtures);

            let handle = tokio::spawn(async move {
                while let Some(message) = message_rx.recv().await {
                    Self::handle_module_message(message, &rhythm_state, &cue_manager, &fixtures)
                        .await;
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
        rhythm.phrase_phase =
            (beat_time / (rhythm.beats_per_bar * rhythm.bars_per_phrase) as f64).fract();
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
        self.module_manager
            .send_to_module(ModuleId::Dmx, ModuleEvent::DmxOutput(1, dmx_data))
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

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
        self.module_manager
            .shutdown()
            .await
            .map_err(|e| anyhow::anyhow!("Module shutdown failed: {}", e))?;

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

    /// Set the BPM/tempo
    pub fn set_bpm(&mut self, bpm: f64) {
        // set the tempo using ableton's boundary
        self.tempo = bpm.min(999.0).max(20.0);
        self.link_state.set_tempo(self.tempo);
    }

    /// Add a new MIDI override configuration
    pub fn add_midi_override(&mut self, note: u8, override_config: MidiOverride) {
        self.midi_overrides.insert(note, override_config);
        self.active_overrides.insert(note, (false, 0));
    }

    /// Create a new show
    pub async fn new_show(&mut self, name: String) -> Result<(), anyhow::Error> {
        let _ = self.show_manager.write().await.new_show(name);
        Ok(())
    }

    /// Reload the current show
    pub async fn reload_show(&mut self) -> Result<(), anyhow::Error> {
        let current_path = {
            let show_manager = self.show_manager.read().await;
            show_manager.get_current_path()
        };
        if let Some(current_path) = current_path {
            let _ = self.load_show(&current_path).await;
        }
        Ok(())
    }

    /// Save the current show
    pub async fn save_show(&mut self) -> Result<std::path::PathBuf, anyhow::Error> {
        let result = self
            .show_manager
            .write()
            .await
            .save_show(&self.get_show().await.clone())?;
        Ok(result)
    }

    /// Save the show with a new name and path
    pub async fn save_show_as(
        &mut self,
        name: String,
        path: std::path::PathBuf,
    ) -> Result<std::path::PathBuf, anyhow::Error> {
        self.show_name = name;
        let result = self
            .show_manager
            .write()
            .await
            .save_show_as(&self.get_show().await.clone(), path)?;
        Ok(result)
    }

    /// Load a show from a path
    pub async fn load_show(&mut self, path: &std::path::Path) -> Result<(), anyhow::Error> {
        let show = self.show_manager.write().await.load_show(path)?;

        // Clear current fixtures and cue lists before loading
        {
            let mut fixtures = self.fixtures.write().await;
            fixtures.clear();
        }

        // For each fixture in the loaded show
        for mut fixture in show.fixtures {
            // Preserve the original fixture ID
            let fixture_id = fixture.id;

            // Look up the profile by ID in the fixture library
            if let Some(profile) = self.fixture_library.profiles.get(&fixture.profile_id) {
                // Set the profile field with the one from the library
                fixture.profile = profile.clone();
                fixture.channels = profile.channel_layout.clone();

                // Ensure the fixture keeps its original ID to maintain cue references
                fixture.id = fixture_id;
                let mut fixtures = self.fixtures.write().await;
                fixtures.push(fixture);
            } else {
                return Err(anyhow::anyhow!(
                    "Fixture profile '{}' not found in library",
                    fixture.profile_id
                ));
            }
        }

        // After all fixtures are loaded with their original IDs, set the cue lists
        self.set_cue_lists(show.cue_lists).await;
        self.show_name = show.name;

        Ok(())
    }

    /// Get the current show
    pub async fn get_show(&self) -> crate::show::show::Show {
        let fixtures = self.fixtures.read().await;
        let cue_lists = self.cue_manager.read().await.get_cue_lists().clone();
        let mut show = crate::show::show::Show::new(self.show_name.clone());
        show.fixtures = fixtures.clone();
        show.cue_lists = cue_lists;
        show.modified_at = std::time::SystemTime::now();
        show
    }

    /// Play audio file through audio module
    pub async fn play_audio(&self, file_path: String) -> Result<(), anyhow::Error> {
        self.module_manager
            .send_to_module(ModuleId::Audio, ModuleEvent::AudioPlay { file_path })
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(())
    }

    /// Set audio volume
    pub async fn set_audio_volume(&self, volume: f32) -> Result<(), anyhow::Error> {
        self.module_manager
            .send_to_module(ModuleId::Audio, ModuleEvent::AudioSetVolume(volume))
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(())
    }
}

/// Synchronous wrapper around the async LightingConsole for UI compatibility
pub struct SyncLightingConsole {
    inner: Arc<Mutex<LightingConsole>>,
    runtime: tokio::runtime::Runtime,
}

impl SyncLightingConsole {
    pub fn new(bpm: f64, network_config: NetworkConfig) -> Result<Self, anyhow::Error> {
        let runtime = tokio::runtime::Runtime::new()?;
        let inner = runtime.block_on(async {
            let mut console = LightingConsole::new(bpm, network_config)?;
            console.initialize().await?;
            Ok::<_, anyhow::Error>(Arc::new(Mutex::new(console)))
        })?;

        Ok(Self { inner, runtime })
    }

    pub fn load_fixture_library(&mut self) {
        // This is a no-op in the async version as fixture library is loaded on demand
    }

    pub fn patch_fixture(
        &mut self,
        name: &str,
        profile_name: &str,
        universe: u8,
        address: u16,
    ) -> Result<usize, String> {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().unwrap();
            console
                .patch_fixture(name, profile_name, universe, address)
                .await
        })
    }

    pub fn set_cue_lists(&mut self, cue_lists: Vec<CueList>) {
        self.runtime.block_on(async {
            let console = self.inner.lock().unwrap();
            console.set_cue_lists(cue_lists).await;
        });
    }

    pub fn set_bpm(&mut self, bpm: f64) {
        let mut console = self.inner.lock().unwrap();
        console.set_bpm(bpm);
    }

    pub fn add_midi_override(&mut self, note: u8, override_config: MidiOverride) {
        let mut console = self.inner.lock().unwrap();
        console.add_midi_override(note, override_config);
    }

    pub fn new_show(&mut self, name: String) -> Result<(), anyhow::Error> {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().unwrap();
            console.new_show(name).await
        })
    }

    pub fn reload_show(&mut self) -> Result<(), anyhow::Error> {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().unwrap();
            console.reload_show().await
        })
    }

    pub fn save_show(&mut self) -> Result<std::path::PathBuf, anyhow::Error> {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().unwrap();
            console.save_show().await
        })
    }

    pub fn save_show_as(
        &mut self,
        name: String,
        path: std::path::PathBuf,
    ) -> Result<std::path::PathBuf, anyhow::Error> {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().unwrap();
            console.save_show_as(name, path).await
        })
    }

    pub fn load_show(&mut self, path: &std::path::Path) -> Result<(), anyhow::Error> {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().unwrap();
            console.load_show(path).await
        })
    }

    pub fn get_show(&self) -> crate::show::show::Show {
        self.runtime.block_on(async {
            let console = self.inner.lock().unwrap();
            console.get_show().await
        })
    }

    pub fn update(&mut self) {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().unwrap();
            if let Err(e) = console.update().await {
                log::error!("Error updating console: {}", e);
            }
        });
    }

    pub fn render(&self) {
        // Rendering is handled internally by the async console
    }

    // Getters for UI access
    pub fn fixtures(&self) -> Vec<Fixture> {
        self.runtime.block_on(async {
            let console = self.inner.lock().unwrap();
            let fixtures = console.fixtures.read().await;
            fixtures.clone()
        })
    }

    pub fn cue_manager(&self) -> CueManager {
        self.runtime.block_on(async {
            let console = self.inner.lock().unwrap();
            let cue_manager = console.cue_manager.read().await;
            cue_manager.clone()
        })
    }

    pub fn get_link_state(&self) -> ableton_link::State {
        let console = self.inner.lock().unwrap();
        // Since ableton_link::State doesn't implement Clone, we'll need to handle this differently
        // For now, return a default state - this is a temporary fix
        ableton_link::State::new(120.0)
    }

    pub fn show_manager(&self) -> ShowManager {
        self.runtime.block_on(async {
            let console = self.inner.lock().unwrap();
            let show_manager = console.show_manager.read().await;
            show_manager.clone()
        })
    }

    pub fn programmer(&self) -> Programmer {
        self.runtime.block_on(async {
            let console = self.inner.lock().unwrap();
            let programmer = console.programmer.read().await;
            programmer.clone()
        })
    }

    pub fn record_cue(&mut self, name: String, fade_time: f64) -> Result<(), anyhow::Error> {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().unwrap();
            let programmer = console.programmer.read().await;
            let values = programmer.get_values().clone();
            drop(programmer);

            let mut cue_manager = console.cue_manager.write().await;
            // For now, add to the first cue list (index 0)
            // TODO: Allow specifying which cue list to add to
            if cue_manager.get_cue_lists().is_empty() {
                cue_manager.add_cue_list(crate::CueList {
                    name: "Main".to_string(),
                    cues: vec![],
                    audio_file: None,
                });
            }

            let cue = crate::Cue {
                id: 0, // Will be assigned by the cue manager
                name,
                fade_time: std::time::Duration::from_secs_f64(fade_time),
                static_values: values,
                effects: vec![],
                timecode: None,
                is_blocking: false,
            };

            cue_manager
                .add_cue(0, cue)
                .map(|_| ())
                .map_err(|e| anyhow::anyhow!("Failed to add cue: {}", e))
        })
    }

    pub fn set_programmer_preview_mode(&mut self, preview_mode: bool) {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().unwrap();
            let mut programmer = console.programmer.write().await;
            programmer.set_preview_mode(preview_mode);
        });
    }

    pub fn clear_programmer(&mut self) {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().unwrap();
            let mut programmer = console.programmer.write().await;
            programmer.clear();
        });
    }

    pub fn add_programmer_effect(&mut self, effect: crate::EffectMapping) {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().unwrap();
            let mut programmer = console.programmer.write().await;
            programmer.add_effect(effect);
        });
    }

    pub fn get_programmer_values(&self) -> Vec<crate::StaticValue> {
        self.runtime.block_on(async {
            let console = self.inner.lock().unwrap();
            let programmer = console.programmer.read().await;
            programmer.get_values().clone()
        })
    }

    pub fn get_programmer_effects(&self) -> Vec<crate::EffectMapping> {
        self.runtime.block_on(async {
            let console = self.inner.lock().unwrap();
            let programmer = console.programmer.read().await;
            programmer.get_effects().clone()
        })
    }

    pub fn is_running(&self) -> bool {
        let console = self.inner.lock().unwrap();
        console.is_running()
    }

    /// Apply master fader to fixtures
    pub fn apply_master_fader(&mut self, master_value: f32) {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().unwrap();
            let mut fixtures = console.fixtures.write().await;

            for fixture in fixtures.iter_mut() {
                for channel in &mut fixture.channels {
                    if let halo_fixtures::ChannelType::Dimmer = channel.channel_type {
                        // Scale the channel value by the master value
                        // Note: We apply the square of the fader value for a more natural feel
                        let scaled_value = (channel.value as f32 * master_value.powi(2)) as u8;
                        channel.value = scaled_value;
                    }
                }
            }
        });
    }

    /// Apply smoke fader to fixtures
    pub fn apply_smoke_fader(&mut self, smoke_value: f32) {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().unwrap();
            let mut fixtures = console.fixtures.write().await;

            for fixture in fixtures.iter_mut() {
                if fixture.name.to_lowercase().contains("smoke") {
                    for channel in &mut fixture.channels {
                        if let halo_fixtures::ChannelType::Other(ref name) = channel.channel_type {
                            if name == "Smoke" {
                                let scaled_value =
                                    (channel.value as f32 * smoke_value.powi(2)) as u8;
                                channel.value = scaled_value;
                            }
                        }
                    }
                }
            }
        });
    }
}
