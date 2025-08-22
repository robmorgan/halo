use tokio::sync::mpsc;

use crate::state::ConsoleState;
use eframe::egui;
use halo_core::ConsoleCommand;

pub struct ShowPanelState {
    new_show_name: String,
    new_show_path: String,
}

impl Default for ShowPanelState {
    fn default() -> Self {
        Self {
            new_show_name: String::new(),
            new_show_path: String::new(),
        }
    }
}

impl ShowPanelState {
    pub fn render(
        &mut self,
        ctx: &egui::Context,
        state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.heading("Show Manager");

                // Show info
                if let Some(show) = &state.show {
                    ui.heading("Current Show");
                    ui.label(format!("Name: {}", show.name));
                    ui.label(format!("Version: {}", show.version));
                    ui.label(format!("Created: {:?}", show.created_at));
                    ui.label(format!("Modified: {:?}", show.modified_at));

                    ui.separator();
                }

                // Show controls
                ui.heading("Show Controls");
                ui.horizontal(|ui| {
                    if ui.button("New Show").clicked() {
                        // TODO: Implement new show creation
                        ui.label("New show creation not yet implemented");
                    }

                    if ui.button("Load Show").clicked() {
                        // TODO: Implement show loading
                        ui.label("Show loading not yet implemented");
                    }

                    if ui.button("Save Show").clicked() {
                        let _ = console_tx.send(ConsoleCommand::SaveShow);
                    }

                    if ui.button("Save Show As").clicked() {
                        // TODO: Implement save as
                        ui.label("Save as not yet implemented");
                    }
                });

                ui.separator();

                // Show statistics
                ui.heading("Show Statistics");
                ui.label(format!("Fixtures: {}", state.fixtures.len()));
                ui.label(format!("Cue Lists: {}", state.cue_lists.len()));
                ui.label(format!("BPM: {:.1}", state.bpm));
                ui.label(format!("Playback State: {:?}", state.playback_state));
            });
        });
    }
}

pub fn render(
    ui: &mut eframe::egui::Ui,
    state: &ConsoleState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    let mut show_panel = ShowPanelState::default();
    show_panel.render(ui.ctx(), state, console_tx);
}
