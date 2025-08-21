use tokio::sync::mpsc;

use eframe::egui;
use halo_core::ConsoleCommand;
use crate::state::ConsoleState;

pub fn render(ui: &mut eframe::egui::Ui, state: &ConsoleState, console_tx: &mpsc::UnboundedSender<ConsoleCommand>) {
    egui::CentralPanel::default().show(ui.ctx(), |ui| {
        ui.vertical(|ui| {
            ui.heading("Master Controls");
            
            // Master fader
            ui.horizontal(|ui| {
                ui.label("Master:");
                let mut master = 100.0;
                if ui.add(egui::Slider::new(&mut master, 0.0..=100.0).text("Master")).changed() {
                    // TODO: Implement master fader via message passing
                }
            });
            
            ui.separator();
            
            // Transport controls
            ui.heading("Transport");
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
            
            // BPM control
            ui.heading("Tempo");
            ui.horizontal(|ui| {
                ui.label("BPM:");
                let mut bpm = state.bpm;
                if ui.add(egui::Slider::new(&mut bpm, 60.0..=200.0).text("BPM")).changed() {
                    let _ = console_tx.send(ConsoleCommand::SetBpm { bpm });
                }
                
                if ui.button("Tap").clicked() {
                    let _ = console_tx.send(ConsoleCommand::TapTempo);
                }
            });
            
            ui.separator();
            
            // Status
            ui.heading("Status");
            ui.label(format!("Playback: {:?}", state.playback_state));
            ui.label(format!("BPM: {:.1}", state.bpm));
            ui.label(format!("Fixtures: {}", state.fixtures.len()));
            ui.label(format!("Cue Lists: {}", state.cue_lists.len()));
        });
    });
}
