use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::time::{Duration, Instant};

// Timeline state
pub struct TimelineState {
    // Time-related properties
    pub total_duration: Duration,
    pub current_position: Duration,
    pub is_playing: bool,
    pub playback_start_time: Option<Instant>,
    pub playback_start_position: Duration,

    // Audio-related properties (placeholder for actual audio implementation)
    pub audio_loaded: bool,
    pub audio_filename: String,
    pub waveform_data: Vec<f32>, // Simplified waveform representation

    // Cue markers
    pub cue_markers: Vec<CueMarker>,

    // UI state
    pub dragging: bool,
    pub show_grid: bool,
    pub zoom_level: f32,               // 1.0 is default, higher values zoom in
    pub visible_range_start: Duration, // Start of the visible timeline window
}

// Represents a cue marker on the timeline
#[derive(Clone)]
pub struct CueMarker {
    pub time: Duration,
    pub name: String,
    pub color: Color32,
}

impl Default for TimelineState {
    fn default() -> Self {
        // Create some demo waveform data
        let mut waveform_data = Vec::with_capacity(1000);
        for i in 0..1000 {
            let t = i as f32 / 50.0;
            let value = (t.sin() * 0.5 + 0.5) * 0.8;
            waveform_data.push(value);
        }

        // Create some demo cue markers
        let cue_markers = vec![
            CueMarker {
                time: Duration::from_secs_f32(15.0),
                name: "Intro".to_string(),
                color: Color32::from_rgb(50, 150, 255),
            },
            CueMarker {
                time: Duration::from_secs_f32(30.0),
                name: "Verse 1".to_string(),
                color: Color32::from_rgb(50, 200, 100),
            },
            CueMarker {
                time: Duration::from_secs_f32(60.0),
                name: "Chorus".to_string(),
                color: Color32::from_rgb(200, 100, 50),
            },
            CueMarker {
                time: Duration::from_secs_f32(90.0),
                name: "Bridge".to_string(),
                color: Color32::from_rgb(200, 50, 200),
            },
        ];

        Self {
            total_duration: Duration::from_secs(120), // 2 minutes
            current_position: Duration::from_secs(0),
            is_playing: false,
            playback_start_time: None,
            playback_start_position: Duration::from_secs(0),
            audio_loaded: true, // For demo purposes
            audio_filename: "demo_track.wav".to_string(),
            waveform_data,
            cue_markers,
            dragging: false,
            show_grid: true,
            zoom_level: 1.0,
            visible_range_start: Duration::from_secs(0),
        }
    }
}

impl TimelineState {
    pub fn new() -> Self {
        Self::default()
    }

    // Start playback
    pub fn play(&mut self) {
        if !self.is_playing {
            self.is_playing = true;
            self.playback_start_time = Some(Instant::now());
            self.playback_start_position = self.current_position;
        }
    }

    // Pause playback
    pub fn pause(&mut self) {
        if self.is_playing {
            self.is_playing = false;
        }
    }

    // Stop playback and reset to beginning
    pub fn stop(&mut self) {
        self.is_playing = false;
        self.current_position = Duration::from_secs(0);
        self.playback_start_position = Duration::from_secs(0);
    }

    // Seek to a specific position
    pub fn seek_to(&mut self, position: Duration) {
        let position = position.min(self.total_duration);
        self.current_position = position;

        if self.is_playing {
            self.playback_start_time = Some(Instant::now());
            self.playback_start_position = position;
        }
    }

    // Update current position based on playback state
    pub fn update(&mut self) {
        if self.is_playing {
            if let Some(start_time) = self.playback_start_time {
                let elapsed = start_time.elapsed();
                let new_position = self.playback_start_position + elapsed;

                if new_position >= self.total_duration {
                    // Reached the end
                    self.current_position = self.total_duration;
                    self.is_playing = false;
                } else {
                    self.current_position = new_position;
                }
            }
        }
    }

    // Convert a timeline position to a screen X coordinate
    pub fn position_to_x(&self, position: Duration, width: f32) -> f32 {
        let visible_duration = self.get_visible_duration();
        let position_sec = position.as_secs_f32();
        let visible_start_sec = self.visible_range_start.as_secs_f32();

        let relative_pos = (position_sec - visible_start_sec) / visible_duration.as_secs_f32();
        relative_pos * width
    }

