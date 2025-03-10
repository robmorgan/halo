use eframe::egui;
use std::sync::{Arc, Mutex};

use halo_core::LightingConsole;

// Fixture visualization data
pub struct FixtureVisualization {
    pub position: egui::Vec2,
    pub size: egui::Vec2,
    pub fixture_type: FixtureType,
    pub fixture_index: usize,
    pub dmx_address: u16,
    pub color: egui::Color32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FixtureType {
    RGBLight,
    MovingHead,
    PARCan,
    LaserScanner,
    StripeLight,
}

impl FixtureType {
    pub fn to_string(&self) -> String {
        match self {
            FixtureType::RGBLight => "RGB Light".to_string(),
            FixtureType::MovingHead => "Moving Head".to_string(),
            FixtureType::PARCan => "PAR Can".to_string(),
            FixtureType::LaserScanner => "Laser Scanner".to_string(),
            FixtureType::StripeLight => "Stripe Light".to_string(),
        }
    }

    pub fn draw(&self, ui: &mut egui::Ui, rect: egui::Rect, color: egui::Color32) {
        let painter = ui.painter();
        match self {
            FixtureType::RGBLight => {
                painter.circle_filled(rect.center(), rect.width() / 2.0, color);
                painter.circle_stroke(
                    rect.center(),
                    rect.width() / 2.0,
                    egui::Stroke::new(1.0, egui::Color32::WHITE),
                );
            }
            FixtureType::MovingHead => {
                painter.rect_filled(rect, 4.0, color);
                let beam_start = rect.center();
                let beam_end = egui::pos2(beam_start.x, beam_start.y + rect.height() * 1.5);
                painter.line_segment(
                    [beam_start, beam_end],
                    egui::Stroke::new(
                        8.0,
                        egui::Color32::from_rgba_premultiplied(
                            color.r(),
                            color.g(),
                            color.b(),
                            100,
                        ),
                    ),
                );
            }
            FixtureType::PARCan => {
                painter.rect_filled(rect, 2.0, color);
            }
            FixtureType::LaserScanner => {
                painter.rect_filled(rect, 3.0, egui::Color32::DARK_GRAY);
                painter.line_segment(
                    [
                        rect.center(),
                        egui::pos2(rect.right() + 50.0, rect.center().y),
                    ],
                    egui::Stroke::new(2.0, color),
                );
            }
            FixtureType::StripeLight => {
                painter.rect_filled(rect, 0.0, color);
            }
        }
    }
}

pub struct VisualizerState {
    pub fixtures: Vec<FixtureVisualization>,
    pub stage_width: f32,
    pub stage_depth: f32,
    pub is_dragging: Option<usize>,
    pub is_editing_layout: bool,
    pub selected_fixture_type: FixtureType,
}

impl VisualizerState {
    pub fn new() -> Self {
        Self {
            fixtures: Vec::new(),
            stage_width: 800.0,
            stage_depth: 600.0,
            is_dragging: None,
            is_editing_layout: false,
            selected_fixture_type: FixtureType::RGBLight,
        }
    }

    pub fn show_visualizer(&mut self, ui: &mut egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        ui.heading("Stage Visualizer");

        // Controls for editing layout
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.is_editing_layout, "Edit Layout");

