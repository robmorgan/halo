use tokio::sync::mpsc;

use eframe::egui;
use halo_core::ConsoleCommand;
use crate::state::ConsoleState;

pub fn render(ui: &mut eframe::egui::Ui, state: &ConsoleState, console_tx: &mpsc::UnboundedSender<ConsoleCommand>) {
    egui::CentralPanel::default().show(ui.ctx(), |ui| {
        ui.vertical(|ui| {
            ui.heading("Transport Controls");
            
            // Transport buttons
            ui.horizontal(|ui| {
                if ui.button("Play").clicked() {
                    let _ = console_tx.send(ConsoleCommand::Play);
                }
                
                if ui.button("Stop").clicked() {
                    let _ = console_tx.send(ConsoleCommand::Stop);
                }
                
                if ui.button("Pause").clicked() {
                    let _ = console_tx.send(ConsoleCommand::Pause);
                }
                
                if ui.button("Resume").clicked() {
                    let _ = console_tx.send(ConsoleCommand::Resume);
                }
            });
            
            ui.separator();
            
            // Status
            ui.label(format!("Playback State: {:?}", state.playback_state));
            ui.label(format!("BPM: {:.1}", state.bpm));
        });
    });
}
