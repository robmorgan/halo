use cue::CuePanel;
use eframe::egui;
use fixture_grid::FixtureGrid;
use session::SessionPanel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use halo_core::{Chase, ChaseStep, EffectMapping, EffectType, LightingConsole};
use halo_fixtures::Fixture;
use patch_panel::PatchPanel;
use visualizer::VisualizerState;

mod cue;
mod fixture_grid;
mod footer;
mod header;
mod patch_panel;
mod session;
mod utils;
mod visualizer;

pub enum ActiveTab {
    Dashboard,
    Programmer,
    CueEditor,
    Visualizer,
    PatchPanel,
}
pub struct HaloApp {
    pub console: Arc<Mutex<LightingConsole>>,
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
    visualizer_state: VisualizerState,
    patch_panel: PatchPanel,
    show_visualizer_window: bool,
    fps: u32,
}

impl HaloApp {
    fn new(_cc: &eframe::CreationContext<'_>, console: Arc<Mutex<LightingConsole>>) -> Self {
        let mut app = Self {
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
            visualizer_state: VisualizerState::new(),
            patch_panel: PatchPanel::new(),
            show_visualizer_window: false,
            fps: 60,
        };

        // Initialize visualizer with existing fixtures
        app.visualizer_state
            .sync_fixtures_with_console(&app.console);

        app
    }
}

impl eframe::App for HaloApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);
        self.last_update = now;
        self.current_time = SystemTime::now();

        // Simulate random FPS changes
        if now.elapsed().as_millis() % 500 == 0 {
            self.fps = 58 + (rand::random::<u32>() % 5);
        }

        // Header
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                header::render(ui, &mut self.active_tab);
            });
        });

        // Footer
        egui::TopBottomPanel::bottom("footer_panel").show(ctx, |ui| {
            // Display the programmer
            ui.label("Programmer");

            footer::render(ui, &self.console, self.fps);
        });

        egui::SidePanel::right("right_panel").show(ctx, |ui| {
            ui.heading("Right Panel");

            // Render Session panel
            let mut session_panel = SessionPanel::default();
            session_panel.render(ui);

            ui.separator();

            // Render Cue List
            let mut cue_panel = CuePanel::default();
            cue_panel.render(ui, &self.console);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Programmer
            //ui.heading("Programmer");
            // let mut programmer_panel = ProgrammerPanel::default();
            // programmer_panel.render(ui);

            // Fixtures
            // TODO - somehow we need to extract the selected fixture id from the component
            // and pass it upstream.
            let main_content_height = ui.available_height() - 80.0; // Subtract header and footer heights
            let mut fixture_grid = FixtureGrid::default();
            fixture_grid.render(ui, &self.console, main_content_height - 120.0);
            // Subtract the height of the overrides grid
        });

        // Request a repaint
        ctx.request_repaint();
    }
}

impl HaloApp {
    // fn show_fixture_editor(&mut self, ui: &mut egui::Ui, fixture_idx: usize) {
    //     let mut console = self.console.lock().unwrap();
    //     let fixture = &mut console.fixtures[fixture_idx];

    //     ui.heading(format!("Editing Fixture: {}", fixture.name));

    //     // Channel editor
    //     ui.heading("Channels");
    //     ui.horizontal(|ui| {
    //         ui.label("Channel Name:");
    //         ui.text_edit_singleline(&mut self.new_channel_name);
    //         if ui.button("Add Channel").clicked() && !self.new_channel_name.is_empty() {
    //             fixture.channels.push(Channel {
    //                 name: self.new_channel_name.clone(),
    //                 value: 0,
    //             });
    //             self.new_channel_name.clear();
    //         }
    //     });

    //     ui.separator();

    //     // List and edit channels
    //     egui::ScrollArea::vertical().show(ui, |ui| {
    //         for (idx, channel) in fixture.channels.iter_mut().enumerate() {
    //             ui.horizontal(|ui| {
    //                 ui.label(format!("{}:", channel.name));
    //                 let mut value = channel.value as f32;
    //                 if ui.add(egui::Slider::new(&mut value, 0.0..=255.0)).changed() {
    //                     channel.value = value as u8;
    //                 }
    //                 if ui.button("Remove").clicked() {
    //                     fixture.channels.remove(idx);
    //                 }
    //             });
    //         }
    //     });
    // }

