use eframe::egui::{self, Color32, Vec2};
use halo_core::ConsoleCommand;
use tokio::sync::mpsc;

use crate::state::ConsoleState;

pub fn render(
    ui: &mut egui::Ui,
    _state: &ConsoleState,
    _console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    // Create a black panel with fixed dimensions
    egui::Frame::new()
        .fill(Color32::BLACK)
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(60)))
        .inner_margin(10.0)
        .show(ui, |ui| {
            // Set both min and max size to prevent expansion
            ui.set_min_size(Vec2::new(250.0, 300.0));
            ui.set_max_size(Vec2::new(250.0, 300.0));

            // Placeholder text in the center
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new("Visualizer Area")
                        .size(16.0)
                        .color(Color32::from_gray(100)),
                );
            });
        });
}
