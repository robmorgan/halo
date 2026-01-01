//! Library module for track management, analysis, and database operations.

mod types;

pub mod analysis;
pub mod database;
pub mod import;

pub use analysis::{analyze_file, analyze_file_streaming, AnalysisConfig, AnalysisResult};
pub use database::LibraryDatabase;
pub use import::{
    import_and_analyze_directory, import_and_analyze_file, import_directory, import_file,
    is_supported_audio_file, supported_extensions, ImportResult,
};
pub use types::{
    AudioFormat, BeatGrid, FrequencyBands, HotCue, MasterTempoMode, TempoRange, Track, TrackId,
    TrackWaveform, WAVEFORM_VERSION_COLORED, WAVEFORM_VERSION_LEGACY,
};
