use eframe::egui;
use halo_core::ConsoleCommand;
use tokio::sync::mpsc;

use crate::state::ConsoleState;

#[derive(Debug, Clone, Copy, PartialEq)]
enum SettingsTab {
    General,
    Audio,
    Midi,
    Outputs,
}

#[derive(Clone)]
pub struct SettingsPanel {
    pub open: bool,
    active_tab: SettingsTab,

    // General settings
    pub target_fps: String,
    pub enable_autosave: bool,
    pub autosave_interval: String,

    // Audio settings
    pub audio_device: String,
    pub audio_buffer_size: String,
    pub audio_sample_rate: String,

    // MIDI settings
    pub midi_enabled: bool,
    pub midi_device: String,
    pub midi_channel: String,

    // Output settings (DMX/Art-Net)
    pub dmx_enabled: bool,
    pub dmx_broadcast: bool,
    pub dmx_source_ip: String,
    pub dmx_dest_ip: String,
    pub dmx_port: String,
    pub wled_enabled: bool,
    pub wled_ip: String,
}

impl Default for SettingsPanel {
    fn default() -> Self {
        Self {
            open: false,
            active_tab: SettingsTab::General,

            // General defaults
            target_fps: "60".to_string(),
            enable_autosave: false,
            autosave_interval: "300".to_string(),

            // Audio defaults
            audio_device: "Default".to_string(),
            audio_buffer_size: "512".to_string(),
            audio_sample_rate: "48000".to_string(),

            // MIDI defaults
            midi_enabled: false,
            midi_device: "None".to_string(),
            midi_channel: "1".to_string(),

            // Output defaults
            dmx_enabled: true,
            dmx_broadcast: false,
            dmx_source_ip: "192.168.1.100".to_string(),
            dmx_dest_ip: "192.168.1.200".to_string(),
            dmx_port: "6454".to_string(),
            wled_enabled: false,
            wled_ip: "192.168.1.50".to_string(),
        }
    }
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn render(
        &mut self,
        ctx: &egui::Context,
        _state: &ConsoleState,
        _console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        if !self.open {
            return;
        }

        let mut open = self.open;

        egui::Window::new("Settings")
            .open(&mut open)
            .default_width(600.0)
            .default_height(500.0)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                self.render_content(ui, _console_tx);
            });

        self.open = open;
    }

    fn render_content(
        &mut self,
        ui: &mut egui::Ui,
        _console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.active_tab, SettingsTab::General, "General");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Audio, "Audio");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Midi, "MIDI");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Outputs, "Outputs");
        });

        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| match self.active_tab {
            SettingsTab::General => self.render_general_tab(ui, _console_tx),
            SettingsTab::Audio => self.render_audio_tab(ui, _console_tx),
            SettingsTab::Midi => self.render_midi_tab(ui, _console_tx),
            SettingsTab::Outputs => self.render_outputs_tab(ui, _console_tx),
        });

        ui.separator();

        // Footer buttons
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() {
                    self.open = false;
                }
                if ui.button("Apply").clicked() {
                    // Apply settings
                    self.apply_settings(_console_tx);
                }
            });
        });
    }

    fn render_general_tab(
        &mut self,
        ui: &mut egui::Ui,
        _console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.heading("General Settings");
        ui.add_space(10.0);

        egui::Grid::new("general_settings_grid")
            .num_columns(2)
            .spacing([40.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label("Target FPS:");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.target_fps).desired_width(100.0));
                    ui.label("(UI refresh rate)");
                });
                ui.end_row();

                ui.label("Auto-save:");
                ui.checkbox(&mut self.enable_autosave, "Enable automatic show saving");
                ui.end_row();

                if self.enable_autosave {
                    ui.label("Auto-save interval:");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.autosave_interval)
                                .desired_width(100.0),
                        );
                        ui.label("seconds");
                    });
                    ui.end_row();
                }
            });

        ui.add_space(20.0);
        ui.separator();
        ui.add_space(10.0);

        ui.label("Application Information");
        ui.add_space(5.0);
        ui.label(format!("Version: {}", env!("CARGO_PKG_VERSION")));
        ui.label("Halo Lighting Console");
    }

    fn render_audio_tab(
        &mut self,
        ui: &mut egui::Ui,
        _console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.heading("Audio Settings");
        ui.add_space(10.0);

        egui::Grid::new("audio_settings_grid")
            .num_columns(2)
            .spacing([40.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label("Audio Device:");
                egui::ComboBox::from_id_salt("audio_device_combo")
                    .selected_text(&self.audio_device)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.audio_device,
                            "Default".to_string(),
                            "Default",
                        );
                        ui.selectable_value(
                            &mut self.audio_device,
                            "System Audio".to_string(),
                            "System Audio",
                        );
                        // In a real implementation, enumerate actual audio devices here
                        ui.label("(More devices would be listed here)");
                    });
                ui.end_row();

                ui.label("Buffer Size:");
                egui::ComboBox::from_id_salt("audio_buffer_size")
                    .selected_text(&self.audio_buffer_size)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.audio_buffer_size, "128".to_string(), "128");
                        ui.selectable_value(&mut self.audio_buffer_size, "256".to_string(), "256");
                        ui.selectable_value(&mut self.audio_buffer_size, "512".to_string(), "512");
                        ui.selectable_value(
                            &mut self.audio_buffer_size,
                            "1024".to_string(),
                            "1024",
                        );
                        ui.selectable_value(
                            &mut self.audio_buffer_size,
                            "2048".to_string(),
                            "2048",
                        );
                    });
                ui.end_row();

                ui.label("Sample Rate:");
                egui::ComboBox::from_id_salt("audio_sample_rate")
                    .selected_text(format!("{} Hz", self.audio_sample_rate))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.audio_sample_rate,
                            "44100".to_string(),
                            "44100 Hz",
                        );
                        ui.selectable_value(
                            &mut self.audio_sample_rate,
                            "48000".to_string(),
                            "48000 Hz",
                        );
                        ui.selectable_value(
                            &mut self.audio_sample_rate,
                            "96000".to_string(),
                            "96000 Hz",
                        );
                    });
                ui.end_row();
            });

        ui.add_space(10.0);
        ui.label("Note: Audio device changes will take effect after restart.");
    }

    fn render_midi_tab(
        &mut self,
        ui: &mut egui::Ui,
        _console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.heading("MIDI Settings");
        ui.add_space(10.0);

        egui::Grid::new("midi_settings_grid")
            .num_columns(2)
            .spacing([40.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label("MIDI Input:");
                ui.checkbox(&mut self.midi_enabled, "Enable MIDI input");
                ui.end_row();

                if self.midi_enabled {
                    ui.label("MIDI Device:");
                    egui::ComboBox::from_id_salt("midi_device_combo")
                        .selected_text(&self.midi_device)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.midi_device, "None".to_string(), "None");
                            ui.selectable_value(
                                &mut self.midi_device,
                                "Virtual MIDI".to_string(),
                                "Virtual MIDI",
                            );
                            // In a real implementation, enumerate actual MIDI devices here
                            ui.label("(Available MIDI devices would be listed here)");
                        });
                    ui.end_row();

                    ui.label("MIDI Channel:");
                    egui::ComboBox::from_id_salt("midi_channel")
                        .selected_text(format!("Channel {}", self.midi_channel))
                        .show_ui(ui, |ui| {
                            for i in 1..=16 {
                                ui.selectable_value(
                                    &mut self.midi_channel,
                                    i.to_string(),
                                    format!("Channel {i}"),
                                );
                            }
                        });
                    ui.end_row();
                }
            });

        ui.add_space(10.0);
        ui.label("MIDI Learn and mapping features coming soon.");
    }

    fn render_outputs_tab(
        &mut self,
        ui: &mut egui::Ui,
        _console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.heading("Output Settings");
        ui.add_space(10.0);

        // DMX / Art-Net Section
        ui.label("DMX Output (Art-Net)");
        ui.separator();
        ui.add_space(5.0);

        egui::Grid::new("dmx_settings_grid")
            .num_columns(2)
            .spacing([40.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label("DMX Output:");
                ui.checkbox(&mut self.dmx_enabled, "Enable DMX output");
                ui.end_row();

                if self.dmx_enabled {
                    ui.label("Mode:");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.dmx_broadcast, true, "Broadcast");
                        ui.radio_value(&mut self.dmx_broadcast, false, "Unicast");
                    });
                    ui.end_row();

                    ui.label("Source IP:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.dmx_source_ip).desired_width(150.0),
                    );
                    ui.end_row();

                    if !self.dmx_broadcast {
                        ui.label("Destination IP:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.dmx_dest_ip).desired_width(150.0),
                        );
                        ui.end_row();
                    }

                    ui.label("Port:");
                    ui.add(egui::TextEdit::singleline(&mut self.dmx_port).desired_width(100.0));
                    ui.end_row();
                }
            });

        ui.add_space(20.0);

        // WLED Section
        ui.label("WLED Support");
        ui.separator();
        ui.add_space(5.0);

        egui::Grid::new("wled_settings_grid")
            .num_columns(2)
            .spacing([40.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label("WLED Output:");
                ui.checkbox(&mut self.wled_enabled, "Enable WLED support (coming soon)");
                ui.end_row();

                if self.wled_enabled {
                    ui.label("WLED IP Address:");
                    ui.add(egui::TextEdit::singleline(&mut self.wled_ip).desired_width(150.0));
                    ui.end_row();
                }
            });

        ui.add_space(10.0);
        ui.label("Note: Output changes require restart to take effect.");
    }

    fn apply_settings(&self, _console_tx: &mpsc::UnboundedSender<ConsoleCommand>) {
        // TODO: Implement settings application
        // For now, settings are mostly UI state
        // Future implementation would send commands to update runtime settings
        println!("Settings applied (placeholder)");

        // Example of what could be done:
        // if let Ok(fps) = self.target_fps.parse::<u32>() {
        //     let _ = console_tx.send(ConsoleCommand::SetTargetFps { fps });
        // }
    }
}
