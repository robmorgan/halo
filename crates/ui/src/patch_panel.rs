use std::collections::HashMap;

use eframe::egui;
use halo_core::ConsoleCommand;
use tokio::sync::mpsc;

use crate::state::ConsoleState;

pub struct PatchPanelState {
    new_fixture_name: String,
    new_fixture_profile: String,
    new_fixture_universe: u8,
    new_fixture_address: u16,
    edit_values: HashMap<usize, EditingFixture>,
}

#[derive(Clone)]
struct EditingFixture {
    name: String,
    universe: u8,
    address: u16,
}

impl Default for PatchPanelState {
    fn default() -> Self {
        Self {
            new_fixture_name: String::new(),
            new_fixture_profile: String::new(),
            new_fixture_universe: 1,
            new_fixture_address: 1,
            edit_values: HashMap::new(),
        }
    }
}

impl PatchPanelState {
    pub fn render(
        &mut self,
        ctx: &egui::Context,
        state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.heading("Patch Panel");

                // Fixture list
                ui.heading("Patched Fixtures");

                let mut fixture_to_remove: Option<usize> = None;

                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        // Convert to vector and sort by fixture ID for consistent ordering
                        let mut fixtures: Vec<_> = state.fixtures.iter().collect();
                        fixtures.sort_by_key(|(_, f)| f.id);

                        for (_, fixture) in fixtures {
                            // Initialize edit values if not present
                            if !self.edit_values.contains_key(&fixture.id) {
                                self.edit_values.insert(
                                    fixture.id,
                                    EditingFixture {
                                        name: fixture.name.clone(),
                                        universe: fixture.universe,
                                        address: fixture.start_address,
                                    },
                                );
                            }

                            ui.group(|ui| {
                                let edit_value = self.edit_values.get_mut(&fixture.id).unwrap();

                                ui.horizontal(|ui| {
                                    ui.label(format!("ID {}:", fixture.id + 1));

                                    ui.label("Name:");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut edit_value.name)
                                            .desired_width(120.0),
                                    );

                                    ui.label("Profile:");
                                    ui.label(&fixture.profile_id);

                                    ui.label("Universe:");
                                    ui.add(
                                        egui::DragValue::new(&mut edit_value.universe)
                                            .range(1..=255),
                                    );

                                    ui.label("Address:");
                                    ui.add(
                                        egui::DragValue::new(&mut edit_value.address)
                                            .range(1..=512),
                                    );

                                    ui.label(format!("Channels: {}", fixture.channels.len()));

                                    // Check if values have changed
                                    let changed = edit_value.name != fixture.name
                                        || edit_value.universe != fixture.universe
                                        || edit_value.address != fixture.start_address;

                                    if changed && ui.button("Save").clicked() {
                                        let _ = console_tx.send(ConsoleCommand::UpdateFixture {
                                            fixture_id: fixture.id,
                                            name: edit_value.name.clone(),
                                            universe: edit_value.universe,
                                            address: edit_value.address,
                                        });
                                    }

                                    if ui.button("Remove").clicked() {
                                        let _ = console_tx.send(ConsoleCommand::UnpatchFixture {
                                            fixture_id: fixture.id,
                                        });
                                        fixture_to_remove = Some(fixture.id);
                                    }
                                });
                            });
                        }
                    });

                // Remove fixture from edit values if requested
                if let Some(fixture_id) = fixture_to_remove {
                    self.edit_values.remove(&fixture_id);
                }

                ui.separator();

                // Add new fixture
                ui.heading("Add Fixture");
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_fixture_name).desired_width(120.0),
                    );

                    ui.label("Profile:");
                    // Get sorted list of profiles for dropdown
                    let mut profile_options: Vec<(String, String)> = state
                        .fixture_library
                        .profiles
                        .iter()
                        .map(|(id, profile)| (id.clone(), profile.to_string()))
                        .collect();
                    profile_options.sort_by(|a, b| a.1.cmp(&b.1));

                    egui::ComboBox::from_id_salt("fixture_profile_selector")
                        .selected_text(if self.new_fixture_profile.is_empty() {
                            "Select a fixture type..."
                        } else {
                            // Find the display name for the selected profile
                            profile_options
                                .iter()
                                .find(|(id, _)| id == &self.new_fixture_profile)
                                .map(|(_, name)| name.as_str())
                                .unwrap_or(&self.new_fixture_profile)
                        })
                        .show_ui(ui, |ui| {
                            for (profile_id, profile_name) in profile_options {
                                ui.selectable_value(
                                    &mut self.new_fixture_profile,
                                    profile_id.clone(),
                                    profile_name,
                                );
                            }
                        });

                    ui.label("Universe:");
                    ui.add(egui::DragValue::new(&mut self.new_fixture_universe).range(1..=255));

                    ui.label("Address:");
                    ui.add(egui::DragValue::new(&mut self.new_fixture_address).range(1..=512));

                    if ui.button("Add").clicked()
                        && !self.new_fixture_name.is_empty()
                        && !self.new_fixture_profile.is_empty()
                    {
                        let _ = console_tx.send(ConsoleCommand::PatchFixture {
                            name: self.new_fixture_name.clone(),
                            profile_name: self.new_fixture_profile.clone(),
                            universe: self.new_fixture_universe,
                            address: self.new_fixture_address,
                        });

                        // Clear the form
                        self.new_fixture_name.clear();
                        self.new_fixture_profile.clear();
                    }
                });
            });
        });
    }
}
