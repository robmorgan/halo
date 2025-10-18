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
    editing_limits_fixture_id: Option<usize>,
    limit_pan_min: u8,
    limit_pan_max: u8,
    limit_tilt_min: u8,
    limit_tilt_max: u8,
    fixture_to_remove: Option<usize>,
    fixture_to_remove_name: String,
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
            editing_limits_fixture_id: None,
            limit_pan_min: 0,
            limit_pan_max: 255,
            limit_tilt_min: 0,
            limit_tilt_max: 255,
            fixture_to_remove: None,
            fixture_to_remove_name: String::new(),
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
        // Render confirmation modal for fixture removal
        if let Some(fixture_id) = self.fixture_to_remove {
            egui::Window::new("Remove Fixture")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Are you sure you want to remove fixture \"{}\"?",
                        self.fixture_to_remove_name
                    ));
                    ui.label("This action cannot be undone.");

                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.fixture_to_remove = None;
                            self.fixture_to_remove_name.clear();
                        }

                        if ui.button("Remove").clicked() {
                            let _ = console_tx.send(ConsoleCommand::UnpatchFixture { fixture_id });
                            self.fixture_to_remove = None;
                            self.fixture_to_remove_name.clear();
                        }
                    });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.heading("Patch Panel");

                // Fixture list
                ui.heading("Patched Fixtures");

                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        // Convert to vector and sort by fixture ID for consistent ordering
                        let mut fixtures: Vec<_> = state.fixtures.iter().collect();
                        fixtures.sort_by_key(|(_, f)| f.id);

                        // Clean up edit_values for fixtures that no longer exist
                        let current_fixture_ids: std::collections::HashSet<_> =
                            fixtures.iter().map(|(_, f)| f.id).collect();
                        self.edit_values
                            .retain(|id, _| current_fixture_ids.contains(id));

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
                                    ui.add_sized(
                                        [50.0, 20.0],
                                        egui::Label::new(format!("ID {}:", fixture.id)),
                                    );

                                    ui.label("Name:");
                                    ui.add_sized(
                                        [120.0, 20.0],
                                        egui::TextEdit::singleline(&mut edit_value.name),
                                    );

                                    ui.label("Profile:");
                                    ui.add_sized(
                                        [150.0, 20.0],
                                        egui::Label::new(&fixture.profile_id),
                                    );

                                    ui.label("Universe:");
                                    ui.add_sized(
                                        [60.0, 20.0],
                                        egui::DragValue::new(&mut edit_value.universe)
                                            .range(1..=255),
                                    );

                                    ui.label("Address:");
                                    ui.add_sized(
                                        [60.0, 20.0],
                                        egui::DragValue::new(&mut edit_value.address)
                                            .range(1..=512),
                                    );

                                    ui.add_sized(
                                        [100.0, 20.0],
                                        egui::Label::new(format!(
                                            "Channels: {}",
                                            fixture.channels.len()
                                        )),
                                    );

                                    // Show limits badge if set
                                    if let Some(limits) = &fixture.pan_tilt_limits {
                                        ui.label(format!(
                                            "🔒 P:{}-{} T:{}-{}",
                                            limits.pan_min,
                                            limits.pan_max,
                                            limits.tilt_min,
                                            limits.tilt_max
                                        ));
                                    }

                                    if ui.button("Limits").clicked() {
                                        // Toggle limit editor for this fixture
                                        if self.editing_limits_fixture_id == Some(fixture.id) {
                                            self.editing_limits_fixture_id = None;
                                        } else {
                                            self.editing_limits_fixture_id = Some(fixture.id);
                                            // Load current limits if they exist
                                            if let Some(limits) = &fixture.pan_tilt_limits {
                                                self.limit_pan_min = limits.pan_min;
                                                self.limit_pan_max = limits.pan_max;
                                                self.limit_tilt_min = limits.tilt_min;
                                                self.limit_tilt_max = limits.tilt_max;
                                            } else {
                                                self.limit_pan_min = 0;
                                                self.limit_pan_max = 255;
                                                self.limit_tilt_min = 0;
                                                self.limit_tilt_max = 255;
                                            }
                                        }
                                    }

                                    if ui.button("Remove").clicked() {
                                        self.fixture_to_remove = Some(fixture.id);
                                        self.fixture_to_remove_name = fixture.name.clone();
                                    }
                                });

                                // Show limit editor if this fixture is being edited
                                if self.editing_limits_fixture_id == Some(fixture.id) {
                                    ui.indent(format!("limits_editor_{}", fixture.id), |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label("Pan Min:");
                                            ui.add(
                                                egui::DragValue::new(&mut self.limit_pan_min)
                                                    .range(0..=255),
                                            );
                                            ui.label("Max:");
                                            ui.add(
                                                egui::DragValue::new(&mut self.limit_pan_max)
                                                    .range(0..=255),
                                            );
                                        });
                                        ui.horizontal(|ui| {
                                            ui.label("Tilt Min:");
                                            ui.add(
                                                egui::DragValue::new(&mut self.limit_tilt_min)
                                                    .range(0..=255),
                                            );
                                            ui.label("Max:");
                                            ui.add(
                                                egui::DragValue::new(&mut self.limit_tilt_max)
                                                    .range(0..=255),
                                            );
                                        });
                                        ui.horizontal(|ui| {
                                            if ui.button("Apply Limits").clicked() {
                                                let _ = console_tx.send(
                                                    ConsoleCommand::SetPanTiltLimits {
                                                        fixture_id: fixture.id,
                                                        pan_min: self.limit_pan_min,
                                                        pan_max: self.limit_pan_max,
                                                        tilt_min: self.limit_tilt_min,
                                                        tilt_max: self.limit_tilt_max,
                                                    },
                                                );
                                                self.editing_limits_fixture_id = None;
                                            }
                                            if ui.button("Clear Limits").clicked() {
                                                let _ = console_tx.send(
                                                    ConsoleCommand::ClearPanTiltLimits {
                                                        fixture_id: fixture.id,
                                                    },
                                                );
                                                self.editing_limits_fixture_id = None;
                                            }
                                            if ui.button("Cancel").clicked() {
                                                self.editing_limits_fixture_id = None;
                                            }
                                        });
                                    });
                                }
                            });
                        }
                    });

                // Global save/cancel buttons
                ui.separator();

                // Check if there are any pending changes
                let mut has_changes = false;
                let mut fixtures: Vec<_> = state.fixtures.iter().collect();
                fixtures.sort_by_key(|(_, f)| f.id);

                for (_, fixture) in &fixtures {
                    if let Some(edit_value) = self.edit_values.get(&fixture.id) {
                        if edit_value.name != fixture.name
                            || edit_value.universe != fixture.universe
                            || edit_value.address != fixture.start_address
                        {
                            has_changes = true;
                            break;
                        }
                    }
                }

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(has_changes, egui::Button::new("Save All Changes"))
                        .clicked()
                    {
                        // Apply all changes
                        for (_, fixture) in &fixtures {
                            if let Some(edit_value) = self.edit_values.get(&fixture.id) {
                                if edit_value.name != fixture.name
                                    || edit_value.universe != fixture.universe
                                    || edit_value.address != fixture.start_address
                                {
                                    let _ = console_tx.send(ConsoleCommand::UpdateFixture {
                                        fixture_id: fixture.id,
                                        name: edit_value.name.clone(),
                                        universe: edit_value.universe,
                                        address: edit_value.address,
                                    });
                                }
                            }
                        }
                    }

                    if ui
                        .add_enabled(has_changes, egui::Button::new("Cancel"))
                        .clicked()
                    {
                        // Reset all edit values to current fixture values
                        for (_, fixture) in &fixtures {
                            self.edit_values.insert(
                                fixture.id,
                                EditingFixture {
                                    name: fixture.name.clone(),
                                    universe: fixture.universe,
                                    address: fixture.start_address,
                                },
                            );
                        }
                    }
                });

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
