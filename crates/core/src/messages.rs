use std::path::PathBuf;

use halo_fixtures::Fixture;
use serde::{Deserialize, Serialize};

use crate::audio::device_enumerator::AudioDeviceInfo;
use crate::{CueList, EffectType, MidiOverride, PlaybackState, RhythmState, Show, TimeCode};

/// Track information for UI display.
#[derive(Debug, Clone)]
pub struct DjTrackInfo {
    pub id: i64,
    pub title: String,
    pub artist: Option<String>,
    pub duration_seconds: f64,
    pub bpm: Option<f64>,
}

/// Commands sent from UI to Console
#[derive(Debug, Clone)]
pub enum ConsoleCommand {
    // System commands
    Initialize,
    Shutdown,
    Update,

    // Show management
    NewShow {
        name: String,
    },
    LoadShow {
        path: PathBuf,
    },
    SaveShow,
    SaveShowAs {
        name: String,
        path: PathBuf,
    },
    ReloadShow,

    // Fixture management
    PatchFixture {
        name: String,
        profile_name: String,
        universe: u8,
        address: u16,
    },
    UnpatchFixture {
        fixture_id: usize,
    },
    UpdateFixture {
        fixture_id: usize,
        name: String,
        universe: u8,
        address: u16,
    },
    UpdateFixtureChannels {
        fixture_id: usize,
        channel_values: Vec<(String, u8)>,
    },
    SetPanTiltLimits {
        fixture_id: usize,
        pan_min: u8,
        pan_max: u8,
        tilt_min: u8,
        tilt_max: u8,
    },
    ClearPanTiltLimits {
        fixture_id: usize,
    },

    // Cue management
    SetCueLists {
        cue_lists: Vec<CueList>,
    },
    UpdateCue {
        list_index: usize,
        cue_index: usize,
        name: String,
        fade_time: f64,
        timecode: Option<String>,
        is_blocking: bool,
    },
    DeleteCue {
        list_index: usize,
        cue_index: usize,
    },
    DeleteCueList {
        list_index: usize,
    },
    SetCueListAudioFile {
        list_index: usize,
        audio_file: Option<String>,
    },
    AddCue {
        list_index: usize,
        name: String,
        fade_time: f64,
        timecode: Option<String>,
        is_blocking: bool,
    },
    PlayCue {
        list_index: usize,
        cue_index: usize,
    },
    StopCue {
        list_index: usize,
    },
    PauseCue {
        list_index: usize,
    },
    ResumeCue {
        list_index: usize,
    },
    GoToCue {
        list_index: usize,
        cue_index: usize,
    },
    NextCue {
        list_index: usize,
    },
    PrevCue {
        list_index: usize,
    },
    SelectNextCueList,
    SelectPreviousCueList,

    // Playback control
    Play,
    Stop,
    Pause,
    Resume,
    SetPlaybackRate {
        rate: f64,
    },

    // Tempo and timing
    SetBpm {
        bpm: f64,
    },
    TapTempo,
    SetTimecode {
        timecode: TimeCode,
    },
    SeekAudio {
        position_seconds: f64,
    },

    // MIDI
    AddMidiOverride {
        note: u8,
        override_config: MidiOverride,
    },
    RemoveMidiOverride {
        note: u8,
    },
    ProcessMidiMessage {
        message: Vec<u8>,
    },

    // Audio
    PlayAudio {
        file_path: String,
    },
    StopAudio,
    SetAudioVolume {
        volume: f32,
    },

    // Ableton Link
    EnableAbletonLink,
    DisableAbletonLink,

    // Tempo source
    SetTempoSource {
        source: crate::rhythm::rhythm::TempoSource,
    },

    // DJ commands
    DjImportFolder {
        path: PathBuf,
    },
    DjLoadTrack {
        deck: u8,
        track_id: i64,
    },
    DjPlay {
        deck: u8,
    },
    DjPause {
        deck: u8,
    },
    DjStop {
        deck: u8,
    },
    DjSetCue {
        deck: u8,
    },
    DjJumpToCue {
        deck: u8,
    },
    DjCuePreview {
        deck: u8,
        pressed: bool,
    },
    DjSetHotCue {
        deck: u8,
        slot: u8,
    },
    DjJumpToHotCue {
        deck: u8,
        slot: u8,
    },
    DjSetPitch {
        deck: u8,
        percent: f64,
    },
    DjToggleSync {
        deck: u8,
    },
    DjSetMaster {
        deck: u8,
    },
    DjSeek {
        deck: u8,
        position_seconds: f64,
    },
    DjSeekBeats {
        deck: u8,
        beats: i32,
    },
    DjNudgePitch {
        deck: u8,
        delta: f64,
    },
    DjQueryLibrary,

