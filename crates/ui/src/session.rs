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
            ui.heading("Session Info");

            // Session statistics
            ui.heading("Statistics");
            ui.label(format!("Fixtures: {}", state.fixtures.len()));
            ui.label(format!("Cue Lists: {}", state.cue_lists.len()));
            ui.label(format!("BPM: {:.1}", state.bpm));
            ui.label(format!("Playback State: {:?}", state.playback_state));

            ui.separator();

            // Rhythm state
            ui.heading("Rhythm State");
            ui.label(format!("Beat Phase: {:.2}", state.rhythm_state.beat_phase));
            ui.label(format!("Bar Phase: {:.2}", state.rhythm_state.bar_phase));
            ui.label(format!(
                "Phrase Phase: {:.2}",
                state.rhythm_state.phrase_phase
            ));
            ui.label(format!(
                "Beats per Bar: {}",
                state.rhythm_state.beats_per_bar
            ));
            ui.label(format!(
                "Bars per Phrase: {}",
                state.rhythm_state.bars_per_phrase
            ));

            ui.separator();

            // Link state
            ui.heading("Ableton Link State");
            
            // Status indicator with color
            let (status_text, status_color) = if state.link_enabled {
                ("● Enabled", egui::Color32::GREEN)
            } else {
                ("○ Disabled", egui::Color32::RED)
            };
            
            ui.colored_label(status_color, status_text);
            ui.label(format!("Connected Peers: {}", state.link_peers));
            
            // Toggle button
            if ui.button(if state.link_enabled { "Disable Link" } else { "Enable Link" }).clicked() {
                if state.link_enabled {
                    let _ = console_tx.send(ConsoleCommand::DisableAbletonLink);
                } else {
                    let _ = console_tx.send(ConsoleCommand::EnableAbletonLink);
                }
            }
            
            // Refresh button
            if ui.button("Refresh Link State").clicked() {
                let _ = console_tx.send(ConsoleCommand::QueryLinkState);
            }

            ui.separator();

            // Show info
            if let Some(show) = &state.show {
                ui.heading("Show Info");
                ui.label(format!("Name: {}", show.name));
                ui.label(format!("Version: {}", show.version));
                ui.label(format!("Created: {:?}", show.created_at));
                ui.label(format!("Modified: {:?}", show.modified_at));
            }
        });
    });
}
