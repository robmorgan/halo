use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use halo_core::LightingConsole;
use halo_fixtures::{Channel, ChannelType, Fixture};
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};

// Define the active tab types for the programmer
#[derive(Clone, Debug, PartialEq)]
enum ActiveProgrammerTab {
    Intensity,
    Color,
    Position,
    Beam,
    Effects,
}

// Struct to hold the state of the programmer panel
pub struct ProgrammerState {
    active_tab: ActiveProgrammerTab,
    selected_fixtures: Vec<usize>,
    params: HashMap<String, f32>,
    color_presets: Vec<Color32>,
    effect_waveform: usize,
    effect_interval: usize,
    effect_distribution: usize,
    preview_mode: bool,
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
        params.insert("effect_ratio".to_string(), 1.0);
        params.insert("effect_phase".to_string(), 0.0);

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
            active_tab: ActiveProgrammerTab::Intensity,
            selected_fixtures: vec![],
            params,
            color_presets,
            effect_waveform: 0,
            effect_interval: 0,
            effect_distribution: 0,
            preview_mode: false,
        }
    }
}

impl ProgrammerState {
    pub fn new() -> Self {
        Self::default()
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
                    _ => {}
                });
                ui.set_min_size(Vec2::new(ui.available_width() - 250.0, 0.0));

                ui.separator();

                // Effects panel on the right
                self.show_effects_panel(ui);
            });

            // Update the fixtures based on the programmer's state if preview mode is enabled
            if self.state.preview_mode {
                self.update_fixtures(&console);
            }
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
                            channel_name.contains("dimmer")
                                || channel_name.contains("intensity")
                                || channel_name.contains("strobe")
                        }
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

    // Improved update_selected_fixture_channels method
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
            // Find the fixture in the console by ID
            if let Some(console_fixture) = console.fixtures.iter_mut().find(|f| f.id == *fixture_id)
            {
                // Map parameter name to actual channel name(s)
                let channel_names = match param_name {
                    "dimmer" => vec!["Dimmer", "Intensity"],
                    "red" => vec!["Red", "Color"],
                    "green" => vec!["Green"],
                    "blue" => vec!["Blue"],
                    "white" => vec!["White"],
                    "pan" => vec!["Pan"],
                    "tilt" => vec!["Tilt"],
                    // Add other mappings as needed
                    _ => vec![param_name],
                };

                // Try each possible channel name
                for channel_name in channel_names {
                    console_fixture.set_channel_value(channel_name, value as u8);
                }
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
    fn show_effects_panel(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.set_min_width(200.0);
            ui.heading("EFFECTS");
            ui.add_space(5.0);

            // Waveform dropdown
            egui::ComboBox::from_label("Waveform")
                .selected_text(match self.state.effect_waveform {
                    0 => "Sine",
                    1 => "Square",
                    2 => "Sawtooth",
                    3 => "Triangle",
                    _ => "Sine",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.state.effect_waveform, 0, "Sine");
                    ui.selectable_value(&mut self.state.effect_waveform, 1, "Square");
                    ui.selectable_value(&mut self.state.effect_waveform, 2, "Sawtooth");
                    ui.selectable_value(&mut self.state.effect_waveform, 3, "Triangle");
                });

            // Interval dropdown
            egui::ComboBox::from_label("Interval")
                .selected_text(match self.state.effect_interval {
                    0 => "Beat",
                    1 => "Bar",
                    2 => "Phrase",
                    _ => "Beat",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.state.effect_interval, 0, "Beat");
                    ui.selectable_value(&mut self.state.effect_interval, 1, "Bar");
                    ui.selectable_value(&mut self.state.effect_interval, 2, "Phrase");
                });

            ui.add_space(10.0);

            // Effect parameter sliders
            ui.horizontal(|ui| {
                let slider_height = 120.0;
                self.vertical_slider(ui, "effect_ratio", "Ratio", 0.0, 2.0, slider_height);

                ui.add_space(15.0);

                self.vertical_slider(ui, "effect_phase", "Phase", 0.0, 360.0, slider_height);
            });

            ui.add_space(10.0);

            // Distribution dropdown
            egui::ComboBox::from_label("Distribution")
                .selected_text(match self.state.effect_distribution {
                    0 => "All",
                    1 => "Step",
                    2 => "Wave",
                    _ => "All",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.state.effect_distribution, 0, "All");
                    ui.selectable_value(&mut self.state.effect_distribution, 1, "Step");
                    ui.selectable_value(&mut self.state.effect_distribution, 2, "Wave");
                });

            ui.add_space(10.0);

            // Apply effect button
            if ui.button("Apply Effect").clicked() {
                // Apply effect functionality would go here
            }
        });
    }
}
