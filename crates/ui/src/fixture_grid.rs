use eframe::egui::{self, Color32, RichText, Rounding, Vec2};
use std::sync::{Arc, Mutex};

use halo_core::LightingConsole;
use halo_fixtures::{Fixture, FixtureType};

/// A panel that shows a grid layout of fixtures with selectable items.
///
/// It has support for fixtures with multiple lights.
pub struct FixtureGrid {}

impl Default for FixtureGrid {
    fn default() -> Self {
        Self {}
    }
}

impl FixtureGrid {
    /// Draw the fixture gird, creating/destroying it as required.
    pub fn render(ui: &mut eframe::egui::Ui, fixtures: Vec<Fixture>) {
        let dark_bg = Color32::from_rgb(0, 0, 0);
        let dark_panel_bg = Color32::from_rgb(16, 16, 16);
        let dark_element_bg = Color32::from_rgb(32, 32, 32);
        let gray_700 = Color32::from_rgb(55, 65, 81);
        let text_color = Color32::from_rgb(255, 255, 255);
        let text_dim = Color32::from_rgb(156, 163, 175);
        let border_color = Color32::from_rgb(55, 65, 81);
        let active_color = Color32::from_rgb(30, 64, 175);
        let highlight_color = Color32::from_rgb(59, 130, 246);

        // Create a scrollable area for fixtures
        egui::ScrollArea::vertical()
            .max_height(height)
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("FIXTURES").color(text_dim).size(12.0));
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
                        for (i, fixture) in self.fixtures.iter_mut().enumerate() {
                            // Create a fixture button
                            let fixture_height = if fixture.fixture_type == FixtureType::LEDBar {
                                70.0
                            } else {
                                80.0
                            };

                            // Background with optional highlight for selected fixtures
                            let is_selected = self.selected_fixtures.contains(&fixture.id);
                            let rect = ui
                                .allocate_space(Vec2::new(fixture_width, fixture_height))
                                .1;

                            // Draw fixture box with potential selection highlight
                            let fixture_bg = button_color;
                            ui.painter()
                                .rect_filled(rect, Rounding::same(4.0), fixture_bg);

                            if is_selected {
                                ui.painter().rect_stroke(
                                    rect,
                                    Rounding::same(4.0),
                                    Stroke::new(2.0, highlight_color),
                                    egui::StrokeKind::Outside,
                                );
                            } else {
                                ui.painter().rect_stroke(
                                    rect,
                                    Rounding::same(4.0),
                                    Stroke::new(1.0, Color32::from_gray(70)),
                                    egui::StrokeKind::Outside,
                                );
                            }

                            // Handle clicks
                            let response = ui.interact(rect, ui.id().with(i), egui::Sense::click());
                            if response.clicked() {
                                if is_selected {
                                    self.selected_fixtures.retain(|&id| id != fixture.id);
                                } else {
                                    self.selected_fixtures.push(fixture.id);
                                }
                            }

                            // Fixture header with color indicator and name
                            let header_rect = Rect::from_min_size(
                                Pos2::new(rect.min.x + 6.0, rect.min.y + 6.0),
                                Vec2::new(rect.width() - 12.0, 20.0),
                            );

                            // Draw color indicator
                            ui.painter().circle_filled(
                                Pos2::new(header_rect.min.x + 10.0, header_rect.center().y),
                                8.0,
                                fixture.color,
                            );

                            // Draw fixture name
                            ui.painter().text(
                                Pos2::new(header_rect.min.x + 25.0, header_rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                &fixture.name,
                                egui::FontId::proportional(14.0),
                                text_color,
                            );

                            // Draw fixture type visualization
                            match fixture.fixture_type {
                                FixtureType::MovingHead => {
                                    let center = Pos2::new(rect.center().x, rect.min.y + 45.0);
                                    ui.painter().circle_stroke(
                                        center,
                                        16.0,
                                        Stroke::new(2.0, Color32::from_gray(100)),
                                    );
                                }
                                FixtureType::PAR => {
                                    let center = Pos2::new(rect.center().x, rect.min.y + 45.0);
                                    ui.painter().circle_stroke(
                                        center,
                                        16.0,
                                        Stroke::new(2.0, Color32::from_gray(100)),
                                    );
                                }
                                FixtureType::LEDBar => {
                                    if let Some(subs) = &fixture.sub_fixtures {
                                        let sub_width = (rect.width() - 20.0) / subs.len() as f32;
                                        let y = rect.min.y + 45.0;
                                        for (i, sub) in subs.iter().enumerate() {
                                            let x = rect.min.x
                                                + 10.0
                                                + i as f32 * sub_width
                                                + sub_width / 2.0;
                                            ui.painter().circle_filled(
                                                Pos2::new(x, y),
                                                sub_width / 2.5,
                                                sub.color,
                                            );
                                        }
                                    }
                                }
                                FixtureType::Wash | FixtureType::Pinspot => {
                                    let center = Pos2::new(rect.center().x, rect.min.y + 45.0);
                                    let size = 16.0;
                                    ui.painter().rect_stroke(
                                        Rect::from_center_size(
                                            center,
                                            Vec2::new(size * 2.0, size * 2.0),
                                        ),
                                        Rounding::none(),
                                        Stroke::new(2.0, Color32::from_gray(100)),
                                    );
                                }
                            }

                            // New row after each column
                            if (i + 1) % columns == 0 && i < self.fixtures.len() - 1 {
                                ui.end_row();
                            }
                        }
                    });
            });
    }
}
