use chrono::{Local, Timelike};
use eframe::egui::{Align, Color32, FontId, Layout, RichText};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

use halo_core::LightingConsole;

enum ClockMode {
    TimeCode,
    System,
}

struct TimeCode {
    hours: u32,
    minutes: u32,
    seconds: u32,
    frames: u32,
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
    last_update: Instant,

    // BPM state
    bpm: f64,

    // Ableton Link state
    link_enabled: bool,
    link_peers: u64,
}

impl Default for SessionPanel {
    fn default() -> Self {
        Self {
            clock_mode: ClockMode::TimeCode,
            timecode: TimeCode {
                hours: 0,
                minutes: 0,
                seconds: 0,
                frames: 0,
            },
            last_update: Instant::now(),
            bpm: 120.0,
            link_enabled: false,
            link_peers: 0,
        }
    }
}

impl SessionPanel {
    pub fn render(&mut self, ui: &mut eframe::egui::Ui, _console: &Arc<Mutex<LightingConsole>>) {
        // Update clock
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);

        if elapsed > Duration::from_millis(33) {
            // ~30fps update
            self.last_update = now;

            match self.clock_mode {
                ClockMode::TimeCode => {
                    // Update timecode (at 30fps)
                    // TODO - in the future we'll allow the user to set the frame rate
                    self.timecode.frames += 1;
                    if self.timecode.frames >= 30 {
                        self.timecode.frames = 0;
                        self.timecode.seconds += 1;
                    }
                    if self.timecode.seconds >= 60 {
                        self.timecode.seconds = 0;
                        self.timecode.minutes += 1;
                    }
                    if self.timecode.minutes >= 60 {
                        self.timecode.minutes = 0;
                        self.timecode.hours += 1;
                    }
                }
                ClockMode::System => {
                    // System clock is updated when displayed
                }
            }
        }

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
            ui.group(|ui| {
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

            ui.add_space(10.0);

            // Ableton Link
            ui.group(|ui| {
                ui.label("Ableton Link");
                ui.horizontal(|ui| {
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

                    ui.vertical(|ui| {
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
        });
    }
}
