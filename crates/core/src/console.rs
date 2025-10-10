use std::collections::HashMap;
use std::sync::Arc;

use halo_fixtures::{Fixture, FixtureLibrary};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::artnet::network_config::NetworkConfig;
use crate::cue::cue_manager::{CueManager, PlaybackState};
use crate::messages::{ConsoleCommand, ConsoleEvent};
use crate::midi::midi::{MidiMessage, MidiOverride};
use crate::modules::{
    AudioModule, DmxModule, MidiModule, ModuleEvent, ModuleId, ModuleManager, ModuleMessage,
    SmpteModule,
};
use crate::programmer::Programmer;
use crate::rhythm::rhythm::RhythmState;
use crate::show::show_manager::ShowManager;
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

    // MIDI overrides
    midi_overrides: HashMap<u8, MidiOverride>,
    active_overrides: HashMap<u8, (bool, u8)>,

    // Rhythm state
    rhythm_state: Arc<RwLock<RhythmState>>,

    // Ableton Link integration
    link_manager: Arc<Mutex<AbletonLinkManager>>,

    // System state
    is_running: bool,
}

impl LightingConsole {
    pub fn new(bpm: f64, network_config: NetworkConfig) -> Result<Self, anyhow::Error> {
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
            link_manager: Arc::new(Mutex::new(AbletonLinkManager::new())),
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
        _fixtures: &Arc<RwLock<Vec<Fixture>>>,
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
                self.load_show(&path).await?;
                let show = self.get_show().await;
                let _ = event_tx.send(ConsoleEvent::ShowLoaded { show });
                log::info!("LoadShow command completed successfully");
            }
            SaveShow => {
                let path = self.save_show().await?;
                let _ = event_tx.send(ConsoleEvent::ShowSaved { path });
            }
            SaveShowAs { name, path } => {
                let saved_path = self.save_show_as(name, path).await?;
                let _ = event_tx.send(ConsoleEvent::ShowSaved { path: saved_path });
            }
            ReloadShow => {
                self.reload_show().await?;
                let show = self.get_show().await;
                let _ = event_tx.send(ConsoleEvent::ShowLoaded { show });
            }

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
                // TODO: Implement programmer channel value setting
                println!(
                    "Setting programmer value: fixture {}, channel {}, value {}",
                    fixture_id, channel, value
                );
            }
            SetProgrammerPreviewMode { preview_mode } => {
                self.programmer.write().await.set_preview_mode(preview_mode);
            }
            SetProgrammerCollapsed { collapsed: _ } => {
                // TODO: Handle collapsed state in UI state
            }
            SetSelectedFixtures { fixture_ids } => {
                self.programmer
                    .write()
                    .await
                    .set_selected_fixtures(fixture_ids.clone());
                let programmer = self.programmer.read().await;
                let _ = event_tx.send(ConsoleEvent::ProgrammerStateUpdated {
                    preview_mode: programmer.get_preview_mode(),
                    collapsed: programmer.get_collapsed(),
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
                    collapsed: programmer.get_collapsed(),
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
                    collapsed: programmer.get_collapsed(),
                    selected_fixtures,
                });
            }
            ClearSelectedFixtures => {
                self.programmer.write().await.clear_selected_fixtures();
                let programmer = self.programmer.read().await;
                let _ = event_tx.send(ConsoleEvent::ProgrammerStateUpdated {
                    preview_mode: programmer.get_preview_mode(),
                    collapsed: programmer.get_collapsed(),
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
                channel_type,
                effect_type,
                waveform,
                interval,
                ratio,
                phase,
                distribution,
                step_value,
                wave_offset,
            } => {
                // TODO: Implement programmer effect application
                println!(
                    "Applying programmer effect: {:?} to fixtures {:?}",
                    effect_type, fixture_ids
                );
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

        // Initialize the console
        log::info!("Initializing console...");
        self.initialize().await?;
        log::info!("Console initialized successfully");

        let _ = event_tx.send(ConsoleEvent::Initialized);
        log::info!("Sent Initialized event");

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
