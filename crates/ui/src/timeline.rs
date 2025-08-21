use tokio::sync::mpsc;

use eframe::egui;
use halo_core::ConsoleCommand;
use crate::state::ConsoleState;

pub fn render(ui: &mut eframe::egui::Ui, state: &ConsoleState, console_tx: &mpsc::UnboundedSender<ConsoleCommand>) {
    egui::CentralPanel::default().show(ui.ctx(), |ui| {
        ui.vertical(|ui| {
            ui.heading("Timeline");
            
            // Timecode display
            if let Some(timecode) = &state.timecode {
                ui.heading(format!("Timecode: {:?}", timecode));
            } else {
                ui.label("No timecode available");
            }
            
            ui.separator();
            
            // Timeline controls
            ui.heading("Timeline Controls");
            ui.horizontal(|ui| {
                if ui.button("Start").clicked() {
                    // TODO: Implement timeline start
                }
                
                if ui.button("Stop").clicked() {
                    // TODO: Implement timeline stop
                }
                
                if ui.button("Reset").clicked() {
                    // TODO: Implement timeline reset
                }
            });
            
            ui.separator();
            
            // Cue timeline
            ui.heading("Cue Timeline");
            if let Some(cue_list) = state.cue_lists.first() {
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    for (idx, cue) in cue_list.cues.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("Cue {}: {}", idx + 1, cue.name));
                            if let Some(timecode) = &cue.timecode {
                                ui.label(format!("@ {}", timecode));
                            }
                        });
                    }
                });
            } else {
                ui.label("No cue list available");
            }
        });
    });
}
