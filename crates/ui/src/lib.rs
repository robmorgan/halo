use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Instant, SystemTime};

use cue::CuePanel;
use cue_editor::CueEditor;
use eframe::egui;
use fixture::FixtureGrid;
use halo_core::{EffectType, EventLoop, LightingConsole};
use master::{MasterPanel, OverridesPanel};
use parking_lot::Mutex;
use patch_panel::PatchPanel;
use programmer::Programmer;
use session::SessionPanel;
use show_panel::ShowPanel;
use timeline::Timeline;

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
    pub console: Arc<Mutex<LightingConsole>>,
    _event_thread: JoinHandle<()>,
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
    patch_panel: PatchPanel,
    fps: u32,
    overrides_panel: OverridesPanel,
    master_panel: MasterPanel,
    fixture_grid: fixture::FixtureGrid,
    session_panel: session::SessionPanel,
    cue_panel: cue::CuePanel,
    programmer: programmer::Programmer,
    timeline: timeline::Timeline,
    cue_editor: cue_editor::CueEditor,
    show_panel: show_panel::ShowPanel,
}

impl HaloApp {
    fn new(_cc: &eframe::CreationContext<'_>, console: Arc<Mutex<LightingConsole>>) -> Self {
        let mut event_loop = EventLoop::new(Arc::clone(&console), 44.0);

        // Spawn the event loop thread
        let event_thread = std::thread::Builder::new()
            .name("HaloWorker".to_string())
            .spawn(move || {
                event_loop.run();
            })
            .expect("failed to spawn thread");

        Self {
            console,
            _event_thread: event_thread,
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
            patch_panel: PatchPanel::new(),
            fps: 60,
            overrides_panel: OverridesPanel::new(),
            master_panel: MasterPanel::new(),
            fixture_grid: FixtureGrid::default(),
            session_panel: SessionPanel::default(),
            cue_panel: CuePanel::default(),
            programmer: Programmer::new(),
            timeline: Timeline::new(),
            cue_editor: CueEditor::new(),
            show_panel: ShowPanel::new(),
        }
    }
}

impl eframe::App for HaloApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        self.last_update = now;
        self.current_time = SystemTime::now();

        // Get the currently patched fixtures from the console
        let fixtures;
        {
            let console = self.console.lock();
            fixtures = console.fixtures.to_vec();
        }

        // Update the programmer with the current fixtures and selection
        self.programmer.set_fixtures(fixtures);
        self.programmer
            .set_selected_fixtures(self.fixture_grid.selected_fixtures().clone());

        // Header
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
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
                //self.patch_panel.show(ui, &self.console)
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

pub fn run_ui(console: Arc<Mutex<LightingConsole>>) -> eframe::Result {
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
