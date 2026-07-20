//! Halo's lighting domain: cue data, the fixture rig, and the programmer
//! override layer. Deliberately UI- and audio-free — the egui painters and
//! the (future) DMX engine thread are both consumers of this crate.
//! `programmer::resolve()` is the single merge point, a pure function of
//! (cue set, programmer state, playhead).

pub mod artnet;
pub mod cues;
pub mod fixture;
pub mod fixture_library;
pub mod output;
pub mod programmer;
