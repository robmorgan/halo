use crate::cue::StaticValue;
use midir::{MidiInput, MidiInputConnection};
use std::collections::HashMap;
use std::error::Error;
use std::sync::mpsc::{channel, Receiver, Sender};

// Represent a MIDI override (could be from keys, pads, or controls)
pub struct MidiOverride {
    pub static_values: Vec<StaticValue>,
    pub velocity_sensitive: bool, // Whether the override responds to velocity
}

// MIDI message types we care about
pub enum MidiMessage {
    NoteOn(u8, u8),        // (note, velocity)
    NoteOff(u8),           // note
    ControlChange(u8, u8), // (controller number, value)
}
