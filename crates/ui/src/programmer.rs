use std::collections::HashMap;
use std::f64::consts::PI;
use tokio::sync::mpsc;

use crate::state::ConsoleState;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use egui_plot::{Line, Plot, PlotPoints};
use halo_core::{ConsoleCommand, EffectType};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum ActiveProgrammerTab {
    Intensity,
    Color,
    Position,
    Beam,
}

#[derive(Debug, Clone)]
pub struct TabEffectConfig {
    pub effect_waveform: u8,
    pub effect_interval: u8,
    pub effect_ratio: f32,
    pub effect_phase: f32,
    pub effect_distribution: u8,
    pub effect_step_value: usize,
    pub effect_wave_offset: f32,
}

impl Default for TabEffectConfig {
    fn default() -> Self {
        Self {
            effect_waveform: 0,
            effect_interval: 0,
            effect_ratio: 1.0,
            effect_phase: 0.0,
            effect_distribution: 0,
            effect_step_value: 1,
            effect_wave_offset: 0.0,
        }
    }
}

pub struct ProgrammerState {
    pub new_cue_name: String,
    selected_fixtures: Vec<usize>,
    params: HashMap<String, f32>,
    color_presets: Vec<Color32>,
    active_tab: ActiveProgrammerTab,
    tab_effects: HashMap<ActiveProgrammerTab, TabEffectConfig>,
}

impl Default for ProgrammerState {
    fn default() -> Self {
        let mut params = HashMap::new();

        // Initialize default parameter values
        params.insert("dimmer".to_string(), 100.0);
        params.insert("strobe".to_string(), 0.0);
        params.insert("red".to_string(), 255.0);
        params.insert("green".to_string(), 127.0);
        params.insert("blue".to_string(), 0.0);
        params.insert("white".to_string(), 0.0);
        params.insert("pan".to_string(), 180.0);
        params.insert("tilt".to_string(), 90.0);
        params.insert("focus".to_string(), 50.0);
        params.insert("zoom".to_string(), 75.0);
        params.insert("gobo_rotation".to_string(), 0.0);
        params.insert("gobo_selection".to_string(), 2.0);

        // Initialize color presets
        let color_presets = vec![
            Color32::from_rgb(255, 0, 0),     // Red
            Color32::from_rgb(255, 127, 0),   // Orange
            Color32::from_rgb(255, 255, 0),   // Yellow
            Color32::from_rgb(0, 255, 0),     // Green
            Color32::from_rgb(0, 255, 255),   // Cyan
            Color32::from_rgb(0, 0, 255),     // Blue
            Color32::from_rgb(139, 0, 255),   // Purple
            Color32::from_rgb(255, 255, 255), // White
        ];

        // Initialize tab effects
        let mut tab_effects = HashMap::new();
        tab_effects.insert(ActiveProgrammerTab::Intensity, TabEffectConfig::default());
        tab_effects.insert(ActiveProgrammerTab::Color, TabEffectConfig::default());
        tab_effects.insert(ActiveProgrammerTab::Position, TabEffectConfig::default());
        tab_effects.insert(ActiveProgrammerTab::Beam, TabEffectConfig::default());

        Self {
            new_cue_name: String::new(),
            selected_fixtures: Vec::new(),
            params,
            color_presets,
            active_tab: ActiveProgrammerTab::Intensity,
            tab_effects,
        }
    }
}

