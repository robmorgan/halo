//! Ableton Push 2 integration for Halo lighting console.
//!
//! This crate provides full Push 2 support including:
//! - USB display driver for the 960x160 LCD
//! - MIDI control for pads, encoders, and buttons
//! - LED feedback reflecting console state
//!
//! # Architecture
//!
//! The Push 2 is controlled via two interfaces:
//! - **USB**: For the LCD display (vendor ID 0x2982, product ID 0x1967)
//! - **MIDI**: For pads, encoders, buttons, and LED feedback
//!
//! # Pad Layout
//!
//! The 8x8 pad grid is split between DJ and lighting:
//! - Top 4 rows (notes 68-99): DJ controls (hot cues, transport, sync)
//! - Bottom 4 rows (notes 36-67): Lighting controls (cue triggers, fixtures)

pub mod display;
pub mod midi;
pub mod module;

pub use module::Push2Module;
