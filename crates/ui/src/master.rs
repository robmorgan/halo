use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, Vec2};
use halo_core::{SyncLightingConsole as LightingConsole, MidiAction, MidiOverride, StaticValue};
use halo_fixtures::ChannelType;
use parking_lot::Mutex;

// Override button state
#[derive(Clone, Debug)]
pub struct OverrideButton {
    pub name: String,
    pub color: Color32,
    pub is_active: bool,
    pub is_momentary: bool, // If true, the button is only active while pressed
    pub midi_key: Option<u8>, // MIDI note number for triggering this override
    pub values: Vec<StaticValue>, // DMX values to set when activated
}

impl OverrideButton {
    pub fn new(name: String, color: Color32) -> Self {
        Self {
            name,
            color,
            is_active: false,
            is_momentary: false,
            midi_key: None,
            values: Vec::new(),
        }
    }

    pub fn with_midi_key(mut self, key: u8) -> Self {
        self.midi_key = Some(key);
        self
    }

    pub fn with_momentary(mut self, momentary: bool) -> Self {
        self.is_momentary = momentary;
        self
    }

    pub fn with_values(mut self, values: Vec<StaticValue>) -> Self {
        self.values = values;
        self
    }
}

pub struct OverridesPanel {
    pub buttons: Vec<OverrideButton>,
    pub active_overrides: HashMap<String, bool>,
    pub last_activated: Option<usize>,
}

impl Default for OverridesPanel {
    fn default() -> Self {
        Self {
            buttons: vec![
                OverrideButton::new("Red".to_string(), Color32::RED),
                OverrideButton::new("Green".to_string(), Color32::GREEN),
                OverrideButton::new("Blue".to_string(), Color32::BLUE),
            ],
            active_overrides: HashMap::new(),
            last_activated: None,
        }
    }
}

impl OverridesPanel {
    pub fn new() -> Self {
        Self::default()
    }

    // Update override button state
    pub fn _set_override_active(&mut self, name: &str, active: bool) {
        if let Some(state) = self.active_overrides.get_mut(name) {
            *state = active;
        }

        // Update button state
        for (i, button) in self.buttons.iter_mut().enumerate() {
            if button.name == name {
                button.is_active = active;
                if active {
                    self.last_activated = Some(i);
                }
                break;
            }
        }
    }

    // Apply overrides to the console
    pub fn _apply_overrides(&self, console: &mut LightingConsole) {
        for button in &self.buttons {
            if button.is_active {
                for value in &button.values {
                    // Find the fixture and set the channel value
                    if let Some(fixture) = console
                        .fixtures()
                        .iter_mut()
                        .find(|f| f.id == value.fixture_id)
                    {
                        fixture.set_channel_value(&value.channel_type, value.value);
                    }
                }
            }
        }
    }

    // Register MIDI overrides with the console
    pub fn _register_midi_overrides(&self, console: &mut LightingConsole) {
        for button in &self.buttons {
            if let Some(midi_key) = button.midi_key {
                let override_config = MidiOverride {
                    action: MidiAction::StaticValues(button.values.clone()),
                };

                console.add_midi_override(midi_key, override_config);
            }
        }
    }

    // Show the simplified overrides panel UI
    pub fn show(&mut self, ui: &mut egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        ui.vertical(|ui| {
            ui.heading("OVERRIDES");

            ui.add_space(5.0);

            // Draw buttons horizontally
            ui.horizontal(|ui| {
                let mut buttons = self.buttons.clone();
                for (i, button) in buttons.iter_mut().enumerate() {
                    let button_width = 120.0;
                    let button_height = 40.0;

                    // Draw the override button
                    let response =
                        self.draw_override_button(ui, button, button_width, button_height);

                    // Handle click
                    if response.clicked() {
                        // Toggle button state
                        button.is_active = !button.is_active;
                        self.active_overrides
                            .insert(button.name.clone(), button.is_active);

                        // Apply override to console
                        let mut console_lock = console.lock();

                        if button.is_active {
                            // Apply this override
                            for value in &button.values {
                                if let Some(fixture) = console_lock
                                    .fixtures()
                                    .iter_mut()
                                    .find(|f| f.id == value.fixture_id)
                                {
                                    fixture.set_channel_value(&value.channel_type, value.value);
                                }
                            }
                        } else {
                            // Reset channels controlled by this override
                            // This would need custom logic to know what values to reset to
                        }
                        drop(console_lock);
                    }

                    // Handle momentary behavior
                    if button.is_momentary && response.drag_stopped() {
                        button.is_active = false;
                        self.active_overrides.insert(button.name.clone(), false);

                        // TODO - Reset values when momentary button is released
                    }

                    ui.add_space(5.0);
                }
            });
        });
    }

    // Draw a single override button
    fn draw_override_button(
        &self,
        ui: &mut egui::Ui,
        button: &OverrideButton,
        width: f32,
        height: f32,
    ) -> Response {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());

