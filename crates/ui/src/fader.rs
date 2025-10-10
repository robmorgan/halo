use eframe::egui;
use halo_core::ConsoleCommand;
use tokio::sync::mpsc;

use crate::state::ConsoleState;

pub fn render(
    ui: &mut eframe::egui::Ui,
    state: &ConsoleState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    egui::CentralPanel::default().show(ui.ctx(), |ui| {
        ui.vertical(|ui| {
            ui.heading("Faders");

            // Master fader
            ui.horizontal(|ui| {
                ui.label("Master:");
                let mut master = 100.0;
                if ui
                    .add(egui::Slider::new(&mut master, 0.0..=100.0).text("Master"))
                    .changed()
                {
                    // TODO: Implement master fader via message passing
                }
            });

            ui.separator();

            // Individual faders
            ui.heading("Individual Faders");
            for (idx, (_, fixture)) in state.fixtures.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}: {}", idx + 1, fixture.name));
                    let mut dimmer = 100.0;
                    if ui
                        .add(egui::Slider::new(&mut dimmer, 0.0..=100.0).text("Dimmer"))
                        .changed()
                    {
                        // TODO: Implement individual fader via message passing
                    }
                });
            }
        });
    });
}
