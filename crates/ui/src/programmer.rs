use std::collections::HashMap;
use tokio::sync::mpsc;

use eframe::egui::{self, Color32};
use halo_core::{ConsoleCommand, EffectType};
use crate::state::ConsoleState;

pub struct ProgrammerState {
    pub new_cue_name: String,
    selected_fixtures: Vec<usize>,
    params: HashMap<String, f32>,
    color_presets: Vec<Color32>,
}

impl Default for ProgrammerState {
    fn default() -> Self {
        let mut params = HashMap::new();

        // Initialize default parameter values
        params.insert("dimmer".to_string(), 100.0);
        params.insert("strobe".to_string(), 0.0);
        params.insert("red".to_string(), 255.0);
        params.insert("green".to_string(), 127.0);
        params.insert("blue".to_string(), 0.0);
        params.insert("white".to_string(), 0.0);
        params.insert("pan".to_string(), 180.0);
        params.insert("tilt".to_string(), 90.0);
        params.insert("focus".to_string(), 50.0);
        params.insert("zoom".to_string(), 75.0);
        params.insert("gobo_rotation".to_string(), 0.0);
        params.insert("gobo_selection".to_string(), 2.0);

        // Initialize color presets
        let color_presets = vec![
            Color32::from_rgb(255, 0, 0),     // Red
            Color32::from_rgb(255, 127, 0),   // Orange
            Color32::from_rgb(255, 255, 0),   // Yellow
            Color32::from_rgb(0, 255, 0),     // Green
            Color32::from_rgb(0, 255, 255),   // Cyan
            Color32::from_rgb(0, 0, 255),     // Blue
            Color32::from_rgb(139, 0, 255),   // Purple
            Color32::from_rgb(255, 255, 255), // White
        ];

        Self {
            new_cue_name: String::new(),
            selected_fixtures: Vec::new(),
            params,
            color_presets,
        }
    }
}

