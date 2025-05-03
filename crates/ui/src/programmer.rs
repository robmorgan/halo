use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::Arc;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use egui_plot::{Line, Plot, PlotPoints};
use halo_core::{
    Effect, EffectDistribution, EffectMapping, EffectParams, EffectType, Interval, LightingConsole,
    StaticValue,
};
use halo_fixtures::{Channel, ChannelType, Fixture};
use parking_lot::Mutex;

use crate::fader::render_vertical_fader;

// Define the active tab types for the programmer
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ActiveProgrammerTab {
    Intensity,
    Color,
    Position,
    Beam,
}

/// TabEffectConfig stores the state of the effect configuration for a specific tab
#[derive(Clone, Debug, PartialEq)]
struct TabEffectConfig {
    effect_waveform: usize,
    effect_interval: usize,
    effect_distribution: usize,
    effect_ratio: f32,
    effect_phase: f32,
    effect_step_value: usize,
    effect_wave_offset: f64,
}

impl Default for TabEffectConfig {
    fn default() -> Self {
        Self {
            effect_waveform: 0,
            effect_interval: 0,
            effect_distribution: 0,
            effect_ratio: 1.0,
            effect_phase: 0.0,
            effect_step_value: 1,
            effect_wave_offset: 30.0,
        }
    }
}

/// ProgrammerState holds the state of the programmer panel
pub struct ProgrammerState {
    pub new_cue_name: String,
    active_tab: ActiveProgrammerTab,
    selected_fixtures: Vec<usize>,
    params: HashMap<String, f32>,
    color_presets: Vec<Color32>,
    tab_effects: HashMap<ActiveProgrammerTab, TabEffectConfig>,
    preview_mode: bool,
    collapsed: bool,
}

