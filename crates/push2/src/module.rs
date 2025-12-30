//! Push2Module - Async module for Ableton Push 2 integration.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use halo_core::{AsyncModule, ConsoleCommand, ModuleEvent, ModuleId, ModuleMessage};
use halo_dj::deck::DeckId;
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use tokio::sync::mpsc;

use crate::display::{DisplayRenderer, FrameBuffer, Push2Display};
use crate::midi::{LedState, Push2Mapping};

/// Push 2 operating mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Push2Mode {
    /// Normal operation
    Normal,
    /// Shift button held - alternate functions
    Shift,
    /// Settings/configuration mode
    Settings,
}

/// State for DJ deck display
#[derive(Debug, Clone, Default)]
pub struct DeckDisplayState {
    pub title: String,
    pub artist: String,
    pub duration_seconds: f64,
    pub position_seconds: f64,
    pub bpm: f64,
    pub is_playing: bool,
    pub is_master: bool,
    pub sync_enabled: bool,
    pub cue_point: Option<f64>,
    pub hot_cues: [Option<f64>; 4],
}

/// State for lighting display
#[derive(Debug, Clone, Default)]
pub struct LightingDisplayState {
    pub current_cue_list: String,
    pub current_cue_index: usize,
    pub total_cues: usize,
    pub selected_fixtures: Vec<usize>,
}

/// Ableton Push 2 controller module.
///
/// Provides integration with the Push 2 hardware including:
/// - USB display for waveforms and track info
/// - MIDI pads for DJ and lighting control
/// - LED feedback for visual state indication
pub struct Push2Module {
    /// USB display connection (None if not connected)
    display: Option<Push2Display>,

    /// Frame buffer for display rendering
    frame_buffer: FrameBuffer,

    /// Display renderer
    renderer: DisplayRenderer,

    /// MIDI input connection
    midi_input: Option<MidiInputConnection<mpsc::UnboundedSender<Vec<u8>>>>,

    /// MIDI output connection for LED feedback
    midi_output: Option<MidiOutputConnection>,

    /// LED state
    led_state: LedState,

    /// DJ deck A state
    deck_a: DeckDisplayState,

    /// DJ deck B state
    deck_b: DeckDisplayState,

    /// Lighting state
    lighting_state: LightingDisplayState,

    /// Current operating mode
    mode: Push2Mode,

    /// Shift button held
    shift_held: bool,

    /// Module status
    status: HashMap<String, String>,