impl ProgrammerState {
    pub fn render(&mut self, ui: &mut eframe::egui::Ui, state: &ConsoleState, console_tx: &mpsc::UnboundedSender<ConsoleCommand>) {
        egui::CentralPanel::default().show(ui.ctx(), |ui| {
            ui.vertical(|ui| {
                // Header area with global controls
                ui.horizontal(|ui| {
                    ui.heading("PROGRAMMER");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("RECORD").clicked() {
                            // Record the current programmer state to a cue
                            if !self.new_cue_name.is_empty() {
                                // TODO: Implement cue recording via message passing
                                ui.label("Cue recording not yet implemented");
                            }
                        }

                        if ui.button("CLEAR").clicked() {
                            // Clear the programmer
                            let _ = console_tx.send(ConsoleCommand::ClearProgrammer);
                        }

                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_cue_name)
                                .hint_text("New cue name...")
                                .desired_width(150.0),
                        );
                    });
                });

                ui.separator();

                // Fixture selection
                ui.heading("Fixtures");
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    for (idx, fixture) in state.fixtures.iter().enumerate() {
                        let is_selected = self.selected_fixtures.contains(&idx);
                        if ui.selectable_label(is_selected, &fixture.name).clicked() {
                            if is_selected {
                                self.selected_fixtures.retain(|&x| x != idx);
                            } else {
                                self.selected_fixtures.push(idx);
                            }
                        }
                    }
                });

                ui.separator();

                // Parameter controls
                ui.heading("Parameters");
                
                // Intensity controls
                egui::CollapsingHeader::new("Intensity")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Dimmer:");
                            let mut dimmer = *self.params.get("dimmer").unwrap_or(&100.0);
                            if ui.add(egui::Slider::new(&mut dimmer, 0.0..=100.0).text("Dimmer")).changed() {
                                self.params.insert("dimmer".to_string(), dimmer);
                                self.update_fixture_values(console_tx);
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Strobe:");
                            let mut strobe = *self.params.get("strobe").unwrap_or(&0.0);
                            if ui.add(egui::Slider::new(&mut strobe, 0.0..=255.0).text("Strobe")).changed() {
                                self.params.insert("strobe".to_string(), strobe);
                                self.update_fixture_values(console_tx);
                            }
                        });
                    });

                // Color controls
                egui::CollapsingHeader::new("Color")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Red:");
                            let mut red = *self.params.get("red").unwrap_or(&255.0);
                            if ui.add(egui::Slider::new(&mut red, 0.0..=255.0).text("Red")).changed() {
                                self.params.insert("red".to_string(), red);
                                self.update_fixture_values(console_tx);
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Green:");
                            let mut green = *self.params.get("green").unwrap_or(&127.0);
                            if ui.add(egui::Slider::new(&mut green, 0.0..=255.0).text("Green")).changed() {
                                self.params.insert("green".to_string(), green);
                                self.update_fixture_values(console_tx);
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Blue:");
                            let mut blue = *self.params.get("blue").unwrap_or(&0.0);
                            if ui.add(egui::Slider::new(&mut blue, 0.0..=255.0).text("Blue")).changed() {
                                self.params.insert("blue".to_string(), blue);
                                self.update_fixture_values(console_tx);
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("White:");
                            let mut white = *self.params.get("white").unwrap_or(&0.0);
                            if ui.add(egui::Slider::new(&mut white, 0.0..=255.0).text("White")).changed() {
                                self.params.insert("white".to_string(), white);
                                self.update_fixture_values(console_tx);
                            }
                        });

                        // Color presets
                        ui.add_space(10.0);
                        ui.label("Color Presets:");
                        ui.horizontal(|ui| {
                            for (i, color) in self.color_presets.iter().enumerate() {
                                let color_name = match i {
                                    0 => "Red",
                                    1 => "Orange", 
                                    2 => "Yellow",
                                    3 => "Green",
                                    4 => "Cyan",
                                    5 => "Blue",
                                    6 => "Purple",
                                    7 => "White",
                                    _ => "Unknown",
                                };
                                
                                if ui.button(color_name).clicked() {
                                    // Set RGB values based on preset
                                    match i {
                                        0 => { // Red
                                            self.params.insert("red".to_string(), 255.0);
                                            self.params.insert("green".to_string(), 0.0);
                                            self.params.insert("blue".to_string(), 0.0);
                                            self.params.insert("white".to_string(), 0.0);
                                        }
                                        1 => { // Orange
                                            self.params.insert("red".to_string(), 255.0);
                                            self.params.insert("green".to_string(), 127.0);
                                            self.params.insert("blue".to_string(), 0.0);
                                            self.params.insert("white".to_string(), 0.0);
                                        }
                                        2 => { // Yellow
                                            self.params.insert("red".to_string(), 255.0);
                                            self.params.insert("green".to_string(), 255.0);
                                            self.params.insert("blue".to_string(), 0.0);
                                            self.params.insert("white".to_string(), 0.0);
                                        }
                                        3 => { // Green
                                            self.params.insert("red".to_string(), 0.0);
                                            self.params.insert("green".to_string(), 255.0);
                                            self.params.insert("blue".to_string(), 0.0);
                                            self.params.insert("white".to_string(), 0.0);
                                        }
                                        4 => { // Cyan
                                            self.params.insert("red".to_string(), 0.0);
                                            self.params.insert("green".to_string(), 255.0);
                                            self.params.insert("blue".to_string(), 255.0);
                                            self.params.insert("white".to_string(), 0.0);
                                        }
                                        5 => { // Blue
                                            self.params.insert("red".to_string(), 0.0);
                                            self.params.insert("green".to_string(), 0.0);
                                            self.params.insert("blue".to_string(), 255.0);
                                            self.params.insert("white".to_string(), 0.0);
                                        }
                                        6 => { // Purple
                                            self.params.insert("red".to_string(), 139.0);
                                            self.params.insert("green".to_string(), 0.0);
                                            self.params.insert("blue".to_string(), 255.0);
                                            self.params.insert("white".to_string(), 0.0);
                                        }
                                        7 => { // White
                                            self.params.insert("red".to_string(), 255.0);
                                            self.params.insert("green".to_string(), 255.0);
                                            self.params.insert("blue".to_string(), 255.0);
                                            self.params.insert("white".to_string(), 255.0);
                                        }
                                        _ => {}
                                    }
                                    self.update_fixture_values(console_tx);
                                }
                            }
                        });
                    });

                // Position controls
                egui::CollapsingHeader::new("Position")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Pan:");
                            let mut pan = *self.params.get("pan").unwrap_or(&180.0);
                            if ui.add(egui::Slider::new(&mut pan, 0.0..=360.0).text("Pan")).changed() {
                                self.params.insert("pan".to_string(), pan);
                                self.update_fixture_values(console_tx);
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Tilt:");
                            let mut tilt = *self.params.get("tilt").unwrap_or(&90.0);
                            if ui.add(egui::Slider::new(&mut tilt, 0.0..=180.0).text("Tilt")).changed() {
                                self.params.insert("tilt".to_string(), tilt);
                                self.update_fixture_values(console_tx);
                            }
                        });
                    });

                // Beam controls
                egui::CollapsingHeader::new("Beam")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Focus:");
                            let mut focus = *self.params.get("focus").unwrap_or(&50.0);
                            if ui.add(egui::Slider::new(&mut focus, 0.0..=100.0).text("Focus")).changed() {
                                self.params.insert("focus".to_string(), focus);
                                self.update_fixture_values(console_tx);
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Zoom:");
                            let mut zoom = *self.params.get("zoom").unwrap_or(&75.0);
                            if ui.add(egui::Slider::new(&mut zoom, 0.0..=100.0).text("Zoom")).changed() {
                                self.params.insert("zoom".to_string(), zoom);
                                self.update_fixture_values(console_tx);
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Gobo Rotation:");
                            let mut gobo_rotation = *self.params.get("gobo_rotation").unwrap_or(&0.0);
                            if ui.add(egui::Slider::new(&mut gobo_rotation, 0.0..=360.0).text("Gobo Rotation")).changed() {
                                self.params.insert("gobo_rotation".to_string(), gobo_rotation);
                                self.update_fixture_values(console_tx);
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Gobo Selection:");
                            let mut gobo_selection = *self.params.get("gobo_selection").unwrap_or(&2.0);
                            if ui.add(egui::Slider::new(&mut gobo_selection, 1.0..=10.0).text("Gobo Selection")).changed() {
                                self.params.insert("gobo_selection".to_string(), gobo_selection);
                                self.update_fixture_values(console_tx);
                            }
                        });
                    });
            });
        });
    }

    fn update_fixture_values(&self, console_tx: &mpsc::UnboundedSender<ConsoleCommand>) {
        for &fixture_id in &self.selected_fixtures {
            for (channel, value) in &self.params {
                let _ = console_tx.send(ConsoleCommand::SetProgrammerValue {
                    fixture_id,
                    channel: channel.clone(),
                    value: *value as u8,
                });
            }
        }
    }
}

pub fn render(ui: &mut eframe::egui::Ui, state: &ConsoleState, console_tx: &mpsc::UnboundedSender<ConsoleCommand>) {
    let mut programmer = ProgrammerState::default();
    programmer.render(ui, state, console_tx);
}
