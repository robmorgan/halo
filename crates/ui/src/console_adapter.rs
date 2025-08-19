use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

use halo_core::{
    ConsoleCommand, ConsoleEvent, ConsoleHandle, CueList, Fixture, MidiOverride, PlaybackState,
    RhythmState, Show, TimeCode,
};

/// State cache for the UI to avoid async operations
#[derive(Clone)]
pub struct ConsoleState {
    pub fixtures: Vec<Fixture>,
    pub cue_lists: Vec<CueList>,
    pub playback_state: PlaybackState,
    pub rhythm_state: RhythmState,
    pub timecode: Option<TimeCode>,
    pub bpm: f64,
    pub show: Option<Show>,
    pub link_enabled: bool,
    pub link_peers: u64,
}

impl Default for ConsoleState {
    fn default() -> Self {
        Self {
            fixtures: Vec::new(),
            cue_lists: Vec::new(),
            playback_state: PlaybackState::Stopped,
            rhythm_state: RhythmState {
                beat_phase: 0.0,
                bar_phase: 0.0,
                phrase_phase: 0.0,
                beats_per_bar: 4,
                bars_per_phrase: 4,
                last_tap_time: None,
                tap_count: 0,
            },
            timecode: None,
            bpm: 120.0,
            show: None,
            link_enabled: false,
            link_peers: 0,
        }
    }
}

/// Adapter that bridges the UI with the channel-based console
pub struct ConsoleAdapter {
    /// Handle for sending commands to the console
    console_handle: ConsoleHandle,
    
    /// Cached state for synchronous UI access
    state: Arc<RwLock<ConsoleState>>,
    
    /// Receiver for events from the console
    event_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<ConsoleEvent>>>,
    
    /// Runtime for executing async operations from sync context
    runtime: Arc<tokio::runtime::Runtime>,
}

impl ConsoleAdapter {
    pub fn new(
        console_handle: ConsoleHandle,
        event_rx: mpsc::UnboundedReceiver<ConsoleEvent>,
    ) -> Self {
        let runtime = Arc::new(
            tokio::runtime::Runtime::new().expect("Failed to create runtime for ConsoleAdapter"),
        );
        
        Self {
            console_handle,
            state: Arc::new(RwLock::new(ConsoleState::default())),
            event_rx: Arc::new(tokio::sync::Mutex::new(event_rx)),
            runtime,
        }
    }
    
    /// Process incoming events and update cached state
    pub fn process_events(&self) {
        let state = self.state.clone();
        let event_rx = self.event_rx.clone();
        
        self.runtime.spawn(async move {
            let mut rx = event_rx.lock().await;
            
            // Process all available events
            while let Ok(event) = rx.try_recv() {
                let mut state = state.write().await;
                
                match event {
                    ConsoleEvent::FixturesUpdated { fixtures } => {
                        state.fixtures = fixtures;
                    }
                    ConsoleEvent::CueListsUpdated { cue_lists } => {
                        state.cue_lists = cue_lists;
                    }
                    ConsoleEvent::PlaybackStateChanged { state: playback_state } => {
                        state.playback_state = playback_state;
                    }
                    ConsoleEvent::RhythmStateUpdated { state: rhythm_state } => {
                        state.rhythm_state = rhythm_state;
                    }
                    ConsoleEvent::TimecodeUpdated { timecode } => {
                        state.timecode = Some(timecode);
                    }
                    ConsoleEvent::BpmChanged { bpm } => {
                        state.bpm = bpm;
                    }
                    ConsoleEvent::ShowLoaded { show } => {
                        state.show = Some(show);
                    }
                    ConsoleEvent::FixturePatched { fixture_id: _, fixture } => {
                        state.fixtures.push(fixture);
                    }
                    ConsoleEvent::LinkStateChanged { enabled, num_peers } => {
                        state.link_enabled = enabled;
                        state.link_peers = num_peers;
                    }
                    ConsoleEvent::FixturesList { fixtures } => {
                        state.fixtures = fixtures;
                    }
                    ConsoleEvent::CueListsList { cue_lists } => {
                        state.cue_lists = cue_lists;
                    }
                    ConsoleEvent::CurrentPlaybackState { state: playback_state } => {
                        state.playback_state = playback_state;
                    }
                    ConsoleEvent::CurrentRhythmState { state: rhythm_state } => {
                        state.rhythm_state = rhythm_state;
                    }
                    ConsoleEvent::CurrentShow { show } => {
                        state.show = Some(show);
                    }
                    _ => {
                        // Handle other events as needed
                    }
                }
            }
        });
    }
    
    /// Get a snapshot of the current state (synchronous)
    pub fn get_state(&self) -> ConsoleState {
        self.runtime.block_on(async {
            self.state.read().await.clone()
        })
    }
    
    /// Send a command to the console (synchronous wrapper)
    pub fn send_command(&self, command: ConsoleCommand) -> Result<(), String> {
        self.console_handle.send_command(command)
    }
    