    // Convert a screen X coordinate to a timeline position
    pub fn x_to_position(&self, x: f32, width: f32) -> Duration {
        let visible_duration = self.get_visible_duration();
        let relative_pos = x / width;

        let seconds =
            self.visible_range_start.as_secs_f32() + relative_pos * visible_duration.as_secs_f32();

        Duration::from_secs_f32(seconds.max(0.0).min(self.total_duration.as_secs_f32()))
    }

    // Get the visible duration based on zoom level
    pub fn get_visible_duration(&self) -> Duration {
        // At zoom level 1.0, we show the entire timeline
        // Higher zoom levels show proportionally less duration
        let visible_seconds = self.total_duration.as_secs_f32() / self.zoom_level;
        Duration::from_secs_f32(visible_seconds)
    }

    // Format duration as time string
    pub fn format_time(&self, duration: Duration) -> String {
        let total_secs = duration.as_secs();
        let minutes = total_secs / 60;
        let seconds = total_secs % 60;
        let milliseconds = duration.subsec_millis();

        format!("{:02}:{:02}:{:03}", minutes, seconds, milliseconds)
    }

    // Add a cue marker at the current position
    pub fn add_cue_marker(&mut self, name: String, color: Color32) {
        self.cue_markers.push(CueMarker {
            time: self.current_position,
            name,
            color,
        });

        // Sort markers by time
        self.cue_markers.sort_by_key(|marker| marker.time);
    }

    // Find the nearest cue marker to a given time
    pub fn find_nearest_cue(&self, position: Duration, threshold_secs: f32) -> Option<usize> {
        let threshold = Duration::from_secs_f32(threshold_secs);

        self.cue_markers
            .iter()
            .enumerate()
            .map(|(idx, marker)| {
                let distance = if marker.time > position {
                    marker.time - position
                } else {
                    position - marker.time
                };
                (idx, distance)
            })
            .filter(|(_, distance)| *distance <= threshold)
            .min_by_key(|(_, distance)| *distance)
            .map(|(idx, _)| idx)
    }

    // Load audio file (placeholder)
    pub fn load_audio(&mut self, filename: &str) {
        // In a real implementation, this would load the audio file
        // and analyze it to generate waveform data
        self.audio_filename = filename.to_string();
        self.audio_loaded = true;

        // Generate some demo waveform data
        self.waveform_data.clear();

        for i in 0..2000 {
            let t = i as f32 / 50.0;

            // Create a more interesting waveform with multiple frequencies
            let value = (t.sin() * 0.3 + (t * 3.0).sin() * 0.2 + (t * 7.0).sin() * 0.1 + 0.5) * 0.8;

            self.waveform_data.push(value);
        }

        // Set a realistic duration based on the filename
        // In a real implementation, this would be determined from the audio file
        self.total_duration = Duration::from_secs(120); // 2 minutes
    }
}

pub struct Timeline {
    state: TimelineState,
}

impl Timeline {
    pub fn new() -> Self {
        Self {
            state: TimelineState::new(),
        }
    }

    pub fn update(&mut self) {
        self.state.update();
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        // Update playback position before rendering
        self.update();

        ui.vertical(|ui| {
            // Transport controls
            ui.horizontal(|ui| {
                if ui
                    .button(if self.state.is_playing {
                        "⏸ Pause"
                    } else {
                        "▶ Play"
                    })
                    .clicked()
                {
                    if self.state.is_playing {
                        self.state.pause();
                    } else {
                        self.state.play();
                    }
                }

                if ui.button("⏹ Stop").clicked() {
                    self.state.stop();
                }

                if ui.button("⏮ Start").clicked() {
                    self.state.seek_to(Duration::from_secs(0));
                }

                if ui.button("⏭ End").clicked() {
                    self.state.seek_to(self.state.total_duration);
                }

                // Current position display
                ui.label(
                    RichText::new(format!(
                        "{} / {}",
                        self.state.format_time(self.state.current_position),
                        self.state.format_time(self.state.total_duration)
                    ))
                    .monospace()
                    .size(16.0),
                );

                // Spacer
                ui.add_space(10.0);

                // Zoom controls
                ui.label("Zoom:");
                if ui.button("-").clicked() {
                    self.state.zoom_level = (self.state.zoom_level * 0.8).max(0.5);
                }

                ui.label(format!("{:.1}x", self.state.zoom_level));

                if ui.button("+").clicked() {
                    self.state.zoom_level = (self.state.zoom_level * 1.25).min(10.0);
                }

                // Audio file info/load button
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Load Audio...").clicked() {
                        // In a real implementation, this would open a file dialog
                        self.state.load_audio("new_track.wav");
                    }

                    if self.state.audio_loaded {
                        ui.label(format!("Audio: {}", self.state.audio_filename));
                    }
                });
            });

