use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use halo_core::{Cue, LightingConsole};

/// A panel that shows the list of cues.
pub struct CuePanel {
    cues: Vec<Cue>,
}

impl Default for CuePanel {
    fn default() -> Self {
        Self { cues: vec![] }
    }
}

impl CuePanel {
    pub fn render(&mut self, ui: &mut eframe::egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        ui.heading("Cues");

        let console_guard = console.lock().unwrap();

        // Cue Playback Controls
        ui.horizontal(|ui| {
            if ui.button("GO").clicked() {
                // do nothing
            }
            if ui.button("HOLD").clicked() {
                // do nothing
            }

            if ui.button("STOP").clicked() {
                // do nothing
            }
        });

        // Display cues with progress bars
        let cues = console_guard.cues.iter();

        egui::ScrollArea::vertical().show(ui, |ui| {
            for cue in cues {
                ui.horizontal(|ui| {
                    let active_color = if cue.is_playing {
                        egui::Color32::from_rgb(100, 200, 100)
                    } else {
                        egui::Color32::from_rgb(150, 150, 150)
                    };

                    ui.label(egui::RichText::new(&cue.name).color(active_color).strong());

                    ui.label(
                        egui::RichText::new(Self::format_duration(cue.start_time))
                            .color(active_color),
                    );

                    // Progress bar
                    let progress_response = ui.add(
                        egui::ProgressBar::new(cue.progress)
                            .desired_width(200.0)
                            .desired_height(30.0)
                            .corner_radius(0.0),
                    );

                    // Show duration on hover
                    if progress_response.hovered() {
                        egui::show_tooltip(
                            ui.ctx(),
                            progress_response.layer_id,
                            egui::Id::new("duration_tooltip"),
                            |ui| {
                                ui.label(format!("Duration: {}s", cue.duration.as_secs()));
                            },
                        );
                    }
                });
            }
        });

        drop(console_guard);
    }

    fn format_duration(duration: Duration) -> String {
        let total_secs = duration.as_secs();
        let minutes = total_secs / 60;
        let seconds = total_secs % 60;
        format!("{:02}:{:02}", minutes, seconds)
    }
}
