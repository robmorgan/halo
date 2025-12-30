use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::mpsc;

/// Unique identifier for each module type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModuleId {
    Audio,
    Dmx,
    Smpte,
    Midi,
    Dj,
    Push2,
}

/// Events that can be sent between modules
#[derive(Debug, Clone)]
pub enum ModuleEvent {
    /// DMX data to output (universe, data)
    DmxOutput(u8, Vec<u8>),
    /// Audio playback command
    AudioPlay {
        file_path: String,
    },
    AudioPause,
    AudioResume,
    AudioStop,
    AudioSetVolume(f32),
    AudioSeek {
        position_seconds: f64,
    },
    /// SMPTE timecode sync
    SmpteSync {
        timecode: crate::timecode::timecode::TimeCode,
    },
    /// MIDI input events
    MidiInput(crate::midi::midi::MidiMessage),
    /// DJ rhythm sync for lighting integration
    DjRhythmSync {
        bpm: f64,
        beat_phase: f64,
        bar_phase: f64,
        phrase_phase: f64,
    },
    /// DJ beat trigger (fired on each beat)
    DjBeat {
        deck: u8,
        beat_number: u64,
        is_downbeat: bool,
    },
    /// DJ command from console
    DjCommand(crate::ConsoleCommand),
    /// DJ library tracks response
    DjLibraryTracks(Vec<crate::DjTrackInfo>),
    /// DJ deck loaded event
    DjDeckLoaded {
        deck: u8,
        track_id: i64,
        title: String,
        artist: Option<String>,
        duration_seconds: f64,
        bpm: Option<f64>,
    },
    /// DJ deck state changed
    DjDeckStateChanged {
        deck: u8,
        is_playing: bool,
        position_seconds: f64,
        bpm: Option<f64>,
    },
    /// DJ cue point set
    DjCuePointSet {
        deck: u8,
        position_seconds: f64,
    },
    /// DJ waveform progress (streaming analysis)
    DjWaveformProgress {
        deck: u8,
        samples: Vec<f32>,
        progress: f32,
    },
    /// DJ waveform loaded (complete)
    DjWaveformLoaded {
        deck: u8,
        samples: Vec<f32>,
        duration_seconds: f64,
    },
    /// DJ beat grid loaded
    DjBeatGridLoaded {
        deck: u8,
        beat_positions: Vec<f64>,
        first_beat_offset: f64,
        bpm: f64,
    },
    /// DJ master tempo changed
    DjMasterTempoChanged {
        deck: u8,
        enabled: bool,
    },
    /// DJ tempo range changed
    DjTempoRangeChanged {
        deck: u8,
        range: u8,
    },
    /// System events
    Shutdown,
}

/// Messages passed between modules and the module manager
#[derive(Debug)]
pub enum ModuleMessage {
    Event(ModuleEvent),
    Status(String),
    Error(String),
}

/// Trait that all async modules must implement
#[async_trait]
pub trait AsyncModule: Send + Sync {
    /// Get the unique identifier for this module
    fn id(&self) -> ModuleId;

    /// Initialize the module (called once at startup)
    async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Start the module's main loop
    async fn run(
        &mut self,
        mut rx: mpsc::Receiver<ModuleEvent>,
        tx: mpsc::Sender<ModuleMessage>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Shutdown the module gracefully
    async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Get the module's status
    fn status(&self) -> HashMap<String, String>;
}
