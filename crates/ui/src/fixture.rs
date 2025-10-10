use tokio::sync::mpsc;

use crate::state::ConsoleState;
use eframe::egui::{self, Color32, CornerRadius, Rect, Stroke, Vec2};
use halo_core::ConsoleCommand;
use halo_fixtures::FixtureType;

const FIXTURE_TYPE_COLORS: [(FixtureType, Color32); 6] = [
    (FixtureType::MovingHead, Color32::from_rgb(255, 165, 0)), // Orange
    (FixtureType::PAR, Color32::from_rgb(0, 255, 255)),        // Cyan
    (FixtureType::Wash, Color32::from_rgb(255, 0, 255)),       // Magenta
    (FixtureType::Pinspot, Color32::from_rgb(255, 255, 0)),    // Yellow
    (FixtureType::LEDBar, Color32::from_rgb(0, 255, 0)),       // Green
    (FixtureType::Smoke, Color32::from_rgb(128, 128, 128)),    // Gray
];

pub fn render(
    ui: &mut eframe::egui::Ui,
    state: &ConsoleState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    egui::CentralPanel::default().show(ui.ctx(), |ui| {
        ui.vertical(|ui| {
            ui.heading("Fixtures");

            // Fixture grid
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (idx, (_, fixture)) in state.fixtures.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}: {}", idx + 1, fixture.name));
                        ui.label(format!("Profile: {}", fixture.profile_id));
                        ui.label(format!("Channels: {}", fixture.channels.len()));

                        // Show channel values
                        ui.label("Values:");
                        for (channel_idx, channel) in fixture.channels.iter().enumerate() {
                            ui.label(format!("{}:{}", channel.name, channel.value));
                        }
                    });
                }
            });
        });
    });
}

pub fn render_grid(
    ui: &mut eframe::egui::Ui,
    state: &ConsoleState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    height: f32,
) {
    let text_color = Color32::from_rgb(255, 255, 255);
    let fixture_bg = Color32::from_rgb(30, 30, 30);
    let highlight_color = Color32::from_rgb(59, 130, 246);

    // Create a scrollable area for fixtures
    egui::ScrollArea::vertical()
        .max_height(height)
        .show(ui, |ui| {
            ui.add_space(8.0);
            ui.heading("FIXTURES");
            ui.add_space(4.0);

            // Determine grid layout based on available width
            let available_width = ui.available_width();
            let fixture_width = 100.0;
            let spacing = 10.0;
            let columns =
                ((available_width + spacing) / (fixture_width + spacing)).floor() as usize;
            let columns = columns.max(1); // At least 1 column

            // Create a grid layout for fixtures
            egui::Grid::new("fixtures_grid")
                .num_columns(columns)
                .spacing([spacing, spacing])
                .show(ui, |ui| {
                    for (i, (fixture_id, fixture)) in state.fixtures.iter().enumerate() {
                        // Create a fixture button
                        let fixture_height = if fixture.profile.fixture_type == FixtureType::LEDBar
                        {
                            70.0
                        } else {
                            80.0
                        };

                        // Draw fixture background
                        let rect = ui
                            .allocate_space(Vec2::new(fixture_width, fixture_height))
                            .1;

                        // Check if fixture is selected
                        let is_selected = state.selected_fixtures.contains(&fixture.id);
                        let border_color = if is_selected {
                            highlight_color
                        } else {
                            Color32::from_gray(70)
                        };
                        let border_width = if is_selected { 2.0 } else { 1.0 };

                        // Draw fixture box
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(4), fixture_bg);

                        ui.painter().rect_stroke(
                            rect,
                            CornerRadius::same(4),
                            Stroke::new(border_width, border_color),
                            egui::StrokeKind::Outside,
                        );

                        // Handle clicks for fixture selection
                        let response = ui.interact(rect, ui.id().with(i), egui::Sense::click());
                        if response.clicked() {
                            let fixture_id = fixture.id;
                            let is_selected = state.selected_fixtures.contains(&fixture_id);

                            if is_selected {
                                // Remove from selection
                                let _ = console_tx
                                    .send(ConsoleCommand::RemoveSelectedFixture { fixture_id });
                            } else {
                                // Add to selection
                                let _ = console_tx
                                    .send(ConsoleCommand::AddSelectedFixture { fixture_id });
                            }
                        }

                        // Draw color strip at the top of the fixture box
                        let color_strip_height = 6.0;
                        let color_strip_rect = Rect::from_min_size(
                            rect.min,
                            Vec2::new(rect.width(), color_strip_height),
                        );
                        ui.painter().rect_filled(
                            color_strip_rect,
                            CornerRadius::same(4).at_least(4),
                            get_fixture_type_color(&fixture.profile.fixture_type),
                        );

                        // Draw fixture name (centered)
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            &fixture.name,
                            egui::FontId::proportional(14.0),
                            text_color,
                        );

                        // Add intensity percentage in bottom right corner
                        let intensity_value = if let Some(channel) =
                            fixture.channels.iter().find(|c| {
                                c.name.to_lowercase().contains("dimmer")
                                    || c.name.to_lowercase().contains("intensity")
                            }) {
                            channel.value
                        } else {
                            0 // Default if no dimmer/intensity channel found
                        };

                        // Format as percentage
                        let intensity_text = format!(
                            "{}%",
                            (intensity_value as f32 / 255.0 * 100.0).round() as u8
                        );

                        // Position in bottom right with some padding
                        let text_pos = rect.right_bottom() - Vec2::new(8.0, 8.0);
                        ui.painter().text(
                            text_pos,
                            egui::Align2::RIGHT_BOTTOM,
                            &intensity_text,
                            egui::FontId::proportional(11.0),
                            if intensity_value > 0 {
                                highlight_color.linear_multiply(0.9)
                            } else {
                                Color32::from_gray(130) // Dimmed when intensity is 0
                            },
                        );

                        // New row after each column
                        if (i + 1) % columns == 0 && i < state.fixtures.len() - 1 {
                            ui.end_row();
                        }
                    }
                });
        });
}

fn get_fixture_type_color(fixture_type: &FixtureType) -> Color32 {
    FIXTURE_TYPE_COLORS
        .iter()
        .find(|(t, _)| t == fixture_type)
        .map(|(_, color)| *color)
        .unwrap_or(Color32::WHITE)
}
