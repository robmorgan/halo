//! Halo DJ Module
//!
//! DJ functionality for Halo lighting console with dual-deck playback,
//! beat analysis, and lighting integration.
//!
//! # Features
//!
//! - Two deck audio playback with separate stereo outputs (external mixer mode)
//! - BPM detection and beat grid analysis
//! - SQLite library for track management
//! - Hot cues and cue points
//! - Tempo sync between decks
//! - MIDI controller support (TRAKTOR Z1 MK1)
//! - Lighting integration via RhythmState sync

pub mod deck;
pub mod library;
pub mod midi;
pub mod module;

// Re-export main types
pub use deck::{Deck, DeckId, DeckState};
pub use library::{BeatGrid, HotCue, Track, TrackId, TrackWaveform};
pub use module::{DjCommand, DjEvent, DjModule};