            // Timeline waveform and position indicator
            let timeline_height = 60.0;
            let (timeline_rect, timeline_response) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), timeline_height),
                Sense::click_and_drag(),
            );

            if timeline_response.clicked() {
                // Seek to clicked position
                if let Some(pos) = timeline_response.interact_pointer_pos() {
                    let new_pos = self
                        .state
                        .x_to_position(pos.x - timeline_rect.min.x, timeline_rect.width());
                    self.state.seek_to(new_pos);
                }
            }

            if timeline_response.dragged() {
                // Handle dragging
                self.state.dragging = true;
                if let Some(pos) = timeline_response.interact_pointer_pos() {
                    let new_pos = self
                        .state
                        .x_to_position(pos.x - timeline_rect.min.x, timeline_rect.width());
                    self.state.seek_to(new_pos);
                }
            } else if self.state.dragging && timeline_response.drag_stopped() {
                self.state.dragging = false;
            }

            // Draw timeline background
            ui.painter()
                .rect_filled(timeline_rect, 0.0, Color32::from_rgb(30, 30, 40));

            // Draw grid lines if enabled
            if self.state.show_grid {
                self.draw_grid(ui, timeline_rect);
            }

            // Draw waveform
            if self.state.audio_loaded {
                self.draw_waveform(ui, timeline_rect);
            }

            // Draw cue markers
            self.draw_cue_markers(ui, timeline_rect);

            // Draw playhead (current position indicator)
            let playhead_x = timeline_rect.min.x
                + self
                    .state
                    .position_to_x(self.state.current_position, timeline_rect.width());

            // Draw playhead line
            ui.painter().line_segment(
                [
                    Pos2::new(playhead_x, timeline_rect.min.y),
                    Pos2::new(playhead_x, timeline_rect.max.y),
                ],
                Stroke::new(2.0, Color32::from_rgb(255, 100, 100)),
            );

            // Draw playhead handle
            ui.painter().rect_filled(
                Rect::from_center_size(
                    Pos2::new(playhead_x, timeline_rect.min.y - 1.0),
                    Vec2::new(10.0, 8.0),
                ),
                0.0,
                Color32::from_rgb(255, 100, 100),
            );

            // Timeline scrubber (horizontal scrollbar for zoom)
            if self.state.zoom_level > 1.0 {
                let scrubber_height = 8.0;
                let (scrubber_rect, scrubber_response) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), scrubber_height),
                    Sense::click_and_drag(),
                );

                // Calculate visible portion and position of the scrollbar thumb
                let visible_portion = 1.0 / self.state.zoom_level;
                let total_secs = self.state.total_duration.as_secs_f32();
                let visible_start_normalized =
                    self.state.visible_range_start.as_secs_f32() / total_secs;

                let thumb_width = scrubber_rect.width() * visible_portion;
                let thumb_x =
                    scrubber_rect.min.x + visible_start_normalized * scrubber_rect.width();

                let thumb_rect = Rect::from_min_size(
                    Pos2::new(thumb_x, scrubber_rect.min.y),
                    Vec2::new(thumb_width, scrubber_height),
                );

                // Draw scrubber background
                ui.painter()
                    .rect_filled(scrubber_rect, 0.0, Color32::from_rgb(20, 20, 30));

                // Draw scrubber thumb
                ui.painter()
                    .rect_filled(thumb_rect, 2.0, Color32::from_rgb(80, 80, 100));

                // Handle scrubber interaction
                if scrubber_response.dragged() {
                    if let Some(pos) = scrubber_response.interact_pointer_pos() {
                        let normalized_pos = ((pos.x - scrubber_rect.min.x)
                            / scrubber_rect.width())
                        .max(0.0)
                        .min(1.0 - visible_portion);

                        let new_start = Duration::from_secs_f32(normalized_pos * total_secs);
                        self.state.visible_range_start = new_start;
                    }
                }
            }

            // Optional: Add cue list or cue editor below the timeline
            ui.add_space(10.0);
            ui.collapsing("Cue Points", |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Add Cue at Current Position").clicked() {
                        let cue_name = format!("Cue {}", self.state.cue_markers.len() + 1);
                        self.state
                            .add_cue_marker(cue_name, Color32::from_rgb(100, 150, 250));
                    }

                    if ui.button("Clear All Cues").clicked() {
                        self.state.cue_markers.clear();
                    }
                });

                ui.separator();

                // Create a copy of the cue markers
                let cue_markers = self.state.cue_markers.clone();

                // List of cue points with timing info
                for (i, marker) in cue_markers.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.button("Go").clicked() {
                            self.state.seek_to(marker.time);
                        }

                        let time_text = self.state.format_time(marker.time);
                        ui.label(format!("{}: {} - {}", i + 1, marker.name, time_text));

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("🗑").clicked() {
                                // Mark for removal (can't remove here due to borrowing)
                                // In a real implementation, use an event system or indices to handle this
                            }

                            let color_button =
                                ui.color_edit_button_srgba(&mut marker.color.clone());
                            if color_button.changed() {
                                // Update color (can't update here due to borrowing)
                                // In a real implementation, use an event system to handle this
                            }
                        });
                    });
                }
            });
        });
    }

    // Helper method to draw the timeline grid
    fn draw_grid(&self, ui: &mut egui::Ui, rect: Rect) {
        let visible_duration = self.state.get_visible_duration();
        let total_seconds = visible_duration.as_secs_f32();

        // Determine grid spacing based on zoom level
        let grid_spacing_seconds = if total_seconds <= 10.0 {
            0.5 // Half-second intervals for high zoom
        } else if total_seconds <= 30.0 {
            1.0 // 1-second intervals for medium zoom
        } else if total_seconds <= 120.0 {
            5.0 // 5-second intervals for low zoom
        } else if total_seconds <= 300.0 {
            10.0 // 10-second intervals
        } else {
            30.0 // 30-second intervals for very low zoom
        };

        // Start time (seconds) of the visible range
        let start_seconds = self.state.visible_range_start.as_secs_f32();

        // Find the first grid line
        let first_line_time = (start_seconds / grid_spacing_seconds).ceil() * grid_spacing_seconds;
        let mut current_time = first_line_time;

        // Draw vertical grid lines
        while current_time <= start_seconds + total_seconds {
            let x = rect.min.x
                + self
                    .state
                    .position_to_x(Duration::from_secs_f32(current_time), rect.width());

            // Major grid lines (every 5 minor lines, or at even multiples)
            let is_major = (current_time / grid_spacing_seconds) % 5.0 == 0.0
                || (grid_spacing_seconds >= 10.0 && current_time % 60.0 == 0.0);

            let line_color = if is_major {
                Color32::from_rgba_premultiplied(150, 150, 150, 80)
            } else {
                Color32::from_rgba_premultiplied(100, 100, 100, 40)
            };

            let line_stroke = if is_major { 1.0 } else { 0.5 };

            ui.painter().line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(line_stroke, line_color),
            );

            // Draw time labels for major grid lines
            if is_major {
                let minutes = (current_time as u32) / 60;
                let seconds = (current_time as u32) % 60;
                let time_label = format!("{:02}:{:02}", minutes, seconds);

                ui.painter().text(
                    Pos2::new(x, rect.min.y + 2.0),
                    egui::Align2::CENTER_TOP,
                    time_label,
                    egui::FontId::monospace(9.0),
                    Color32::from_rgb(180, 180, 180),
                );
            }

            current_time += grid_spacing_seconds;
        }

        // Draw horizontal center line
        ui.painter().line_segment(
            [
                Pos2::new(rect.min.x, rect.center().y),
                Pos2::new(rect.max.x, rect.center().y),
            ],
            Stroke::new(0.5, Color32::from_rgba_premultiplied(100, 100, 100, 40)),
        );
    }

    // Helper method to draw the audio waveform
    fn draw_waveform(&self, ui: &mut egui::Ui, rect: Rect) {
        if self.state.waveform_data.is_empty() {
            return;
        }

        // Calculate how many waveform points we need to display
        let visible_duration = self.state.get_visible_duration();
        let visible_start_time = self.state.visible_range_start.as_secs_f32();
        let total_duration = self.state.total_duration.as_secs_f32();

        // Determine which portion of the waveform data to display
        let start_idx = ((visible_start_time / total_duration)
            * self.state.waveform_data.len() as f32) as usize;

        let visible_points = ((visible_duration.as_secs_f32() / total_duration)
            * self.state.waveform_data.len() as f32) as usize;

        // Don't try to access beyond the array bounds
        let end_idx = (start_idx + visible_points).min(self.state.waveform_data.len());

        if start_idx >= end_idx || start_idx >= self.state.waveform_data.len() {
            return;
        }

        // Draw the waveform
        let center_y = rect.center().y;
        let height = rect.height() * 0.8; // Use 80% of the height

        let points_per_pixel = (end_idx - start_idx) as f32 / rect.width();

        // If we have more points than pixels, we need to downsample
        if points_per_pixel > 1.0 {
            // Downsample by taking min/max in windows
            let mut x = rect.min.x;
            let mut i = start_idx;

            while i < end_idx && x < rect.max.x {
                let points_in_pixel = points_per_pixel.ceil() as usize;
                let end_window = (i + points_in_pixel).min(end_idx);

                if i >= end_window {
                    break;
                }

                // Find min and max in this window
                let mut min_val: f32 = 1.0;
                let mut max_val: f32 = 0.0;

                for j in i..end_window {
                    let val = self.state.waveform_data[j];
                    min_val = min_val.min(val);
                    max_val = max_val.max(val);
                }

                // Draw a vertical line from min to max
                let y1 = center_y - (max_val - 0.5) * height;
                let y2 = center_y - (min_val - 0.5) * height;

                ui.painter().line_segment(
                    [Pos2::new(x, y1), Pos2::new(x, y2)],
                    Stroke::new(1.0, Color32::from_rgb(100, 180, 100)),
                );

                i = end_window;
                x += 1.0;
            }
        } else {
            // If we have fewer points than pixels, interpolate
            for i in start_idx..end_idx - 1 {
                let t1 = (i - start_idx) as f32 / (end_idx - start_idx) as f32;
                let t2 = (i + 1 - start_idx) as f32 / (end_idx - start_idx) as f32;

                let x1 = rect.min.x + t1 * rect.width();
                let x2 = rect.min.x + t2 * rect.width();

                let y1 = center_y - (self.state.waveform_data[i] - 0.5) * height;
                let y2 = center_y - (self.state.waveform_data[i + 1] - 0.5) * height;

                ui.painter().line_segment(
                    [Pos2::new(x1, y1), Pos2::new(x2, y2)],
                    Stroke::new(1.5, Color32::from_rgb(100, 180, 100)),
                );
            }
        }
    }

    // Helper method to draw cue markers
    fn draw_cue_markers(&self, ui: &mut egui::Ui, rect: Rect) {
        for marker in &self.state.cue_markers {
            // Skip if the marker is outside the visible range
            if marker.time < self.state.visible_range_start
                || marker.time > self.state.visible_range_start + self.state.get_visible_duration()
            {
                continue;
            }

            let x = rect.min.x + self.state.position_to_x(marker.time, rect.width());

            // Draw marker line
            ui.painter().line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(2.0, marker.color),
            );

            // Draw marker triangle
            let triangle_size = 8.0;
            let triangle_points = [
                Pos2::new(x, rect.min.y),
                Pos2::new(x - triangle_size, rect.min.y - triangle_size),
                Pos2::new(x + triangle_size, rect.min.y - triangle_size),
            ];

            ui.painter().add(egui::Shape::convex_polygon(
                triangle_points.to_vec(),
                marker.color,
                Stroke::new(1.0, Color32::from_rgb(50, 50, 50)),
            ));

            // Draw marker name
            ui.painter().text(
                Pos2::new(x, rect.min.y - triangle_size - 2.0),
                egui::Align2::CENTER_BOTTOM,
                &marker.name,
                egui::FontId::proportional(10.0),
                Color32::WHITE,
            );
        }
    }

    // Get a reference to the state for external use
    pub fn state(&self) -> &TimelineState {
        &self.state
    }

    // Get a mutable reference to the state for external use
    pub fn state_mut(&mut self) -> &mut TimelineState {
        &mut self.state
    }
}
