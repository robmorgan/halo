use std::collections::HashMap;
use std::sync::Arc;

use halo_fixtures::{Fixture, FixtureLibrary};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::artnet::network_config::NetworkConfig;
use crate::audio::device_enumerator;
use crate::cue::cue_manager::{CueManager, PlaybackState};
use crate::messages::{ConsoleCommand, ConsoleEvent, Settings};
use crate::midi::midi::{MidiMessage, MidiOverride};
use crate::modules::{
    AudioModule, DmxModule, MidiModule, ModuleEvent, ModuleId, ModuleManager, ModuleMessage,
    SmpteModule,
};
use crate::pixel::PixelEngine;
use crate::programmer::Programmer;
use crate::rhythm::rhythm::RhythmState;
use crate::show::show_manager::ShowManager;
use crate::tracking_state::TrackingState;
use crate::{AbletonLinkManager, CueList};

pub struct LightingConsole {
    // Core components
    show_name: String,
    tempo: f64,
    fixture_library: FixtureLibrary,
    pub fixtures: Arc<RwLock<Vec<Fixture>>>,
    pub cue_manager: Arc<RwLock<CueManager>>,
    pub programmer: Arc<RwLock<Programmer>>,
    pub show_manager: Arc<RwLock<ShowManager>>,

    // Async module system
    module_manager: ModuleManager,
    message_handler: Option<JoinHandle<()>>,
    message_rx: Option<mpsc::Receiver<ModuleMessage>>,

    // MIDI overrides
    midi_overrides: HashMap<u8, MidiOverride>,
    active_overrides: HashMap<u8, (bool, u8)>,

    // Rhythm state
    rhythm_state: Arc<RwLock<RhythmState>>,

    // Ableton Link integration
    link_manager: Arc<Mutex<AbletonLinkManager>>,

    // Settings
    settings: Arc<RwLock<Settings>>,

    // Pixel engine
    pixel_engine: Arc<RwLock<PixelEngine>>,

    // Tracking state for tracking console behavior
    tracking_state: Arc<RwLock<TrackingState>>,

    // System state
    is_running: bool,
}

impl LightingConsole {
    pub fn new(bpm: f64, network_config: NetworkConfig) -> Result<Self, anyhow::Error> {
        Self::new_with_settings(bpm, network_config, Settings::default())
    }

