use std::sync::Arc;
use std::time::{Instant, SystemTime};

use console_adapter::ConsoleAdapter;
use eframe::egui;
use halo_core::{ConsoleCommand, ConsoleEvent};

use crate::state::ConsoleState;

pub mod console_adapter;
mod footer;
mod header;
mod state;
mod utils;

// TODO - renable these when you get message passing working
// mod cue;
// mod cue_editor;
// mod fader;
// mod fixture;
// mod master;
// mod patch_panel;
// mod programmer;
// mod session;
// mod show_panel;
// mod timeline;

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
    current_time: SystemTime,
    active_tab: ActiveTab,
    fps: u32,
}

impl HaloApp {
    fn new(
        _cc: &eframe::CreationContext<'_>,
        engine_tx: mpsc::UnboundedSender<ConsoleCommand>,
        engine_rx: std::sync::mpsc::Receiver<ConsoleEvent>,
    ) -> Self {
        Self {
            state: EngineState::default(),
            engine_tx,
            engine_rx,
            last_update: Instant::now(),
            current_time: SystemTime::now(),
            active_tab: ActiveTab::Dashboard,
            fps: 60,
        }
    }

    fn process_engine_updates(&mut self) {
        while let Ok(event) = self.engine_rx.try_recv() {
            match event {
                EngineEvent::Update(update) => self.state.update(update),
            }
        }
    }

    fn render_ui(&mut self, ctx: &egui::Context) {
        // Header
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                header::render(ui, &mut self.active_tab, &self.console);
            });
        });

        // Bottom UI
        egui::TopBottomPanel::bottom("footer_panel").show(ctx, |ui| {
            footer::render(ui, &self.console, self.fps);
        });

        match self.active_tab {
            ActiveTab::Dashboard => {
                egui::SidePanel::right("right_panel").show(ctx, |ui| {
                    ui.set_min_width(400.0);
                    ui.heading("Session Info");
                    ui.label(format!("Fixtures: {}", state.fixtures.len()));
                    ui.label(format!("BPM: {:.1}", state.bpm));
                    ui.label(format!("Playback: {:?}", state.playback_state));
                    ui.label(format!("Link Enabled: {}", state.link_enabled));
                    ui.label(format!("Link Peers: {}", state.link_peers));
                });

                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Halo Lighting Console");
                    ui.label("Message passing architecture enabled");
                    ui.label(format!("Active Fixtures: {}", state.fixtures.len()));
                    ui.label(format!("Cue Lists: {}", state.cue_lists.len()));

                    ui.add_space(20.0);

                    // Basic controls
                    ui.horizontal(|ui| {
                        if ui.button("Play").clicked() {
                            let _ = self.console.play();
                        }
                        if ui.button("Stop").clicked() {
                            let _ = self.console.stop();
                        }
                        if ui.button("Pause").clicked() {
                            let _ = self.console.pause();
                        }
                    });

                    ui.add_space(10.0);

                    // BPM control
                    ui.horizontal(|ui| {
                        ui.label("BPM:");
                        if ui.button("-").clicked() {
                            let _ = self.console.set_bpm(state.bpm - 1.0);
                        }
                        ui.label(format!("{:.1}", state.bpm));
                        if ui.button("+").clicked() {
                            let _ = self.console.set_bpm(state.bpm + 1.0);
                        }
                    });
                });
            }
            ActiveTab::CueEditor => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Cue Editor");
                    ui.label("Cue editor functionality coming soon...");
                });
            }
            ActiveTab::Programmer => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Programmer");
                    ui.label("Programmer functionality coming soon...");
                });
            }
            ActiveTab::PatchPanel => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Patch Panel");
                    ui.label("Patch panel functionality coming soon...");
                });
            }
            ActiveTab::ShowManager => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Show Manager");
                    ui.label("Show manager functionality coming soon...");
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

        // Clear dirty flags and react to changes
        if self.state.dirty_flags.fixtures_changed {
            self.rebuild_fixture_ui_cache();
            self.state.dirty_flags.fixtures_changed = false;
        }

        if self.state.dirty_flags.cues_changed {
            self.update_cue_list_view();
            self.state.dirty_flags.cues_changed = false;
        }

        // Interpolate time-based values for smooth display
        self.interpolate_timecode();
        self.update_beat_animation();

        // Render UI
        self.render_ui(ctx);

        // Smart repaint based on playback state
        if self.state.playback_state.is_playing() {
            ctx.request_repaint(); // Continuous
        } else {
            ctx.request_repaint_after(Duration::from_millis(100)); // Slower
        }
    }
}

pub fn run_ui(console: Arc<ConsoleAdapter>) -> eframe::Result {
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
        Box::new(|cc| Ok(Box::new(HaloApp::new(cc, console)))),
    )
}
