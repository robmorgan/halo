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
    /// SMPTE timecode sync
    SmpteSync {
        timecode: crate::timecode::timecode::TimeCode,
    },
    /// MIDI input events
    MidiInput(crate::midi::midi::MidiMessage),
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