    pub fn new_with_settings(
        bpm: f64,
        network_config: NetworkConfig,
        settings: Settings,
    ) -> Result<Self, anyhow::Error> {
        let mut module_manager = ModuleManager::new();

        // Register async modules
        module_manager.register_module(Box::new(DmxModule::new(network_config)));
        module_manager.register_module(Box::new(AudioModule::new()));
        module_manager.register_module(Box::new(SmpteModule::new(30))); // 30fps default

        // Only register MIDI module if enabled and device is not "None"
        if settings.midi_enabled && settings.midi_device != "None" {
            module_manager.register_module(Box::new(MidiModule::new(settings.midi_device.clone())));
        }

        let show_manager = ShowManager::new()?;

        Ok(Self {
            show_name: "Untitled Show".to_string(),
            tempo: bpm,
            fixture_library: FixtureLibrary::new(),
            fixtures: Arc::new(RwLock::new(Vec::new())),
            cue_manager: Arc::new(RwLock::new(CueManager::new(Vec::new()))),
            programmer: Arc::new(RwLock::new(Programmer::new())),
            show_manager: Arc::new(RwLock::new(show_manager)),
            module_manager,
            message_handler: None,
            message_rx: None,
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
            link_manager: Arc::new(Mutex::new(AbletonLinkManager::new())),
            settings: Arc::new(RwLock::new(settings)),
            pixel_engine: Arc::new(RwLock::new(PixelEngine::new())),
            tracking_state: Arc::new(RwLock::new(TrackingState::new())),
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

        // Store message receiver for main loop processing
        if let Some(message_rx) = self.module_manager.take_message_receiver() {
            self.message_rx = Some(message_rx);
        }

        self.is_running = true;
        log::info!("Async lighting console initialized successfully");
        Ok(())
    }

    async fn handle_midi_input(
        midi_msg: MidiMessage,
        _rhythm_state: &Arc<RwLock<RhythmState>>,
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
        {
            let mut link_manager = self.link_manager.lock().await;
            if let Some((tempo, beat_time)) = link_manager.update().await {
                self.tempo = tempo;
                self.update_rhythm_state(beat_time).await;
            }
        }

        // Process current cue if playing - update tracking state
        {
            let cue_manager = self.cue_manager.read().await;
            if cue_manager.get_playback_state() == PlaybackState::Playing {
                if let Some(current_cue) = cue_manager.get_current_cue() {
                    // Update tracking state with current cue
                    self.update_tracking_state(current_cue.clone()).await;
                }
            }
        }

        // Apply accumulated tracking state to fixtures
        self.apply_tracking_state().await;

        // Apply programmer values (highest priority)
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

    /// Update tracking state with current cue
    async fn update_tracking_state(&self, cue: crate::cue::cue::Cue) {
        let mut tracking_state = self.tracking_state.write().await;

        if cue.is_blocking {
            // Blocking cue: clear state and apply this cue
            tracking_state.apply_blocking_cue(&cue);
        } else {
            // Non-blocking cue: merge into tracking state
            tracking_state.apply_cue(&cue);
        }
    }

    /// Apply accumulated tracking state to fixtures
    async fn apply_tracking_state(&self) {
        let tracking_state = self.tracking_state.read().await;
        let mut fixtures = self.fixtures.write().await;

        // Apply static values from tracking state
        for value in tracking_state.get_static_values() {
            if let Some(fixture) = fixtures.iter_mut().find(|f| f.id == value.fixture_id) {
                fixture.set_channel_value(&value.channel_type, value.value);
            }
        }

        // Release fixtures lock before processing effects
        drop(fixtures);

        // Apply effects from tracking state
        self.apply_effects().await;

        // Apply pixel effects from tracking state
        let pixel_effects = tracking_state.get_pixel_effects();
        if !pixel_effects.is_empty() {
            let mut pixel_engine = self.pixel_engine.write().await;
            let pixel_effect_data: Vec<_> = pixel_effects
                .iter()
                .map(|pm| {
                    (
                        pm.name.clone(),
                        pm.fixture_ids.clone(),
                        pm.effect.clone(),
                        pm.distribution.clone(),
                    )
                })
                .collect();
            pixel_engine.set_effects(pixel_effect_data);
        }
    }

    /// Apply effects from tracking state to fixtures
    async fn apply_effects(&self) {
        let tracking_state = self.tracking_state.read().await;
        let effects = tracking_state.get_effects();
        let rhythm_state = self.rhythm_state.read().await;
        let mut fixtures = self.fixtures.write().await;

        for effect_mapping in effects {
            // Calculate effect phase based on rhythm state
            let phase = crate::effect::effect::get_effect_phase(
                &rhythm_state,
                &effect_mapping.effect.params,
            );

            // Apply the effect to get normalized value (0.0 to 1.0)
            let normalized_value = effect_mapping.effect.apply(phase);

            // Scale to min/max range
            let min = effect_mapping.effect.min as f64;
            let max = effect_mapping.effect.max as f64;
            let scaled_value = (min + (max - min) * normalized_value) as u8;

            // Apply effect to fixtures based on distribution
            match &effect_mapping.distribution {
                crate::EffectDistribution::All => {
                    // Apply same value to all fixtures
                    for fixture_id in &effect_mapping.fixture_ids {
                        if let Some(fixture) = fixtures.iter_mut().find(|f| f.id == *fixture_id) {
                            for channel_type in &effect_mapping.channel_types {
                                fixture.set_channel_value(channel_type, scaled_value);
                            }
                        }
                    }
                }
                crate::EffectDistribution::Step(step_size) => {
                    // Apply effect with step distribution
                    for (idx, fixture_id) in effect_mapping.fixture_ids.iter().enumerate() {
                        let step_phase = (phase + (idx / step_size) as f64) % 1.0;
                        let step_normalized = effect_mapping.effect.apply(step_phase);
                        let step_value = (min + (max - min) * step_normalized) as u8;

                        if let Some(fixture) = fixtures.iter_mut().find(|f| f.id == *fixture_id) {
                            for channel_type in &effect_mapping.channel_types {
                                fixture.set_channel_value(channel_type, step_value);
                            }
                        }
                    }
                }
                crate::EffectDistribution::Wave(phase_offset) => {
                    // Apply effect with wave distribution (phase offset per fixture)
                    for (idx, fixture_id) in effect_mapping.fixture_ids.iter().enumerate() {
                        let wave_phase = (phase + idx as f64 * phase_offset) % 1.0;
                        let wave_normalized = effect_mapping.effect.apply(wave_phase);
                        let wave_value = (min + (max - min) * wave_normalized) as u8;

                        if let Some(fixture) = fixtures.iter_mut().find(|f| f.id == *fixture_id) {
                            for channel_type in &effect_mapping.channel_types {
                                fixture.set_channel_value(channel_type, wave_value);
                            }
                        }
                    }
                }
            }
        }
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

        // Render regular fixtures (non-pixel bars)
        for fixture in fixtures.iter() {
            if fixture.profile.fixture_type != halo_fixtures::FixtureType::PixelBar {
                let start_channel = (fixture.start_address - 1) as usize;
                let end_channel = (start_channel + fixture.channels.len()).min(dmx_data.len());
                dmx_data[start_channel..end_channel].copy_from_slice(&fixture.get_dmx_values());
            }
        }

        // Send regular fixtures to DMX module (universe 1)
        self.module_manager
            .send_to_module(ModuleId::Dmx, ModuleEvent::DmxOutput(1, dmx_data))
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        // Render and send pixel fixtures
        let pixel_engine = self.pixel_engine.read().await;
        let rhythm_state = self.rhythm_state.read().await;
        let pixel_universe_data = pixel_engine.render(&fixtures, &rhythm_state);

        for (universe, data) in pixel_universe_data {
            self.module_manager
                .send_to_module(ModuleId::Dmx, ModuleEvent::DmxOutput(universe, data))
                .await
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        Ok(())
    }

    /// Load fixture library
    pub fn load_fixture_library(&mut self) {
        self.fixture_library = FixtureLibrary::new();
    }

    /// Convert a channel name string to a ChannelType
    fn channel_string_to_type(channel: &str) -> halo_fixtures::ChannelType {
        use halo_fixtures::ChannelType;

        match channel.to_lowercase().as_str() {
            "dimmer" => ChannelType::Dimmer,
            "color" => ChannelType::Color,
            "gobo" => ChannelType::Gobo,
            "red" => ChannelType::Red,
            "green" => ChannelType::Green,
            "blue" => ChannelType::Blue,
            "white" => ChannelType::White,
            "amber" => ChannelType::Amber,
            "uv" => ChannelType::UV,
            "strobe" => ChannelType::Strobe,
            "pan" => ChannelType::Pan,
            "tilt" => ChannelType::Tilt,
            "tiltspeed" | "tilt_speed" => ChannelType::TiltSpeed,
            "beam" => ChannelType::Beam,
            "focus" => ChannelType::Focus,
            "zoom" => ChannelType::Zoom,
            "function" => ChannelType::Function,
            "functionspeed" | "function_speed" => ChannelType::FunctionSpeed,
            "gobo_rotation" | "gobo_rot" => ChannelType::Other("gobo_rotation".to_string()),
            "gobo_selection" | "gobo_sel" => ChannelType::Other("gobo_selection".to_string()),
            _ => ChannelType::Other(channel.to_string()),
        }
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
            pan_tilt_limits: None,
        };

        fixtures.push(fixture);
        Ok(id)
    }

    /// Update an existing fixture
    pub async fn update_fixture(
        &mut self,
        fixture_id: usize,
        name: String,
        universe: u8,
        address: u16,
    ) -> Result<Fixture, String> {
        let mut fixtures = self.fixtures.write().await;
        let fixture = fixtures
            .get_mut(fixture_id)
            .ok_or_else(|| format!("Fixture {} not found", fixture_id))?;

        fixture.name = name;
        fixture.universe = universe;
        fixture.start_address = address;

        Ok(fixture.clone())
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

    /// Enable Ableton Link
    pub async fn enable_ableton_link(&mut self) -> Result<(), anyhow::Error> {
        {
            let mut link_manager = self.link_manager.lock().await;
            link_manager
                .enable()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to enable Ableton Link: {}", e))?;

            // Enable start/stop sync
            link_manager
                .enable_start_stop_sync(true)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to enable start/stop sync: {}", e))?;
        }

        log::info!("Ableton Link enabled and synchronized");
        Ok(())
    }

    /// Disable Ableton Link
    pub async fn disable_ableton_link(&mut self) {
        let mut link_manager = self.link_manager.lock().await;
        link_manager.disable();
        log::info!("Ableton Link disabled");
    }

    /// Check if Ableton Link is enabled
    pub async fn is_ableton_link_enabled(&self) -> bool {
        let link_manager = self.link_manager.lock().await;
        link_manager.is_enabled()
    }

    /// Get the number of Ableton Link peers
    pub async fn get_ableton_link_peers(&self) -> u64 {
        let link_manager = self.link_manager.lock().await;
        link_manager.num_peers()
    }

    /// Set the BPM/tempo
    pub async fn set_bpm(&mut self, bpm: f64) -> Result<(), anyhow::Error> {
        // Set the tempo using ableton's boundary
        let bounded_bpm = bpm.min(999.0).max(20.0);
        self.tempo = bounded_bpm;

        // Update Ableton Link tempo if enabled
        {
            let link_manager = self.link_manager.lock().await;
            if link_manager.is_enabled() {
                drop(link_manager); // Release lock before async call
                let mut link_manager = self.link_manager.lock().await;
                if let Err(e) = link_manager.set_tempo(bounded_bpm).await {
                    log::warn!("Failed to set Ableton Link tempo: {}", e);
                }
            }
        }

        Ok(())
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
        // Validate that the file exists
        if !path.exists() {
            return Err(anyhow::anyhow!("Show file not found: {}", path.display()));
        }

        // Load the show from the file
        let show = self
            .show_manager
            .write()
            .await
            .load_show(path)
            .map_err(|e| anyhow::anyhow!("Failed to load show file '{}': {}", path.display(), e))?;

        log::info!(
            "Loaded show '{}' with {} fixtures and {} cue lists",
            show.name,
            show.fixtures.len(),
            show.cue_lists.len()
        );

        // Clear current fixtures and cue lists before loading
        {
            let mut fixtures = self.fixtures.write().await;
            fixtures.clear();
        }

        // Track missing profiles for better error reporting
        let mut missing_profiles = Vec::new();

        // For each fixture in the loaded show
        for mut fixture in show.fixtures {
            // Preserve the original fixture ID
            let fixture_id = fixture.id;
            let fixture_name = fixture.name.clone();
            let profile_id = fixture.profile_id.clone();

            // Look up the profile by ID in the fixture library
            if let Some(profile) = self.fixture_library.profiles.get(&profile_id) {
                // Set the profile field with the one from the library
                fixture.profile = profile.clone();
                fixture.channels = profile.channel_layout.clone();

                // Ensure the fixture keeps its original ID to maintain cue references
                fixture.id = fixture_id;
                let mut fixtures = self.fixtures.write().await;
                fixtures.push(fixture);
                log::debug!(
                    "Loaded fixture '{}' with profile '{}'",
                    fixture_name,
                    profile_id
                );
            } else {
                missing_profiles.push(format!(
                    "  - Fixture '{}' (ID: {}) requires profile '{}'",
                    fixture_name, fixture_id, profile_id
                ));
            }
        }

        // If any profiles are missing, return a detailed error
        if !missing_profiles.is_empty() {
            return Err(anyhow::anyhow!(
                "Failed to load show '{}': {} fixture profile(s) not found in library:\n{}",
                path.display(),
                missing_profiles.len(),
                missing_profiles.join("\n")
            ));
        }

        // After all fixtures are loaded with their original IDs, set the cue lists
        self.set_cue_lists(show.cue_lists).await;
        self.show_name = show.name.clone();

        log::info!("Successfully loaded show '{}'", show.name);

        // Settings are now loaded separately from config file, not from show

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

    /// Process a command from the UI
    pub async fn process_command(
        &mut self,
        command: ConsoleCommand,
        event_tx: &mpsc::UnboundedSender<ConsoleEvent>,
    ) -> Result<(), anyhow::Error> {
        use ConsoleCommand::*;

        log::debug!("Processing command: {:?}", command);

        match command {
            Initialize => {
                log::info!("Processing Initialize command");
                self.initialize().await?;
                let _ = event_tx.send(ConsoleEvent::Initialized);
            }
            Shutdown => {
                log::info!("Processing Shutdown command");
                self.shutdown().await?;
                let _ = event_tx.send(ConsoleEvent::ShutdownComplete);
            }
            Update => {
                self.update().await?;
            }

            // Show management
            NewShow { name } => {
                self.new_show(name.clone()).await?;
                let _ = event_tx.send(ConsoleEvent::ShowCreated { name });
            }
            LoadShow { path } => {
                log::info!("Processing LoadShow command for path: {:?}", path);
                match self.load_show(&path).await {
                    Ok(_) => {
                        let show = self.get_show().await;
                        let settings = self.settings.read().await.clone();
                        let _ = event_tx.send(ConsoleEvent::ShowLoaded { show });
                        let _ = event_tx.send(ConsoleEvent::CurrentSettings { settings });
                        log::info!("LoadShow command completed successfully");
                    }
                    Err(e) => {
                        let error_message = format!("Failed to load show: {}", e);
                        log::error!("{}", error_message);
                        let _ = event_tx.send(ConsoleEvent::Error {
                            message: error_message,
                        });
                    }
                }
            }
            SaveShow => {
                let path = self.save_show().await?;
                let _ = event_tx.send(ConsoleEvent::ShowSaved { path });
            }
            SaveShowAs { name, path } => {
                let saved_path = self.save_show_as(name, path).await?;
                let _ = event_tx.send(ConsoleEvent::ShowSaved { path: saved_path });
            }
            ReloadShow => match self.reload_show().await {
                Ok(_) => {
                    let show = self.get_show().await;
                    let settings = self.settings.read().await.clone();
                    let _ = event_tx.send(ConsoleEvent::ShowLoaded { show });
                    let _ = event_tx.send(ConsoleEvent::CurrentSettings { settings });
                    log::info!("ReloadShow command completed successfully");
                }
                Err(e) => {
                    let error_message = format!("Failed to reload show: {}", e);
                    log::error!("{}", error_message);
                    let _ = event_tx.send(ConsoleEvent::Error {
                        message: error_message,
                    });
                }
            },

            // Fixture management
            PatchFixture {
                name,
                profile_name,
                universe,
                address,
            } => {
                let fixture_id = self
                    .patch_fixture(&name, &profile_name, universe, address)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
                let fixtures = self.fixtures.read().await;
                if let Some(fixture) = fixtures.get(fixture_id) {
                    let _ = event_tx.send(ConsoleEvent::FixturePatched {
                        fixture_id,
                        fixture: fixture.clone(),
                    });
                }
            }
            UnpatchFixture { fixture_id } => {
                // TODO: Implement unpatch_fixture method
                let _ = event_tx.send(ConsoleEvent::FixtureUnpatched { fixture_id });
            }
            UpdateFixture {
                fixture_id,
                name,
                universe,
                address,
            } => {
                let fixture = self
                    .update_fixture(fixture_id, name, universe, address)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
                let _ = event_tx.send(ConsoleEvent::FixtureUpdated {
                    fixture_id,
                    fixture,
                });
            }
            UpdateFixtureChannels {
                fixture_id,
                channel_values,
            } => {
                // TODO: Implement fixture channel update
                let _ = event_tx.send(ConsoleEvent::FixtureValuesChanged {
                    fixture_id,
                    values: channel_values,
                });
            }
            SetPanTiltLimits {
                fixture_id,
                pan_min,
                pan_max,
                tilt_min,
                tilt_max,
            } => {
                let mut fixtures = self.fixtures.write().await;
                if let Some(fixture) = fixtures.iter_mut().find(|f| f.id == fixture_id) {
                    fixture.set_pan_tilt_limits(halo_fixtures::PanTiltLimits {
                        pan_min,
                        pan_max,
                        tilt_min,
                        tilt_max,
                    });
                    log::info!("Set pan/tilt limits for fixture {fixture_id}: pan({pan_min}-{pan_max}), tilt({tilt_min}-{tilt_max})");
                }
            }
            ClearPanTiltLimits { fixture_id } => {
                let mut fixtures = self.fixtures.write().await;
                if let Some(fixture) = fixtures.iter_mut().find(|f| f.id == fixture_id) {
                    fixture.clear_pan_tilt_limits();
                    log::info!("Cleared pan/tilt limits for fixture {fixture_id}");
                }
            }

            // Cue management
            SetCueLists { cue_lists } => {
                self.set_cue_lists(cue_lists.clone()).await;
                let _ = event_tx.send(ConsoleEvent::CueListsUpdated { cue_lists });
            }
            PlayCue {
                list_index,
                cue_index,
            } => {
                let _ = self
                    .cue_manager
                    .write()
                    .await
                    .go_to_cue(list_index, cue_index);
                let _ = event_tx.send(ConsoleEvent::CueStarted {
                    list_index,
                    cue_index,
                });
            }
            StopCue { list_index } => {
                let _ = self.cue_manager.write().await.stop();
                let _ = event_tx.send(ConsoleEvent::CueStopped { list_index });
            }
            PauseCue { list_index: _ } => {
                let _ = self.cue_manager.write().await.hold();
                let state = self.cue_manager.read().await.get_playback_state();
                let _ = event_tx.send(ConsoleEvent::PlaybackStateChanged { state });
            }
            ResumeCue { list_index: _ } => {
                let _ = self.cue_manager.write().await.go();
                let state = self.cue_manager.read().await.get_playback_state();
                let _ = event_tx.send(ConsoleEvent::PlaybackStateChanged { state });
            }
            GoToCue {
                list_index,
                cue_index,
            } => {
                let _ = self
                    .cue_manager
                    .write()
                    .await
                    .go_to_cue(list_index, cue_index);
                let _ = event_tx.send(ConsoleEvent::CueStarted {
                    list_index,
                    cue_index,
                });
                // Send current cue update
                let cue_manager = self.cue_manager.read().await;
                let current_cue_index = cue_manager.get_current_cue_idx().unwrap_or(0);
                let progress = cue_manager.get_current_cue_progress();
                let _ = event_tx.send(ConsoleEvent::CurrentCueChanged {
                    cue_index: current_cue_index,
                    progress,
                });
            }
            NextCue { list_index: _ } => {
                let _ = self.cue_manager.write().await.go_to_next_cue();
                // Send current cue update
                let cue_manager = self.cue_manager.read().await;
                let cue_index = cue_manager.get_current_cue_idx().unwrap_or(0);
                let progress = cue_manager.get_current_cue_progress();
                let _ = event_tx.send(ConsoleEvent::CurrentCueChanged {
                    cue_index,
                    progress,
                });
            }
            PrevCue { list_index: _ } => {
                let _ = self.cue_manager.write().await.go_to_previous_cue();
                // Send current cue update
                let cue_manager = self.cue_manager.read().await;
                let cue_index = cue_manager.get_current_cue_idx().unwrap_or(0);
                let progress = cue_manager.get_current_cue_progress();
                let _ = event_tx.send(ConsoleEvent::CurrentCueChanged {
                    cue_index,
                    progress,
                });
            }
            SelectNextCueList => {
                let mut cue_manager = self.cue_manager.write().await;
                if let Err(err) = cue_manager.select_next_cue_list() {
                    log::warn!("Error selecting next cue list: {}", err);
                } else {
                    let current_index = cue_manager.get_current_cue_list_idx();
                    let _ = event_tx.send(ConsoleEvent::CueListSelected {
                        list_index: current_index,
                    });
                }
            }
            SelectPreviousCueList => {
                let mut cue_manager = self.cue_manager.write().await;
                if let Err(err) = cue_manager.select_previous_cue_list() {
                    log::warn!("Error selecting previous cue list: {}", err);
                } else {
                    let current_index = cue_manager.get_current_cue_list_idx();
                    let _ = event_tx.send(ConsoleEvent::CueListSelected {
                        list_index: current_index,
                    });
                }
            }

            // Playback control
            Play => {
                println!("Console received Play command");
                log::info!("Console received Play command");
                let _ = self.cue_manager.write().await.go();
                let state = self.cue_manager.read().await.get_playback_state();
                let _ = event_tx.send(ConsoleEvent::PlaybackStateChanged { state });

                // Check if current cuelist has an audio file and play it
                let cue_manager = self.cue_manager.read().await;
                if let Some(current_cue_list) = cue_manager.get_current_cue_list() {
                    println!("Current cuelist: {}", current_cue_list.name);
                    log::info!("Current cuelist: {}", current_cue_list.name);
                    if let Some(audio_file) = &current_cue_list.audio_file {
                        println!("Found audio file for cuelist: {}", audio_file);
                        log::info!("Found audio file for cuelist: {}", audio_file);
                        if let Err(e) = self.play_audio(audio_file.clone()).await {
                            println!("ERROR: Failed to play audio file {}: {}", audio_file, e);
                            log::error!("Failed to play audio file {}: {}", audio_file, e);
                        } else {
                            println!("Successfully sent audio play command for: {}", audio_file);
                            log::info!("Successfully sent audio play command for: {}", audio_file);
                        }
                    } else {
                        println!(
                            "No audio file found for current cuelist: {}",
                            current_cue_list.name
                        );
                        log::info!(
                            "No audio file found for current cuelist: {}",
                            current_cue_list.name
                        );
                    }
                } else {
                    println!("No current cuelist found");
                    log::warn!("No current cuelist found");
                }
            }
            Stop => {
                let _ = self.cue_manager.write().await.stop();
                let state = self.cue_manager.read().await.get_playback_state();
                let _ = event_tx.send(ConsoleEvent::PlaybackStateChanged { state });

                // Clear tracking state when stopping
                self.tracking_state.write().await.clear();

                // Stop audio playback when stopping the cuelist
                if let Err(e) = self
                    .module_manager
                    .send_to_module(ModuleId::Audio, ModuleEvent::AudioStop)
                    .await
                {
                    log::error!("Failed to stop audio: {}", e);
                }
            }
            Pause => {
                let _ = self.cue_manager.write().await.hold();
                let state = self.cue_manager.read().await.get_playback_state();
                let _ = event_tx.send(ConsoleEvent::PlaybackStateChanged { state });

                // Pause audio playback when pausing the cuelist
                if let Err(e) = self
                    .module_manager
                    .send_to_module(ModuleId::Audio, ModuleEvent::AudioPause)
                    .await
                {
                    log::error!("Failed to pause audio: {}", e);
                }
            }
            Resume => {
                let _ = self.cue_manager.write().await.go();
                let state = self.cue_manager.read().await.get_playback_state();
                let _ = event_tx.send(ConsoleEvent::PlaybackStateChanged { state });

                // Resume audio playback when resuming the cuelist
                if let Err(e) = self
                    .module_manager
                    .send_to_module(ModuleId::Audio, ModuleEvent::AudioResume)
                    .await
                {
                    log::error!("Failed to resume audio: {}", e);
                }
            }
            SetPlaybackRate { rate: _ } => {
                // TODO: Implement playback rate control
            }

            // Tempo and timing
            SetBpm { bpm } => {
                if let Err(e) = self.set_bpm(bpm).await {
                    log::error!("Failed to set BPM: {}", e);
                }
                let _ = event_tx.send(ConsoleEvent::BpmChanged { bpm: self.tempo });
            }
            TapTempo => {
                // TODO: Implement tap tempo
                let bpm = self.tempo;
                let _ = event_tx.send(ConsoleEvent::BpmChanged { bpm });
            }
            SetTimecode { timecode } => {
                self.cue_manager.write().await.current_timecode = Some(timecode);
                let _ = event_tx.send(ConsoleEvent::TimecodeUpdated { timecode });
            }

            // MIDI
            AddMidiOverride {
                note,
                override_config,
            } => {
                self.add_midi_override(note, override_config);
                let _ = event_tx.send(ConsoleEvent::MidiOverrideAdded { note });
            }
            RemoveMidiOverride { note } => {
                self.midi_overrides.remove(&note);
                let _ = event_tx.send(ConsoleEvent::MidiOverrideRemoved { note });
            }
            ProcessMidiMessage { message } => {
                // TODO: Process MIDI message
                let _ = event_tx.send(ConsoleEvent::MidiMessageReceived { message });
            }

            // Audio
            PlayAudio { file_path } => {
                self.play_audio(file_path.clone()).await?;
                let _ = event_tx.send(ConsoleEvent::AudioStarted { file_path });
            }
            StopAudio => {
                // TODO: Implement stop_audio method
                let _ = event_tx.send(ConsoleEvent::AudioStopped);
            }
            SetAudioVolume { volume } => {
                self.set_audio_volume(volume).await?;
                let _ = event_tx.send(ConsoleEvent::AudioVolumeChanged { volume });
            }

            // Effects
            ApplyEffect {
                fixture_ids: _,
                channel_type: _,
                effect_type: _,
                frequency: _,
                amplitude: _,
                offset: _,
            } => {
                // TODO: Implement apply_effect method
            }
            ClearEffect {
                fixture_ids: _,
                channel_type: _,
            } => {
                // TODO: Implement clear_effect method
            }

            // Programmer
            SetProgrammerValue {
                fixture_id,
                channel,
                value,
            } => {
                // Convert channel string to ChannelType
                let channel_type = Self::channel_string_to_type(&channel);
                self.programmer
                    .write()
                    .await
                    .add_value(fixture_id, channel_type, value);
            }
            SetProgrammerPreviewMode { preview_mode } => {
                self.programmer.write().await.set_preview_mode(preview_mode);
            }
            SetSelectedFixtures { fixture_ids } => {
                self.programmer
                    .write()
                    .await
                    .set_selected_fixtures(fixture_ids.clone());
                let programmer = self.programmer.read().await;
                let _ = event_tx.send(ConsoleEvent::ProgrammerStateUpdated {
                    preview_mode: programmer.get_preview_mode(),
                    selected_fixtures: fixture_ids,
                });
            }
            AddSelectedFixture { fixture_id } => {
                self.programmer
                    .write()
                    .await
                    .add_selected_fixture(fixture_id);
                let programmer = self.programmer.read().await;
                let selected_fixtures = programmer.get_selected_fixtures().clone();
                let _ = event_tx.send(ConsoleEvent::ProgrammerStateUpdated {
                    preview_mode: programmer.get_preview_mode(),
                    selected_fixtures,
                });
            }
            RemoveSelectedFixture { fixture_id } => {
                self.programmer
                    .write()
                    .await
                    .remove_selected_fixture(fixture_id);
                let programmer = self.programmer.read().await;
                let selected_fixtures = programmer.get_selected_fixtures().clone();
                let _ = event_tx.send(ConsoleEvent::ProgrammerStateUpdated {
                    preview_mode: programmer.get_preview_mode(),
                    selected_fixtures,
                });
            }
            ClearSelectedFixtures => {
                self.programmer.write().await.clear_selected_fixtures();
                let programmer = self.programmer.read().await;
                let _ = event_tx.send(ConsoleEvent::ProgrammerStateUpdated {
                    preview_mode: programmer.get_preview_mode(),
                    selected_fixtures: Vec::new(),
                });
            }
            ClearProgrammer => {
                self.programmer.write().await.clear();
            }
            RecordProgrammerToCue {
                cue_name,
                list_index: _,
            } => {
                // TODO: Implement record_programmer_to_cue method
                println!("Recording programmer to cue: {}", cue_name);
            }
            ApplyProgrammerEffect {
                fixture_ids,
                channel_types,
                effect_type,
                waveform,
                interval,
                ratio,
                phase,
                distribution,
                step_value,
                wave_offset,
            } => {
                // Convert string channel types to ChannelType enum
                let channel_types_enum: Vec<halo_fixtures::ChannelType> = channel_types
                    .iter()
                    .map(|s| Self::channel_string_to_type(s))
                    .collect();

                // Convert UI parameters to effect parameters
                let interval_enum = match interval {
                    0 => crate::Interval::Beat,
                    1 => crate::Interval::Bar,
                    2 => crate::Interval::Phrase,
                    _ => crate::Interval::Beat,
                };

                let distribution_enum = match distribution {
                    0 => crate::EffectDistribution::All,
                    1 => crate::EffectDistribution::Step(step_value.unwrap_or(1)),
                    2 => crate::EffectDistribution::Wave(wave_offset.unwrap_or(0.0) as f64),
                    _ => crate::EffectDistribution::All,
                };

                // Create the effect
                let effect = crate::Effect {
                    effect_type,
                    min: 0,
                    max: 255,
                    amplitude: 1.0,
                    frequency: 1.0,
                    offset: 0.0,
                    params: crate::EffectParams {
                        interval: interval_enum,
                        interval_ratio: ratio as f64,
                        phase: phase as f64,
                    },
                };

                // Create effect mapping
                let effect_mapping = crate::EffectMapping {
                    name: format!("Programmer_{}_{}", effect_type.as_str(), fixture_ids.len()),
                    effect,
                    fixture_ids,
                    channel_types: channel_types_enum,
                    distribution: distribution_enum,
                    release: crate::EffectRelease::Hold,
                };

                // Add to tracking state
                let mut tracking_state = self.tracking_state.write().await;
                tracking_state.add_effect(effect_mapping);
            }

            // Query commands
            QueryFixtures => {
                let fixtures = self.fixtures.read().await.clone();
                let _ = event_tx.send(ConsoleEvent::FixturesList { fixtures });
            }
            QueryCueLists => {
                let cue_lists = self.cue_manager.read().await.get_cue_lists().clone();
                let _ = event_tx.send(ConsoleEvent::CueListsList { cue_lists });
            }
            QueryCurrentCueListIndex => {
                let index = self.cue_manager.read().await.get_current_cue_list_idx();
                let _ = event_tx.send(ConsoleEvent::CurrentCueListIndex { index });
            }
            QueryCurrentCue => {
                let cue_manager = self.cue_manager.read().await;
                let cue_index = cue_manager.get_current_cue_idx().unwrap_or(0);
                let progress = cue_manager.get_current_cue_progress();
                let _ = event_tx.send(ConsoleEvent::CurrentCue {
                    cue_index,
                    progress,
                });
            }
            QueryPlaybackState => {
                let state = self.cue_manager.read().await.get_playback_state();
                let _ = event_tx.send(ConsoleEvent::CurrentPlaybackState { state });
            }
            QueryRhythmState => {
                let rhythm_guard = self.rhythm_state.read().await;
                let state = RhythmState {
                    beat_phase: rhythm_guard.beat_phase,
                    bar_phase: rhythm_guard.bar_phase,
                    phrase_phase: rhythm_guard.phrase_phase,
                    beats_per_bar: rhythm_guard.beats_per_bar,
                    bars_per_phrase: rhythm_guard.bars_per_phrase,
                    last_tap_time: rhythm_guard.last_tap_time,
                    tap_count: rhythm_guard.tap_count,
                };
                let _ = event_tx.send(ConsoleEvent::CurrentRhythmState { state });
            }
            QueryShow => {
                let show = self.get_show().await;
                let _ = event_tx.send(ConsoleEvent::CurrentShow { show });
            }
            QueryLinkState => {
                let enabled = self.is_ableton_link_enabled().await;
                let num_peers = self.get_ableton_link_peers().await;
                let _ = event_tx.send(ConsoleEvent::LinkStateChanged { enabled, num_peers });
            }
            QueryFixtureLibrary => {
                let profiles: Vec<(String, String)> = self
                    .fixture_library
                    .profiles
                    .iter()
                    .map(|(id, profile)| (id.clone(), profile.to_string()))
                    .collect();
                let _ = event_tx.send(ConsoleEvent::FixtureLibraryList { profiles });
            }
            EnableAbletonLink => {
                if let Err(e) = self.enable_ableton_link().await {
                    let _ = event_tx.send(ConsoleEvent::Error {
                        message: format!("Failed to enable Ableton Link: {}", e),
                    });
                } else {
                    let enabled = self.is_ableton_link_enabled().await;
                    let num_peers = self.get_ableton_link_peers().await;
                    let _ = event_tx.send(ConsoleEvent::LinkStateChanged { enabled, num_peers });
                }
            }
            DisableAbletonLink => {
                self.disable_ableton_link().await;
                let enabled = self.is_ableton_link_enabled().await;
                let num_peers = self.get_ableton_link_peers().await;
                let _ = event_tx.send(ConsoleEvent::LinkStateChanged { enabled, num_peers });
            }

            // Settings management
            UpdateSettings { settings } => {
                log::info!("Updating settings");
                *self.settings.write().await = settings.clone();
                let _ = event_tx.send(ConsoleEvent::SettingsUpdated { settings });
            }
            QuerySettings => {
                let settings = self.settings.read().await.clone();
                let _ = event_tx.send(ConsoleEvent::CurrentSettings { settings });
            }
            QueryAudioDevices => match device_enumerator::enumerate_audio_devices() {
                Ok(devices) => {
                    log::info!("Found {} audio devices", devices.len());
                    let _ = event_tx.send(ConsoleEvent::AudioDevicesList { devices });
                }
                Err(e) => {
                    log::error!("Failed to enumerate audio devices: {}", e);
                    let _ = event_tx.send(ConsoleEvent::Error {
                        message: format!("Failed to enumerate audio devices: {e}"),
                    });
                }
            },

            // Pixel engine commands
            ConfigurePixelEngine {
                enabled,
                universe_mapping,
            } => {
                log::info!("Configuring pixel engine: enabled={}", enabled);
                let mut pixel_engine = self.pixel_engine.write().await;
                pixel_engine.set_enabled(enabled);
                pixel_engine.clear_universe_mappings();
                for (fixture_id, universe) in universe_mapping {
                    pixel_engine.set_fixture_universe(fixture_id, universe);
                }
            }
            AddPixelEffect {
                name,
                fixture_ids,
                effect,
                distribution,
            } => {
                log::info!("Adding pixel effect: {}", name);
                let mut pixel_engine = self.pixel_engine.write().await;
                pixel_engine.add_effect(name, fixture_ids, effect, distribution);
            }
            RemovePixelEffect { name } => {
                log::info!("Removing pixel effect: {}", name);
                let mut pixel_engine = self.pixel_engine.write().await;
                pixel_engine.remove_effect(&name);
            }
            ClearPixelEffects => {
                log::info!("Clearing all pixel effects");
                let mut pixel_engine = self.pixel_engine.write().await;
                pixel_engine.clear_effects();
            }
        }

        Ok(())
    }

    /// Run the console with channel-based communication
    pub async fn run_with_channels(
        mut self,
        mut command_rx: mpsc::UnboundedReceiver<ConsoleCommand>,
        event_tx: mpsc::UnboundedSender<ConsoleEvent>,
    ) -> Result<(), anyhow::Error> {
        log::info!("Console run_with_channels starting...");

        // Start the update loop
        let mut update_interval = tokio::time::interval(std::time::Duration::from_millis(23)); // ~44Hz
        log::info!("Starting console main loop...");

        loop {
            tokio::select! {
                // Process commands from UI
                Some(command) = command_rx.recv() => {
                    log::debug!("Received command: {:?}", command);

                    if let ConsoleCommand::Shutdown = command {
                        log::info!("Received shutdown command");
                        self.shutdown().await?;
                        let _ = event_tx.send(ConsoleEvent::ShutdownComplete);
                        break;
                    }

                    if let Err(e) = self.process_command(command, &event_tx).await {
                        log::error!("Command processing error: {}", e);
                        let _ = event_tx.send(ConsoleEvent::Error {
                            message: format!("Command processing error: {}", e)
                        });
                    }
                }

                // Regular update tick
                _ = update_interval.tick() => {
                    if let Err(e) = self.update().await {
                        log::error!("Update error: {}", e);
                    }

                    // Send periodic state updates
                    if let Some(timecode) = self.cue_manager.read().await.current_timecode {
                        let _ = event_tx.send(ConsoleEvent::TimecodeUpdated { timecode });
                    }

                    // Send current cue information
                    let cue_manager = self.cue_manager.read().await;
                    let cue_index = cue_manager.get_current_cue_idx().unwrap_or(0);
                    let progress = cue_manager.get_current_cue_progress();
                    let _ = event_tx.send(ConsoleEvent::CurrentCueChanged { cue_index, progress });

                    let rhythm_guard = self.rhythm_state.read().await;
                    let rhythm_state = RhythmState {
                        beat_phase: rhythm_guard.beat_phase,
                        bar_phase: rhythm_guard.bar_phase,
                        phrase_phase: rhythm_guard.phrase_phase,
                        beats_per_bar: rhythm_guard.beats_per_bar,
                        bars_per_phrase: rhythm_guard.bars_per_phrase,
                        last_tap_time: rhythm_guard.last_tap_time,
                        tap_count: rhythm_guard.tap_count,
                    };
                    let _ = event_tx.send(ConsoleEvent::RhythmStateUpdated { state: rhythm_state });

                    // Send tracking state information
                    let tracking_state = self.tracking_state.read().await;
                    let active_effect_count = tracking_state.active_effect_count();
                    let _ = event_tx.send(ConsoleEvent::TrackingStateUpdated { active_effect_count });
                }

                // Process module messages (if available)
                Some(message) = async {
                    if let Some(rx) = self.message_rx.as_mut() {
                        rx.recv().await
                    } else {
                        // Return a future that never resolves if no receiver
                        std::future::pending().await
                    }
                } => {
                    match message {
                        ModuleMessage::Event(event) => {
                            match event {
                                ModuleEvent::MidiInput(midi_msg) => {
                                    Self::handle_midi_input(midi_msg, &self.rhythm_state, &self.cue_manager).await;
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
                            // Send error to UI
                            let _ = event_tx.send(ConsoleEvent::Error { message: error });
                        }
                    }
                }
            }
        }

        log::info!("Console run_with_channels completed");
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
            let mut console = self.inner.lock().await;
            console
                .patch_fixture(name, profile_name, universe, address)
                .await
        })
    }

    pub fn set_cue_lists(&mut self, cue_lists: Vec<CueList>) {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
            console.set_cue_lists(cue_lists).await;
        });
    }

    pub fn set_bpm(&mut self, bpm: f64) {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().await;
            if let Err(e) = console.set_bpm(bpm).await {
                log::error!("Failed to set BPM: {}", e);
            }
        });
    }

    pub fn add_midi_override(&mut self, note: u8, override_config: MidiOverride) {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().await;
            console.add_midi_override(note, override_config);
        });
    }

    pub fn new_show(&mut self, name: String) -> Result<(), anyhow::Error> {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().await;
            console.new_show(name).await
        })
    }

    pub fn reload_show(&mut self) -> Result<(), anyhow::Error> {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().await;
            console.reload_show().await
        })
    }

    pub fn save_show(&mut self) -> Result<std::path::PathBuf, anyhow::Error> {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().await;
            console.save_show().await
        })
    }

    pub fn save_show_as(
        &mut self,
        name: String,
        path: std::path::PathBuf,
    ) -> Result<std::path::PathBuf, anyhow::Error> {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().await;
            console.save_show_as(name, path).await
        })
    }

    pub fn load_show(&mut self, path: &std::path::Path) -> Result<(), anyhow::Error> {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().await;
            console.load_show(path).await
        })
    }

    pub fn get_show(&self) -> crate::show::show::Show {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
            console.get_show().await
        })
    }

    pub fn update(&mut self) {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().await;
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
            let console = self.inner.lock().await;
            let fixtures = console.fixtures.read().await;
            fixtures.clone()
        })
    }

    pub fn cue_manager(&self) -> CueManager {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
            let cue_manager = console.cue_manager.read().await;
            cue_manager.clone()
        })
    }

    pub fn show_manager(&self) -> ShowManager {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
            let show_manager = console.show_manager.read().await;
            show_manager.clone()
        })
    }

    pub fn programmer(&self) -> Programmer {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
            let programmer = console.programmer.read().await;
            programmer.clone()
        })
    }

    pub fn record_cue(&mut self, name: String, fade_time: f64) -> Result<(), anyhow::Error> {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
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
                pixel_effects: vec![],
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
            let console = self.inner.lock().await;
            let mut programmer = console.programmer.write().await;
            programmer.set_preview_mode(preview_mode);
        });
    }

    pub fn clear_programmer(&mut self) {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
            let mut programmer = console.programmer.write().await;
            programmer.clear();
        });
    }

    pub fn add_programmer_effect(&mut self, effect: crate::EffectMapping) {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
            let mut programmer = console.programmer.write().await;
            programmer.add_effect(effect);
        });
    }

    pub fn get_programmer_values(&self) -> Vec<crate::StaticValue> {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
            let programmer = console.programmer.read().await;
            programmer.get_values().clone()
        })
    }

    pub fn get_programmer_effects(&self) -> Vec<crate::EffectMapping> {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
            let programmer = console.programmer.read().await;
            programmer.get_effects().clone()
        })
    }

    pub fn is_running(&self) -> bool {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
            console.is_running()
        })
    }

    /// Enable Ableton Link
    pub fn enable_ableton_link(&mut self) -> Result<(), anyhow::Error> {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().await;
            console.enable_ableton_link().await
        })
    }

    /// Disable Ableton Link
    pub fn disable_ableton_link(&mut self) {
        self.runtime.block_on(async {
            let mut console = self.inner.lock().await;
            console.disable_ableton_link().await;
        });
    }

    /// Check if Ableton Link is enabled
    pub fn is_ableton_link_enabled(&self) -> bool {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
            console.is_ableton_link_enabled().await
        })
    }

    /// Get the number of Ableton Link peers
    pub fn get_ableton_link_peers(&self) -> u64 {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
            console.get_ableton_link_peers().await
        })
    }

    /// Apply master fader to fixtures
    pub fn apply_master_fader(&mut self, master_value: f32) {
        self.runtime.block_on(async {
            let console = self.inner.lock().await;
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
            let console = self.inner.lock().await;
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

impl Drop for SyncLightingConsole {
    fn drop(&mut self) {
        // Ensure module manager is properly shut down
        self.runtime.block_on(async {
            let mut console = self.inner.lock().await;
            if let Err(e) = console.module_manager.shutdown().await {
                log::error!("Error shutting down module manager during drop: {}", e);
            }
        });
    }
}
