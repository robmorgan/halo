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

        // Column headers for cue list
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Cue").strong());
            ui.add_space(80.0); // Adjust spacing based on your UI needs

            ui.label(egui::RichText::new("Timecode").strong());
            ui.add_space(60.0);

            ui.label(egui::RichText::new("Duration").strong());
            ui.add_space(40.0);

            ui.label(egui::RichText::new("Progress").strong());
        });
        ui.separator();

        // Display cues with neat alignment and timecode
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

                    // Cue name with fixed width
                    ui.scope(|ui| {
                        ui.style_mut().spacing.item_spacing.x = 0.0;
                        ui.add_sized(
                            [100.0, 20.0],
                            egui::Label::new(
                                egui::RichText::new(&cue.name).color(active_color).strong(),
                            ),
                        );
                    });

                    // Timecode marker (estimated position in the timeline)
                    let timecode = if let Some(timecode) = &cue.timecode {
                        timecode
                    } else {
                        &"N/A".to_string()
                    };

                    ui.add_sized(
                        [100.0, 20.0],
                        egui::Label::new(
                            egui::RichText::new(timecode)
                                .color(active_color)
                                .monospace(),
                        ),
                    );

                    // Duration with fixed width
                    ui.add_sized(
                        [80.0, 20.0],
                        egui::Label::new(
                            egui::RichText::new(Self::format_duration(cue.fade_time))
                                .color(active_color)
                                .monospace(),
                        ),
                    );

                    // Progress bar
                    let progress = if is_active {
                        console_lock.cue_manager.get_current_cue_progress()
                    } else {
                        0.0
                    };

                    let progress_response = ui.add(
                        egui::ProgressBar::new(progress)
                            .desired_width(200.0)
                            .desired_height(30.0)
                            .corner_radius(0.0)
                            .animate(is_active)
                            .fill(if is_active {
                                egui::Color32::from_rgb(75, 2, 245)
                            } else {
                                egui::Color32::from_rgb(100, 100, 100)
                            }),
                    );

                    // Show detailed info on hover
                    if progress_response.hovered() {
                        egui::show_tooltip(
                            ui.ctx(),
                            progress_response.layer_id,
                            egui::Id::new("cue_tooltip"),
                            |ui| {
                                ui.vertical(|ui| {
                                    ui.label(format!("Cue: {}", cue.name));
                                    ui.label(format!("Duration: {}s", cue.fade_time.as_secs()));
                                    ui.label(format!("Progress: {:.1}%", progress * 100.0));
                                    if is_active {
                                        ui.label("Status: Active");
                                    } else {
                                        ui.label("Status: Inactive");
                                    }
                                });
                            },
                        );
                    }
                });
                ui.add_space(2.0); // Spacing between cue rows
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
