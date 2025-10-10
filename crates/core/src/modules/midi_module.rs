use std::collections::HashMap;

use async_trait::async_trait;
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use tokio::sync::mpsc;

use super::traits::{AsyncModule, ModuleEvent, ModuleId, ModuleMessage};
use crate::midi::midi::MidiMessage;

pub struct MidiModule {
    device_name: String,
    input_connection: Option<MidiInputConnection<()>>,
    output_connection: Option<MidiOutputConnection>,
    midi_sender: Option<mpsc::Sender<ModuleMessage>>,
    status: HashMap<String, String>,
}

impl MidiModule {
    pub fn new(device_name: String) -> Self {
        Self {
            device_name,
            input_connection: None,
            output_connection: None,
            midi_sender: None,
            status: HashMap::new(),
        }
    }

    async fn connect_midi(
        &mut self,
        tx: mpsc::Sender<ModuleMessage>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let midi_in = MidiInput::new("halo_async_controller")?;
        let midi_out = MidiOutput::new("halo_async_controller")?;

        // Find the device port for input
        let in_port = midi_in
            .ports()
            .into_iter()
            .find(|port| {
                midi_in
                    .port_name(port)
                    .map(|name| name.contains(&self.device_name))
                    .unwrap_or(false)
            })
            .ok_or_else(|| format!("{} input not found", self.device_name))?;

        let tx_clone = tx.clone();
        let connection = midi_in
            .connect(
                &in_port,
                "async-midi-input",
                move |_timestamp, message, _| {
                    if message.len() >= 3 {
                        let midi_msg = match message[0] & 0xF0 {
                            0xF8 => Some(MidiMessage::Clock),
                            0x90 => {
                                // Note On
                                if message[2] > 0 {
                                    Some(MidiMessage::NoteOn(message[1], message[2]))
                                } else {
                                    Some(MidiMessage::NoteOff(message[1]))
                                }
                            }
                            0x80 => Some(MidiMessage::NoteOff(message[1])),
                            0xB0 => Some(MidiMessage::ControlChange(message[1], message[2])),
                            _ => None,
                        };

                        if let Some(midi_msg) = midi_msg {
                            let event = ModuleEvent::MidiInput(midi_msg);

                            // Since we're in a callback, we need to use try_send
                            // to avoid blocking if the channel is full
                            if let Err(e) = tx_clone.try_send(ModuleMessage::Event(event)) {
                                log::warn!("Failed to send MIDI message: {}", e);
                            }
                        }
                    }
                },
                (),
            )
            .map_err(|_| "Failed to connect MIDI input")?;

        // Find the device port for output
        let out_port = midi_out
            .ports()
            .into_iter()
            .find(|port| {
                midi_out
                    .port_name(port)
                    .map(|name| name.contains(&self.device_name))
                    .unwrap_or(false)
            })
            .ok_or_else(|| format!("{} output not found", self.device_name))?;

        let output_connection = midi_out
            .connect(&out_port, "async-midi-output")
            .map_err(|_| "Failed to connect MIDI output")?;

        self.input_connection = Some(connection);
        self.output_connection = Some(output_connection);
        self.midi_sender = Some(tx);

        self.status
            .insert("input_connected".to_string(), "true".to_string());
        self.status
            .insert("output_connected".to_string(), "true".to_string());
        self.status
            .insert("device".to_string(), self.device_name.clone());

        Ok(())
    }

    pub fn send_midi_message(&mut self, data: &[u8]) -> Result<(), String> {
        if let Some(output) = &mut self.output_connection {
            output
                .send(data)
                .map_err(|e| format!("Failed to send MIDI: {}", e))?;
            Ok(())
        } else {
            Err("MIDI output not connected".to_string())
        }
    }
}

#[async_trait]
impl AsyncModule for MidiModule {
    fn id(&self) -> ModuleId {
        ModuleId::Midi
    }

    async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Initializing MIDI module for device: {}", self.device_name);

        self.status
            .insert("device_name".to_string(), self.device_name.clone());
        self.status
            .insert("status".to_string(), "initialized".to_string());
        self.status
            .insert("input_connected".to_string(), "false".to_string());
        self.status
            .insert("output_connected".to_string(), "false".to_string());

        Ok(())
    }

    async fn run(
        &mut self,
        mut rx: mpsc::Receiver<ModuleEvent>,
        tx: mpsc::Sender<ModuleMessage>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!("MIDI module starting for device: {}", self.device_name);

        // Connect to MIDI device
        match self.connect_midi(tx.clone()).await {
            Ok(_) => {
                log::info!("MIDI device '{}' connected successfully", self.device_name);
                let _ = tx
                    .send(ModuleMessage::Status(format!(
                        "MIDI device '{}' connected",
                        self.device_name
                    )))
                    .await;
            }
            Err(e) => {
                let error_msg = format!(
                    "Failed to connect MIDI device '{}': {}",
                    self.device_name, e
                );
                log::error!("{}", error_msg);
                let _ = tx.send(ModuleMessage::Error(error_msg)).await;

                // Continue running even if MIDI connection fails
                // This allows the system to run without MIDI hardware
            }
        }

        // Main event loop
        while let Some(event) = rx.recv().await {
            match event {
                ModuleEvent::Shutdown => {
                    log::info!("MIDI module received shutdown signal");
                    break;
                }
                _ => {
                    // MIDI module primarily handles input via the callback
                    // Other events are ignored for now, but could be extended
                    // to handle MIDI output commands in the future
                }
            }
        }

        log::info!("MIDI module shutting down");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Drop connections to properly close MIDI ports
        self.input_connection = None;
        self.output_connection = None;

        self.status
            .insert("status".to_string(), "shutdown".to_string());
        self.status
            .insert("input_connected".to_string(), "false".to_string());
        self.status
            .insert("output_connected".to_string(), "false".to_string());

        log::info!("MIDI module shutdown complete");
        Ok(())
    }

    fn status(&self) -> HashMap<String, String> {
        self.status.clone()
    }
}