impl Default for ProgrammerState {
    fn default() -> Self {
        let mut params = HashMap::new();
        let mut tab_effects = HashMap::new();

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

        // Initialize tab-specific effect configurations
        tab_effects.insert(ActiveProgrammerTab::Intensity, TabEffectConfig::default());
        tab_effects.insert(ActiveProgrammerTab::Color, TabEffectConfig::default());
        tab_effects.insert(ActiveProgrammerTab::Position, TabEffectConfig::default());
        tab_effects.insert(ActiveProgrammerTab::Beam, TabEffectConfig::default());

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

        Self {
            new_cue_name: String::new(),
            active_tab: ActiveProgrammerTab::Intensity,
            selected_fixtures: vec![],
            params,
            color_presets,
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

    pub fn _toggle_collapsed(&mut self) {
        self.collapsed = !self.collapsed;
    }

    pub fn _is_collapsed(&self) -> bool {
        self.collapsed
    }

    pub fn _set_selected_fixtures(&mut self, fixtures: Vec<usize>) {
        self.selected_fixtures = fixtures;
    }

    pub fn _add_selected_fixture(&mut self, fixture_id: usize) {
        if !self.selected_fixtures.contains(&fixture_id) {
            self.selected_fixtures.push(fixture_id);
        }
    }

    pub fn _remove_selected_fixture(&mut self, fixture_id: usize) {
        self.selected_fixtures.retain(|&id| id != fixture_id);
    }

    pub fn _clear_selected_fixtures(&mut self) {
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
}

pub struct Programmer {
    state: ProgrammerState,
    fixtures: Vec<Fixture>,
}

impl Programmer {
    pub fn new() -> Self {
        Self {
            state: ProgrammerState::new(),
            fixtures: vec![],
        }
    }

    // Main rendering function for the programmer panel
    pub fn show(&mut self, ui: &mut egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        ui.vertical(|ui| {
            // Programmer header with title and action buttons
            ui.horizontal(|ui| {
                let collapse_icon = if self.state.collapsed { "▶" } else { "▼" };
                if ui.button(collapse_icon).clicked() {
                    self.state.collapsed = !self.state.collapsed;
                }

                ui.heading("PROGRAMMER");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("RECORD").clicked() {
                        // Record function would go here
                    }

                    if ui.button("CLEAR").clicked() {
                        // Clear function would go here
                    }

                    if ui.button("HIGHLIGHT").clicked() {
                        // Highlight function would go here
                    }

                    // If the preview button is toggled on, enter preview mode
                    if ui
                        .add(egui::Button::new("PREVIEW").selected(self.state.preview_mode))
                        .clicked()
                    {
                        self.state.preview_mode = !self.state.preview_mode;
                    }

                    ui.label(format!(
                        "{} fixtures selected",
                        self.state.selected_fixtures.len()
                    ));
                });
            });

            // Only show the rest of the programmer if not collapsed
            if !self.state.collapsed {
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
                    ui.vertical(|ui| match self.state.active_tab {
                        ActiveProgrammerTab::Intensity => self.show_intensity_tab(ui),
                        ActiveProgrammerTab::Color => self.show_color_tab(ui),
                        ActiveProgrammerTab::Position => self.show_position_tab(ui),
                        ActiveProgrammerTab::Beam => self.show_beam_tab(ui),
                    });
                    ui.set_min_size(Vec2::new(ui.available_width() - 250.0, 0.0));

                    ui.separator();

                    // Effects panel on the right
                    self.show_effects_panel(ui, console);
                });
            } else {
                // When collapsed, show a compact summary of selected fixtures and active parameters
                ui.horizontal(|ui| {
                    if !self.state.selected_fixtures.is_empty() {
                        let active_tab_name = match self.state.active_tab {
                            ActiveProgrammerTab::Intensity => "Intensity",
                            ActiveProgrammerTab::Color => "Color",
                            ActiveProgrammerTab::Position => "Position",
                            ActiveProgrammerTab::Beam => "Beam",
                        };

                        ui.label(format!(
                            "{} fixtures | Active tab: {}",
                            self.state.selected_fixtures.len(),
                            active_tab_name
                        ));

                        // Show a few key parameters based on the active tab
                        match self.state.active_tab {
                            ActiveProgrammerTab::Intensity => {
                                ui.label(format!(
                                    "Dimmer: {}%",
                                    self.state.get_param("dimmer").round()
                                ));
                            }
                            ActiveProgrammerTab::Color => {
                                let r = self.state.get_param("red").round() as u8;
                                let g = self.state.get_param("green").round() as u8;
                                let b = self.state.get_param("blue").round() as u8;
                                //let w = self.state.get_param("white").round() as u8;
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
                                    self.state.get_param("pan").round(),
                                    self.state.get_param("tilt").round()
                                ));
                            }
                            _ => {}
                        }
                    } else {
                        ui.label("No fixtures selected");
                    }
                });
            }

            // Update programmer state based on parameter values
            self.update_fixtures(console);

