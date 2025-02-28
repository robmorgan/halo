use eframe::{egui, epi};
use egui::{Color32, Pos2, Rect, RichText, Rounding, Stroke, Ui, Vec2};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct HaloApp {
    show_timecode: bool,
    last_update: Instant,
    current_time: SystemTime,
    selected_fixtures: Vec<usize>,
    ableton_linked: bool,
    fps: u32,
    bpm: f32,
    active_cue_id: usize,
    timeline_position: f32,
    is_playing: bool,
    fixtures: Vec<Fixture>,
    cues: Vec<Cue>,
    overrides: Vec<Override>,
}

#[derive(Clone)]
struct Fixture {
    id: usize,
    name: String,
    fixture_type: FixtureType,
    color: Color32,
    selected: bool,
    sub_fixtures: Option<Vec<SubFixture>>,
}

#[derive(Clone)]
struct SubFixture {
    id: usize,
    color: Color32,
}

#[derive(Clone, PartialEq)]
enum FixtureType {
    MovingHead,
    PAR,
    LEDBar,
    Wash,
    Pinspot,
}

#[derive(Clone)]
struct Cue {
    id: usize,
    name: String,
    fade_time: String,
    timecode: String,
    progress: f32,
}

struct Override {
    id: usize,
    name: String,
    icon: OverrideIcon,
}

enum OverrideIcon {
    Smoke,
    Strobe,
    Blackout,
}

impl Default for HaloApp {
    fn default() -> Self {
        // Sample fixture data
        let fixtures = vec![
            Fixture {
                id: 1,
                name: "Spot 1".to_string(),
                fixture_type: FixtureType::MovingHead,
                color: Color32::from_rgb(255, 85, 85),
                selected: false,
                sub_fixtures: None,
            },
            Fixture {
                id: 2,
                name: "Spot 2".to_string(),
                fixture_type: FixtureType::MovingHead,
                color: Color32::from_rgb(85, 85, 255),
                selected: false,
                sub_fixtures: None,
            },
            Fixture {
                id: 3,
                name: "PAR 1".to_string(),
                fixture_type: FixtureType::PAR,
                color: Color32::from_rgb(85, 255, 85),
                selected: false,
                sub_fixtures: None,
            },
            Fixture {
                id: 4,
                name: "PAR 2".to_string(),
                fixture_type: FixtureType::PAR,
                color: Color32::from_rgb(255, 255, 85),
                selected: false,
                sub_fixtures: None,
            },
            Fixture {
                id: 5,
                name: "LED Bar 1".to_string(),
                fixture_type: FixtureType::LEDBar,
                color: Color32::from_rgb(255, 85, 255),
                selected: false,
                sub_fixtures: Some(vec![
                    SubFixture {
                        id: 51,
                        color: Color32::from_rgb(255, 85, 85),
                    },
                    SubFixture {
                        id: 52,
                        color: Color32::from_rgb(255, 255, 255),
                    },
                    SubFixture {
                        id: 53,
                        color: Color32::from_rgb(85, 85, 255),
                    },
                    SubFixture {
                        id: 54,
                        color: Color32::from_rgb(85, 255, 85),
                    },
                ]),
            },
            Fixture {
                id: 6,
                name: "Wash 1".to_string(),
                fixture_type: FixtureType::Wash,
                color: Color32::from_rgb(85, 255, 255),
                selected: false,
                sub_fixtures: None,
            },
            Fixture {
                id: 7,
                name: "Wash 2".to_string(),
                fixture_type: FixtureType::Wash,
                color: Color32::from_rgb(255, 255, 255),
                selected: false,
                sub_fixtures: None,
            },
            Fixture {
                id: 8,
                name: "Pinspot".to_string(),
                fixture_type: FixtureType::Pinspot,
                color: Color32::from_rgb(170, 170, 255),
                selected: false,
                sub_fixtures: None,
            },
        ];

        // Sample cues data
        let cues = vec![
            Cue {
                id: 1,
                name: "Cue 1: Intro".to_string(),
                fade_time: "3s".to_string(),
                timecode: "00:00:10:00".to_string(),
                progress: 0.8,
            },
            Cue {
                id: 2,
                name: "Cue 2: Verse".to_string(),
                fade_time: "2s".to_string(),
                timecode: "00:00:30:00".to_string(),
                progress: 0.0,
            },
            Cue {
                id: 3,
                name: "Cue 3: Chorus".to_string(),
                fade_time: "1.5s".to_string(),
                timecode: "00:01:15:00".to_string(),
                progress: 0.0,
            },
            Cue {
                id: 4,
                name: "Cue 4: Bridge".to_string(),
                fade_time: "4s".to_string(),
                timecode: "00:02:00:00".to_string(),
                progress: 0.0,
            },
            Cue {
                id: 5,
                name: "Cue 5: Outro".to_string(),
                fade_time: "5s".to_string(),
                timecode: "00:02:45:00".to_string(),
                progress: 0.0,
            },
        ];

        // Sample overrides
        let overrides = vec![
            Override {
                id: 1,
                name: "Smoke".to_string(),
                icon: OverrideIcon::Smoke,
            },
            Override {
                id: 2,
                name: "Strobe".to_string(),
                icon: OverrideIcon::Strobe,
            },
            Override {
                id: 3,
                name: "Blackout".to_string(),
                icon: OverrideIcon::Blackout,
            },
        ];

        Self {
            show_timecode: false,
            last_update: Instant::now(),
            current_time: SystemTime::now(),
            selected_fixtures: Vec::new(),
            ableton_linked: false,
            fps: 60,
            bpm: 120.0,
            active_cue_id: 1,
            timeline_position: 30.0,
            is_playing: false,
            fixtures,
            cues,
            overrides,
        }
    }
}

