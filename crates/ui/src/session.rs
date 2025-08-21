use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

use chrono::{Local, Timelike};
use eframe::egui::{Align, Color32, FontId, Layout, RichText};
use halo_core::{ConsoleCommand, PlaybackState, TimeCode};

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
pub struct SessionPanel {
    // Clock state
    clock_mode: ClockMode,
    timecode: TimeCode,

    // BPM state
    pub bpm: f64,

    // Ableton Link state
    link_enabled: bool,
    link_peers: u64,

    // Playback state
    playback_state: PlaybackState,
}

impl Default for SessionPanel {
    fn default() -> Self {
        Self {
            clock_mode: ClockMode::TimeCode,
            timecode: TimeCode::default(),
            bpm: 120.0,
            link_enabled: false,
            link_peers: 0,
            playback_state: PlaybackState::Stopped,
        }
    }
}

impl SessionPanel {
    // if ui.add(egui::DragValue::new(&mut temp_bpm).speed(0.1)).changed() {
    //     self.state.bpm = temp_bpm; // Update local state for immediate UI feedback
    //     let _ = self.engine_tx.send(EngineCommand::SetTempo(temp_bpm));
    // }
    pub fn render(
        &mut self,
        ui: &mut eframe::egui::Ui,
        tx: &mpsc::UnboundedSender<ConsoleCommand>,
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
                        format!(
                            "{:02}:{:02}:{:02}.{:02}",
                            self.timecode.hours,
                            self.timecode.minutes,
                            self.timecode.seconds,
                            self.timecode.frames
                        )
                    }
                    ClockMode::System => {
                        let now = Local::now();
                        format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second())
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
                                self.bpm = (self.bpm - 0.1).max(20.0);
                            }

                            let bpm_text = format!("{:.1}", self.bpm);
                            let font_id = FontId::monospace(24.0);
                            ui.colored_label(
                                Color32::from_rgb(255, 215, 0),
                                RichText::new(bpm_text).font(font_id),
                            );

                            if ui.button("+").clicked() {
                                self.bpm = (self.bpm + 0.1).min(999.0);
                            }
                        });

                        ui.horizontal(|ui| {
                            if ui.button("-1.0").clicked() {
                                self.bpm = (self.bpm - 1.0).max(1.0);
                            }
                            if ui.button("+1.0").clicked() {
                                self.bpm = (self.bpm + 1.0).min(999.0);
                            }
                        });
                    });
                });

                ui.add_space(10.0);

                // Ableton Link
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Ableton Link");
                        let link_text = if self.link_enabled {
                            "LINK ●"
                        } else {
                            "LINK ○"
                        };
                        let link_color = if self.link_enabled {
                            Color32::from_rgb(66, 133, 244)
                        } else {
                            ui.style().visuals.text_color()
                        };

                        if ui
                            .button(RichText::new(link_text).color(link_color))
                            .clicked()
                        {
                            self.link_enabled = !self.link_enabled;
                            if self.link_enabled {
                                // Simulate peers connecting
                                self.link_peers = (rand::random::<u64>() % 3) + 1;
                            } else {
                                self.link_peers = 0;
                            }
                        }

                        let status_text = if self.link_enabled {
                            "Connected"
                        } else {
                            "Disabled"
                        };
                        ui.label(status_text);

                        let peers_text = if self.link_enabled {
                            if self.link_peers == 1 {
                                "1 peer connected".to_string()
                            } else {
                                format!("{} peers connected", self.link_peers)
                            }
                        } else {
                            "No peers connected".to_string()
                        };
                        ui.label(peers_text);
                    });
                });
            });

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
                            .color(match self.playback_state {
                                PlaybackState::Playing => ui.style().visuals.text_color(),
                                _ => Color32::from_rgb(120, 255, 120),
                            });

                    let play_button = ui.add_sized(
                        [button_width, button_height],
                        eframe::egui::Button::new(play_text),
                    );

                    if play_button.clicked() {
                        self.playback_state = PlaybackState::Playing;
                        let _ = tx.send(ConsoleCommand::Play);
                    }

                    // Hold button
                    let hold_text =
                        RichText::new("⏸ HOLD")
                            .size(18.0)
                            .color(match self.playback_state {
                                PlaybackState::Playing => Color32::from_rgb(255, 215, 0),
                                _ => ui.style().visuals.text_color(),
                            });

                    let hold_button = ui.add_sized(
                        [button_width, button_height],
                        eframe::egui::Button::new(hold_text),
                    );

                    if hold_button.clicked() {
                        self.playback_state = PlaybackState::Holding;
                        let _ = tx.send(ConsoleCommand::Pause);
                    }

                    // Stop button
                    let stop_text =
                        RichText::new("⏹ STOP")
                            .size(18.0)
                            .color(match self.playback_state {
                                PlaybackState::Stopped => ui.style().visuals.text_color(),
                                _ => Color32::from_rgb(255, 100, 100),
                            });

                    let stop_button = ui.add_sized(
                        [button_width, button_height],
                        eframe::egui::Button::new(stop_text),
                    );

                    if stop_button.clicked() {
                        self.playback_state = PlaybackState::Stopped;
                        let _ = tx.send(ConsoleCommand::Stop);
                    }
                });
            });
        });
    }

    pub fn set_playback_state(&mut self, state: PlaybackState) {
        self.playback_state = state;
    }

    pub fn set_timecode(&mut self, timecode: TimeCode) {
        self.timecode = timecode;
    }
}
