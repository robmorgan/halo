use std::sync::Arc;
use std::time::{Instant, SystemTime};

use console_adapter::ConsoleAdapter;
use eframe::egui;


pub mod console_adapter;
mod footer;
mod header;
mod utils;
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
    pub console: Arc<ConsoleAdapter>,
    last_update: Instant,
    current_time: SystemTime,
    active_tab: ActiveTab,
    fps: u32,
}

pub struct HaloAppSync {
    pub console: Arc<parking_lot::Mutex<halo_core::SyncLightingConsole>>,
    last_update: Instant,
    current_time: SystemTime,
    active_tab: ActiveTab,
    fps: u32,
}

impl HaloApp {
    fn new(_cc: &eframe::CreationContext<'_>, console: Arc<ConsoleAdapter>) -> Self {
        Self {
            console,
            last_update: Instant::now(),
            current_time: SystemTime::now(),
            active_tab: ActiveTab::Dashboard,
            fps: 60,
        }
    }
}

impl HaloAppSync {
    fn new(_cc: &eframe::CreationContext<'_>, console: Arc<parking_lot::Mutex<halo_core::SyncLightingConsole>>) -> Self {
        Self {
            console,
            last_update: Instant::now(),
            current_time: SystemTime::now(),
            active_tab: ActiveTab::Dashboard,
            fps: 60,
        }
    }
}

impl eframe::App for HaloAppSync {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        self.last_update = now;
        self.current_time = SystemTime::now();

        // Get the current state from the console
        let console_lock = self.console.lock();
        let fixtures = console_lock.fixtures().len();
        let cue_manager = console_lock.cue_manager();
        let playback_state = cue_manager.get_playback_state();
        drop(console_lock);
        
        // Use a default BPM for now
        let bpm = 120.0;

        // Header
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                // Simplified header for sync console
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });

        // Bottom UI
        egui::TopBottomPanel::bottom("footer_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("FPS: {}", self.fps));
                ui.label(format!("Fixtures: {}", fixtures));
                ui.label(format!("BPM: {:.1}", bpm));
                ui.label(format!("State: {:?}", playback_state));
            });
        });

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Halo Lighting Console (Sync Mode)");
            ui.label("Message passing architecture enabled");
            ui.label(format!("Active Fixtures: {}", fixtures));
            
            ui.add_space(20.0);
            
            // Basic controls
            ui.horizontal(|ui| {
                if ui.button("Play").clicked() {
                    let mut console_lock = self.console.lock();
                    console_lock.cue_manager().go();
                }
                if ui.button("Stop").clicked() {
                    let mut console_lock = self.console.lock();
                    console_lock.cue_manager().stop();
                }
                if ui.button("Pause").clicked() {
                    let mut console_lock = self.console.lock();
                    console_lock.cue_manager().hold();
                }
            });
            
            ui.add_space(10.0);
            
            // BPM control
            ui.horizontal(|ui| {
                ui.label("BPM:");
                if ui.button("-").clicked() {
                    let mut console_lock = self.console.lock();
                    console_lock.set_bpm(bpm - 1.0);
                }
                ui.label(format!("{:.1}", bpm));
                if ui.button("+").clicked() {
                    let mut console_lock = self.console.lock();
                    console_lock.set_bpm(bpm + 1.0);
                }
            });
        });

        // Request a repaint
        ctx.request_repaint();
    }
}

impl eframe::App for HaloApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        self.last_update = now;
        self.current_time = SystemTime::now();

        // Process any pending events from the console
        self.console.process_events();

        // Get the current state from the console
        let state = self.console.get_state();

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

        // Request a repaint
        ctx.request_repaint();
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

pub fn run_ui_sync(console: Arc<parking_lot::Mutex<halo_core::SyncLightingConsole>>) -> eframe::Result {
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
        Box::new(|cc| Ok(Box::new(HaloAppSync::new(cc, console)))),
    )
}
