use tokio::sync::mpsc;

use crate::state::ConsoleState;
use eframe::egui;
use halo_core::ConsoleCommand;

pub struct PatchPanelState {
    new_fixture_name: String,
    new_fixture_profile: String,
    new_fixture_universe: u8,
    new_fixture_address: u16,
}

impl Default for PatchPanelState {
    fn default() -> Self {
        Self {
            new_fixture_name: String::new(),
            new_fixture_profile: String::new(),
            new_fixture_universe: 1,
            new_fixture_address: 1,
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

                            if ui.button("Remove").clicked() {
                                let _ = console_tx
                                    .send(ConsoleCommand::UnpatchFixture { fixture_id: fixture.id });
                            }
                        });
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
