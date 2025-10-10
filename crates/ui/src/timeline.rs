use eframe::egui;
use halo_core::ConsoleCommand;
use tokio::sync::mpsc;

use crate::state::ConsoleState;

pub fn render(
    ui: &mut eframe::egui::Ui,
    state: &ConsoleState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    ui.horizontal(|ui| {
        ui.heading("TIMELINE");

        ui.add_space(20.0);

        // Timecode display
        if let Some(timecode) = &state.timecode {
            ui.label("Timecode:");
            ui.strong(format!("{:?}", timecode));
        } else {
            ui.label("No timecode");
        }

        ui.add_space(20.0);

        // Playback controls
        if ui.button("Play").clicked() {
            let _ = console_tx.send(ConsoleCommand::Play);
        }
        if ui.button("Stop").clicked() {
            let _ = console_tx.send(ConsoleCommand::Stop);
        }
        if ui.button("Pause").clicked() {
            let _ = console_tx.send(ConsoleCommand::Pause);
        }

        ui.add_space(20.0);

        // Quick timeline visualization
        if let Some(cue_list) = state.cue_lists.first() {
            ui.label("Cues:");
            for (idx, cue) in cue_list.cues.iter().take(3).enumerate() {
                ui.label(format!("{}:{}", idx + 1, cue.name));
            }
            if cue_list.cues.len() > 3 {
                ui.label("...");
            }
        } else {
            ui.label("No cue list available");
        }
    });
}
