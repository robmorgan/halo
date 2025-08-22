use std::time::{Duration, Instant, SystemTime};
use tokio::sync::mpsc;

use eframe::egui;
use halo_core::{ConsoleCommand, ConsoleEvent};

use crate::state::ConsoleState;
mod footer;
mod header;
mod state;
mod utils;

// Enable all UI modules
mod cue;
mod cue_editor;
mod fader;
mod fixture;
mod master;
mod patch_panel;
mod programmer;
mod session;
mod show_panel;
mod timeline;

pub enum ActiveTab {
    Dashboard,
    Programmer,
    CueEditor,
    PatchPanel,
    ShowManager,
}

pub struct HaloApp {
    state: ConsoleState,

    // Communication channels
    console_tx: mpsc::UnboundedSender<ConsoleCommand>,
    console_rx: std::sync::mpsc::Receiver<ConsoleEvent>,

    last_update: Instant,
    last_link_query: Instant,
    current_time: SystemTime,
    active_tab: ActiveTab,
    fps: u32,
}

impl HaloApp {
    fn new(
        _cc: &eframe::CreationContext<'_>,
        console_tx: mpsc::UnboundedSender<ConsoleCommand>,
        console_rx: std::sync::mpsc::Receiver<ConsoleEvent>,
    ) -> Self {
        Self {
            state: ConsoleState::default(),
            console_tx,
            console_rx,
            last_update: Instant::now(),
            last_link_query: Instant::now(),
            current_time: SystemTime::now(),
            active_tab: ActiveTab::Dashboard,
            fps: 60,
        }
    }

    fn process_engine_updates(&mut self) {
        while let Ok(event) = self.console_rx.try_recv() {
            self.state.update(event);
        }
    }

    fn render_ui(&mut self, ctx: &egui::Context) {
        // Header
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                header::render(ui, &mut self.active_tab, &self.console_tx, &self.state);
            });
        });

        // Bottom UI
        egui::TopBottomPanel::bottom("footer_panel").show(ctx, |ui| {
            footer::render(ui, &self.console_tx, &self.state, self.fps);
        });

        match self.active_tab {
            ActiveTab::Dashboard => {
                egui::SidePanel::right("right_panel").show(ctx, |ui| {
                    ui.set_min_width(400.0);
                    ui.heading("Session Info");
                    ui.label(format!("Fixtures: {}", self.state.fixtures.len()));
                    ui.label(format!("BPM: {:.1}", self.state.bpm));
                    ui.label(format!("Playback: {:?}", self.state.playback_state));
                    
                    // Link status with color
                    let (link_status, link_color) = if self.state.link_enabled {
                        ("● Link Active", egui::Color32::GREEN)
                    } else {
                        ("○ Link Inactive", egui::Color32::RED)
                    };
                    ui.colored_label(link_color, link_status);
                    ui.label(format!("Peers: {}", self.state.link_peers));
                });

                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Halo Lighting Console");
                    ui.label("Message passing architecture enabled");
                    ui.label(format!("Active Fixtures: {}", self.state.fixtures.len()));
                    ui.label(format!("Cue Lists: {}", self.state.cue_lists.len()));

                    ui.add_space(20.0);

                    // Basic controls
                    ui.horizontal(|ui| {
                        if ui.button("Play").clicked() {
                            let _ = self.console_tx.send(ConsoleCommand::Play);
                        }
                        if ui.button("Stop").clicked() {
                            let _ = self.console_tx.send(ConsoleCommand::Stop);
                        }
                        if ui.button("Pause").clicked() {
                            let _ = self.console_tx.send(ConsoleCommand::Pause);
                        }
                    });

                    ui.add_space(10.0);

                    // BPM control
                    ui.horizontal(|ui| {
                        ui.label("BPM:");
                        if ui.button("-").clicked() {
                            let _ = self.console_tx.send(ConsoleCommand::SetBpm {
                                bpm: self.state.bpm - 1.0,
                            });
                        }
                        ui.label(format!("{:.1}", self.state.bpm));
                        if ui.button("+").clicked() {
                            let _ = self.console_tx.send(ConsoleCommand::SetBpm {
                                bpm: self.state.bpm + 1.0,
                            });
                        }
                    });
                });
            }
            ActiveTab::CueEditor => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    cue_editor::render(ui, &self.state, &self.console_tx);
                });
            }
            ActiveTab::Programmer => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    programmer::render(ui, &self.state, &self.console_tx);
                });
            }
            ActiveTab::PatchPanel => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    patch_panel::render(ui, &self.state, &self.console_tx);
                });
            }
            ActiveTab::ShowManager => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show_panel::render(ui, &self.state, &self.console_tx);
                });
            }
        }
    }
}

impl eframe::App for HaloApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        self.last_update = now;
        self.current_time = SystemTime::now();

        // Process all updates first
        self.process_engine_updates();

        // Periodically query Link state (every 2 seconds)
        if now.duration_since(self.last_link_query).as_secs() >= 2 {
            let _ = self.console_tx.send(ConsoleCommand::QueryLinkState);
            self.last_link_query = now;
        }

        // Render UI
        self.render_ui(ctx);

        // Smart repaint based on playback state
        if matches!(self.state.playback_state, halo_core::PlaybackState::Playing) {
            ctx.request_repaint(); // Continuous
        } else {
            ctx.request_repaint_after(Duration::from_millis(100)); // Slower
        }
    }
}

pub fn run_ui(
    console_tx: mpsc::UnboundedSender<ConsoleCommand>,
    console_rx: std::sync::mpsc::Receiver<ConsoleEvent>,
) -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder {
            title: Some(String::from("Halo")),
            app_id: Some(String::from("io.github.robmorgan.halo")),
            maximized: Some(true),
            ..eframe::egui::ViewportBuilder::default()
        },
        ..Default::default()
    };

    eframe::run_native(
        "Halo",
        native_options,
        Box::new(|cc| Ok(Box::new(HaloApp::new(cc, console_tx, console_rx)))),
    )
}
