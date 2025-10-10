pub mod audio_module;
pub mod dmx_module;
pub mod midi_module;
pub mod module_manager;
pub mod smpte_module;
pub mod traits;

// Re-export for convenience
pub use audio_module::AudioModule;
pub use dmx_module::DmxModule;
pub use midi_module::MidiModule;
pub use module_manager::ModuleManager;
pub use smpte_module::SmpteModule;
pub use traits::{AsyncModule, ModuleEvent, ModuleId, ModuleMessage};
