use std::path::PathBuf;

use halo_fixtures::Fixture;

use crate::{CueList, EffectType, MidiOverride, PlaybackState, RhythmState, Show, TimeCode};

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
    UpdateFixtureChannels {
        fixture_id: usize,
        channel_values: Vec<(String, u8)>,
    },

    // Cue management
    SetCueLists {
        cue_lists: Vec<CueList>,
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
    ClearProgrammer,
    RecordProgrammerToCue {
        list_index: usize,
        cue_index: Option<usize>,
    },

    // Query commands (request state)
    QueryFixtures,
    QueryCueLists,
    QueryPlaybackState,
    QueryRhythmState,
    QueryShow,
    QueryLinkState,
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

    // Response to queries
    FixturesList {
        fixtures: Vec<Fixture>,
    },
    CueListsList {
        cue_lists: Vec<CueList>,
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
}