    // Convenience methods for common operations
    
    pub fn play(&self) -> Result<(), String> {
        self.send_command(ConsoleCommand::Play)
    }
    
    pub fn stop(&self) -> Result<(), String> {
        self.send_command(ConsoleCommand::Stop)
    }
    
    pub fn pause(&self) -> Result<(), String> {
        self.send_command(ConsoleCommand::Pause)
    }
    
    pub fn resume(&self) -> Result<(), String> {
        self.send_command(ConsoleCommand::Resume)
    }
    
    pub fn set_bpm(&self, bpm: f64) -> Result<(), String> {
        self.send_command(ConsoleCommand::SetBpm { bpm })
    }
    
    pub fn tap_tempo(&self) -> Result<(), String> {
        self.send_command(ConsoleCommand::TapTempo)
    }
    
    pub fn play_cue(&self, list_index: usize, cue_index: usize) -> Result<(), String> {
        self.send_command(ConsoleCommand::PlayCue { list_index, cue_index })
    }
    
    pub fn stop_cue(&self, list_index: usize) -> Result<(), String> {
        self.send_command(ConsoleCommand::StopCue { list_index })
    }
    
    pub fn next_cue(&self, list_index: usize) -> Result<(), String> {
        self.send_command(ConsoleCommand::NextCue { list_index })
    }
    
    pub fn prev_cue(&self, list_index: usize) -> Result<(), String> {
        self.send_command(ConsoleCommand::PrevCue { list_index })
    }
    
    pub fn patch_fixture(
        &self,
        name: String,
        profile_name: String,
        universe: u8,
        address: u16,
    ) -> Result<(), String> {
        self.send_command(ConsoleCommand::PatchFixture {
            name,
            profile_name,
            universe,
            address,
        })
    }
    
    pub fn set_cue_lists(&self, cue_lists: Vec<CueList>) -> Result<(), String> {
        self.send_command(ConsoleCommand::SetCueLists { cue_lists })
    }
    
    pub fn load_show(&self, path: std::path::PathBuf) -> Result<(), String> {
        self.send_command(ConsoleCommand::LoadShow { path })
    }
    
    pub fn save_show(&self) -> Result<(), String> {
        self.send_command(ConsoleCommand::SaveShow)
    }
    
    pub fn save_show_as(&self, name: String, path: std::path::PathBuf) -> Result<(), String> {
        self.send_command(ConsoleCommand::SaveShowAs { name, path })
    }
    
    pub fn new_show(&self, name: String) -> Result<(), String> {
        self.send_command(ConsoleCommand::NewShow { name })
    }
    
    pub fn reload_show(&self) -> Result<(), String> {
        self.send_command(ConsoleCommand::ReloadShow)
    }
    
    pub fn add_midi_override(&self, note: u8, override_config: MidiOverride) -> Result<(), String> {
        self.send_command(ConsoleCommand::AddMidiOverride { note, override_config })
    }
    
    pub fn play_audio(&self, file_path: String) -> Result<(), String> {
        self.send_command(ConsoleCommand::PlayAudio { file_path })
    }
    
    pub fn set_audio_volume(&self, volume: f32) -> Result<(), String> {
        self.send_command(ConsoleCommand::SetAudioVolume { volume })
    }
    
    pub fn set_programmer_value(&self, fixture_id: usize, channel: String, value: u8) -> Result<(), String> {
        self.send_command(ConsoleCommand::SetProgrammerValue {
            fixture_id,
            channel,
            value,
        })
    }
    
    pub fn clear_programmer(&self) -> Result<(), String> {
        self.send_command(ConsoleCommand::ClearProgrammer)
    }
    
    pub fn query_fixtures(&self) -> Result<(), String> {
        self.send_command(ConsoleCommand::QueryFixtures)
    }
    
    pub fn query_cue_lists(&self) -> Result<(), String> {
        self.send_command(ConsoleCommand::QueryCueLists)
    }
    
    pub fn query_playback_state(&self) -> Result<(), String> {
        self.send_command(ConsoleCommand::QueryPlaybackState)
    }
    
    pub fn query_show(&self) -> Result<(), String> {
        self.send_command(ConsoleCommand::QueryShow)
    }
    
    // Getters for UI components (synchronous)
    
    pub fn fixtures(&self) -> Vec<Fixture> {
        self.get_state().fixtures
    }
    
    pub fn cue_lists(&self) -> Vec<CueList> {
        self.get_state().cue_lists
    }
    
    pub fn playback_state(&self) -> PlaybackState {
        self.get_state().playback_state
    }
    
    pub fn rhythm_state(&self) -> RhythmState {
        self.get_state().rhythm_state
    }
    
    pub fn timecode(&self) -> Option<TimeCode> {
        self.get_state().timecode
    }
    
    pub fn bpm(&self) -> f64 {
        self.get_state().bpm
    }
    
    pub fn show(&self) -> Option<Show> {
        self.get_state().show
    }
}
