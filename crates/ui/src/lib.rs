use std::sync::Arc;
use std::time::{Instant, SystemTime};

use console_adapter::ConsoleAdapter;
use cue::CuePanel;
use cue_editor::CueEditor;
use eframe::egui;
use fixture::FixtureGrid;
use halo_core::EffectType;
use master::{MasterPanel, OverridesPanel};
use patch_panel::PatchPanel;
use programmer::Programmer;
use session::SessionPanel;
use show_panel::ShowPanel;
use timeline::Timeline;

mod console_adapter;
mod cue;
mod cue_editor;
mod fader;
mod fixture;
mod footer;
mod header;
mod master;
mod patch_panel;
mod programmer;
mod session;
mod show_panel;
mod timeline;
mod utils;

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
    selected_fixture_index: Option<usize>,
    selected_cue_index: Option<usize>,
    selected_chase_index: Option<usize>,
    selected_step_index: Option<usize>,
    new_fixture_name: String,
    new_channel_name: String,
    new_cue_name: String,
    new_chase_name: String,
    step_duration_ms: u64,
    effect_frequency: f32,
    effect_amplitude: f32,
    effect_offset: f32,
    selected_effect_type: EffectType,
    temp_color_value: [f32; 3], // RGB for color picker
    fps: u32,
    overrides_panel: OverridesPanel,
    master_panel: MasterPanel,
    fixture_grid: fixture::FixtureGrid,
    session_panel: session::SessionPanel,
    cue_panel: cue::CuePanel,
    programmer: programmer::Programmer,
    timeline: timeline::Timeline,
    cue_editor: cue_editor::CueEditor,
    patch_panel: PatchPanel,
    show_panel: show_panel::ShowPanel,
}

impl HaloApp {
    fn new(_cc: &eframe::CreationContext<'_>, console: Arc<ConsoleAdapter>) -> Self {
        Self {
            console,
            last_update: Instant::now(),
            current_time: SystemTime::now(),
            active_tab: ActiveTab::Dashboard,
            selected_fixture_index: None,
            selected_cue_index: None,
            selected_chase_index: None,
            selected_step_index: None,
            new_fixture_name: String::new(),
            new_channel_name: String::new(),
            new_cue_name: String::new(),
            new_chase_name: String::new(),
            step_duration_ms: 1000,
            effect_frequency: 1.0,
            effect_amplitude: 1.0,
            effect_offset: 0.5,
            selected_effect_type: EffectType::Sine,
            temp_color_value: [0.5, 0.5, 0.5],
            fps: 60,
            overrides_panel: OverridesPanel::new(),
            master_panel: MasterPanel::new(),
            fixture_grid: FixtureGrid::default(),
            session_panel: SessionPanel::default(),
            cue_panel: CuePanel::default(),
            programmer: Programmer::new(),
            timeline: Timeline::new(),
            cue_editor: CueEditor::new(),
            patch_panel: PatchPanel::new(),
            show_panel: ShowPanel::new(),
        }
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

        // Update the session panel with the current playback state and time
        self.session_panel.set_playback_state(state.playback_state);
        if let Some(timecode) = state.timecode {
            self.session_panel.set_timecode(timecode);
        }

        // Update the programmer with the current fixtures and selection
        self.programmer.set_fixtures(state.fixtures.clone());
        self.programmer
            .set_selected_fixtures(self.fixture_grid.selected_fixtures().clone());

        // Update the console with the current bpm if it changed
        if (self.session_panel.bpm - state.bpm).abs() > 0.001 {
            let _ = self.console.set_bpm(self.session_panel.bpm);
        }

        // Header
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                header::render(ui, &mut self.active_tab, &self.console);
            });
        });

        // Bottom UI
        egui::TopBottomPanel::bottom("footer_panel").show(ctx, |ui| {
            self.programmer.show(ui, &self.console);
            self.timeline.show(ui);
            footer::render(ui, &self.console, self.fps);
        });

        match self.active_tab {
            ActiveTab::Dashboard => {
                egui::SidePanel::right("right_panel").show(ctx, |ui| {
                    ui.set_min_width(400.0);
                    self.session_panel.render(ui, &self.console);
                    ui.separator();
                    self.cue_panel.render(ui, &self.console);
                });

                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        // Overrides Grid
                        self.overrides_panel.show(ui, &self.console);

                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(10.0);

                        // Master Panel
                        self.master_panel.show(ui, &self.console);
                    });

                    // Fixtures
                    let main_content_height = ui.available_height(); // Subtract header and footer heights
                    self.fixture_grid
                        .render(ui, &self.console, main_content_height - 60.0);
                    // TODO - Subtract the height of the overrides grid
                });
            }
            ActiveTab::CueEditor => {
                self.cue_editor.render(ctx, &self.console);
            }
            ActiveTab::Programmer => {
                self.programmer.render_full_view(ctx, &self.console);
            }
            ActiveTab::PatchPanel => {
                self.patch_panel.render(ctx, &self.console);
            }
            ActiveTab::ShowManager => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.show_panel.show(ui, &self.console);
                });
            }
        }

        // Request a repaint
        ctx.request_repaint();
    }
}

pub fn run_ui(console: Arc<ConsoleAdapter>) -> eframe::Result {
    let native_options = eframe::NativeOptions {
        // initial_window_size: Some(egui::vec2(400.0, 200.0)),
        // min_window_size: Some(egui::vec2(300.0, 150.0)),
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