            // Update preview mode based on button state
            let mut console_lock = console.lock();
            console_lock
                .programmer
                .set_preview_mode(self.state.preview_mode);
            drop(console_lock);
        });
    }

    pub fn render_full_view(&mut self, ctx: &egui::Context, console: &Arc<Mutex<LightingConsole>>) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                // Header area with global controls
                ui.horizontal(|ui| {
                    ui.heading("PROGRAMMER");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("RECORD").clicked() {
                            // Record the current programmer state to a cue
                            if !self.state.new_cue_name.is_empty() {
                                let mut console_lock = console.lock();
                                console_lock.record_cue(self.state.new_cue_name.clone(), 0.0);
                                drop(console_lock);
                            }
                        }

                        if ui.button("CLEAR").clicked() {
                            // Clear the programmer
                            let mut console_lock = console.lock();
                            console_lock.programmer.clear();
                            drop(console_lock);

                            // TODO - deselect fixtures also
                        }

                        ui.add(
                            egui::TextEdit::singleline(&mut self.state.new_cue_name)
                                .hint_text("New cue name...")
                                .desired_width(150.0),
                        );
                    });
                });

                ui.separator();

                // Get all programmer values from the console
                let programmer_values: Vec<StaticValue>;
                let effects: Vec<EffectMapping>;
                {
                    let console_lock = console.lock();
                    programmer_values = console_lock
                        .programmer
                        .get_values()
                        .iter()
                        .map(|v| StaticValue {
                            fixture_id: v.fixture_id,
                            channel_type: v.channel_type.clone(),
                            value: v.value,
                        })
                        .collect();

                    effects = console_lock.programmer.get_effects().clone();
                }

                // Group values by fixture_id
                let mut fixture_values: std::collections::HashMap<usize, Vec<StaticValue>> =
                    std::collections::HashMap::new();

                for value in programmer_values {
                    fixture_values
                        .entry(value.fixture_id)
                        .or_insert_with(Vec::new)
                        .push(value);
                }

                // Sort
                let mut sorted_fixtures: Vec<(usize, Vec<StaticValue>)> =
                    fixture_values.into_iter().collect();
                sorted_fixtures.sort_by_key(|(fixture_id, _)| *fixture_id);

                // Display fixture groups with their parameter values
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (fixture_id, values) in sorted_fixtures.iter() {
                        // Find the actual fixture to get its name
                        let fixture_name = self
                            .fixtures
                            .iter()
                            .find(|f| f.id == *fixture_id)
                            .map(|f| f.name.clone())
                            .unwrap_or_else(|| format!("Fixture #{}", fixture_id));

                        ui.collapsing(format!("{} (ID: {})", fixture_name, fixture_id), |ui| {
                            self.render_fixture_parameters(ui, values);
                        });
                    }

                    // If there are any effects, show them in a separate section
                    if !effects.is_empty() {
                        ui.add_space(10.0);
                        ui.heading("EFFECTS");
                        ui.separator();

                        for (i, effect) in effects.iter().enumerate() {
                            ui.collapsing(format!("Effect #{}: {}", i + 1, effect.name), |ui| {
                                self.render_effect_details(ui, effect);
                            });
                        }
                    }
                });
            });
        });
    }

    // Helper method to render parameters for a fixture
    fn render_fixture_parameters(&self, ui: &mut egui::Ui, values: &[StaticValue]) {
        egui::Grid::new("fixture_params")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Parameter");
                ui.label("Type");
                ui.label("Value");
                ui.label("Graphical");
                ui.end_row();

                for value in values {
                    // Parameter name
                    let param_name = match &value.channel_type {
                        ChannelType::Dimmer => "Dimmer",
                        ChannelType::Red => "Red",
                        ChannelType::Green => "Green",
                        ChannelType::Blue => "Blue",
                        ChannelType::White => "White",
                        ChannelType::Pan => "Pan",
                        ChannelType::Tilt => "Tilt",
                        ChannelType::Focus => "Focus",
                        ChannelType::Zoom => "Zoom",
                        ChannelType::Strobe => "Strobe",
                        ChannelType::Gobo => "Gobo",
                        ChannelType::Color => "Color Wheel",
                        ChannelType::Other(name) => &name,
                        _ => "Unknown",
                    };

                    ui.label(param_name);
                    ui.label(format!("{:?}", value.channel_type));
                    ui.label(format!("{}", value.value));

                    // Create a graphical representation based on parameter type
                    let progress = value.value as f32 / 255.0;

                    match value.channel_type {
                        ChannelType::Red
                        | ChannelType::Green
                        | ChannelType::Blue
                        | ChannelType::White => {
                            let color = match value.channel_type {
                                ChannelType::Red => Color32::from_rgb(value.value, 0, 0),
                                ChannelType::Green => Color32::from_rgb(0, value.value, 0),
                                ChannelType::Blue => Color32::from_rgb(0, 0, value.value),
                                ChannelType::White => {
                                    let v = value.value;
                                    Color32::from_rgb(v, v, v)
                                }
                                _ => Color32::WHITE,
                            };

                            let rect = ui.available_rect_before_wrap().shrink(2.0);
                            let response = ui.allocate_rect(rect, egui::Sense::hover());

                            ui.painter().rect_filled(response.rect, 4.0, color);
                        }
                        _ => {
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

                            ui.painter().rect_filled(
                                filled_rect,
                                4.0,
                                Color32::from_rgb(0, 150, 255),
                            );
                        }
                    }

                    ui.end_row();
                }
            });
    }

    // Helper method to render effect details
    fn render_effect_details(&self, ui: &mut egui::Ui, effect: &EffectMapping) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(format!("Type: {:?}", effect.effect.effect_type));
                ui.label(format!("Channel Type: {:?}", effect.channel_type));
                ui.label(format!("Distribution: {:?}", effect.distribution));
                ui.label(format!("Fixtures: {} fixtures", effect.fixture_ids.len()));
                ui.label(format!("Amplitude: {:.2}", effect.effect.amplitude));
                ui.label(format!("Frequency: {:.2}", effect.effect.frequency));
                ui.label(format!("Offset: {:.2}", effect.effect.offset));
                ui.label(format!(
                    "Interval: {:?}, Ratio: {:.2}",
                    effect.effect.params.interval, effect.effect.params.interval_ratio
                ));
            });

            // Add a visual preview of the effect
            let plot_height = 100.0;
            let plot_width = 200.0;

            Plot::new(format!("effect_plot_{:?}", effect.effect.effect_type))
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
                        let phase = effect.effect.params.phase;
                        let ratio = effect.effect.params.interval_ratio;

                        // This is a simplified version - the actual effect could be more complex
                        let y = match effect.effect.effect_type {
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

    pub fn set_fixtures(&mut self, fixtures: Vec<Fixture>) {
        self.fixtures = fixtures;
    }

    pub fn set_selected_fixtures(&mut self, selected_fixtures: Vec<usize>) {
        self.state.selected_fixtures = selected_fixtures;
    }

    pub fn update_fixtures(&mut self, console: &Arc<Mutex<LightingConsole>>) {
        // Intensity
        let intensity_channels = self.get_selected_fixture_channels("intensity");
        if !intensity_channels.is_empty() {
            self.update_selected_fixture_channels("dimmer", console);
        }

        // Strobe
        let strobe_channels = self.get_selected_fixture_channels("strobe");
        if !strobe_channels.is_empty() {
            self.update_selected_fixture_channels("strobe", console);
        }

        // Color
        let color_channels = self.get_selected_fixture_channels("color");
        if !color_channels.is_empty() {
            self.update_selected_fixture_channels("red", console);
            self.update_selected_fixture_channels("green", console);
            self.update_selected_fixture_channels("blue", console);
            self.update_selected_fixture_channels("white", console);
        }

        // Position
        let position_channels = self.get_selected_fixture_channels("position");
        if !position_channels.is_empty() {
            self.update_selected_fixture_channels("pan", console);
            self.update_selected_fixture_channels("tilt", console);
        }

        // Beam
        let beam_channels = self.get_selected_fixture_channels("beam");
        if !beam_channels.is_empty() {
            self.update_selected_fixture_channels("beam", console);
        }
    }

    // Helper function to draw tab buttons
    fn draw_tab_button(&mut self, ui: &mut egui::Ui, label: &str, tab: ActiveProgrammerTab) {
        let is_active = self.state.active_tab == tab;

        let mut button = egui::Button::new(label);
        if is_active {
            button = button
                .fill(Color32::from_rgb(30, 30, 30))
                .stroke(Stroke::new(1.0, Color32::from_rgb(0, 100, 255)));
        } else {
            button = button.fill(Color32::from_rgb(40, 40, 40));
        }

        if ui.add(button).clicked() {
            self.state.active_tab = tab;
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
    ) -> bool {
        let mut value = self.state.get_param(param_name);
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
                self.state.set_param(param_name, value);
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
                        self.state.set_param(param_name, value);
                        changed = true;
                    }

                    if ui.button("+").clicked() {
                        value = (value + (max - min) / 20.0).min(max);
                        self.state.set_param(param_name, value);
                        changed = true;
                    }
                });
            }
        });

        changed
    }

    // Helper method to get channels of selected fixtures by type
    fn get_selected_fixture_channels(&self, channel_type: &str) -> Vec<(&Fixture, &Channel)> {
        let mut channels = Vec::new();

        for fixture in &self.fixtures {
            if self.state.selected_fixtures.contains(&fixture.id) {
                for channel in &fixture.channels {
                    let channel_name = channel.name.to_lowercase();
                    let matches = match channel_type {
                        "intensity" => {
                            channel_name.contains("dimmer") || channel_name.contains("intensity")
                        }
                        "strobe" => channel_name.contains("strobe"),
                        "color" => {
                            channel_name.contains("red")
                                || channel_name.contains("green")
                                || channel_name.contains("blue")
                                || channel_name.contains("white")
                                || channel_name.contains("amber")
                                || channel_name.contains("color")
                        }
                        "position" => channel_name.contains("pan") || channel_name.contains("tilt"),
                        "beam" => {
                            channel_name.contains("focus")
                                || channel_name.contains("zoom")
                                || channel_name.contains("gobo")
                                || channel_name.contains("prism")
                        }
                        _ => false,
                    };

                    if matches {
                        channels.push((fixture, channel));
                    }
                }
            }
        }

        channels
    }

    fn create_effect_mapping(
        &self,
        console: &Arc<Mutex<LightingConsole>>,
    ) -> Option<EffectMapping> {
        if self.state.selected_fixtures.is_empty() {
            return None;
        }

        // Get the current tab's effect config
        let tab_effect = match self.state.tab_effects.get(&self.state.active_tab) {
            Some(config) => config,
            None => return None,
        };

        // Map UI waveform to EffectType
        let effect_type = match tab_effect.effect_waveform {
            0 => EffectType::Sine,
            1 => EffectType::Square,
            2 => EffectType::Sawtooth,
            3 => EffectType::Triangle,
            _ => EffectType::Sine, // Default to sine
        };

        // Map UI interval to Interval
        let interval = match tab_effect.effect_interval {
            0 => Interval::Beat,
            1 => Interval::Bar,
            2 => Interval::Phrase,
            _ => Interval::Beat, // Default to beat
        };

        // Create effect parameters
        let effect_ratio = tab_effect.effect_ratio;
        let effect_phase = tab_effect.effect_phase / 360.0; // Convert degrees to 0-1 range

        let effect_params = EffectParams {
            interval,
            interval_ratio: effect_ratio as f64,
            phase: effect_phase as f64,
        };

        // Create the Effect object
        let effect = Effect {
            effect_type,
            min: 0,
            max: 255,
            amplitude: 1.0,
            frequency: 1.0,
            offset: 0.0,
            params: effect_params,
        };

        // Determine channel type based on active tab
        let channel_type = match self.state.active_tab {
            ActiveProgrammerTab::Intensity => ChannelType::Dimmer,
            ActiveProgrammerTab::Color => ChannelType::Color,
            ActiveProgrammerTab::Position => {
                if tab_effect.effect_waveform % 2 == 0 {
                    ChannelType::Pan
                } else {
                    ChannelType::Tilt
                }
            }
            ActiveProgrammerTab::Beam => ChannelType::Beam,
        };

        // Map UI distribution to EffectDistribution
        let distribution = match tab_effect.effect_distribution {
            0 => EffectDistribution::All,
            1 => EffectDistribution::Step(tab_effect.effect_step_value),
            2 => EffectDistribution::Wave(tab_effect.effect_wave_offset),
            _ => EffectDistribution::All,
        };

        let console_lock = console.lock();
        let fixtures = console_lock.fixtures.iter().collect::<Vec<&Fixture>>();

        // Extract names of selected fixtures
        let selected_fixture_names: Vec<String> = fixtures
            .iter()
            .filter(|fixture| self.state.selected_fixtures.contains(&fixture.id))
            .map(|fixture| fixture.name.clone())
            .collect();

        drop(console_lock);

        // Join the names for display, limit to first few if there are many
        let fixture_names_display = if selected_fixture_names.len() <= 3 {
            selected_fixture_names.join(", ")
        } else {
            format!(
                "{} and {} more",
                selected_fixture_names[0..2].join(", "),
                selected_fixture_names.len() - 2
            )
        };

        // Create a name based on the selected fixtures
        let name = format!("{:?} Effect on {}", effect_type, fixture_names_display);

        // Create and return the EffectMapping
        Some(EffectMapping {
            name,
            effect,
            fixture_ids: self.state.selected_fixtures.clone(),
            channel_type,
            distribution,
        })
    }

    fn update_selected_fixture_channels(
        &mut self,
        param_name: &str, // The programmer parameter name (e.g., "dimmer")
        console: &Arc<Mutex<LightingConsole>>,
    ) {
        // Get the value from programmer state
        let value = self.state.get_param(param_name);

        let mut console = console.lock();

        // For each selected fixture
        for fixture_id in &self.state.selected_fixtures {
            // Map parameter name to actual channel type
            let channel_types = match param_name {
                "intensity" => vec![ChannelType::Dimmer],
                "dimmer" => vec![ChannelType::Dimmer],
                "strobe" => vec![ChannelType::Strobe],
                "red" => vec![ChannelType::Red, ChannelType::Color],
                "green" => vec![ChannelType::Green],
                "blue" => vec![ChannelType::Blue],
                "white" => vec![ChannelType::White],
                "pan" => vec![ChannelType::Pan],
                "tilt" => vec![ChannelType::Tilt],
                // Add other mappings as needed
                _ => vec![ChannelType::Other(param_name.to_string())],
            };

            // Add the value to the programmer state
            for channel_type in channel_types {
                console
                    .programmer
                    .add_value(*fixture_id, channel_type, value as u8);
            }
        }
    }

    // Intensity tab content
    fn show_intensity_tab(&mut self, ui: &mut egui::Ui) {
        let intensity_channels = self.get_selected_fixture_channels("intensity");

        if intensity_channels.is_empty() {
            ui.label("No intensity channels available for selected fixtures");
            return;
        }

        ui.horizontal(|ui| {
            let spacing = 20.0;
            let slider_height = 180.0;

            ui.add_space(spacing);
            self.vertical_slider(ui, "dimmer", "Dimmer", 0.0, 100.0, slider_height);

            ui.add_space(spacing);
            self.vertical_slider(ui, "strobe", "Strobe", 0.0, 100.0, slider_height);

            ui.add_space(spacing * 2.0);
        });
    }

    // Color tab content
    fn show_color_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let spacing = 20.0;
            let slider_height = 180.0;

            ui.add_space(spacing);
            self.vertical_slider(ui, "red", "Red", 0.0, 255.0, slider_height);

            ui.add_space(spacing);
            self.vertical_slider(ui, "green", "Green", 0.0, 255.0, slider_height);

            ui.add_space(spacing);
            self.vertical_slider(ui, "blue", "Blue", 0.0, 255.0, slider_height);

            ui.add_space(spacing);
            self.vertical_slider(ui, "white", "White", 0.0, 255.0, slider_height);

            ui.add_space(spacing * 2.0);

            // Color presets
            ui.vertical(|ui| {
                ui.label("Presets");
                ui.add_space(5.0);

                // get a mutable copy of the color presets
                let color_presets = self.state.color_presets.clone();

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

                                self.state.set_param("red", r as f32);
                                self.state.set_param("green", g as f32);
                                self.state.set_param("blue", b as f32);

                                if r == g && g == b && r > 200 {
                                    // White preset also sets white channel for RGBW fixtures
                                    self.state.set_param("white", 255.0);
                                } else {
                                    self.state.set_param("white", 0.0);
                                }
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
    fn show_position_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let spacing = 20.0;
            let slider_height = 180.0;

            ui.add_space(spacing);
            self.vertical_slider(ui, "pan", "Pan", 0.0, 360.0, slider_height);

            ui.add_space(spacing);
            self.vertical_slider(ui, "tilt", "Tilt", 0.0, 270.0, slider_height);

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
                let pan = self.state.get_param("pan");
                let tilt = self.state.get_param("tilt");

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

                        self.state.set_param("pan", new_pan);
                        self.state.set_param("tilt", new_tilt.clamp(0.0, 270.0));
                    }
                }
            });
        });
    }

    // Beam tab content
    fn show_beam_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let spacing = 20.0;
            let slider_height = 180.0;

            ui.add_space(spacing);
            self.vertical_slider(ui, "focus", "Focus", 0.0, 100.0, slider_height);

            ui.add_space(spacing);
            self.vertical_slider(ui, "zoom", "Zoom", 0.0, 100.0, slider_height);

            ui.add_space(spacing);
            self.vertical_slider(
                ui,
                "gobo_rotation",
                "Gobo Rot.",
                -180.0,
                180.0,
                slider_height,
            );

            ui.add_space(spacing * 2.0);

            // Gobo selection
            ui.vertical(|ui| {
                ui.label("Gobo");
                let gobo_selection = self.state.get_param("gobo_selection") as usize;
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
                                self.state.set_param("gobo_selection", i as f32);
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
    fn show_effects_panel(&mut self, ui: &mut egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_min_width(200.0);
                ui.heading("EFFECTS");

                // Add a dynamic subtitle based on the active tab
                let effects_subtitle = match self.state.active_tab {
                    ActiveProgrammerTab::Intensity => "Effects on Intensity",
                    ActiveProgrammerTab::Color => "Effects on Color",
                    ActiveProgrammerTab::Position => "Effects on Position",
                    ActiveProgrammerTab::Beam => "Effects on Beam",
                };
                ui.label(effects_subtitle);

                ui.add_space(5.0);

                // Clone the current tab's effect config
                let tab_effect_mut = self.state.tab_effects.get_mut(&self.state.active_tab);

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

                    // Effect parameter sliders
                    ui.horizontal(|ui| {
                        let slider_height = 120.0;

                        // Use custom sliders for ratio and phase
                        ui.vertical(|ui| {
                            ui.label("Ratio");
                            let mut ratio = tab_effect.effect_ratio;
                            if render_vertical_fader(ui, &mut ratio, 0.0, 2.0, slider_height) {
                                tab_effect.effect_ratio = ratio;
                            }
                        });

                        ui.add_space(15.0);

                        ui.vertical(|ui| {
                            ui.label("Phase");
                            let mut phase = tab_effect.effect_phase;
                            if render_vertical_fader(ui, &mut phase, 0.0, 360.0, slider_height) {
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
                                            .clamp_range(1..=16)
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
                                    .add(
                                        egui::Slider::new(&mut wave_offset, 0.0..=180.0)
                                            .suffix("°"),
                                    )
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
                        if let Some(effect_mapping) = self.create_effect_mapping(console) {
                            let mut console_lock = console.lock();
                            console_lock.programmer.add_effect(effect_mapping);
                            drop(console_lock);
                        }
                    }
                }
                ui.add_space(10.0);
            });

            ui.vertical(|ui| {
                let tab_effect_opt = self.state.tab_effects.get(&self.state.active_tab);
                if let Some(tab_effect) = tab_effect_opt {
                    self.show_waveform_visualization(ui, tab_effect);
                }
            });
        });
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
        let line = Line::new("", plot_points).color(match self.state.active_tab {
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
}