    // Ableton Link toggle
    ToggleAbletonLink,

    // Effects
    ApplyEffect {
        fixture_ids: Vec<usize>,
        channel_type: String,
        effect_type: EffectType,
        frequency: f32,
        amplitude: f32,
        offset: f32,
    },
    ClearEffect {
        fixture_ids: Vec<usize>,
        channel_type: String,
    },

    // Programmer
    SetProgrammerValue {
        fixture_id: usize,
        channel: String,
        value: u8,
    },
    SetProgrammerPreviewMode {
        preview_mode: bool,
    },
    SetSelectedFixtures {
        fixture_ids: Vec<usize>,
    },
    AddSelectedFixture {
        fixture_id: usize,
    },
    RemoveSelectedFixture {
        fixture_id: usize,
    },
    ClearSelectedFixtures,
    RecordProgrammerToCue {
        cue_name: String,
        list_index: Option<usize>,
    },
    ClearProgrammer,
    ApplyProgrammerEffect {
        fixture_ids: Vec<usize>,
        channel_types: Vec<String>,
        effect_type: EffectType,
        waveform: u8,
        interval: u8,
        ratio: f32,
        phase: f32,
        distribution: u8,
        step_value: Option<usize>,
        wave_offset: Option<f32>,
    },

    // Settings commands
    UpdateSettings {
        settings: Settings,
    },
    QuerySettings,
    QueryAudioDevices,

    // Pixel engine commands
    ConfigurePixelEngine {
        enabled: bool,
        universe_mapping: std::collections::HashMap<usize, u8>,
    },
    AddPixelEffect {
        name: String,
        fixture_ids: Vec<usize>,
        effect: crate::pixel::PixelEffect,
        distribution: crate::EffectDistribution,
    },
    RemovePixelEffect {
        name: String,
    },
    ClearPixelEffects,

    // Query commands (request state)
    QueryFixtures,
    QueryCueLists,
    QueryCurrentCueListIndex,
    QueryCurrentCue,
    QueryPlaybackState,
    QueryRhythmState,
    QueryShow,
    QueryLinkState,
    QueryFixtureLibrary,
}

/// Settings configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    // General settings
    pub target_fps: u32,
    pub enable_autosave: bool,
    pub autosave_interval_secs: u32,

    // Audio settings
    pub audio_device: String,
    pub audio_buffer_size: u32,
    pub audio_sample_rate: u32,

    // MIDI settings
    pub midi_enabled: bool,
    pub midi_device: String,
    pub midi_channel: u8,

    // Output settings (DMX/Art-Net)
    pub dmx_enabled: bool,
    pub dmx_broadcast: bool,
    pub dmx_source_ip: String,
    pub dmx_dest_ip: String,
    pub dmx_port: u16,
    pub wled_enabled: bool,
    pub wled_ip: String,

    // Pixel engine settings
    pub pixel_engine_enabled: bool,
    pub pixel_engine_fps: f64,
    pub pixel_universe_mapping: std::collections::HashMap<usize, u8>,

    // Fixture settings
    pub enable_pan_tilt_limits: bool,

    // Push 2 settings
    pub push2_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            // General defaults
            target_fps: 60,
            enable_autosave: false,
            autosave_interval_secs: 300,

            // Audio defaults
            audio_device: "Default".to_string(),
            audio_buffer_size: 512,
            audio_sample_rate: 48000,

            // MIDI defaults
            midi_enabled: false,
            midi_device: "None".to_string(),
            midi_channel: 1,

            // Output defaults
            dmx_enabled: true,
            dmx_broadcast: false,
            dmx_source_ip: "192.168.1.100".to_string(),
            dmx_dest_ip: "192.168.1.200".to_string(),
            dmx_port: 6454,
            wled_enabled: false,
            wled_ip: "192.168.1.50".to_string(),

            // Pixel engine defaults
            pixel_engine_enabled: false,
            pixel_engine_fps: 44.0,
            pixel_universe_mapping: std::collections::HashMap::new(),

            // Fixture defaults
            enable_pan_tilt_limits: true,

            // Push 2 defaults
            push2_enabled: false,
        }
    }
}

/// Events sent from Console to UI
#[derive(Debug, Clone)]
pub enum ConsoleEvent {
    // System events
    Initialized,
    ShutdownComplete,
    Error {
        message: String,
    },

