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
    preview_mode: bool,
    collapsed: bool,
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
            preview_mode: false,
            collapsed: false,
        }
    }
}

impl ProgrammerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle_collapsed(&mut self) {
        self.collapsed = !self.collapsed;
    }

    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    pub fn set_selected_fixtures(&mut self, fixtures: Vec<usize>) {
        self.selected_fixtures = fixtures;
    }

    pub fn add_selected_fixture(&mut self, fixture_id: usize) {
        if !self.selected_fixtures.contains(&fixture_id) {
            self.selected_fixtures.push(fixture_id);
        }
    }

    pub fn remove_selected_fixture(&mut self, fixture_id: usize) {
        self.selected_fixtures.retain(|&id| id != fixture_id);
    }

    pub fn clear_selected_fixtures(&mut self) {
        self.selected_fixtures.clear();
    }

    pub fn get_param(&self, param_name: &str) -> f32 {
        *self.params.get(param_name).unwrap_or(&0.0)
    }

    pub fn set_param(&mut self, param_name: &str, value: f32) {
        if let Some(param) = self.params.get_mut(param_name) {
            *param = value;
        }
    }

    // Main rendering function for the programmer panel
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.vertical(|ui| {
            // Programmer header with title and action buttons
            ui.horizontal(|ui| {
                let collapse_icon = if self.collapsed { "▶" } else { "▼" };
                if ui.button(collapse_icon).clicked() {
                    self.collapsed = !self.collapsed;
                    let _ = console_tx.send(ConsoleCommand::SetProgrammerCollapsed {
                        collapsed: self.collapsed,
                    });
                }

                ui.heading("PROGRAMMER");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("RECORD").clicked() {
                        // Record the current programmer state to a cue
                        if !self.new_cue_name.is_empty() {
                            let _ = console_tx.send(ConsoleCommand::RecordProgrammerToCue {
                                cue_name: self.new_cue_name.clone(),
                                list_index: None,
                            });
                        }
                    }

                    if ui.button("CLEAR").clicked() {
                        // Clear the programmer
                        let _ = console_tx.send(ConsoleCommand::ClearProgrammer);
                    }

                    if ui.button("HIGHLIGHT").clicked() {
                        // Highlight function would go here
                    }

                    // If the preview button is toggled on, enter preview mode
                    if ui
                        .add(egui::Button::new("PREVIEW").selected(self.preview_mode))
                        .clicked()
                    {
                        self.preview_mode = !self.preview_mode;
                        let _ = console_tx.send(ConsoleCommand::SetProgrammerPreviewMode {
                            preview_mode: self.preview_mode,
                        });
                    }

                    ui.label(format!(
                        "{} fixtures selected",
                        self.selected_fixtures.len()
                    ));
                });
            });

            // Only show the rest of the programmer if not collapsed
            if !self.collapsed {
                // Programmer tabs
                ui.horizontal(|ui| {
                    self.draw_tab_button(ui, "Intensity", ActiveProgrammerTab::Intensity);
                    self.draw_tab_button(ui, "Color", ActiveProgrammerTab::Color);
                    self.draw_tab_button(ui, "Position", ActiveProgrammerTab::Position);
                    self.draw_tab_button(ui, "Beam", ActiveProgrammerTab::Beam);
                });

                ui.separator();

                // Tab content and effects panel
                ui.horizontal(|ui| {
                    ui.vertical(|ui| match self.active_tab {
                        ActiveProgrammerTab::Intensity => self.show_intensity_tab(ui, console_tx),
                        ActiveProgrammerTab::Color => self.show_color_tab(ui, console_tx),
                        ActiveProgrammerTab::Position => self.show_position_tab(ui, console_tx),
                        ActiveProgrammerTab::Beam => self.show_beam_tab(ui, console_tx),
                    });
                    ui.set_min_size(Vec2::new(ui.available_width() - 250.0, 0.0));

                    ui.separator();

                    // Effects panel on the right
                    self.show_effects_panel(ui, state, console_tx);
                });
            } else {
                // When collapsed, show a compact summary of selected fixtures and active parameters
                ui.horizontal(|ui| {
                    if !self.selected_fixtures.is_empty() {
                        let active_tab_name = match self.active_tab {
                            ActiveProgrammerTab::Intensity => "Intensity",
                            ActiveProgrammerTab::Color => "Color",
                            ActiveProgrammerTab::Position => "Position",
                            ActiveProgrammerTab::Beam => "Beam",
                        };

                        ui.label(format!(
                            "{} fixtures | Active tab: {}",
                            self.selected_fixtures.len(),
                            active_tab_name
                        ));

                        // Show a few key parameters based on the active tab
                        match self.active_tab {
                            ActiveProgrammerTab::Intensity => {
                                ui.label(format!("Dimmer: {}%", self.get_param("dimmer").round()));
                            }
                            ActiveProgrammerTab::Color => {
                                let r = self.get_param("red").round() as u8;
                                let g = self.get_param("green").round() as u8;
                                let b = self.get_param("blue").round() as u8;
                                let color_preview = Color32::from_rgb(r, g, b);

                                ui.label("RGB:");
                                ui.painter().rect_filled(
                                    ui.available_rect_before_wrap(),
                                    4.0,
                                    color_preview,
                                );
                            }
                            ActiveProgrammerTab::Position => {
                                ui.label(format!(
                                    "Pan: {}° | Tilt: {}°",
                                    self.get_param("pan").round(),
                                    self.get_param("tilt").round()
                                ));
                            }
                            _ => {}
                        }
                    } else {
                        ui.label("No fixtures selected");
                    }
                });
            }
        });
    }

    pub fn render(
        &mut self,
        ctx: &egui::Context,
        state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.show(ui, state, console_tx);
        });
    }

    pub fn render_full_view(
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
                                let _ = console_tx.send(ConsoleCommand::RecordProgrammerToCue {
                                    cue_name: self.new_cue_name.clone(),
                                    list_index: None,
                                });
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
                    for (_, fixture) in state.fixtures.iter() {
                        let is_selected = self.selected_fixtures.contains(&fixture.id);
                        if ui.selectable_label(is_selected, &fixture.name).clicked() {
                            if is_selected {
                                self.remove_selected_fixture(fixture.id);
                            } else {
                                self.add_selected_fixture(fixture.id);
                            }
                            let _ = console_tx.send(ConsoleCommand::SetSelectedFixtures {
                                fixture_ids: self.selected_fixtures.clone(),
                            });
                        }
                    }
                });

                ui.separator();

                // Display programmer values grouped by fixture
                if !state.programmer_values.is_empty() {
                    ui.heading("Programmer Values");
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        // Group values by fixture_id
                        let mut fixture_values: HashMap<usize, Vec<(String, u8)>> = HashMap::new();
                        for ((fixture_id, channel), value) in &state.programmer_values {
                            fixture_values
                                .entry(*fixture_id)
                                .or_insert_with(Vec::new)
                                .push((channel.clone(), *value));
                        }

                        // Sort fixtures by ID
                        let mut sorted_fixtures: Vec<(usize, Vec<(String, u8)>)> =
                            fixture_values.into_iter().collect();
                        sorted_fixtures.sort_by_key(|(fixture_id, _)| *fixture_id);

                        for (fixture_id, values) in sorted_fixtures {
                            // Find the actual fixture to get its name
                            let fixture_name = state
                                .fixtures
                                .values()
                                .find(|f| f.id == fixture_id)
                                .map(|f| f.name.clone())
                                .unwrap_or_else(|| format!("Fixture #{}", fixture_id));

                            ui.collapsing(format!("{} (ID: {})", fixture_name, fixture_id), |ui| {
                                self.render_fixture_parameters(ui, &values);
                            });
                        }
                    });
                }

                // If there are any effects, show them in a separate section
                if !state.programmer_effects.is_empty() {
                    ui.add_space(10.0);
                    ui.heading("EFFECTS");
                    ui.separator();

                    for (i, (name, effect_type, fixture_ids)) in
                        state.programmer_effects.iter().enumerate()
                    {
                        ui.collapsing(format!("Effect #{}: {}", i + 1, name), |ui| {
                            self.render_effect_details(ui, name, *effect_type, fixture_ids);
                        });
                    }
                }
            });
        });
    }

    // Helper method to render parameters for a fixture
    fn render_fixture_parameters(&self, ui: &mut egui::Ui, values: &[(String, u8)]) {
        egui::Grid::new("fixture_params")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Parameter");
                ui.label("Value");
                ui.label("Graphical");
                ui.end_row();

                for (channel, value) in values {
                    ui.label(channel);
                    ui.label(format!("{}", value));

                    // Create a graphical representation based on parameter type
                    let progress = *value as f32 / 255.0;

                    if channel.to_lowercase().contains("red")
                        || channel.to_lowercase().contains("green")
                        || channel.to_lowercase().contains("blue")
                        || channel.to_lowercase().contains("white")
                    {
                        let color = if channel.to_lowercase().contains("red") {
                            Color32::from_rgb(*value, 0, 0)
                        } else if channel.to_lowercase().contains("green") {
                            Color32::from_rgb(0, *value, 0)
                        } else if channel.to_lowercase().contains("blue") {
                            Color32::from_rgb(0, 0, *value)
                        } else if channel.to_lowercase().contains("white") {
                            let v = *value;
                            Color32::from_rgb(v, v, v)
                        } else {
                            Color32::WHITE
                        };

                        let rect = ui.available_rect_before_wrap().shrink(2.0);
                        let response = ui.allocate_rect(rect, egui::Sense::hover());

                        ui.painter().rect_filled(response.rect, 4.0, color);
                    } else {
                        // For other parameter types, draw a progress bar
                        let rect = ui.available_rect_before_wrap().shrink(2.0);
                        let response = ui.allocate_rect(rect, egui::Sense::hover());

                        // Background
                        ui.painter()
                            .rect_filled(response.rect, 4.0, Color32::from_gray(30));

                        // Foreground
                        let filled_width = response.rect.width() * progress;
                        let filled_rect = egui::Rect::from_min_size(
                            response.rect.min,
                            egui::Vec2::new(filled_width, response.rect.height()),
                        );

                        ui.painter()
                            .rect_filled(filled_rect, 4.0, Color32::from_rgb(0, 150, 255));
                    }

                    ui.end_row();
                }
            });
    }

    // Helper method to render effect details
    fn render_effect_details(
        &self,
        ui: &mut egui::Ui,
        name: &str,
        effect_type: EffectType,
        fixture_ids: &[usize],
    ) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(format!("Name: {}", name));
                ui.label(format!("Type: {:?}", effect_type));
                ui.label(format!("Fixtures: {} fixtures", fixture_ids.len()));
            });

            // Add a visual preview of the effect
            let plot_height = 100.0;
            let plot_width = 200.0;

            Plot::new(format!("effect_plot_{:?}", effect_type))
                .height(plot_height)
                .width(plot_width)
                .show_axes([false, false])
                .view_aspect(2.0)
                .show(ui, |plot_ui| {
                    // Generate the waveform for this effect
                    let n_points = 100;
                    let mut points = Vec::with_capacity(n_points);

                    for i in 0..n_points {
                        let x = i as f64 / (n_points - 1) as f64 * 2.0 * std::f64::consts::PI;
                        let phase = 0.0; // Default phase
                        let ratio = 1.0; // Default ratio

                        // This is a simplified version - the actual effect could be more complex
                        let y = match effect_type {
                            EffectType::Sine => (x * ratio + phase).sin(),
                            EffectType::Square => {
                                if ((x * ratio + phase) % (2.0 * std::f64::consts::PI)).sin() >= 0.0
                                {
                                    1.0
                                } else {
                                    -1.0
                                }
                            }
                            EffectType::Sawtooth => {
                                let mut v = ((x * ratio + phase) % (2.0 * std::f64::consts::PI))
                                    / std::f64::consts::PI
                                    - 1.0;
                                if v > 1.0 {
                                    v -= 2.0
                                };
                                v
                            }
                            _ => (x * ratio + phase).sin(), // Default
                        };

                        points.push([x, y]);
                    }

                    let plot_points = PlotPoints::from(points);
                    plot_ui
                        .line(Line::new("", plot_points).color(Color32::from_rgb(100, 200, 255)));
                });
        });
    }

    // Helper function to draw tab buttons
    fn draw_tab_button(&mut self, ui: &mut egui::Ui, label: &str, tab: ActiveProgrammerTab) {
        let is_active = self.active_tab == tab;

        let mut button = egui::Button::new(label);
        if is_active {
            button = button
                .fill(Color32::from_rgb(30, 30, 30))
                .stroke(Stroke::new(1.0, Color32::from_rgb(0, 100, 255)));
        } else {
            button = button.fill(Color32::from_rgb(40, 40, 40));
        }

        if ui.add(button).clicked() {
            self.active_tab = tab;
        }
    }

    // Draw a vertical slider with scale markings
    fn vertical_slider(
        &mut self,
        ui: &mut egui::Ui,
        param_name: &str,
        display_name: &str,
        min: f32,
        max: f32,
        height: f32,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) -> bool {
        let mut value = self.get_param(param_name);
        let mut changed = false;

        ui.vertical(|ui| {
            ui.label(display_name);

            let display_value =
                if param_name.contains("color") || param_name == "dimmer" || param_name == "strobe"
                {
                    format!("{}%", (value / max * 100.0).round())
                } else if param_name.contains("pan") || param_name.contains("tilt") {
                    format!("{}°", value.round())
                } else {
                    format!("{}", value.round())
                };

            ui.label(display_value);

            // Create a custom vertical slider
            let slider_height = height;
            let slider_width = 36.0;
            let (rect, response) = ui.allocate_exact_size(
                Vec2::new(slider_width, slider_height),
                Sense::click_and_drag(),
            );

            if response.dragged() {
                let mouse_pos = response
                    .interact_pointer_pos()
                    .unwrap_or(Pos2::new(0.0, 0.0));
                let normalized = 1.0 - ((mouse_pos.y - rect.min.y) / slider_height).clamp(0.0, 1.0);
                value = min + normalized * (max - min);
                self.set_param(param_name, value);
                self.update_fixture_values(console_tx);
                changed = true;
            }

            // Draw the slider background
            ui.painter()
                .rect_filled(rect, 4.0, Color32::from_rgb(30, 30, 30));

            // Draw the fill
            let fill_height =
                ((value - min) / (max - min) * slider_height).clamp(0.0, slider_height);
            let fill_rect = Rect::from_min_size(
                Pos2::new(rect.min.x, rect.max.y - fill_height),
                Vec2::new(slider_width, fill_height),
            );

            // Choose appropriate slider color based on parameter
            let fill_color = if param_name == "red" {
                Color32::from_rgb(255, 50, 50)
            } else if param_name == "green" {
                Color32::from_rgb(50, 255, 50)
            } else if param_name == "blue" {
                Color32::from_rgb(50, 50, 255)
            } else if param_name == "white" {
                Color32::from_rgb(200, 200, 200)
            } else if param_name.contains("effect") {
                Color32::from_rgb(150, 50, 200)
            } else {
                Color32::from_rgb(0, 150, 255)
            };

            ui.painter().rect_filled(fill_rect, 4.0, fill_color);

            // Draw tick marks
            for i in 0..=4 {
                let y = rect.min.y + i as f32 * (slider_height / 4.0);
                ui.painter().line_segment(
                    [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
                    Stroke::new(1.0, Color32::from_rgb(70, 70, 70)),
                );
            }

            // Draw + and - buttons for some sliders
            if param_name == "dimmer" || param_name == "strobe" {
                ui.horizontal(|ui| {
                    if ui.button("-").clicked() {
                        value = (value - (max - min) / 20.0).max(min);
                        self.set_param(param_name, value);
                        self.update_fixture_values(console_tx);
                        changed = true;
                    }

                    if ui.button("+").clicked() {
                        value = (value + (max - min) / 20.0).min(max);
                        self.set_param(param_name, value);
                        self.update_fixture_values(console_tx);
                        changed = true;
                    }
                });
            }
        });

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

    // Intensity tab content
    fn show_intensity_tab(
        &mut self,
        ui: &mut egui::Ui,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.horizontal(|ui| {
            let spacing = 20.0;
            let slider_height = 180.0;

            ui.add_space(spacing);
            self.vertical_slider(
                ui,
                "dimmer",
                "Dimmer",
                0.0,
                100.0,
                slider_height,
                console_tx,
            );

            ui.add_space(spacing);
            self.vertical_slider(
                ui,
                "strobe",
                "Strobe",
                0.0,
                100.0,
                slider_height,
                console_tx,
            );

            ui.add_space(spacing * 2.0);
        });
    }

    // Color tab content
    fn show_color_tab(
        &mut self,
        ui: &mut egui::Ui,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.horizontal(|ui| {
            let spacing = 20.0;
            let slider_height = 180.0;

            ui.add_space(spacing);
            self.vertical_slider(ui, "red", "Red", 0.0, 255.0, slider_height, console_tx);

            ui.add_space(spacing);
            self.vertical_slider(ui, "green", "Green", 0.0, 255.0, slider_height, console_tx);

            ui.add_space(spacing);
            self.vertical_slider(ui, "blue", "Blue", 0.0, 255.0, slider_height, console_tx);

            ui.add_space(spacing);
            self.vertical_slider(ui, "white", "White", 0.0, 255.0, slider_height, console_tx);

            ui.add_space(spacing * 2.0);

            // Color presets
            ui.vertical(|ui| {
                ui.label("Presets");
                ui.add_space(5.0);

                // get a mutable copy of the color presets
                let color_presets = self.color_presets.clone();

                egui::Grid::new("color_presets")
                    .spacing([5.0, 5.0])
                    .show(ui, |ui| {
                        for (i, color) in color_presets.iter().enumerate() {
                            let button_size = Vec2::new(30.0, 30.0);
                            let (rect, response) =
                                ui.allocate_exact_size(button_size, Sense::click());

                            // Draw the colored button
                            ui.painter().rect_filled(rect, 4.0, *color);
                            ui.painter().rect_stroke(
                                rect,
                                4.0,
                                Stroke::new(1.0, Color32::from_gray(100)),
                                egui::StrokeKind::Inside,
                            );

                            if response.clicked() {
                                let r = color.r();
                                let g = color.g();
                                let b = color.b();

                                self.set_param("red", r as f32);
                                self.set_param("green", g as f32);
                                self.set_param("blue", b as f32);

                                if r == g && g == b && r > 200 {
                                    // White preset also sets white channel for RGBW fixtures
                                    self.set_param("white", 255.0);
                                } else {
                                    self.set_param("white", 0.0);
                                }
                                self.update_fixture_values(console_tx);
                            }

                            if (i + 1) % 2 == 0 {
                                ui.end_row();
                            }
                        }
                    });
            });
        });
    }

    // Position tab content
    fn show_position_tab(
        &mut self,
        ui: &mut egui::Ui,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.horizontal(|ui| {
            let spacing = 20.0;
            let slider_height = 180.0;

            ui.add_space(spacing);
            self.vertical_slider(ui, "pan", "Pan", 0.0, 360.0, slider_height, console_tx);

            ui.add_space(spacing);
            self.vertical_slider(ui, "tilt", "Tilt", 0.0, 270.0, slider_height, console_tx);

            ui.add_space(spacing * 2.0);

            // Position Grid
            ui.vertical(|ui| {
                ui.label("Position Grid");
                ui.add_space(5.0);

                let grid_size = 140.0;
                let (rect, response) = ui
                    .allocate_exact_size(Vec2::new(grid_size, grid_size), Sense::click_and_drag());

                // Draw position grid background
                ui.painter()
                    .rect_filled(rect, grid_size / 2.0, Color32::from_gray(30));
                ui.painter().rect_stroke(
                    rect,
                    grid_size / 2.0,
                    Stroke::new(1.0, Color32::from_gray(70)),
                    egui::StrokeKind::Inside,
                );

                // Draw crosshairs
                ui.painter().line_segment(
                    [
                        Pos2::new(rect.min.x, rect.center().y),
                        Pos2::new(rect.max.x, rect.center().y),
                    ],
                    Stroke::new(1.0, Color32::from_gray(70)),
                );
                ui.painter().line_segment(
                    [
                        Pos2::new(rect.center().x, rect.min.y),
                        Pos2::new(rect.center().x, rect.max.y),
                    ],
                    Stroke::new(1.0, Color32::from_gray(70)),
                );

                // Calculate the current position based on pan and tilt values
                let pan = self.get_param("pan");
                let tilt = self.get_param("tilt");

                let pan_normalized = (pan / 360.0).clamp(0.0, 1.0);
                let tilt_normalized = (tilt / 270.0).clamp(0.0, 1.0);

                let pos_x = rect.min.x + pan_normalized * grid_size;
                let pos_y = rect.min.y + (1.0 - tilt_normalized) * grid_size;

                // Draw the current position marker
                ui.painter().circle_filled(
                    Pos2::new(pos_x, pos_y),
                    6.0,
                    Color32::from_rgb(0, 150, 255),
                );

                // Update position if dragged
                if response.dragged() {
                    if let Some(mouse_pos) = response.interact_pointer_pos() {
                        let new_pan =
                            ((mouse_pos.x - rect.min.x) / grid_size * 360.0).clamp(0.0, 360.0);
                        let new_tilt = (1.0 - (mouse_pos.y - rect.min.y) / grid_size) * 270.0;

                        self.set_param("pan", new_pan);
                        self.set_param("tilt", new_tilt.clamp(0.0, 270.0));
                        self.update_fixture_values(console_tx);
                    }
                }
            });
        });
    }

    // Beam tab content
    fn show_beam_tab(
        &mut self,
        ui: &mut egui::Ui,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.horizontal(|ui| {
            let spacing = 20.0;
            let slider_height = 180.0;

            ui.add_space(spacing);
            self.vertical_slider(ui, "focus", "Focus", 0.0, 100.0, slider_height, console_tx);

            ui.add_space(spacing);
            self.vertical_slider(ui, "zoom", "Zoom", 0.0, 100.0, slider_height, console_tx);

            ui.add_space(spacing);
            self.vertical_slider(
                ui,
                "gobo_rotation",
                "Gobo Rot.",
                -180.0,
                180.0,
                slider_height,
                console_tx,
            );

            ui.add_space(spacing * 2.0);

            // Gobo selection
            ui.vertical(|ui| {
                ui.label("Gobo");
                let gobo_selection = self.get_param("gobo_selection") as usize;
                ui.label(format!("{}/8", gobo_selection + 1));

                egui::Grid::new("gobo_selection")
                    .spacing([5.0, 5.0])
                    .show(ui, |ui| {
                        for i in 0..8 {
                            let button_size = Vec2::new(30.0, 30.0);
                            let (rect, response) =
                                ui.allocate_exact_size(button_size, Sense::click());

                            // Draw the gobo button
                            let bg_color = if i == gobo_selection {
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
                                self.set_param("gobo_selection", i as f32);
                                self.update_fixture_values(console_tx);
                            }

                            if (i + 1) % 2 == 0 {
                                ui.end_row();
                            }
                        }
                    });
            });
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
                ui.add_space(10.0);
            });

            ui.vertical(|ui| {
                let tab_effect_opt = self.tab_effects.get(&self.active_tab);
                if let Some(tab_effect) = tab_effect_opt {
                    self.show_waveform_visualization(ui, tab_effect);
                }
            });
        });
    }

    fn render_effects_controls(
        &mut self,
        ui: &mut egui::Ui,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        // Get the current tab's effect config
        let active_tab = self.active_tab.clone();
        let tab_effect_mut = self.tab_effects.get_mut(&active_tab);

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
                if !self.selected_fixtures.is_empty() {
                    let effect_type = match tab_effect.effect_waveform {
                        0 => EffectType::Sine,
                        1 => EffectType::Square,
                        2 => EffectType::Sawtooth,
                        3 => EffectType::Triangle,
                        _ => EffectType::Sine,
                    };

                    let channel_type = match self.active_tab {
                        ActiveProgrammerTab::Intensity => "dimmer".to_string(),
                        ActiveProgrammerTab::Color => "color".to_string(),
                        ActiveProgrammerTab::Position => "pan".to_string(),
                        ActiveProgrammerTab::Beam => "beam".to_string(),
                    };

                    let _ = console_tx.send(ConsoleCommand::ApplyProgrammerEffect {
                        fixture_ids: self.selected_fixtures.clone(),
                        channel_type,
                        effect_type,
                        waveform: tab_effect.effect_waveform,
                        interval: tab_effect.effect_interval,
                        ratio: tab_effect.effect_ratio,
                        phase: tab_effect.effect_phase,
                        distribution: tab_effect.effect_distribution,
                        step_value: if tab_effect.effect_distribution == 1 {
                            Some(tab_effect.effect_step_value)
                        } else {
                            None
                        },
                        wave_offset: if tab_effect.effect_distribution == 2 {
                            Some(tab_effect.effect_wave_offset)
                        } else {
                            None
                        },
                    });
                }
            }
        }
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
}

pub struct Programmer {
    state: ProgrammerState,
}

impl Programmer {
    pub fn new() -> Self {
        Self {
            state: ProgrammerState::new(),
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        self.state.show(ui, state, console_tx);
    }

    pub fn render_full_view(
        &mut self,
        ctx: &egui::Context,
        state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        self.state.render_full_view(ctx, state, console_tx);
    }

    pub fn set_selected_fixtures(&mut self, selected_fixtures: Vec<usize>) {
        self.state.set_selected_fixtures(selected_fixtures);
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

pub fn render_compact(
    ui: &mut eframe::egui::Ui,
    state: &ConsoleState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    programmer_state: &mut ProgrammerState,
) {
    programmer_state.show(ui, state, console_tx);
}
