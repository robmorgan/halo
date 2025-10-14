use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use halo_core::{ConfigManager, ConsoleCommand, ConsoleEvent};
use tokio::sync::mpsc;

use crate::state::ConsoleState;
mod footer;
mod header;
mod settings;
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

    // Track if initial show load has been triggered
    initial_show_loaded: bool,
    show_file_path: Option<std::path::PathBuf>,

    // Configuration manager
    config_manager: ConfigManager,

    // Component state - maintain state between renders
    programmer_state: programmer::ProgrammerState,
    cue_editor_state: cue_editor::CueEditor,
    patch_panel_state: patch_panel::PatchPanelState,
    show_panel_state: show_panel::ShowPanelState,
    session_panel_state: session::SessionPanel,
    settings_panel: settings::SettingsPanel,
}

impl HaloApp {
    fn new(
        _cc: &eframe::CreationContext<'_>,
        console_tx: mpsc::UnboundedSender<ConsoleCommand>,
        console_rx: std::sync::mpsc::Receiver<ConsoleEvent>,
        show_file_path: Option<std::path::PathBuf>,
        config_manager: ConfigManager,
    ) -> Self {
        // Request initial data from console
        let _ = console_tx.send(ConsoleCommand::QueryFixtures);
        let _ = console_tx.send(ConsoleCommand::QueryCueLists);
        let _ = console_tx.send(ConsoleCommand::QueryCurrentCueListIndex);
        let _ = console_tx.send(ConsoleCommand::QueryCurrentCue);
        let _ = console_tx.send(ConsoleCommand::QueryPlaybackState);
        let _ = console_tx.send(ConsoleCommand::QueryRhythmState);
        let _ = console_tx.send(ConsoleCommand::QueryShow);
        let _ = console_tx.send(ConsoleCommand::QueryLinkState);

        Self {
            state: ConsoleState::default(),
            console_tx,
            console_rx,
            last_update: Instant::now(),
            last_link_query: Instant::now(),
            current_time: SystemTime::now(),
            active_tab: ActiveTab::Dashboard,
            fps: 60,
            initial_show_loaded: false,
            show_file_path,
            config_manager,
            programmer_state: programmer::ProgrammerState::default(),
            cue_editor_state: cue_editor::CueEditor::new(),
            patch_panel_state: patch_panel::PatchPanelState::default(),
            show_panel_state: show_panel::ShowPanelState::default(),
            session_panel_state: session::SessionPanel::default(),
            settings_panel: settings::SettingsPanel::new(),
        }
    }

    fn process_engine_updates(&mut self) {
        while let Ok(event) = self.console_rx.try_recv() {
            self.state.update(event);
        }
    }

    fn render_error_dialog(&mut self, ctx: &egui::Context) {
        if let Some(error) = self.state.last_error.clone() {
            egui::Window::new("Error")
                .collapsible(false)
                .resizable(true)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(400.0);
                    ui.vertical(|ui| {
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("⚠")
                                .size(40.0)
                                .color(egui::Color32::from_rgb(255, 100, 100)),
                        );
                        ui.add_space(10.0);

                        ui.label(egui::RichText::new(&error).color(egui::Color32::WHITE));

                        ui.add_space(20.0);

                        if ui.button("OK").clicked() {
                            self.state.last_error = None;
                        }
                    });
                });
        }
    }

    fn render_ui(&mut self, ctx: &egui::Context) {
        // Header
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                header::render(
                    ui,
                    &mut self.active_tab,
                    &self.console_tx,
                    &self.state,
                    &mut self.settings_panel,
                );
            });
        });

        // Bottom UI
        egui::TopBottomPanel::bottom("footer_panel").show(ctx, |ui| {
            // Sync programmer state from console state before rendering
            self.programmer_state
                .set_selected_fixtures(self.state.selected_fixtures.clone());
            self.programmer_state.sync_from_console_state(&self.state);

            // Show programmer panel
            programmer::render_compact(
                ui,
                &self.state,
                &self.console_tx,
                &mut self.programmer_state,
            );
            ui.separator();

            // Show timeline
            timeline::render(ui, &self.state, &self.console_tx);
            ui.separator();

            // Show footer status
            footer::render(ui, &self.console_tx, &self.state, self.fps);
        });

        match self.active_tab {
            ActiveTab::Dashboard => {
                egui::SidePanel::right("right_panel").show(ctx, |ui| {
                    ui.set_min_width(400.0);
                    self.session_panel_state
                        .render(ui, &self.state, &self.console_tx);
                    ui.separator();
                    cue::render(ui, &self.state, &self.console_tx);
                });

                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        // Master Panel with overrides and master faders
                        master::render(ui, &self.state, &self.console_tx);

                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(10.0);
                    });

                    // Fixtures Grid
                    let main_content_height = ui.available_height();
                    fixture::render_grid(
                        ui,
                        &self.state,
                        &self.console_tx,
                        main_content_height - 60.0,
                    );
                });
            }
            ActiveTab::CueEditor => {
                self.cue_editor_state
                    .render(ctx, &self.state, &self.console_tx);
            }
            ActiveTab::Programmer => {
                self.programmer_state
                    .render_full_view(ctx, &self.state, &self.console_tx);
            }
            ActiveTab::PatchPanel => {
                self.patch_panel_state
                    .render(ctx, &self.state, &self.console_tx);
            }
            ActiveTab::ShowManager => {
                self.show_panel_state
                    .render(ctx, &self.state, &self.console_tx);
            }
        }

        // Render settings panel (modal window)
        self.settings_panel
            .render(ctx, &self.state, &self.console_tx);
    }
}

impl eframe::App for HaloApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        self.last_update = now;
        self.current_time = SystemTime::now();

        // Load show file on first update if provided
        if !self.initial_show_loaded {
            if let Some(ref path) = self.show_file_path {
                println!("Loading show file on UI startup: {}", path.display());
                let _ = self
                    .console_tx
                    .send(ConsoleCommand::LoadShow { path: path.clone() });
            }
            self.initial_show_loaded = true;
        }

        // Process all updates first
        self.process_engine_updates();

        // Periodically query Link state (every 2 seconds)
        if now.duration_since(self.last_link_query).as_secs() >= 2 {
            let _ = self.console_tx.send(ConsoleCommand::QueryLinkState);
            self.last_link_query = now;
        }

        // Render UI
        self.render_ui(ctx);

        // Render error dialog on top of everything
        self.render_error_dialog(ctx);

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
    show_file_path: Option<std::path::PathBuf>,
    config_manager: ConfigManager,
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
        Box::new(move |cc| {
            Ok(Box::new(HaloApp::new(
                cc,
                console_tx,
                console_rx,
                show_file_path,
                config_manager,
            )))
        }),
    )
}