impl ProgrammerState {
    pub fn render(
        &mut self,
        ctx: &egui::Context,
        state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                // Header area with global controls
                ui.horizontal(|ui| {
                    ui.heading("PROGRAMMER");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("RECORD").clicked() {
                            // Record the current programmer state to a cue
                            if !self.new_cue_name.is_empty() {
                                // TODO: Implement cue recording via message passing
                                ui.label("Cue recording not yet implemented");
                            }
                        }

                        if ui.button("CLEAR").clicked() {
                            // Clear the programmer
                            let _ = console_tx.send(ConsoleCommand::ClearProgrammer);
                        }

                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_cue_name)
                                .hint_text("New cue name...")
                                .desired_width(150.0),
                        );
                    });
                });

                ui.separator();

                // Fixture selection
                ui.heading("Fixtures");
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    for (idx, (_, fixture)) in state.fixtures.iter().enumerate() {
                        let is_selected = self.selected_fixtures.contains(&fixture.id);
                        if ui.selectable_label(is_selected, &fixture.name).clicked() {
                            if is_selected {
                                self.selected_fixtures.retain(|&x| x != fixture.id);
                            } else {
                                self.selected_fixtures.push(fixture.id);
                            }
                        }
                    }
                });

                ui.separator();

                // Tab selection
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(
                            matches!(self.active_tab, ActiveProgrammerTab::Intensity),
                            "Intensity",
                        )
                        .clicked()
                    {
                        self.active_tab = ActiveProgrammerTab::Intensity;
                    }
                    if ui
                        .selectable_label(
                            matches!(self.active_tab, ActiveProgrammerTab::Color),
                            "Color",
                        )
                        .clicked()
                    {
                        self.active_tab = ActiveProgrammerTab::Color;
                    }
                    if ui
                        .selectable_label(
                            matches!(self.active_tab, ActiveProgrammerTab::Position),
                            "Position",
                        )
                        .clicked()
                    {
                        self.active_tab = ActiveProgrammerTab::Position;
                    }
                    if ui
                        .selectable_label(
                            matches!(self.active_tab, ActiveProgrammerTab::Beam),
                            "Beam",
                        )
                        .clicked()
                    {
                        self.active_tab = ActiveProgrammerTab::Beam;
                    }
                });

                ui.separator();

                // Parameter controls based on active tab
                match self.active_tab {
                    ActiveProgrammerTab::Intensity => {
                        self.render_intensity_controls(ui, console_tx)
                    }
                    ActiveProgrammerTab::Color => self.render_color_controls(ui, console_tx),
                    ActiveProgrammerTab::Position => self.render_position_controls(ui, console_tx),
                    ActiveProgrammerTab::Beam => self.render_beam_controls(ui, console_tx),
                }

                ui.separator();

                // Effects panel
                self.show_effects_panel(ui, state, console_tx);
            });
        });
    }

    fn render_intensity_controls(
        &mut self,
        ui: &mut egui::Ui,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.heading("Intensity Parameters");

        ui.horizontal(|ui| {
            ui.label("Dimmer:");
            let mut dimmer = *self.params.get("dimmer").unwrap_or(&100.0);
            if ui
                .add(egui::Slider::new(&mut dimmer, 0.0..=100.0).text("Dimmer"))
                .changed()
            {
                self.params.insert("dimmer".to_string(), dimmer);
                self.update_fixture_values(console_tx);
            }
        });

        ui.horizontal(|ui| {
            ui.label("Strobe:");
            let mut strobe = *self.params.get("strobe").unwrap_or(&0.0);
            if ui
                .add(egui::Slider::new(&mut strobe, 0.0..=255.0).text("Strobe"))
                .changed()
            {
                self.params.insert("strobe".to_string(), strobe);
                self.update_fixture_values(console_tx);
            }
        });
    }

    fn render_color_controls(
        &mut self,
        ui: &mut egui::Ui,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.heading("Color Parameters");

        ui.horizontal(|ui| {
            ui.label("Red:");
            let mut red = *self.params.get("red").unwrap_or(&255.0);
            if ui
                .add(egui::Slider::new(&mut red, 0.0..=255.0).text("Red"))
                .changed()
            {
                self.params.insert("red".to_string(), red);
                self.update_fixture_values(console_tx);
            }
        });

        ui.horizontal(|ui| {
            ui.label("Green:");
            let mut green = *self.params.get("green").unwrap_or(&127.0);
            if ui
                .add(egui::Slider::new(&mut green, 0.0..=255.0).text("Green"))
                .changed()
            {
                self.params.insert("green".to_string(), green);
                self.update_fixture_values(console_tx);
            }
        });

        ui.horizontal(|ui| {
            ui.label("Blue:");
            let mut blue = *self.params.get("blue").unwrap_or(&0.0);
            if ui
                .add(egui::Slider::new(&mut blue, 0.0..=255.0).text("Blue"))
                .changed()
            {
                self.params.insert("blue".to_string(), blue);
                self.update_fixture_values(console_tx);
            }
        });

        ui.horizontal(|ui| {
            ui.label("White:");
            let mut white = *self.params.get("white").unwrap_or(&0.0);
            if ui
                .add(egui::Slider::new(&mut white, 0.0..=255.0).text("White"))
                .changed()
            {
                self.params.insert("white".to_string(), white);
                self.update_fixture_values(console_tx);
            }
        });

        // Color presets
        ui.add_space(10.0);
        ui.label("Color Presets:");
        ui.horizontal(|ui| {
            for (i, _color) in self.color_presets.iter().enumerate() {
                let color_name = match i {
                    0 => "Red",
                    1 => "Orange",
                    2 => "Yellow",
                    3 => "Green",
                    4 => "Cyan",
                    5 => "Blue",
                    6 => "Purple",
                    7 => "White",
                    _ => "Unknown",
                };

                if ui.button(color_name).clicked() {
                    // Set RGB values based on preset
                    match i {
                        0 => {
                            // Red
                            self.params.insert("red".to_string(), 255.0);
                            self.params.insert("green".to_string(), 0.0);
                            self.params.insert("blue".to_string(), 0.0);
                            self.params.insert("white".to_string(), 0.0);
                        }
                        1 => {
                            // Orange
                            self.params.insert("red".to_string(), 255.0);
                            self.params.insert("green".to_string(), 127.0);
                            self.params.insert("blue".to_string(), 0.0);
                            self.params.insert("white".to_string(), 0.0);
                        }
                        2 => {
                            // Yellow
                            self.params.insert("red".to_string(), 255.0);
                            self.params.insert("green".to_string(), 255.0);
                            self.params.insert("blue".to_string(), 0.0);
                            self.params.insert("white".to_string(), 0.0);
                        }
                        3 => {
                            // Green
                            self.params.insert("red".to_string(), 0.0);
                            self.params.insert("green".to_string(), 255.0);
                            self.params.insert("blue".to_string(), 0.0);
                            self.params.insert("white".to_string(), 0.0);
                        }
                        4 => {
                            // Cyan
                            self.params.insert("red".to_string(), 0.0);
                            self.params.insert("green".to_string(), 255.0);
                            self.params.insert("blue".to_string(), 255.0);
                            self.params.insert("white".to_string(), 0.0);
                        }
                        5 => {
                            // Blue
                            self.params.insert("red".to_string(), 0.0);
                            self.params.insert("green".to_string(), 0.0);
                            self.params.insert("blue".to_string(), 255.0);
                            self.params.insert("white".to_string(), 0.0);
                        }
                        6 => {
                            // Purple
                            self.params.insert("red".to_string(), 139.0);
                            self.params.insert("green".to_string(), 0.0);
                            self.params.insert("blue".to_string(), 255.0);
                            self.params.insert("white".to_string(), 0.0);
                        }
                        7 => {
                            // White
                            self.params.insert("red".to_string(), 255.0);
                            self.params.insert("green".to_string(), 255.0);
                            self.params.insert("blue".to_string(), 255.0);
                            self.params.insert("white".to_string(), 255.0);
                        }
                        _ => {}
                    }
                    self.update_fixture_values(console_tx);
                }
            }
        });
    }

    fn render_position_controls(
        &mut self,
        ui: &mut egui::Ui,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.heading("Position Parameters");

        ui.horizontal(|ui| {
            ui.label("Pan:");
            let mut pan = *self.params.get("pan").unwrap_or(&180.0);
            if ui
                .add(egui::Slider::new(&mut pan, 0.0..=360.0).text("Pan"))
                .changed()
            {
                self.params.insert("pan".to_string(), pan);
                self.update_fixture_values(console_tx);
            }
        });

        ui.horizontal(|ui| {
            ui.label("Tilt:");
            let mut tilt = *self.params.get("tilt").unwrap_or(&90.0);
            if ui
                .add(egui::Slider::new(&mut tilt, 0.0..=180.0).text("Tilt"))
                .changed()
            {
                self.params.insert("tilt".to_string(), tilt);
                self.update_fixture_values(console_tx);
            }
        });
    }

    fn render_beam_controls(
        &mut self,
        ui: &mut egui::Ui,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.heading("Beam Parameters");

        ui.horizontal(|ui| {
            ui.label("Focus:");
            let mut focus = *self.params.get("focus").unwrap_or(&50.0);
            if ui
                .add(egui::Slider::new(&mut focus, 0.0..=100.0).text("Focus"))
                .changed()
            {
                self.params.insert("focus".to_string(), focus);
                self.update_fixture_values(console_tx);
            }
        });

        ui.horizontal(|ui| {
            ui.label("Zoom:");
            let mut zoom = *self.params.get("zoom").unwrap_or(&75.0);
            if ui
                .add(egui::Slider::new(&mut zoom, 0.0..=100.0).text("Zoom"))
                .changed()
            {
                self.params.insert("zoom".to_string(), zoom);
                self.update_fixture_values(console_tx);
            }
        });

        ui.horizontal(|ui| {
            ui.label("Gobo Rotation:");
            let mut gobo_rotation = *self.params.get("gobo_rotation").unwrap_or(&0.0);
            if ui
                .add(egui::Slider::new(&mut gobo_rotation, 0.0..=360.0).text("Gobo Rotation"))
                .changed()
            {
                self.params
                    .insert("gobo_rotation".to_string(), gobo_rotation);
                self.update_fixture_values(console_tx);
            }
        });

        // Gobo selection buttons
        ui.horizontal(|ui| {
            ui.label("Gobo Selection:");
            let gobo_selection = *self.params.get("gobo_selection").unwrap_or(&2.0) as usize;

            for i in 0..10 {
                let button_size = Vec2::new(30.0, 30.0);
                let (rect, response) = ui.allocate_exact_size(button_size, Sense::click());

                // Draw the gobo button
                let bg_color = if i == gobo_selection - 1 {
                    Color32::from_rgb(0, 100, 200)
                } else {
                    Color32::from_rgb(40, 40, 40)
                };

                ui.painter().rect_filled(rect, 4.0, bg_color);
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    Stroke::new(1.0, Color32::from_gray(100)),
                    egui::StrokeKind::Inside,
                );

                // Draw the number in the center of the button
                let text = format!("{}", i + 1);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(12.0),
                    Color32::WHITE,
                );

                if response.clicked() {
                    self.params
                        .insert("gobo_selection".to_string(), (i + 1) as f32);
                    self.update_fixture_values(console_tx);
                }

                if (i + 1) % 2 == 0 {
                    ui.end_row();
                }
            }
        });
    }

    // Effects panel
    fn show_effects_panel(
        &mut self,
        ui: &mut egui::Ui,
        _state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_min_width(200.0);
                ui.heading("EFFECTS");

                // Add a dynamic subtitle based on the active tab
                let effects_subtitle = match self.active_tab {
                    ActiveProgrammerTab::Intensity => "Effects on Intensity",
                    ActiveProgrammerTab::Color => "Effects on Color",
                    ActiveProgrammerTab::Position => "Effects on Position",
                    ActiveProgrammerTab::Beam => "Effects on Beam",
                };
                ui.label(effects_subtitle);

                ui.add_space(5.0);

                // Render effects controls
                self.render_effects_controls(ui, console_tx);
            });

            ui.vertical(|ui| {
                // Create a temporary copy of the active tab for visualization
                let active_tab = self.active_tab.clone();
                if let Some(tab_effect) = self.tab_effects.get(&active_tab) {
                    self.show_waveform_visualization(ui, tab_effect);
                }
            });
        });
    }

    fn render_effects_controls(
        &mut self,
        ui: &mut egui::Ui,
        _console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        // Clone the current tab's effect config
        let tab_effect_mut = self.tab_effects.get_mut(&self.active_tab);

        if let Some(tab_effect) = tab_effect_mut {
            // Waveform dropdown
            egui::ComboBox::from_label("Waveform")
                .selected_text(match tab_effect.effect_waveform {
                    0 => "Sine",
                    1 => "Square",
                    2 => "Sawtooth",
                    3 => "Triangle",
                    _ => "Sine",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut tab_effect.effect_waveform, 0, "Sine");
                    ui.selectable_value(&mut tab_effect.effect_waveform, 1, "Square");
                    ui.selectable_value(&mut tab_effect.effect_waveform, 2, "Sawtooth");
                    ui.selectable_value(&mut tab_effect.effect_waveform, 3, "Triangle");
                });

            // Interval dropdown
            egui::ComboBox::from_label("Interval")
                .selected_text(match tab_effect.effect_interval {
                    0 => "Beat",
                    1 => "Bar",
                    2 => "Phrase",
                    _ => "Beat",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut tab_effect.effect_interval, 0, "Beat");
                    ui.selectable_value(&mut tab_effect.effect_interval, 1, "Bar");
                    ui.selectable_value(&mut tab_effect.effect_interval, 2, "Phrase");
                });

            ui.add_space(10.0);

            // Effect parameter sliders - simplified to avoid borrow checker issues
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("Ratio");
                    let mut ratio = tab_effect.effect_ratio;
                    if ui.add(egui::Slider::new(&mut ratio, 0.0..=2.0)).changed() {
                        tab_effect.effect_ratio = ratio;
                    }
                });

                ui.add_space(15.0);

                ui.vertical(|ui| {
                    ui.label("Phase");
                    let mut phase = tab_effect.effect_phase;
                    if ui.add(egui::Slider::new(&mut phase, 0.0..=360.0)).changed() {
                        tab_effect.effect_phase = phase;
                    }
                });
            });

            ui.add_space(10.0);

            // Distribution dropdown
            egui::ComboBox::from_label("Distribution")
                .selected_text(match tab_effect.effect_distribution {
                    0 => "All",
                    1 => "Step",
                    2 => "Wave",
                    _ => "All",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut tab_effect.effect_distribution, 0, "All");
                    ui.selectable_value(&mut tab_effect.effect_distribution, 1, "Step");
                    ui.selectable_value(&mut tab_effect.effect_distribution, 2, "Wave");
                });

            // After the Distribution dropdown
            ui.add_space(10.0);

            // Only show appropriate input field based on selected distribution
            match tab_effect.effect_distribution {
                1 => {
                    // Step distribution
                    ui.horizontal(|ui| {
                        ui.label("Step Value:");
                        let mut step_value = tab_effect.effect_step_value as i32;
                        if ui
                            .add(
                                egui::DragValue::new(&mut step_value)
                                    .range(1..=16)
                                    .speed(0.1),
                            )
                            .changed()
                        {
                            tab_effect.effect_step_value = step_value.max(1) as usize;
                        }
                    });
                }
                2 => {
                    // Wave distribution
                    ui.horizontal(|ui| {
                        ui.label("Wave Offset:");
                        let mut wave_offset = tab_effect.effect_wave_offset;
                        if ui
                            .add(egui::Slider::new(&mut wave_offset, 0.0..=180.0).suffix("°"))
                            .changed()
                        {
                            tab_effect.effect_wave_offset = wave_offset;
                        }
                    });
                }
                _ => {}
            }

            // Apply Effects Button
            if ui.button("Apply Effects").clicked() {
                // TODO: Implement effect application via message passing
                ui.label("Effect application not yet implemented");
            }
        }
        ui.add_space(10.0);
    }

    fn show_waveform_visualization(&self, ui: &mut egui::Ui, tab_effect: &TabEffectConfig) {
        // Get effect parameters from the current tab's effect config
        let waveform_type = tab_effect.effect_waveform;
        let ratio = tab_effect.effect_ratio;
        let phase_degrees = tab_effect.effect_phase;
        let phase_radians = phase_degrees * PI as f32 / 180.0;

        // Generate points for the selected waveform
        let n_points = 100;
        let mut points = Vec::with_capacity(n_points);

        for i in 0..n_points {
            let x = i as f64 / (n_points - 1) as f64 * 2.0 * PI;
            let phase = phase_radians as f64;
            let r = ratio as f64;

            // Calculate y based on waveform type
            let y = match waveform_type {
                0 => {
                    // Sine
                    (x * r + phase).sin()
                }
                1 => {
                    // Square
                    if ((x * r + phase) % (2.0 * PI)).sin() >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    }
                }
                2 => {
                    // Sawtooth
                    let mut v = ((x * r + phase) % (2.0 * PI)) / PI - 1.0;
                    if v > 1.0 {
                        v -= 2.0
                    };
                    v
                }
                3 => {
                    // Triangle
                    let p = (x * r + phase) % (2.0 * PI);
                    if p < PI {
                        -1.0 + 2.0 * p / PI
                    } else {
                        3.0 - 2.0 * p / PI
                    }
                }
                _ => (x * r + phase).sin(), // Default to sine
            };

            points.push([x, y]);
        }

        // Create plot points and line
        let plot_points = PlotPoints::from(points);
        let line = Line::new("", plot_points).color(match self.active_tab {
            ActiveProgrammerTab::Intensity => egui::Color32::from_rgb(0, 150, 255),
            ActiveProgrammerTab::Color => egui::Color32::from_rgb(255, 100, 100),
            ActiveProgrammerTab::Position => egui::Color32::from_rgb(100, 255, 100),
            ActiveProgrammerTab::Beam => egui::Color32::from_rgb(255, 200, 0),
        });

        // Create and show the plot
        Plot::new("effect_waveform")
            .height(120.0)
            .allow_zoom(false)
            .allow_drag(false)
            .show_axes([false, false])
            .include_y(-1.2)
            .include_y(1.2)
            .show(ui, |plot_ui| {
                plot_ui.line(line);
            });

        // Add title for the plot
        ui.label("Waveform Preview");
    }

    fn render_vertical_fader(
        &self,
        ui: &mut egui::Ui,
        value: &mut f32,
        min: f32,
        max: f32,
        height: f32,
    ) -> bool {
        let mut changed = false;
        let rect = ui
            .allocate_exact_size(Vec2::new(30.0, height), Sense::click_and_drag())
            .0;

        // Draw background
        ui.painter().rect_filled(rect, 4.0, Color32::from_gray(40));

        // Calculate value position
        let normalized_value = (*value - min) / (max - min);
        let value_y = rect.bottom() - normalized_value * rect.height();

        // Draw value indicator
        let indicator_rect = Rect::from_min_size(
            Pos2::new(rect.left(), value_y - 2.0),
            Vec2::new(rect.width(), 4.0),
        );
        ui.painter()
            .rect_filled(indicator_rect, 2.0, Color32::from_rgb(0, 150, 255));

        // Handle input
        if ui.is_rect_visible(rect) {
            let response = ui.interact(rect, ui.id().with("fader"), Sense::click_and_drag());
            if response.dragged() {
                let delta = -response.drag_delta().y;
                let value_delta = delta / rect.height() * (max - min);
                *value = (*value + value_delta).clamp(min, max);
                changed = true;
            }
        }

        // Draw value text
        let text = format!("{:.1}", *value);
        ui.painter().text(
            Pos2::new(rect.right() + 5.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::proportional(12.0),
            Color32::WHITE,
        );

        changed
    }

    fn update_fixture_values(&self, console_tx: &mpsc::UnboundedSender<ConsoleCommand>) {
        for &fixture_id in &self.selected_fixtures {
            for (channel, value) in &self.params {
                let _ = console_tx.send(ConsoleCommand::SetProgrammerValue {
                    fixture_id,
                    channel: channel.clone(),
                    value: *value as u8,
                });
            }
        }
    }
}

pub fn render(
    ui: &mut eframe::egui::Ui,
    state: &ConsoleState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    let mut programmer = ProgrammerState::default();
    programmer.render(ui.ctx(), state, console_tx);
}
