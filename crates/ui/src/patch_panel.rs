use tokio::sync::mpsc;

use crate::state::ConsoleState;
use eframe::egui;
use halo_core::ConsoleCommand;

pub fn render(
    ui: &mut eframe::egui::Ui,
    state: &ConsoleState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    egui::CentralPanel::default().show(ui.ctx(), |ui| {
        ui.vertical(|ui| {
            ui.heading("Patch Panel");

            // Fixture list
            ui.heading("Patched Fixtures");
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (idx, fixture) in state.fixtures.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}: {}", idx + 1, fixture.name));
                        ui.label(format!("Profile: {}", fixture.profile_id));
                        ui.label(format!("Channels: {}", fixture.channels.len()));

                        if ui.button("Remove").clicked() {
                            let _ =
                                console_tx.send(ConsoleCommand::UnpatchFixture { fixture_id: idx });
                        }
                    });
                }
            });

            ui.separator();

            // Add new fixture
            ui.heading("Add Fixture");
            ui.horizontal(|ui| {
                ui.label("Name:");
                let mut name = String::new();
                ui.text_edit_singleline(&mut name);

                ui.label("Profile:");
                let mut profile = String::new();
                ui.text_edit_singleline(&mut profile);

                ui.label("Universe:");
                let mut universe = 1u8;
                ui.add(egui::DragValue::new(&mut universe).range(1..=255));

                ui.label("Address:");
                let mut address = 1u16;
                ui.add(egui::DragValue::new(&mut address).range(1..=512));

                if ui.button("Add").clicked() && !name.is_empty() && !profile.is_empty() {
                    let _ = console_tx.send(ConsoleCommand::PatchFixture {
                        name,
                        profile_name: profile,
                        universe,
                        address,
                    });
                }
            });
        });
    });
}