            if self.is_editing_layout {
                ui.separator();
                ui.label("Add Fixture:");
                egui::ComboBox::from_label("Type")
                    .selected_text(self.selected_fixture_type.to_string())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.selected_fixture_type,
                            FixtureType::RGBLight,
                            "RGB Light",
                        );
                        ui.selectable_value(
                            &mut self.selected_fixture_type,
                            FixtureType::MovingHead,
                            "Moving Head",
                        );
                        ui.selectable_value(
                            &mut self.selected_fixture_type,
                            FixtureType::PARCan,
                            "PAR Can",
                        );
                        ui.selectable_value(
                            &mut self.selected_fixture_type,
                            FixtureType::LaserScanner,
                            "Laser Scanner",
                        );
                        ui.selectable_value(
                            &mut self.selected_fixture_type,
                            FixtureType::StripeLight,
                            "Stripe Light",
                        );
                    });

                if ui.button("Add to Stage").clicked() {
                    // Place in center of stage
                    let fixture_size = match self.selected_fixture_type {
                        FixtureType::MovingHead => egui::vec2(40.0, 40.0),
                        FixtureType::RGBLight => egui::vec2(30.0, 30.0),
                        FixtureType::PARCan => egui::vec2(25.0, 35.0),
                        FixtureType::LaserScanner => egui::vec2(45.0, 25.0),
                        FixtureType::StripeLight => egui::vec2(100.0, 15.0),
                    };

                    let console_guard = console.lock().unwrap();
                    if !console_guard.fixtures.is_empty() {
                        self.fixtures.push(FixtureVisualization {
                            position: egui::vec2(self.stage_width / 2.0, self.stage_depth / 2.0),
                            size: fixture_size,
                            fixture_type: self.selected_fixture_type.clone(),
                            fixture_index: 0, // Will be assigned correctly during sync
                            dmx_address: 1,   // Will be assigned correctly during sync
                            color: egui::Color32::WHITE,
                        });
                        self.sync_fixtures_with_console(&console);
                    } else {
                        drop(console_guard);
                        ui.label("Please create fixtures in the fixture panel first");
                    }
                }
            }
        });

        // Stage visualization area
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(self.stage_width, self.stage_depth),
            egui::Sense::click_and_drag(),
        );

        // Draw stage background
        ui.painter()
            .rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 20, 30));

        // Draw grid
        let grid_spacing = 50.0;
        for x in (0..(self.stage_width as i32)).step_by(grid_spacing as usize) {
            ui.painter().line_segment(
                [
                    egui::pos2(rect.min.x + x as f32, rect.min.y),
                    egui::pos2(rect.min.x + x as f32, rect.max.y),
                ],
                egui::Stroke::new(
                    0.5,
                    egui::Color32::from_rgba_premultiplied(100, 100, 100, 50),
                ),
            );
        }

        for y in (0..(self.stage_depth as i32)).step_by(grid_spacing as usize) {
            ui.painter().line_segment(
                [
                    egui::pos2(rect.min.x, rect.min.y + y as f32),
                    egui::pos2(rect.max.x, rect.min.y + y as f32),
                ],
                egui::Stroke::new(
                    0.5,
                    egui::Color32::from_rgba_premultiplied(100, 100, 100, 50),
                ),
            );
        }

        // Get actual fixture colors from console state for visualization
        let console_guard = console.lock().unwrap();

        // Draw fixtures
        for (idx, fixture_vis) in self.fixtures.iter_mut().enumerate() {
            let fixture_rect = egui::Rect::from_center_size(
                egui::pos2(
                    rect.min.x + fixture_vis.position.x,
                    rect.min.y + fixture_vis.position.y,
                ),
                fixture_vis.size,
            );

            // Get color from actual DMX values if possible
            if let Some(fixture) = console_guard.fixtures.get(fixture_vis.fixture_index) {
                // Try to determine color from RGB channels if available
                let mut r = 0u8;
                let mut g = 0u8;
                let mut b = 0u8;

                for (i, channel) in fixture.channels.iter().enumerate() {
                    if channel.name.to_lowercase().contains("red") {
                        r = channel.value;
                    } else if channel.name.to_lowercase().contains("green") {
                        g = channel.value;
                    } else if channel.name.to_lowercase().contains("blue") {
                        b = channel.value;
                    }
                }

                if r > 0 || g > 0 || b > 0 {
                    fixture_vis.color = egui::Color32::from_rgb(r, g, b);
                }
            }

            // Draw the fixture
            fixture_vis
                .fixture_type
                .draw(ui, fixture_rect, fixture_vis.color);

            // Show fixture name
            if let Some(fixture) = console_guard.fixtures.get(fixture_vis.fixture_index) {
                ui.painter().text(
                    egui::pos2(fixture_rect.center().x, fixture_rect.max.y + 5.0),
                    egui::Align2::CENTER_TOP,
                    &fixture.name,
                    egui::FontId::default(),
                    egui::Color32::WHITE,
                );
            }

            // Handle dragging in edit mode
            if self.is_editing_layout {
                if self.is_dragging == Some(idx) {
                    if response.dragged() {
                        fixture_vis.position += response.drag_delta();
                        // Keep within bounds
                        fixture_vis.position.x = fixture_vis.position.x.clamp(
                            fixture_vis.size.x / 2.0,
                            self.stage_width - fixture_vis.size.x / 2.0,
                        );
                        fixture_vis.position.y = fixture_vis.position.y.clamp(
                            fixture_vis.size.y / 2.0,
                            self.stage_depth - fixture_vis.size.y / 2.0,
                        );
                    } else if response.drag_released() {
                        self.is_dragging = None;
                    }
                } else if response.clicked()
                    && fixture_rect.contains(response.interact_pointer_pos().unwrap_or_default())
                {
                    self.is_dragging = Some(idx);
                }
            }
        }

        drop(console_guard);
    }

    pub fn sync_fixtures_with_console(&mut self, console: &Arc<Mutex<LightingConsole>>) {
        let console_guard = console.lock().unwrap();

        // Remove visualizations for fixtures that no longer exist
        self.fixtures
            .retain(|vis| vis.fixture_index < console_guard.fixtures.len());

        // Make sure each console fixture has a visualization
        let mut fixture_indices_with_vis: Vec<usize> =
            self.fixtures.iter().map(|f| f.fixture_index).collect();

        for (idx, _) in console_guard.fixtures.iter().enumerate() {
            if !fixture_indices_with_vis.contains(&idx) {
                // New fixture needs visualization
                let fixture_type = if idx < self.fixtures.len() {
                    self.fixtures[idx].fixture_type.clone()
                } else {
                    FixtureType::RGBLight // Default
                };

                let fixture_size = match fixture_type {
                    FixtureType::MovingHead => egui::vec2(40.0, 40.0),
                    FixtureType::RGBLight => egui::vec2(30.0, 30.0),
                    FixtureType::PARCan => egui::vec2(25.0, 35.0),
                    FixtureType::LaserScanner => egui::vec2(45.0, 25.0),
                    FixtureType::StripeLight => egui::vec2(100.0, 15.0),
                };

                // Place randomly on stage
                let x = 50.0 + (idx as f32 * 100.0) % (self.stage_width - 100.0);
                let y = 100.0 + ((idx as f32 * 120.0) % (self.stage_depth - 200.0));

                self.fixtures.push(FixtureVisualization {
                    position: egui::vec2(x, y),
                    size: fixture_size,
                    fixture_type,
                    fixture_index: idx,
                    dmx_address: (idx * 10 + 1) as u16, // Placeholder address
                    color: egui::Color32::WHITE,
                });
            }
        }

        drop(console_guard);
    }
}