    // State updates
    FixturesUpdated {
        fixtures: Vec<Fixture>,
    },
    CueListsUpdated {
        cue_lists: Vec<CueList>,
    },
    PlaybackStateChanged {
        state: PlaybackState,
    },
    RhythmStateUpdated {
        state: RhythmState,
    },
    TrackingStateUpdated {
        active_effect_count: usize,
    },
    TimecodeUpdated {
        timecode: TimeCode,
    },
    BpmChanged {
        bpm: f64,
    },

    // Show events
    ShowLoaded {
        show: Show,
    },
    ShowSaved {
        path: PathBuf,
    },
    ShowCreated {
        name: String,
    },

    // Fixture events
    FixturePatched {
        fixture_id: usize,
        fixture: Fixture,
    },
    FixtureUnpatched {
        fixture_id: usize,
    },
    FixtureUpdated {
        fixture_id: usize,
        fixture: Fixture,
    },
    FixtureValuesChanged {
        fixture_id: usize,
        values: Vec<(String, u8)>,
    },

    // Cue events
    CueStarted {
        list_index: usize,
        cue_index: usize,
    },
    CueStopped {
        list_index: usize,
    },
    CueCompleted {
        list_index: usize,
        cue_index: usize,
    },
    CueListCompleted {
        list_index: usize,
    },
    CueListSelected {
        list_index: usize,
    },
    CurrentCueChanged {
        cue_index: usize,
        progress: f32,
    },

    // MIDI events
    MidiOverrideAdded {
        note: u8,
    },
    MidiOverrideRemoved {
        note: u8,
    },
    MidiMessageReceived {
        message: Vec<u8>,
    },

    // Audio events
    AudioStarted {
        file_path: String,
    },
    AudioStopped,
    AudioVolumeChanged {
        volume: f32,
    },

    // Link events
    LinkStateChanged {
        enabled: bool,
        num_peers: u64,
    },

    // DJ events
    DjLibraryUpdated {
        track_count: usize,
    },
    DjImportProgress {
        current: usize,
        total: usize,
        current_file: String,
    },
    DjImportComplete {
        imported_count: usize,
        skipped_count: usize,
    },
    DjTrackLoaded {
        deck: u8,
        track_id: i64,
        title: String,
        artist: Option<String>,
        duration_seconds: f64,
        bpm: Option<f64>,
    },
    DjDeckStateChanged {
        deck: u8,
        is_playing: bool,
        position_seconds: f64,
    },
    DjCuePointSet {
        deck: u8,
        position_seconds: f64,
    },
    DjWaveformProgress {
        deck: u8,
        samples: Vec<f32>,
        progress: f32,
    },
    DjWaveformLoaded {
        deck: u8,
        samples: Vec<f32>,
        duration_seconds: f64,
    },
    DjLibraryTracks {
        tracks: Vec<DjTrackInfo>,
    },

    // Programmer events
    ProgrammerStateUpdated {
        preview_mode: bool,
        selected_fixtures: Vec<usize>,
    },
    ProgrammerValuesUpdated {
        values: Vec<(usize, String, u8)>, // (fixture_id, channel, value)
    },
    ProgrammerEffectsUpdated {
        effects: Vec<(String, EffectType, Vec<usize>)>, // (name, effect_type, fixture_ids)
    },

    // Response to queries
    FixturesList {
        fixtures: Vec<Fixture>,
    },
    CueListsList {
        cue_lists: Vec<CueList>,
    },
    CurrentCueListIndex {
        index: usize,
    },
    CurrentCue {
        cue_index: usize,
        progress: f32,
    },
    CurrentPlaybackState {
        state: PlaybackState,
    },
    CurrentRhythmState {
        state: RhythmState,
    },
    CurrentShow {
        show: Show,
    },

    // Settings events
    SettingsUpdated {
        settings: Settings,
    },
    CurrentSettings {
        settings: Settings,
    },
    AudioDevicesList {
        devices: Vec<AudioDeviceInfo>,
    },
    WaveformAnalyzed {
        waveform_data: crate::audio::waveform::WaveformData,
        duration: f64,
        bpm: Option<f64>,
    },
    FixtureLibraryList {
        profiles: Vec<(String, String)>, // (id, display_name)
    },
    PixelDataUpdated {
        pixel_data: Vec<(usize, Vec<(u8, u8, u8)>)>, // (fixture_id, pixels_rgb)
    },
}