impl epi::App for HaloApp {
    fn name(&self) -> &str {
        "Halo Lighting Console"
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &epi::Frame) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);
        self.last_update = now;
        self.current_time = SystemTime::now();

        // Simulate random FPS changes
        if now.elapsed().as_millis() % 500 == 0 {
            self.fps = 58 + (rand::random::<u32>() % 5);
        }

        let dark_bg = Color32::from_rgb(0, 0, 0);
        let dark_panel_bg = Color32::from_rgb(16, 16, 16);
        let dark_element_bg = Color32::from_rgb(32, 32, 32);
        let gray_700 = Color32::from_rgb(55, 65, 81);
        let text_color = Color32::from_rgb(255, 255, 255);
        let text_dim = Color32::from_rgb(156, 163, 175);
        let border_color = Color32::from_rgb(55, 65, 81);
        let active_color = Color32::from_rgb(30, 64, 175);
        let highlight_color = Color32::from_rgb(59, 130, 246);

        egui::CentralPanel::default().show(ctx, |ui| {
            // Fill the entire window with black background
            let painter = ui.painter();
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(0.0, 0.0), ui.available_size()),
                Rounding::none(),
                dark_bg,
            );

            // Header Bar
            self.draw_header_bar(ui, dark_panel_bg, text_color, dark_element_bg);

            // Main content area
            let main_content_height = ui.available_height() - 80.0; // Subtract header and footer heights
            ui.horizontal(|ui| {
                // Left panel (Overrides and Fixtures)
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() * 0.6);

                    // Overrides Grid
                    self.draw_overrides_grid(
                        ui,
                        dark_panel_bg,
                        dark_element_bg,
                        text_color,
                        text_dim,
                    );

                    // Fixtures Grid
                    self.draw_fixtures_grid(
                        ui,
                        dark_panel_bg,
                        dark_element_bg,
                        text_color,
                        text_dim,
                        highlight_color,
                        main_content_height - 120.0, // Subtract the height of the overrides grid
                    );
                });

                // Right panel (Session and Cues)
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());

                    // Session Pane
                    self.draw_session_pane(ui, dark_panel_bg, text_color, text_dim);

                    // Cue Pane
                    self.draw_cue_pane(
                        ui,
                        dark_panel_bg,
                        dark_element_bg,
                        text_color,
                        text_dim,
                        highlight_color,
                        active_color,
                        main_content_height - 80.0, // Subtract session pane height
                    );
                });
            });

            // Programmer
            self.draw_programmer(ui, dark_panel_bg, dark_element_bg, text_color, text_dim);

            // Timeline
            self.draw_timeline(
                ui,
                dark_panel_bg,
                dark_element_bg,
                text_color,
                text_dim,
                highlight_color,
            );

            // Footer
            self.draw_footer(ui, dark_panel_bg, text_dim);

            // Request a repaint for continuous updates
            ctx.request_repaint();
        });
    }
}

