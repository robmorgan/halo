use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use halo_core::ConsoleCommand;
use halo_fixtures::FixtureType;
use tokio::sync::mpsc;

use crate::state::ConsoleState;

pub fn render(
    ui: &mut egui::Ui,
    state: &ConsoleState,
    _console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    // Create a black panel with fixed dimensions
    egui::Frame::new()
        .fill(Color32::BLACK)
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(60)))
        .inner_margin(10.0)
        .show(ui, |ui| {
            // Set both min and max size to prevent expansion
            ui.set_min_size(Vec2::new(250.0, 300.0));
            ui.set_max_size(Vec2::new(250.0, 300.0));

            // Get pixel bar fixtures
            let mut pixel_fixtures: Vec<_> = state
                .fixtures
                .values()
                .filter(|f| f.profile.fixture_type == FixtureType::PixelBar)
                .collect();

            // Sort by fixture ID for consistent ordering
            pixel_fixtures.sort_by_key(|f| f.id);

            if pixel_fixtures.is_empty() {
                // Show placeholder if no pixel fixtures
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new("No Pixel Fixtures")
                            .size(14.0)
                            .color(Color32::from_gray(100)),
                    );
                });
            } else {
                // Use a scrollable area for many fixtures
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;

                    for fixture in pixel_fixtures {
                        render_fixture_pixels(ui, fixture, &state.pixel_data);
                    }
                });
            }
        });
}

fn render_fixture_pixels(
    ui: &mut egui::Ui,
    fixture: &halo_fixtures::Fixture,
    pixel_data: &std::collections::HashMap<usize, Vec<(u8, u8, u8)>>,
) {
    ui.vertical(|ui| {
        // Fixture label
        ui.label(
            egui::RichText::new(format!("{} (ID: {})", fixture.name, fixture.id))
                .size(11.0)
                .color(Color32::from_gray(200)),
        );

        ui.add_space(2.0);

        // Get pixel data for this fixture
        if let Some(pixels) = pixel_data.get(&fixture.id) {
            // Calculate pixel size to fit within available width
            let available_width = 230.0; // Slightly less than panel width for padding
            let pixel_count = pixels.len();
            let pixel_width = if pixel_count > 0 {
                (available_width / pixel_count as f32).min(10.0)
            } else {
                5.0
            };
            let pixel_height = 15.0;

            // Draw the pixel bar
            let (response, painter) = ui.allocate_painter(
                Vec2::new(available_width, pixel_height),
                egui::Sense::hover(),
            );

            let rect = response.rect;
            let start_x = rect.min.x;
            let y = rect.min.y;

            for (i, (r, g, b)) in pixels.iter().enumerate() {
                let x = start_x + (i as f32 * pixel_width);
                let pixel_rect =
                    Rect::from_min_size(Pos2::new(x, y), Vec2::new(pixel_width, pixel_height));

                // Draw the pixel with its RGB color
                painter.rect_filled(pixel_rect, 0.0, Color32::from_rgb(*r, *g, *b));

                // Draw a subtle border between pixels
                if pixel_width > 2.0 {
                    painter.rect_stroke(
                        pixel_rect,
                        0.0,
                        egui::Stroke::new(0.5, Color32::from_gray(40)),
                        egui::StrokeKind::Middle,
                    );
                }
            }
        } else {
            // No data available for this fixture
            ui.label(
                egui::RichText::new("No data")
                    .size(10.0)
                    .color(Color32::from_gray(80)),
            );
        }

        ui.add_space(4.0);
    });
}
