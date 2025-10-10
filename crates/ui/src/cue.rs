use std::time::Duration;

use eframe::egui;
use halo_core::{ConsoleCommand, PlaybackState};
use tokio::sync::mpsc;

use crate::state::ConsoleState;

/// A panel that shows the list of cues.
#[derive(Default)]
pub struct CuePanel {
    playback_state: PlaybackState,
}

impl CuePanel {
    pub fn render(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &ConsoleState,
        _console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.heading("Cues");

        let cue_lists = &state.cue_lists;

        if let Some(current_list) = cue_lists.get(state.current_cue_list_index) {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Current List:");

                    // Left arrow button
                    if ui.button("←").clicked() {
                        let _ = _console_tx.send(ConsoleCommand::SelectPreviousCueList);
                    }

                    ui.strong(egui::RichText::new(&current_list.name).size(16.0));

                    // Right arrow button
                    if ui.button("→").clicked() {
                        let _ = _console_tx.send(ConsoleCommand::SelectNextCueList);
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

                // Add audio transport controls
                ui.horizontal(|ui| {
                    if ui.button("▶").clicked() {
                        // TODO: Implement audio play
                        // console_tx.send(ConsoleCommand::PlayAudio { file_path:
                        // current_list.audio_file.clone().unwrap_or_default() }).ok();
                    }

                    if ui.button("⏸").clicked() {
                        // TODO: Implement audio pause
                        // console_tx.send(ConsoleCommand::PauseAudio).ok();
                    }

                    if ui.button("⏹").clicked() {
                        // TODO: Implement audio stop
                        // console_tx.send(ConsoleCommand::StopAudio).ok();
                    }
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
        if let Some(current_list) = cue_lists.get(state.current_cue_list_index) {
            let cues = &current_list.cues;

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (cue_index, cue) in cues.iter().enumerate() {
                    ui.horizontal(|ui| {
                        // Check if this is the current active cue
                        let is_current_cue = cue_index == state.current_cue_index
                            && state.playback_state == PlaybackState::Playing;

                        let active_color = if is_current_cue {
                            egui::Color32::from_rgb(100, 200, 100) // Green for current cue when
                                                                   // playing
                        } else {
                            ui.style().visuals.text_color() // Default color for all other cues
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

                        // Progress bar - only show progress for the current cue
                        let progress = if is_current_cue {
                            state.current_cue_progress
                        } else {
                            0.0
                        };

                        let progress_response = ui.add(
                            egui::ProgressBar::new(progress)
                                .desired_width(200.0)
                                .desired_height(30.0)
                                .corner_radius(0.0)
                                .animate(is_current_cue)
                                .fill(if is_current_cue {
                                    egui::Color32::from_rgb(75, 2, 245) // Blue for current cue
                                                                        // progress
                                } else {
                                    egui::Color32::from_rgb(100, 100, 100) // Gray for inactive cues
                                }),
                        );

                        // Show detailed info on hover
                        if progress_response.hovered() {
                            // Simple tooltip with cue info
                            ui.label(format!(
                                "Cue: {} | Duration: {}s | Progress: {:.1}%",
                                cue.name,
                                cue.fade_time.as_secs(),
                                progress * 100.0
                            ));
                        }
                    });
                    ui.add_space(2.0); // Spacing between cue rows
                }
            });
        }
    }

    pub fn set_playback_state(&mut self, state: PlaybackState) {
        self.playback_state = state;
    }

    fn format_duration(duration: Duration) -> String {
        let total_secs = duration.as_secs();
        let minutes = total_secs / 60;
        let seconds = total_secs % 60;
        format!("{:02}:{:02}", minutes, seconds)
    }
}

pub fn render(
    ui: &mut eframe::egui::Ui,
    state: &ConsoleState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    let mut cue_panel = CuePanel::default();
    cue_panel.set_playback_state(state.playback_state);
    cue_panel.render(ui, state, console_tx);
}
