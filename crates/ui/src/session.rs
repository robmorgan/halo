use std::time::SystemTime;

use eframe::egui::{Align, Color32, FontId, Layout, RichText};
use halo_core::{ConsoleCommand, PlaybackState};
use tokio::sync::mpsc;

use crate::state::ConsoleState;

enum ClockMode {
    TimeCode,
    System,
}

/// A panel that shows the current session overview.
///
/// This includes:
/// - A toggable clock that can show either timecode or the system clock.
/// - The Master BPM display +/- buttons.
/// - Ableton Link status and connected peers.
/// - Large transport controls (GO, HOLD, STOP).
pub struct SessionPanel {
    // Clock state
    clock_mode: ClockMode,
}

impl Default for SessionPanel {
    fn default() -> Self {
        Self {
            clock_mode: ClockMode::TimeCode,
        }
    }
}

impl SessionPanel {
    pub fn render(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        // Session UI
        ui.vertical(|ui| {
            // Top row - header and mode toggle
            ui.horizontal(|ui| {
                ui.heading("Session");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mode_text = match self.clock_mode {
                        ClockMode::TimeCode => "TC",
                        ClockMode::System => "SYS",
                    };

                    if ui.button(format!("🕒 {}", mode_text)).clicked() {
                        self.clock_mode = match self.clock_mode {
                            ClockMode::TimeCode => ClockMode::System,
                            ClockMode::System => ClockMode::TimeCode,
                        };
                    }
                });
            });

            ui.add_space(10.0);

            // Clock display
            ui.group(|ui| {
                ui.set_min_width(ui.available_width());

                let clock_text = match self.clock_mode {
                    ClockMode::TimeCode => {
                        if let Some(timecode) = &state.timecode {
                            format!(
                                "{:02}:{:02}:{:02}.{:02}",
                                timecode.hours, timecode.minutes, timecode.seconds, timecode.frames
                            )
                        } else {
                            "00:00:00.00".to_string()
                        }
                    }
                    ClockMode::System => {
                        let now = SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default();
                        let total_secs = now.as_secs();
                        let hours = (total_secs / 3600) % 24;
                        let minutes = (total_secs / 60) % 60;
                        let seconds = total_secs % 60;
                        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
                    }
                };

                // Large monospace font for clock
                let font_id = FontId::monospace(32.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(clock_text).font(font_id));

                    let mode_label = match self.clock_mode {
                        ClockMode::TimeCode => "Timecode",
                        ClockMode::System => "System Clock",
                    };
                    ui.label(mode_label);
                });
            });

            ui.add_space(10.0);

            // BPM controls
            ui.horizontal(|ui| {
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Master BPM");

                        ui.horizontal(|ui| {
                            if ui.button("-").clicked() {
                                let _ = console_tx.send(ConsoleCommand::SetBpm {
                                    bpm: state.bpm - 0.1,
                                });
                            }

                            let bpm_text = format!("{:.1}", state.bpm);
                            let font_id = FontId::monospace(24.0);
                            ui.colored_label(
                                Color32::from_rgb(255, 215, 0),
                                RichText::new(bpm_text).font(font_id),
                            );

                            if ui.button("+").clicked() {
                                let _ = console_tx.send(ConsoleCommand::SetBpm {
                                    bpm: state.bpm + 0.1,
                                });
                            }
                        });

                        ui.horizontal(|ui| {
                            if ui.button("-1.0").clicked() {
                                let _ = console_tx.send(ConsoleCommand::SetBpm {
                                    bpm: state.bpm - 1.0,
                                });
                            }
                            if ui.button("+1.0").clicked() {
                                let _ = console_tx.send(ConsoleCommand::SetBpm {
                                    bpm: state.bpm + 1.0,
                                });
                            }
                        });
                    });
                });

                ui.add_space(10.0);

                // Ableton Link
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Ableton Link");
                        let link_text = if state.link_enabled {
                            "LINK ●"
                        } else {
                            "LINK ○"
                        };
                        let link_color = if state.link_enabled {
                            Color32::from_rgb(66, 133, 244)
                        } else {
                            ui.style().visuals.text_color()
                        };

                        if ui
                            .button(RichText::new(link_text).color(link_color))
                            .clicked()
                        {
                            if state.link_enabled {
                                let _ = console_tx.send(ConsoleCommand::DisableAbletonLink);
                            } else {
                                let _ = console_tx.send(ConsoleCommand::EnableAbletonLink);
                            }
                        }

                        let status_text = if state.link_enabled {
                            "Connected"
                        } else {
                            "Disabled"
                        };
                        ui.label(status_text);

                        let peers_text = if state.link_enabled {
                            if state.link_peers == 1 {
                                "1 peer connected".to_string()
                            } else {
                                format!("{} peers connected", state.link_peers)
                            }
                        } else {
                            "No peers connected".to_string()
                        };
                        ui.label(peers_text);
                    });
                });
            });

            ui.add_space(10.0);

            // Large transport controls
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.set_min_width(ui.available_width());
                    // Create large buttons with current state colors
                    let button_height = 60.0;
                    let button_width = ui.available_width() / 3.0 - 10.0;

                    // Go button
                    let play_text =
                        RichText::new("▶ GO")
                            .size(18.0)
                            .color(match state.playback_state {
                                PlaybackState::Playing => ui.style().visuals.text_color(),
                                _ => Color32::from_rgb(120, 255, 120),
                            });

                    let play_button = ui.add_sized(
                        [button_width, button_height],
                        eframe::egui::Button::new(play_text),
                    );

                    if play_button.clicked() {
                        let _ = console_tx.send(ConsoleCommand::Play);
                    }

                    // Hold button
                    let hold_text =
                        RichText::new("⏸ HOLD")
                            .size(18.0)
                            .color(match state.playback_state {
                                PlaybackState::Playing => Color32::from_rgb(255, 215, 0),
                                _ => ui.style().visuals.text_color(),
                            });

                    let hold_button = ui.add_sized(
                        [button_width, button_height],
                        eframe::egui::Button::new(hold_text),
                    );

                    if hold_button.clicked() {
                        let _ = console_tx.send(ConsoleCommand::Pause);
                    }

                    // Stop button
                    let stop_text =
                        RichText::new("⏹ STOP")
                            .size(18.0)
                            .color(match state.playback_state {
                                PlaybackState::Stopped => ui.style().visuals.text_color(),
                                _ => Color32::from_rgb(255, 100, 100),
                            });

                    let stop_button = ui.add_sized(
                        [button_width, button_height],
                        eframe::egui::Button::new(stop_text),
                    );

                    if stop_button.clicked() {
                        let _ = console_tx.send(ConsoleCommand::Stop);
                    }
                });
            });
        });
    }
}
