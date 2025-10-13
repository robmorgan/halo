use eframe::egui;
use halo_core::ConsoleCommand;
use tokio::sync::mpsc;

use crate::state::ConsoleState;

pub struct PatchPanelState {
    new_fixture_name: String,
    new_fixture_profile: String,
    new_fixture_universe: u8,
    new_fixture_address: u16,
    editing_limits_fixture_id: Option<usize>,
    limit_pan_min: u8,
    limit_pan_max: u8,
    limit_tilt_min: u8,
    limit_tilt_max: u8,
}

impl Default for PatchPanelState {
    fn default() -> Self {
        Self {
            new_fixture_name: String::new(),
            new_fixture_profile: String::new(),
            new_fixture_universe: 1,
            new_fixture_address: 1,
            editing_limits_fixture_id: None,
            limit_pan_min: 0,
            limit_pan_max: 255,
            limit_tilt_min: 0,
            limit_tilt_max: 255,
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
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (idx, (_, fixture)) in state.fixtures.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{}: {}", idx + 1, fixture.name));
                            ui.label(format!("Profile: {}", fixture.profile_id));
                            ui.label(format!("Channels: {}", fixture.channels.len()));

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
                                let _ = console_tx.send(ConsoleCommand::UnpatchFixture {
                                    fixture_id: fixture.id,
                                });
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
                                        let _ = console_tx.send(ConsoleCommand::SetPanTiltLimits {
                                            fixture_id: fixture.id,
                                            pan_min: self.limit_pan_min,
                                            pan_max: self.limit_pan_max,
                                            tilt_min: self.limit_tilt_min,
                                            tilt_max: self.limit_tilt_max,
                                        });
                                        self.editing_limits_fixture_id = None;
                                    }
                                    if ui.button("Clear Limits").clicked() {
                                        let _ =
                                            console_tx.send(ConsoleCommand::ClearPanTiltLimits {
                                                fixture_id: fixture.id,
                                            });
                                        self.editing_limits_fixture_id = None;
                                    }
                                    if ui.button("Cancel").clicked() {
                                        self.editing_limits_fixture_id = None;
                                    }
                                });
                            });
                        }
                    }
                });

                ui.separator();

                // Add new fixture
                ui.heading("Add Fixture");
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.new_fixture_name);

                    ui.label("Profile:");
                    ui.text_edit_singleline(&mut self.new_fixture_profile);

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

pub fn render(
    ui: &mut eframe::egui::Ui,
    state: &ConsoleState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    let mut patch_panel = PatchPanelState::default();
    patch_panel.render(ui.ctx(), state, console_tx);
}