        // Determine button colors
        let (bg_color, text_color, stroke_color) = if button.is_active {
            // Active button colors
            (button.color, Color32::BLACK, Color32::WHITE)
        } else {
            // Inactive button colors
            (
                button.color.linear_multiply(0.3), // Darker version of the color
                Color32::WHITE,
                Color32::from_gray(100),
            )
        };

        // Draw button
        ui.painter().rect_filled(
            rect, 4.0, // rounded corners
            bg_color,
        );

        // Draw button outline
        ui.painter().rect_stroke(
            rect,
            4.0, // rounded corners
            Stroke::new(1.0, stroke_color),
            egui::StrokeKind::Inside,
        );

        // Draw button text
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &button.name,
            egui::FontId::proportional(14.0),
            text_color,
        );

        response
    }
}

// Master fader state
pub struct MasterFader {
    pub name: String,
    pub value: f32, // 0.0 to 1.0
    pub color: Color32,
    pub is_active: bool,
    pub midi_cc: Option<u8>, // MIDI CC number for controlling this fader
    pub affected_channels: Vec<ChannelType>, // Channel types affected by this master
}

impl MasterFader {
    pub fn new(name: String, value: f32, color: Color32) -> Self {
        Self {
            name,
            value,
            color,
            is_active: true,
            midi_cc: None,
            affected_channels: Vec::new(),
        }
    }

    pub fn with_midi_cc(mut self, cc: u8) -> Self {
        self.midi_cc = Some(cc);
        self
    }

    pub fn with_affected_channels(mut self, channels: Vec<ChannelType>) -> Self {
        self.affected_channels = channels;
        self
    }
}

// Simplified master panel with just two faders
pub struct MasterPanel {
    pub master_fader: MasterFader,
    pub smoke_fader: MasterFader,
}

impl Default for MasterPanel {
    fn default() -> Self {
        // Create the two master faders
        let master_fader =
            MasterFader::new("Master".to_string(), 1.0, Color32::from_rgb(150, 150, 150))
                .with_midi_cc(7) // CC #7 is standard for volume
                .with_affected_channels(vec![ChannelType::Dimmer]);

        let smoke_fader = MasterFader::new(
            "Smoke %".to_string(),
            0.75,
            Color32::from_rgb(100, 100, 100),
        )
        .with_midi_cc(16)
        .with_affected_channels(vec![ChannelType::Other("Smoke".to_string())]);

        Self {
            master_fader,
            smoke_fader,
        }
    }
}

impl MasterPanel {
    pub fn new() -> Self {
        Self::default()
    }

    // Apply master faders to the console
    pub fn apply_masters(&self, console: &mut LightingConsole) {
        // Apply master dimmer
        if self.master_fader.is_active {
            console.apply_master_fader(self.master_fader.value);
        }

        // Apply smoke master
        if self.smoke_fader.is_active {
            console.apply_smoke_fader(self.smoke_fader.value);
        }
    }

    // Show the master panel UI
    pub fn show(&mut self, ui: &mut egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        ui.vertical(|ui| {
            ui.heading("MASTER");

            ui.add_space(5.0);

            // Draw master fader
            Self::draw_master_fader(ui, &mut self.master_fader, console);
            ui.add_space(15.0);

            // Draw smoke fader
            Self::draw_master_fader(ui, &mut self.smoke_fader, console);
        });
    }
    // Draw a single master fader
    fn draw_master_fader(
        ui: &mut egui::Ui,
        fader: &mut MasterFader,
        _console: &Arc<Mutex<LightingConsole>>,
    ) {
        ui.vertical(|ui| {
            // Fader label and value
            ui.horizontal(|ui| {
                ui.label(&fader.name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{:.0}%", fader.value * 100.0));
                });
            });

            // Fader slider
            let height = 20.0;
            let response = ui.add(
                egui::Slider::new(&mut fader.value, 0.0..=1.0)
                    .show_value(false)
                    .fixed_decimals(2)
                    .orientation(egui::SliderOrientation::Horizontal),
            );

            // Customize fader appearance
            let slider_rect = response.rect; // Draw the fader track
            let track_height = height * 0.8;
            let track_rect = Rect::from_min_size(
                Pos2::new(
                    slider_rect.min.x,
                    slider_rect.center().y - track_height / 2.0,
                ),
                Vec2::new(slider_rect.width(), track_height),
            );

            // Draw filled portion
            let fill_width = slider_rect.width() * fader.value;
            let fill_rect =
                Rect::from_min_size(track_rect.min, Vec2::new(fill_width, track_height));

            ui.painter()
                .rect_filled(track_rect, 2.0, Color32::from_rgb(40, 40, 40));

            ui.painter().rect_filled(fill_rect, 2.0, fader.color);

            // Apply fader value changes to console
            if response.changed() {
                if fader.name == "Master" {
                    // TODO - Apply master dimmer
                    // When we are rendering a frame, we'll loop over each fixture and limit its
                    // intensity to the master dimmer value.
                } else if fader.name == "Smoke %" {
                    // TODO - Apply smoke master
                    // We'll do the same here but limit smoke instead.
                }
            }
        });
    }
}
