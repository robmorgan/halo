use crate::StaticValue;

#[derive(Debug, Clone)]
pub enum MidiAction {
    StaticValues(Vec<StaticValue>),
    TriggerCue(String), // Cue name to trigger
}

// Represent a MIDI override (could be from keys, pads, or controls)
#[derive(Debug, Clone)]
pub struct MidiOverride {
    pub action: MidiAction,
}

// MIDI message types we care about
#[derive(Debug, Clone)]
pub enum MidiMessage {
    NoteOn(u8, u8),        // (note, velocity)
    NoteOff(u8),           // note
    ControlChange(u8, u8), // (controller number, value)
    Clock,                 // MIDI clock messages
}