    /// MIDI message receiver (from callback)
    midi_rx: Option<mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Push2Module {
    /// Create a new Push2Module.
    pub fn new() -> Self {
        Self {
            display: None,
            frame_buffer: FrameBuffer::new(),
            renderer: DisplayRenderer::new(),
            midi_input: None,
            midi_output: None,
            led_state: LedState::new(),
            deck_a: DeckDisplayState::default(),
            deck_b: DeckDisplayState::default(),
            lighting_state: LightingDisplayState::default(),
            mode: Push2Mode::Normal,
            shift_held: false,
            status: HashMap::new(),
            midi_rx: None,
        }
    }

    /// Try to connect to the Push 2 display via USB.
    fn connect_display(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match Push2Display::new() {
            Ok(display) => {
                self.display = Some(display);
                self.status
                    .insert("display".to_string(), "connected".to_string());
                tracing::info!("Push 2 display connected");
                Ok(())
            }
            Err(e) => {
                self.display = None;
                self.status
                    .insert("display".to_string(), "not_connected".to_string());
                tracing::warn!("Push 2 display not available: {}. MIDI-only mode.", e);
                // Don't fail - continue with MIDI only
                Ok(())
            }
        }
    }

    /// Try to connect to the Push 2 MIDI ports.
    fn connect_midi(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Create MIDI input
        let midi_in = MidiInput::new("halo_push2_in")?;

        // Find Push 2 input port
        let in_ports = midi_in.ports();
        let in_port = in_ports.iter().find(|p| {
            midi_in
                .port_name(p)
                .map(|n| n.contains("Ableton Push 2") || n.contains("Push 2"))
                .unwrap_or(false)
        });

        let in_port = match in_port {
            Some(p) => p.clone(),
            None => {
                self.status
                    .insert("midi_input".to_string(), "not_found".to_string());
                return Err("Push 2 MIDI input not found".into());
            }
        };

        // Create channel for MIDI messages
        let (tx, rx) = mpsc::unbounded_channel();
        self.midi_rx = Some(rx);

        // Connect MIDI input
        let connection = midi_in.connect(
            &in_port,
            "push2-input",
            move |_timestamp, message, tx| {
                // Send raw MIDI bytes to async handler
                let _ = tx.send(message.to_vec());
            },
            tx,
        )?;

        self.midi_input = Some(connection);
        self.status
            .insert("midi_input".to_string(), "connected".to_string());

        // Create MIDI output
        let midi_out = MidiOutput::new("halo_push2_out")?;
        let out_ports = midi_out.ports();
        let out_port = out_ports.iter().find(|p| {
            midi_out
                .port_name(p)
                .map(|n| n.contains("Ableton Push 2") || n.contains("Push 2"))
                .unwrap_or(false)
        });

        if let Some(port) = out_port {
            let connection = midi_out.connect(port, "push2-output")?;
            self.midi_output = Some(connection);
            self.status
                .insert("midi_output".to_string(), "connected".to_string());
        } else {
            self.status
                .insert("midi_output".to_string(), "not_found".to_string());
            tracing::warn!("Push 2 MIDI output not found - LED feedback disabled");
        }

        tracing::info!("Push 2 MIDI connected");
        Ok(())
    }

    /// Handle incoming MIDI message.
    fn handle_midi_message(&mut self, message: &[u8]) -> Option<ModuleEvent> {
        if message.is_empty() {
            return None;
        }

        let status = message[0] & 0xF0;
        match status {
            0x90 => {
                // Note On
                if message.len() >= 3 {
                    let note = message[1];
                    let velocity = message[2];
                    if velocity > 0 {
                        self.handle_pad_press(note, velocity)
                    } else {
                        self.handle_pad_release(note)
                    }
                } else {
                    None
                }
            }
            0x80 => {
                // Note Off
                if message.len() >= 2 {
                    let note = message[1];
                    self.handle_pad_release(note)
                } else {
                    None
                }
            }
            0xB0 => {
                // Control Change
                if message.len() >= 3 {
                    let cc = message[1];
                    let value = message[2];
                    self.handle_cc(cc, value)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Handle pad press (Note On with velocity > 0).
    fn handle_pad_press(&mut self, note: u8, velocity: u8) -> Option<ModuleEvent> {
        // Check for shift button
        if note == Push2Mapping::BUTTON_SHIFT {
            self.shift_held = true;
            return None;
        }

        // Translate pad press to command
        if let Some(command) = Push2Mapping::translate_note_on(note, velocity, self.shift_held) {
            return Some(ModuleEvent::DjCommand(command));
        }

        // Check for lighting cue triggers (Row 4: notes 60-67, Row 3: notes 52-59)
        if (52..=67).contains(&note) {
            let cue_index = (note - 52) as usize;
            return Some(ModuleEvent::DjCommand(ConsoleCommand::PlayCue {
                list_index: 0,
                cue_index,
            }));
        }

        // Check for fixture selection (Row 2: notes 44-51)
        if (44..=51).contains(&note) {
            let fixture_id = (note - 44) as usize;
            return Some(ModuleEvent::DjCommand(ConsoleCommand::AddSelectedFixture {
                fixture_id,
            }));
        }

        // Lighting transport (Row 1: notes 36-43)
        match note {
            40 => {
                // GO button
                return Some(ModuleEvent::DjCommand(ConsoleCommand::NextCue {
                    list_index: 0,
                }));
            }
            41 => {
                // STOP button
                return Some(ModuleEvent::DjCommand(ConsoleCommand::StopCue {
                    list_index: 0,
                }));
            }
            42 => {
                // PREV button
                return Some(ModuleEvent::DjCommand(ConsoleCommand::PrevCue {
                    list_index: 0,
                }));
            }
            43 => {
                // NEXT cue list
                return Some(ModuleEvent::DjCommand(ConsoleCommand::SelectNextCueList));
            }
            _ => {}
        }

        None
    }

    /// Handle pad release (Note Off or Note On with velocity 0).
    fn handle_pad_release(&mut self, note: u8) -> Option<ModuleEvent> {
        // Check for shift button release
        if note == Push2Mapping::BUTTON_SHIFT {
            self.shift_held = false;
            return None;
        }

        // Translate pad release to command (for CuePreview, etc.)
        Push2Mapping::translate_note_off(note).map(ModuleEvent::DjCommand)
    }

    /// Handle control change (encoders, faders).
    fn handle_cc(&mut self, cc: u8, value: u8) -> Option<ModuleEvent> {
        Push2Mapping::translate_cc(cc, value).map(ModuleEvent::DjCommand)
    }

    /// Update deck display state from events.
    fn update_deck_state(&mut self, deck: u8, is_playing: bool, position_seconds: f64) {
        let state = if deck == 0 {
            &mut self.deck_a
        } else {
            &mut self.deck_b
        };
        state.is_playing = is_playing;
        state.position_seconds = position_seconds;

        // Update LED state
        let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
        self.led_state.update_transport(deck_id, is_playing);
    }

    /// Update deck loaded state.
    fn update_deck_loaded(
        &mut self,
        deck: u8,
        title: String,
        artist: Option<String>,
        duration: f64,
        bpm: Option<f64>,
    ) {
        let state = if deck == 0 {
            &mut self.deck_a
        } else {
            &mut self.deck_b
        };
        state.title = title;
        state.artist = artist.unwrap_or_default();
        state.duration_seconds = duration;
        state.bpm = bpm.unwrap_or(0.0);
    }

    /// Render the display frame.
    fn render_display(&mut self) {
        self.renderer
            .render(&mut self.frame_buffer, &self.deck_a, &self.deck_b);
    }

    /// Send display frame to Push 2.
    fn send_display_frame(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref mut display) = self.display {
            display.send_frame(&self.frame_buffer)?;
        }
        Ok(())
    }

    /// Send LED state to Push 2 via MIDI.
    fn send_led_state(&mut self) {
        if let Some(ref mut output) = self.midi_output {
            for message in self.led_state.to_midi_messages() {
                let _ = output.send(&message);
            }
        }
    }
}

impl Default for Push2Module {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AsyncModule for Push2Module {
    fn id(&self) -> ModuleId {
        ModuleId::Push2
    }

    async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!("Initializing Push 2 module");

        // Try to connect display (non-fatal if it fails)
        let _ = self.connect_display();

        // Connect MIDI (required)
        self.connect_midi()?;

        // Initialize LED state
        self.send_led_state();

        // Send initial display frame if connected
        if self.display.is_some() {
            self.render_display();
            let _ = self.send_display_frame();
        }

        self.status
            .insert("state".to_string(), "initialized".to_string());
        Ok(())
    }

    async fn run(
        &mut self,
        mut rx: mpsc::Receiver<ModuleEvent>,
        tx: mpsc::Sender<ModuleMessage>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!("Push 2 module running");
        self.status
            .insert("state".to_string(), "running".to_string());

        // Display refresh interval (~30fps)
        let mut display_interval = tokio::time::interval(Duration::from_millis(33));

        // LED update interval (slower, ~10fps)
        let mut led_interval = tokio::time::interval(Duration::from_millis(100));

        // Take ownership of MIDI receiver
        let mut midi_rx = self.midi_rx.take();

        loop {
            tokio::select! {
                // Handle module events
                Some(event) = rx.recv() => {
                    match event {
                        ModuleEvent::Shutdown => {
                            tracing::info!("Push 2 module received shutdown");
                            break;
                        }

                        ModuleEvent::DjDeckStateChanged { deck, is_playing, position_seconds, bpm: _ } => {
                            self.update_deck_state(deck, is_playing, position_seconds);
                        }

                        ModuleEvent::DjDeckLoaded { deck, title, artist, duration_seconds, bpm, .. } => {
                            self.update_deck_loaded(deck, title, artist, duration_seconds, bpm);
                        }

                        ModuleEvent::DjRhythmSync { bpm, beat_phase, .. } => {
                            // Update BPM display for master deck
                            if self.deck_a.is_master {
                                self.deck_a.bpm = bpm;
                            } else if self.deck_b.is_master {
                                self.deck_b.bpm = bpm;
                            }
                            // Could pulse LEDs on beat here
                            let _ = beat_phase;
                        }

                        _ => {}
                    }
                }

                // Handle MIDI input
                Some(message) = async {
                    if let Some(ref mut rx) = midi_rx {
                        rx.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    if let Some(event) = self.handle_midi_message(&message) {
                        // Use try_send to avoid blocking the event loop
                        // If the channel is full, the message is dropped (acceptable for MIDI)
                        if let Err(e) = tx.try_send(ModuleMessage::Event(event)) {
                            tracing::debug!("Failed to send MIDI event (channel full): {}", e);
                        }
                    }
                }

                // Display refresh
                _ = display_interval.tick() => {
                    if self.display.is_some() {
                        self.render_display();
                        if let Err(e) = self.send_display_frame() {
                            tracing::warn!("Display update failed: {}", e);
                        }
                    }
                }

                // LED refresh
                _ = led_interval.tick() => {
                    self.send_led_state();
                }
            }
        }

        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!("Shutting down Push 2 module");

        // Clear display
        if let Some(ref mut display) = self.display {
            self.frame_buffer.clear();
            let _ = display.send_frame(&self.frame_buffer);
        }

        // Turn off all LEDs
        self.led_state.clear();
        self.send_led_state();

        // Close connections (dropped automatically)
        self.midi_input = None;
        self.midi_output = None;
        self.display = None;

        self.status
            .insert("state".to_string(), "shutdown".to_string());
        Ok(())
    }

    fn status(&self) -> HashMap<String, String> {
        self.status.clone()
    }
}
