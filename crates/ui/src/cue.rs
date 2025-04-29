use std::sync::Arc;
use std::time::Duration;

use eframe::egui;
use halo_core::LightingConsole;
use parking_lot::Mutex;

/// A panel that shows the list of cues.
#[derive(Default)]
pub struct CuePanel {}

impl CuePanel {
    pub fn render(&mut self, ui: &mut eframe::egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        ui.heading("Cues");

        let current_list;
        {
            let console_lock = console.lock();
            current_list = console_lock.cue_manager.get_current_cue_list().cloned();
            drop(console_lock);
        }

        if let Some(current_list) = current_list {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Current List:");

                    // Left arrow button
                    if ui.button("←").clicked() {
                        let mut console_lock = console.lock();
                        if let Err(err) = console_lock.cue_manager.select_previous_cue_list() {
                            println!("Error switching to previous cue list: {}", err);
                        }
                        drop(console_lock);
                    }

                    ui.strong(egui::RichText::new(&current_list.name).size(16.0));

                    // Right arrow button
                    if ui.button("→").clicked() {
                        let mut console_lock = console.lock();
                        if let Err(err) = console_lock.cue_manager.select_next_cue_list() {
                            println!("Error switching to next cue list: {}", err);
                        }
                        drop(console_lock);
                    }
                });
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label("Audio:");
                    ui.strong(
                        egui::RichText::new(
                            &current_list
                                .audio_file
                                .clone()
                                .map(|path| {
                                    // Extract just the filename from the path
                                    let filename = std::path::Path::new(&path)
                                        .file_name()
                                        .and_then(|name| name.to_str())
                                        .unwrap_or(&path);

                                    // Truncate to 50 characters if longer
                                    if filename.len() > 50 {
                                        format!("{}...", &filename[..47])
                                    } else {
                                        filename.to_string()
                                    }
                                })
                                .unwrap_or_else(|| "None".to_string()),
                        )
                        .size(16.0),
                    );
                });
            });
        }

        // Display cues with progress bars
        let console_lock = console.lock();
        let cues = console_lock.cue_manager.get_current_cues();

        egui::ScrollArea::vertical().show(ui, |ui| {
            for cue in cues {
                ui.horizontal(|ui| {
                    let is_active = console_lock.cue_manager.is_cue_active(cue.id);
                    let active_color = if is_active {
                        egui::Color32::from_rgb(100, 200, 100)
                    } else {
                        egui::Color32::from_rgb(150, 150, 150)
                    };

                    ui.label(egui::RichText::new(&cue.name).color(active_color).strong());

                    ui.label(
                        egui::RichText::new(Self::format_duration(cue.fade_time))
                            .color(active_color),
                    );

                    // Progress bar
                    let progress = console_lock.cue_manager.get_current_cue_progress();
                    let progress_response = ui.add(
                        egui::ProgressBar::new(progress)
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
                                ui.label(format!("Duration: {}s", cue.fade_time.as_secs()));
                            },
                        );
                    }
                });
            }
        });
        drop(console_lock);
    }

    fn format_duration(duration: Duration) -> String {
        let total_secs = duration.as_secs();
        let minutes = total_secs / 60;
        let seconds = total_secs % 60;
        format!("{:02}:{:02}", minutes, seconds)
    }
}
