use tokio::sync::mpsc;

use eframe::egui::{self, Color32};
use halo_core::ConsoleCommand;
use crate::state::ConsoleState;

pub fn render(ui: &mut eframe::egui::Ui, state: &ConsoleState, console_tx: &mpsc::UnboundedSender<ConsoleCommand>) {
    egui::CentralPanel::default().show(ui.ctx(), |ui| {
        ui.vertical(|ui| {
            ui.heading("Fixtures");
            
            // Fixture grid
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (idx, fixture) in state.fixtures.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}: {}", idx + 1, fixture.name));
                        ui.label(format!("Profile: {}", fixture.profile_id));
                        ui.label(format!("Channels: {}", fixture.channels.len()));
                        
                        // Show channel values
                        ui.label("Values:");
                        for (channel_idx, channel) in fixture.channels.iter().enumerate() {
                            ui.label(format!("{}:{}", channel.name, channel.value));
                        }
                    });
                }
            });
        });
    });
}
