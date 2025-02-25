use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use halo_core::{Chase, ChaseStep, Cue, EffectMapping, EffectType, LightingConsole};
use halo_fixtures::Fixture;
use patch_panel::PatchPanel;
use visualizer::VisualizerState;

mod header;
mod patch_panel;
mod visualizer;

pub enum ActiveTab {
    Dashboard,
    CueEditor,
    Visualizer,
    PatchPanel,
}
pub struct HaloApp {
    pub console: Arc<Mutex<LightingConsole>>,
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
}

impl HaloApp {
    fn new(_cc: &eframe::CreationContext<'_>, console: Arc<Mutex<LightingConsole>>) -> Self {
        let mut app = Self {
            console,
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
        };

        // Initialize visualizer with existing fixtures
        app.visualizer_state
            .sync_fixtures_with_console(&app.console);

        app
    }
}

impl eframe::App for HaloApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                header::render(ui, &mut self.active_tab);
            });
        });

        egui::SidePanel::left("fixture_panel").show(ctx, |ui| {
            ui.heading("Fixtures");

            // Add new fixture UI
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut self.new_fixture_name);
                if ui.button("Add Fixture").clicked() && !self.new_fixture_name.is_empty() {
                    // TODO - patch fixture here
                    //let console = self.console.lock().unwrap();
                    // console.add_fixture(Fixture {
                    //     name: self.new_fixture_name.clone(),
                    //     channels: Vec::new(),
                    // });
                    self.new_fixture_name.clear();
                }
            });

            ui.separator();

            // List fixtures
            egui::ScrollArea::vertical().show(ui, |ui| {
                let fixtures;
                {
                    let console = self.console.lock().unwrap();
                    fixtures = console.fixtures.clone();
                    drop(console);
                }
                for (idx, fixture) in fixtures.iter().enumerate() {
                    let is_selected = self.selected_fixture_index == Some(idx);
                    if ui.selectable_label(is_selected, &fixture.name).clicked() {
                        self.selected_fixture_index = Some(idx);
                        return;
                    }
                }
            });
        });

        egui::SidePanel::right("cue_panel").show(ctx, |ui| {
            ui.heading("Cues");

            // Add new cue UI
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut self.new_cue_name);
                if ui.button("Add Cue").clicked() && !self.new_cue_name.is_empty() {
                    // TODO - add cues here
                    // let mut console = self.console.lock().unwrap();
                    // console.add_cue(Cue {
                    //     name: self.new_cue_name.clone(),
                    //     chases: Vec::new(),
                    //     ..Default::default()
                    // });
                    self.new_cue_name.clear();
                }
            });

            ui.separator();

            // List cues
            let console = self.console.lock().unwrap();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (idx, cue) in console.cues.iter().enumerate() {
                    let is_selected = self.selected_cue_index == Some(idx);
                    let is_current = console.current_cue == idx;
                    let label = if is_current {
                        format!("▶ {}", cue.name)
                    } else {
                        cue.name.clone()
                    };

                    if ui.selectable_label(is_selected, label).clicked() {
                        self.selected_cue_index = Some(idx);
                        // When selecting a cue, clear lower-level selections
                        self.selected_chase_index = None;
                        self.selected_step_index = None;
                    }
                }
            });

            ui.separator();

            // Go button to activate selected cue
            if let Some(cue_idx) = self.selected_cue_index {
                if ui.button("GO").clicked() {
                    let mut console = self.console.lock().unwrap();
                    console.current_cue = cue_idx;
                }
            }
            drop(console);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Respond based on what's selected
            if let Some(fixture_idx) = self.selected_fixture_index {
                //self.show_fixture_editor(ui, fixture_idx);
                // do nothing for now
                ui.label("Fixture Editor");
            } else if let Some(cue_idx) = self.selected_cue_index {
                self.show_cue_editor(ui, cue_idx);
            } else {
                ui.heading("Halo Lighting Console");
                ui.label("Select a fixture or cue to begin editing.");

                // Show status information
                let mut console = self.console.lock().unwrap();
                ui.separator();
                ui.heading("Status");
                ui.label(format!("Total Fixtures: {}", console.fixtures.len()));
                ui.label(format!("Total Cues: {}", console.cues.len()));
                ui.label(format!("Current Cue: {}", console.current_cue));

                // Show Ableton Link status
                let clock = console.link_state.get_clock_state();
                ui.label(format!("Ableton Link: Connected"));
                ui.label(format!("Tempo: {:.1} BPM", clock.tempo));
                ui.label(format!("Beat: {:.2}", clock.beats));
                ui.label(format!("Phase: {:.2}", clock.phase));

                // Quick visualizer preview
                ui.separator();
                ui.heading("Stage Preview");
                let preview_height = 200.0;
                let available_width = ui.available_width();
                let aspect_ratio =
                    self.visualizer_state.stage_width / self.visualizer_state.stage_depth;
                let preview_width = preview_height * aspect_ratio;
                drop(console);

                ui.allocate_ui(egui::vec2(preview_width, preview_height), |ui| {
                    // Simple preview - we'll calculate a scaling factor
                    let scale_x = preview_width / self.visualizer_state.stage_width;
                    let scale_y = preview_height / self.visualizer_state.stage_depth;

                    // Draw stage background
                    let stage_rect = ui.max_rect();
                    ui.painter()
                        .rect_filled(stage_rect, 0.0, egui::Color32::from_rgb(20, 20, 30));

                    // Draw fixtures (simplified)
                    for fixture_vis in &self.visualizer_state.fixtures {
                        let fixture_rect = egui::Rect::from_center_size(
                            egui::pos2(
                                stage_rect.min.x + fixture_vis.position.x * scale_x,
                                stage_rect.min.y + fixture_vis.position.y * scale_y,
                            ),
                            egui::vec2(fixture_vis.size.x * scale_x, fixture_vis.size.y * scale_y),
                        );

                        // Draw simple fixture representation
                        ui.painter()
                            .rect_filled(fixture_rect, 2.0, fixture_vis.color);
                    }
                });

                if ui.button("Open Full Visualizer").clicked() {
                    self.active_tab = ActiveTab::Visualizer;
                }

                self.render_timeline(ui);
            }
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

    fn render_timeline(&mut self, ui: &mut egui::Ui) {}

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