impl HaloApp {
    fn draw_header_bar(
        &mut self,
        ui: &mut Ui,
        bg_color: Color32,
        text_color: Color32,
        button_color: Color32,
    ) {
        ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(ui.available_width(), 40.0)),
            Rounding::none(),
            bg_color,
        );

        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.label(RichText::new("HALO").color(text_color).size(24.0).strong());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                if ui
                    .button(RichText::new("⚙").size(18.0).color(text_color))
                    .on_hover_text("Settings")
                    .clicked()
                {
                    // Open settings
                }

                if ui
                    .button(RichText::new("PATCH").size(16.0).color(text_color))
                    .on_hover_text("Open patch panel")
                    .clicked()
                {
                    // Open patch panel
                }
            });
        });
    }

    fn draw_overrides_grid(
        &self,
        ui: &mut Ui,
        bg_color: Color32,
        button_color: Color32,
        text_color: Color32,
        text_dim: Color32,
    ) {
        ui.painter()
            .rect_filled(ui.available_rect_before_wrap(), Rounding::none(), bg_color);

        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("OVERRIDES").color(text_dim).size(12.0));
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                for override_item in &self.overrides {
                    ui.vertical(|ui| {
                        ui.set_width(80.0);
                        ui.set_height(60.0);

                        let button =
                            ui.add_sized([80.0, 60.0], egui::Button::new("").fill(button_color));

                        if button.clicked() {
                            // Trigger override action
                        }

                        // Draw the icon and label centered in the button
                        if button.rect.is_positive() {
                            let painter = ui.painter();
                            let center = button.rect.center();

                            // Draw the icon
                            match override_item.icon {
                                OverrideIcon::Smoke => {
                                    painter.circle_filled(
                                        Pos2::new(center.x, center.y - 8.0),
                                        10.0,
                                        text_color,
                                    );
                                    let drop_points = [
                                        Pos2::new(center.x, center.y - 18.0),
                                        Pos2::new(center.x - 5.0, center.y - 8.0),
                                        Pos2::new(center.x + 5.0, center.y - 8.0),
                                    ];
                                    painter.add(egui::Shape::convex_polygon(
                                        drop_points.to_vec(),
                                        text_color,
                                        Stroke::none(),
                                    ));
                                }
                                OverrideIcon::Strobe => {
                                    painter.line_segment(
                                        [
                                            Pos2::new(center.x - 10.0, center.y - 5.0),
                                            Pos2::new(center.x + 10.0, center.y - 5.0),
                                        ],
                                        Stroke::new(2.0, text_color),
                                    );
                                    painter.line_segment(
                                        [
                                            Pos2::new(center.x, center.y - 15.0),
                                            Pos2::new(center.x, center.y - 5.0),
                                        ],
                                        Stroke::new(2.0, text_color),
                                    );
                                }
                                OverrideIcon::Blackout => {
                                    painter.circle_stroke(
                                        center,
                                        12.0,
                                        Stroke::new(2.0, text_color),
                                    );
                                    painter.line_segment(
                                        [
                                            Pos2::new(center.x - 8.0, center.y),
                                            Pos2::new(center.x + 8.0, center.y),
                                        ],
                                        Stroke::new(2.0, text_color),
                                    );
                                }
                            }

                            // Draw the label
                            painter.text(
                                Pos2::new(center.x, center.y + 15.0),
                                egui::Align2::CENTER_CENTER,
                                &override_item.name,
                                egui::FontId::proportional(12.0),
                                text_color,
                            );
                        }
                    });
                }
            });

            ui.add_space(8.0);
        });
    }

    fn draw_fixtures_grid(
        &mut self,
        ui: &mut Ui,
        bg_color: Color32,
        button_color: Color32,
        text_color: Color32,
        text_dim: Color32,
        highlight_color: Color32,
        height: f32,
    ) {
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
                                );
                            } else {
                                ui.painter().rect_stroke(
                                    rect,
                                    Rounding::same(4.0),
                                    Stroke::new(1.0, Color32::from_gray(70)),
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

    fn draw_session_pane(
        &mut self,
        ui: &mut Ui,
        bg_color: Color32,
        text_color: Color32,
        text_dim: Color32,
    ) {
        ui.painter().rect_filled(
            ui.available_rect_before_wrap()
                .intersect(Rect::from_min_size(
                    ui.min_rect().min,
                    Vec2::new(ui.available_width(), 80.0),
                )),
            Rounding::none(),
            bg_color,
        );

        ui.vertical(|ui| {
            ui.add_space(12.0);

            ui.horizontal(|ui| {
                ui.add_space(12.0);

                // Clock/Timecode
                let time_text = if self.show_timecode {
                    "00:01:24:15".to_string() // Sample timecode
                } else {
                    let time = self
                        .current_time
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or(Duration::from_secs(0));
                    let secs = time.as_secs();
                    let hours = (secs / 3600) % 24;
                    let minutes = (secs / 60) % 60;
                    let seconds = secs % 60;
                    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
                };

                ui.label(
                    RichText::new(time_text)
                        .font(egui::FontId::monospace(24.0))
                        .color(text_color),
                );

                if ui
                    .button(RichText::new("⏱").size(14.0).color(text_color))
                    .clicked()
                {
                    self.show_timecode = !self.show_timecode;
                }

                ui.add_space(ui.available_width() - 120.0);

                // BPM and Link
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("BPM").size(12.0).color(text_dim));
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(format!("{:.1}", self.bpm))
                                .size(24.0)
                                .strong()
                                .color(text_color),
                        );

                        ui.add_space(12.0);

                        let link_button =
                            ui.button(RichText::new("Link").size(14.0).color(text_color));

                        if link_button.clicked() {
                            self.ableton_linked = !self.ableton_linked;
                        }

                        if self.ableton_linked {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new("3 peers")
                                    .size(10.0)
                                    .color(Color32::from_rgb(74, 222, 128)),
                            );
                        }
                    });
                });
            });

            ui.add_space(12.0);
        });
    }

    fn draw_cue_pane(
        &self,
        ui: &mut Ui,
        bg_color: Color32,
        button_color: Color32,
        text_color: Color32,
        text_dim: Color32,
        highlight_color: Color32,
        active_color: Color32,
        height: f32,
    ) {
        egui::ScrollArea::vertical()
            .max_height(height)
            .show(ui, |ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("CUE LIST").color(text_dim).size(12.0));
                ui.add_space(8.0);

                for cue in &self.cues {
                    let is_active = cue.id == self.active_cue_id;
                    let bg = if is_active {
                        active_color
                    } else {
                        button_color
                    };

                    ui.add_space(4.0);
                    let cue_height = 40.0;
                    let cue_rect = ui
                        .allocate_space(Vec2::new(ui.available_width(), cue_height))
                        .1;

                    // Draw cue background
                    ui.painter().rect_filled(cue_rect, Rounding::same(4.0), bg);

                    // Draw cue content
                    let text_rect = Rect::from_min_size(
                        Pos2::new(cue_rect.min.x + 8.0, cue_rect.min.y + 8.0),
                        Vec2::new(cue_rect.width() - 16.0, 16.0),
                    );

                    // Draw cue name
                    ui.painter().text(
                        text_rect.min,
                        egui::Align2::LEFT_TOP,
                        &cue.name,
                        egui::FontId::proportional(14.0),
                        text_color,
                    );

                    // Draw cue details (fade time and timecode)
                    ui.painter().text(
                        Pos2::new(text_rect.max.x, text_rect.min.y),
                        egui::Align2::RIGHT_TOP,
                        format!("{} | {}", cue.fade_time, cue.timecode),
                        egui::FontId::proportional(10.0),
                        text_dim,
                    );

                    // Draw progress bar
                    let progress_rect = Rect::from_min_size(
                        Pos2::new(cue_rect.min.x + 8.0, cue_rect.max.y - 10.0),
                        Vec2::new(cue_rect.width() - 16.0, 4.0),
                    );

                    ui.painter().rect_filled(
                        progress_rect,
                        Rounding::same(2.0),
                        Color32::from_gray(60),
                    );

                    if cue.progress > 0.0 {
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                progress_rect.min,
                                Vec2::new(
                                    progress_rect.width() * cue.progress,
                                    progress_rect.height(),
                                ),
                            ),
                            Rounding::same(2.0),
                            highlight_color,
                        );
                    }

                    // Handle click
                    let response =
                        ui.interact(cue_rect, ui.id().with(cue.id), egui::Sense::click());
                    if response.clicked() {
                        // Select cue logic would go here
                    }
                }
            });
    }

    fn draw_programmer(
        &self,
        ui: &mut Ui,
        bg_color: Color32,
        button_color: Color32,
        text_color: Color32,
        text_dim: Color32,
    ) {
        ui.painter().rect_filled(
            ui.available_rect_before_wrap()
                .intersect(Rect::from_min_size(
                    ui.min_rect().min,
                    Vec2::new(ui.available_width(), 120.0),
                )),
            Rounding::none(),
            bg_color,
        );

        ui.vertical(|ui| {
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("PROGRAMMER").color(text_dim).size(12.0));

                ui.add_space(ui.available_width() - 200.0);

                ui.label(RichText::new("EFFECTS").color(text_dim).size(12.0));
            });

            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.add_space(12.0);

                // Parameter controls
                let control_width = (ui.available_width() * 0.6 - 24.0) / 5.0;

                // Intensity
                ui.vertical(|ui| {
                    ui.set_width(control_width);
                    ui.label(RichText::new("Intensity").size(10.0).color(text_dim));
                    ui.add_space(2.0);

                    let control_rect = ui.allocate_space(Vec2::new(control_width - 10.0, 20.0)).1;
                    ui.painter()
                        .rect_filled(control_rect, Rounding::same(4.0), button_color);

                    // Add slider
                    let mut intensity_value = 75.0; // Example value
                    let slider =
                        egui::Slider::new(&mut intensity_value, 0.0..=100.0).show_value(false);
                    ui.add(slider);
                });

                // Color
                ui.vertical(|ui| {
                    ui.set_width(control_width);
                    ui.label(RichText::new("Color").size(10.0).color(text_dim));
                    ui.add_space(2.0);

                    ui.horizontal(|ui| {
                        for color in [
                            Color32::from_rgb(255, 0, 0),
                            Color32::from_rgb(0, 255, 0),
                            Color32::from_rgb(0, 0, 255),
                            Color32::from_rgb(255, 255, 255),
                        ] {
                            let color_rect = ui.allocate_space(Vec2::new(20.0, 20.0)).1;
                            ui.painter().circle_filled(color_rect.center(), 10.0, color);

                            // Add interaction
                            let response =
                                ui.interact(color_rect, ui.id().with(color), egui::Sense::click());
                            if response.clicked() {
                                // Select color logic would go here
                            }
                        }
                    });
                });

                // Pan
                ui.vertical(|ui| {
                    ui.set_width(control_width);
                    ui.label(RichText::new("Pan").size(10.0).color(text_dim));
                    ui.add_space(2.0);

                    let mut pan_value = 50.0; // Example value
                    let slider = egui::Slider::new(&mut pan_value, 0.0..=100.0).show_value(false);
                    ui.add(slider);
                });

                // Tilt
                ui.vertical(|ui| {
                    ui.set_width(control_width);
                    ui.label(RichText::new("Tilt").size(10.0).color(text_dim));
                    ui.add_space(2.0);

                    let mut tilt_value = 60.0; // Example value
                    let slider = egui::Slider::new(&mut tilt_value, 0.0..=100.0).show_value(false);
                    ui.add(slider);
                });

                // Gobo
                ui.vertical(|ui| {
                    ui.set_width(control_width);
                    ui.label(RichText::new("Gobo").size(10.0).color(text_dim));
                    ui.add_space(2.0);

                    ui.horizontal(|ui| {
                        for i in 0..4 {
                            let gobo_rect = ui.allocate_space(Vec2::new(20.0, 20.0)).1;
                            ui.painter().circle_stroke(
                                gobo_rect.center(),
                                8.0,
                                Stroke::new(1.0, Color32::from_gray(180)),
                            );

                            // Add some pattern to make it look like a gobo
                            let center = gobo_rect.center();
                            match i {
                                0 => {
                                    // Star pattern
                                    for j in 0..5 {
                                        let angle = j as f32 * 2.0 * std::f32::consts::PI / 5.0;
                                        let end_x = center.x + angle.cos() * 6.0;
                                        let end_y = center.y + angle.sin() * 6.0;
                                        ui.painter().line_segment(
                                            [center, Pos2::new(end_x, end_y)],
                                            Stroke::new(1.0, Color32::from_gray(180)),
                                        );
                                    }
                                }
                                1 => {
                                    // Circle pattern
                                    ui.painter().circle_stroke(
                                        center,
                                        4.0,
                                        Stroke::new(1.0, Color32::from_gray(180)),
                                    );
                                }
                                2 => {
                                    // Cross pattern
                                    ui.painter().line_segment(
                                        [
                                            Pos2::new(center.x - 5.0, center.y),
                                            Pos2::new(center.x + 5.0, center.y),
                                        ],
                                        Stroke::new(1.0, Color32::from_gray(180)),
                                    );
                                    ui.painter().line_segment(
                                        [
                                            Pos2::new(center.x, center.y - 5.0),
                                            Pos2::new(center.x, center.y + 5.0),
                                        ],
                                        Stroke::new(1.0, Color32::from_gray(180)),
                                    );
                                }
                                3 => {
                                    // Dots pattern
                                    for j in 0..3 {
                                        for k in 0..3 {
                                            if (j + k) % 2 == 0 {
                                                ui.painter().circle_filled(
                                                    Pos2::new(
                                                        center.x + (j as f32 - 1.0) * 3.0,
                                                        center.y + (k as f32 - 1.0) * 3.0,
                                                    ),
                                                    1.0,
                                                    Color32::from_gray(180),
                                                );
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }

                            // Add interaction
                            let response = ui.interact(
                                gobo_rect,
                                ui.id().with("gobo").with(i),
                                egui::Sense::click(),
                            );
                            if response.clicked() {
                                // Select gobo logic would go here
                            }
                        }
                    });
                });

                // Effects section
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() - 64.0);

                    let effects_rect = ui.allocate_space(Vec2::new(ui.available_width(), 80.0)).1;
                    ui.painter()
                        .rect_filled(effects_rect, Rounding::same(4.0), button_color);

                    ui.allocate_ui_at_rect(effects_rect, |ui| {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);

                            // Waveform
                            ui.vertical(|ui| {
                                ui.set_width(90.0);
                                ui.label(RichText::new("Waveform").size(10.0).color(text_dim));

                                let mut selected = 0;
                                egui::ComboBox::from_id_source("waveform_combo")
                                    .selected_text(
                                        ["Sine", "Sawtooth", "Square", "Triangle"][selected],
                                    )
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut selected, 0, "Sine");
                                        ui.selectable_value(&mut selected, 1, "Sawtooth");
                                        ui.selectable_value(&mut selected, 2, "Square");
                                        ui.selectable_value(&mut selected, 3, "Triangle");
                                    });
                            });

                            // Interval
                            ui.vertical(|ui| {
                                ui.set_width(90.0);
                                ui.label(RichText::new("Interval").size(10.0).color(text_dim));

                                let mut selected = 0;
                                egui::ComboBox::from_id_source("interval_combo")
                                    .selected_text(["Beat", "Bar", "Phrase"][selected])
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut selected, 0, "Beat");
                                        ui.selectable_value(&mut selected, 1, "Bar");
                                        ui.selectable_value(&mut selected, 2, "Phrase");
                                    });
                            });

                            // Ratio
                            ui.vertical(|ui| {
                                ui.set_width(90.0);
                                ui.label(RichText::new("Ratio").size(10.0).color(text_dim));

                                let mut ratio = 1.0;
                                let slider =
                                    egui::Slider::new(&mut ratio, 0.1..=4.0).show_value(true);
                                ui.add(slider);
                            });

                            // Phase
                            ui.vertical(|ui| {
                                ui.set_width(90.0);
                                ui.label(RichText::new("Phase").size(10.0).color(text_dim));

                                let mut phase = 0.0;
                                let slider = egui::Slider::new(&mut phase, 0.0..=360.0)
                                    .suffix("°")
                                    .show_value(true);
                                ui.add(slider);
                            });

                            // Min/Max
                            ui.vertical(|ui| {
                                ui.set_width(90.0);
                                ui.label(RichText::new("Min/Max").size(10.0).color(text_dim));

                                ui.horizontal(|ui| {
                                    let mut min_val = 0;
                                    let mut max_val = 255;
                                    ui.add(
                                        egui::DragValue::new(&mut min_val)
                                            .speed(1.0)
                                            .clamp_range(0..=255),
                                    );
                                    ui.label("-");
                                    ui.add(
                                        egui::DragValue::new(&mut max_val)
                                            .speed(1.0)
                                            .clamp_range(0..=255),
                                    );
                                });
                            });

                            // Distribution
                            ui.vertical(|ui| {
                                ui.set_width(90.0);
                                ui.label(RichText::new("Distribution").size(10.0).color(text_dim));

                                let mut selected = 0;
                                egui::ComboBox::from_id_source("distribution_combo")
                                    .selected_text(["All", "Step", "Wave"][selected])
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut selected, 0, "All");
                                        ui.selectable_value(&mut selected, 1, "Step");
                                        ui.selectable_value(&mut selected, 2, "Wave");
                                    });
                            });
                        });
                    });
                });

                // Clear and Record buttons
                ui.vertical(|ui| {
                    ui.set_width(60.0);
                    ui.add_space(4.0);

                    if ui.button(RichText::new("Clear").size(14.0)).clicked() {
                        // Clear programmer logic
                    }

                    ui.add_space(8.0);

                    if ui.button(RichText::new("Record").size(14.0)).clicked() {
                        // Record programmer state logic
                    }
                });
            });
        });
    }

    fn draw_timeline(
        &self,
        ui: &mut Ui,
        bg_color: Color32,
        button_color: Color32,
        text_color: Color32,
        text_dim: Color32,
        highlight_color: Color32,
    ) {
        ui.painter().rect_filled(
            ui.available_rect_before_wrap()
                .intersect(Rect::from_min_size(
                    ui.min_rect().min,
                    Vec2::new(ui.available_width(), 40.0),
                )),
            Rounding::none(),
            bg_color,
        );

        ui.vertical(|ui| {
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("TIMELINE").color(text_dim).size(12.0));

                ui.add_space(8.0);

                // Playback controls
                ui.horizontal(|ui| {
                    if ui.button("⏮").clicked() {
                        // Skip backward logic
                    }

                    if ui.button(if self.is_playing { "⏸" } else { "▶" }).clicked() {
                        // Toggle play/pause logic
                    }

                    if ui.button("⏭").clicked() {
                        // Skip forward logic
                    }
                });

                ui.add_space(8.0);
                ui.label(
                    RichText::new("00:01:24:15")
                        .font(egui::FontId::monospace(12.0))
                        .color(text_color),
                );
            });

            ui.add_space(4.0);

            // Timeline bar
            let timeline_rect = ui
                .allocate_space(Vec2::new(ui.available_width() - 24.0, 8.0))
                .1;
            ui.painter()
                .rect_filled(timeline_rect, Rounding::same(2.0), button_color);

            // Timeline position indicator
            let position_x =
                timeline_rect.min.x + timeline_rect.width() * (self.timeline_position / 100.0);
            ui.painter().rect_filled(
                Rect::from_center_size(
                    Pos2::new(position_x, timeline_rect.center().y),
                    Vec2::new(2.0, timeline_rect.height()),
                ),
                Rounding::none(),
                text_color,
            );

            // Add cue markers
            for cue in &self.cues {
                // Extract the seconds from the timecode (HH:MM:SS:FF format)
                let parts: Vec<&str> = cue.timecode.split(':').collect();
                if parts.len() >= 3 {
                    let seconds = parts[0].parse::<f32>().unwrap_or(0.0) * 3600.0
                        + parts[1].parse::<f32>().unwrap_or(0.0) * 60.0
                        + parts[2].parse::<f32>().unwrap_or(0.0);

                    // Position based on total timeline duration (assumed 3 minutes for example)
                    let total_duration = 180.0; // 3 minutes in seconds
                    let position = (seconds / total_duration).min(1.0) * timeline_rect.width();

                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            Pos2::new(timeline_rect.min.x + position - 1.0, timeline_rect.min.y),
                            Vec2::new(2.0, 3.0),
                        ),
                        Rounding::none(),
                        highlight_color,
                    );
                }
            }

            // Add interaction for scrubbing
            let response = ui.interact(
                timeline_rect,
                ui.id().with("timeline"),
                egui::Sense::click_and_drag(),
            );
            if response.dragged() || response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let normalized_pos =
                        ((pos.x - timeline_rect.min.x) / timeline_rect.width()).clamp(0.0, 1.0);
                    // Timeline position logic would go here
                }
            }
        });
    }

    fn draw_footer(&self, ui: &mut Ui, bg_color: Color32, text_dim: Color32) {
        ui.painter()
            .rect_filled(ui.available_rect_before_wrap(), Rounding::none(), bg_color);

        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(
                RichText::new(format!("FPS: {}", self.fps))
                    .size(12.0)
                    .color(text_dim),
            );

            ui.with_layout(
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    ui.label(
                        RichText::new(format!("{} Fixtures | 42 Parameters", self.fixtures.len()))
                            .size(12.0)
                            .color(text_dim),
                    );
                },
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("Halo v1.0").size(12.0).color(text_dim));
            });
        });
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(1200.0, 800.0)),
        min_window_size: Some(egui::vec2(800.0, 600.0)),
        default_theme: eframe::Theme::Dark,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "Halo Lighting Console",
        options,
        Box::new(|_cc| Box::new(HaloApp::default())),
    )
}