    fn show_cue_editor(&mut self, ui: &mut egui::Ui, cue_idx: usize) {
        let mut console = self.console.lock().unwrap();
        let fixtures: Vec<Fixture> = console.fixtures.iter().cloned().collect();
        let cue = &mut console.cues[cue_idx];

        // Chase editor
        ui.heading("Chases");
        ui.horizontal(|ui| {
            ui.label("Chase Name:");
            ui.text_edit_singleline(&mut self.new_chase_name);
            if ui.button("Add Chase").clicked() && !self.new_chase_name.is_empty() {
                cue.chases
                    .push(Chase::new(self.new_chase_name.clone(), Vec::new(), None));
                self.new_chase_name.clear();
            }
        });

        ui.separator();

        // List chases
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (idx, chase) in cue.chases.iter().enumerate() {
                let is_selected = self.selected_chase_index == Some(idx);
                if ui.selectable_label(is_selected, &chase.name).clicked() {
                    self.selected_chase_index = Some(idx);
                    self.selected_step_index = None;
                }
            }
        });

        // Show chase editor if one is selected
        if let Some(chase_idx) = self.selected_chase_index {
            if chase_idx < cue.chases.len() {
                let chase = &mut cue.chases[chase_idx];

                ui.separator();
                ui.heading(format!("Editing Chase: {}", chase.name));

                // Step editor
                ui.heading("Steps");
                ui.horizontal(|ui| {
                    ui.label("Duration (ms):");
                    ui.add(egui::DragValue::new(&mut self.step_duration_ms).speed(10));
                    if ui.button("Add Step").clicked() {
                        chase.steps.push(ChaseStep {
                            duration: Duration::from_millis(self.step_duration_ms),
                            effect_mappings: Vec::new(),
                            static_values: Vec::new(),
                        });
                    }
                });

                ui.separator();

                // List steps
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (idx, step) in chase.steps.iter().enumerate() {
                        let is_selected = self.selected_step_index == Some(idx);
                        if ui
                            .selectable_label(
                                is_selected,
                                format!("Step {} ({} ms)", idx + 1, step.duration.as_millis()),
                            )
                            .clicked()
                        {
                            self.selected_step_index = Some(idx);
                        }
                    }
                });

                // Show effect editor if a step is selected
                if let Some(step_idx) = self.selected_step_index {
                    if step_idx < chase.steps.len() {
                        let step = &mut chase.steps[step_idx];

                        ui.separator();
                        ui.heading(format!("Editing Step {}", step_idx + 1));

                        // Effect editor
                        ui.heading("Effects");

                        // Effect parameters
                        ui.horizontal(|ui| {
                            ui.label("Effect Type:");
                            ui.radio_value(
                                &mut self.selected_effect_type,
                                EffectType::Sine,
                                "Sine",
                            );
                            // ui.radio_value(
                            //     &mut self.selected_effect_type,
                            //     EffectType::Sawtooth,
                            //     "Sawtooth",
                            // );
                        });

                        ui.horizontal(|ui| {
                            ui.label("Frequency:");
                            ui.add(
                                egui::Slider::new(&mut self.effect_frequency, 0.1..=10.0)
                                    .text("Hz"),
                            );
                        });

                        ui.horizontal(|ui| {
                            ui.label("Amplitude:");
                            ui.add(egui::Slider::new(&mut self.effect_amplitude, 0.0..=1.0));
                        });

                        ui.horizontal(|ui| {
                            ui.label("Offset:");
                            ui.add(egui::Slider::new(&mut self.effect_offset, 0.0..=1.0));
                        });

                        // Color picker helper for RGB fixtures
                        ui.horizontal(|ui| {
                            ui.label("Quick Color:");
                            ui.color_edit_button_rgb(&mut self.temp_color_value);
                            if ui.button("Apply to RGB").clicked() {
                                // Find RGB channels and apply
                                // This is a placeholder - you'd need to implement the logic
                                // to identify RGB channels in the selected fixture
                            }
                        });

                        ui.separator();

                        // Fixture/channel selector for effect
                        ui.heading("Add Effect to Channel");
                        egui::ComboBox::from_label("Fixture")
                            .selected_text(if let Some(idx) = self.selected_fixture_index {
                                //fixture_names.get(idx).map_or("Select Fixture", |f| &f)
                                //fixtures.get(idx).map_or("Select Fixture", |f| &f)

                                fixtures.get(idx).map_or("Select Fixture", |f| &f.name)
                            } else {
                                "Select Fixture"
                            })
                            .show_ui(ui, |ui| {
                                for (idx, fixture) in fixtures.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.selected_fixture_index,
                                        Some(idx),
                                        fixture.name.clone().as_str(),
                                    );
                                }
                            });

                        if let Some(fixture_idx) = self.selected_fixture_index {
                            if let Some(fixture) = fixtures.get(fixture_idx) {
                                // Channel selector
                                let channel_names: Vec<&str> =
                                    fixture.channels.iter().map(|c| c.name.as_str()).collect();

                                if !channel_names.is_empty() {
                                    let mut selected_channel_idx = 0;
                                    egui::ComboBox::from_label("Channel")
                                        .selected_text(
                                            channel_names[selected_channel_idx].to_string(),
                                        )
                                        .show_ui(ui, |ui| {
                                            for (idx, name) in channel_names.iter().enumerate() {
                                                ui.selectable_value(
                                                    &mut selected_channel_idx,
                                                    idx,
                                                    *name,
                                                );
                                            }
                                        });

                                    // egui::ComboBox::from_label("Channel")
                                    //     .selected_text(format!("{radio:?}"))
                                    //     .show_ui(ui, |ui| {
                                    //         ui.selectable_value(
                                    //             selected_channel_idx,
                                    //             Enum::First,
                                    //             "First",
                                    //         );
                                    //         ui.selectable_value(
                                    //             selected_channel_idx,
                                    //             Enum::Second,
                                    //             "Second",
                                    //         );
                                    //         ui.selectable_value(
                                    //             selected_channel_idx,
                                    //             Enum::Third,
                                    //             "Third",
                                    //         );
                                    //     });

                                    // if ui.button("Add Effect").clicked() {
                                    //     step.effect_mappings.push((
                                    //         fixture_idx,
                                    //         selected_channel_idx,
                                    //         Effect {
                                    //             effect_type: self.selected_effect_type.clone(),
                                    //             frequency: self.effect_frequency,
                                    //             amplitude: self.effect_amplitude,
                                    //             offset: self.effect_offset,
                                    //             ..Default::default()
                                    //         },
                                    //     ));
                                    // }
                                } else {
                                    ui.label("No channels in selected fixture");
                                }
                            }
                        }

                        // List current effects in step
                        ui.separator();
                        ui.heading("Current Effects");

                        for (
                            i,
                            EffectMapping {
                                effect,
                                fixture_names,
                                channel_types,
                                distribution: _,
                            },
                        ) in step.effect_mappings.iter().enumerate()
                        {
                            // let fixture_name = console
                            //     .fixtures
                            //     .get(*fixture_idx)
                            //     .map_or("Unknown Fixture", |f| &f.name);

                            // let channel_name = console
                            //     .fixtures
                            //     .get(*fixture_idx)
                            //     .and_then(|f| f.channels.get(*channel_idx))
                            //     .map_or("Unknown Channel", |c| &c.name);

                            let effect_type = effect.effect_type.as_str();

                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "{:#?}.{:#?} - {} (f:{:.1}Hz, a:{:.2}, o:{:.2})",
                                    fixture_names,
                                    channel_types,
                                    effect_type,
                                    effect.frequency,
                                    effect.amplitude,
                                    effect.offset
                                ));

                                if ui.button("Remove").clicked() {
                                    // TODO: Figure out a way to remove the effect mapping from an already borrowed
                                    // effect mappings.
                                    //step.effect_mappings.remove(i);
                                }
                            });
                        }
                    }
                }
            }
        }
    }
}

pub fn run_ui(console: Arc<Mutex<LightingConsole>>) -> eframe::Result<()> {
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
