use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, Vec2};
use halo_core::ConsoleCommand;
use tokio::sync::mpsc;

use crate::state::ConsoleState;
use crate::visualizer;

// Override button state
#[derive(Clone, Debug)]
pub struct OverrideButton {
    pub name: String,
    pub color: Color32,
    pub is_active: bool,
    pub is_momentary: bool,
    pub values: Vec<(usize, String, u8)>, // (fixture_id, channel_name, value)
}

impl OverrideButton {
    pub fn new(name: String, color: Color32) -> Self {
        Self {
            name,
            color,
            is_active: false,
            is_momentary: false,
            values: Vec::new(),
        }
    }
}

// Master fader state
pub struct MasterFader {
    pub name: String,
    pub value: f32, // 0.0 to 1.0
    pub color: Color32,
    pub is_active: bool,
}

impl MasterFader {
    pub fn new(name: String, value: f32, color: Color32) -> Self {
        Self {
            name,
            value,
            color,
            is_active: true,
        }
    }
}

pub fn render(
    ui: &mut eframe::egui::Ui,
    state: &ConsoleState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    ui.horizontal(|ui| {
        // Visualizer section
        ui.vertical(|ui| {
            ui.heading("VISUALIZER");
            ui.add_space(5.0);
            visualizer::render(ui, state, console_tx);
        });

        // Left side - Overrides section
        ui.vertical(|ui| {
            ui.heading("OVERRIDES");
            ui.add_space(5.0);

            // Static override buttons (similar to main branch implementation)
            ui.horizontal(|ui| {
                // Red override
                let red_button = draw_override_button(ui, "Red", Color32::RED, false, 120.0, 40.0);
                if red_button.clicked() {
                    // TODO: Send override command
                }

                ui.add_space(5.0);

                // Green override
                let green_button =
                    draw_override_button(ui, "Green", Color32::GREEN, false, 120.0, 40.0);
                if green_button.clicked() {
                    // TODO: Send override command
                }

                ui.add_space(5.0);

                // Blue override
                let blue_button =
                    draw_override_button(ui, "Blue", Color32::BLUE, false, 120.0, 40.0);
                if blue_button.clicked() {
                    // TODO: Send override command
                }
            });
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        // Right side - Master faders section
        ui.vertical(|ui| {
            ui.heading("MASTER");
            ui.add_space(5.0);

            // Stack faders vertically
            ui.vertical(|ui| {
                // Master fader
                draw_master_fader(ui, "Master", 1.0, Color32::from_rgb(150, 150, 150));
                ui.add_space(10.0);

                // Smoke fader
                draw_master_fader(ui, "Smoke", 0.75, Color32::from_rgb(100, 100, 100));
            });
        });
    });
}

// Draw a single override button
fn draw_override_button(
    ui: &mut egui::Ui,
    name: &str,
    color: Color32,
    is_active: bool,
    width: f32,
    height: f32,
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());

    // Determine button colors
    let (bg_color, text_color, stroke_color) = if is_active {
        // Active button colors
        (color, Color32::BLACK, Color32::WHITE)
    } else {
        // Inactive button colors
        (
            color.linear_multiply(0.3), // Darker version of the color
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
        name,
        egui::FontId::proportional(14.0),
        text_color,
    );

    response
}

// Draw a single master fader
fn draw_master_fader(ui: &mut egui::Ui, name: &str, mut value: f32, color: Color32) {
    ui.vertical(|ui| {
        // Fader label with percentage immediately following
        ui.label(format!("{} {:.0}%", name, value * 100.0));

        // Fader slider
        let response = ui.add(
            egui::Slider::new(&mut value, 0.0..=1.0)
                .show_value(false)
                .fixed_decimals(2)
                .orientation(egui::SliderOrientation::Horizontal),
        );

        // Customize fader appearance with visual feedback
        let slider_rect = response.rect;
        let track_height = 20.0 * 0.8;
        let track_rect = Rect::from_min_size(
            Pos2::new(
                slider_rect.min.x,
                slider_rect.center().y - track_height / 2.0,
            ),
            Vec2::new(slider_rect.width(), track_height),
        );

        // Draw filled portion
        let fill_width = slider_rect.width() * value;
        let fill_rect = Rect::from_min_size(track_rect.min, Vec2::new(fill_width, track_height));

        ui.painter()
            .rect_filled(track_rect, 2.0, Color32::from_rgb(40, 40, 40));

        ui.painter().rect_filled(fill_rect, 2.0, color);

        // Apply fader value changes (TODO: implement via message passing)
        if response.changed() {
            // TODO: Send master fader command
        }
    });
}
