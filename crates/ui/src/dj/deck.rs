//! Deck widget for DJ playback display and control.

use eframe::egui::{self, Color32, Rect, Rounding, Stroke, Vec2};
use halo_core::ConsoleCommand;
use tokio::sync::mpsc;

use super::TrackDragPayload;

/// Visual state for a single deck.
#[derive(Default)]
pub struct DeckWidget {
    /// Currently loaded track title.
    pub track_title: Option<String>,
    /// Currently loaded track artist.
    pub track_artist: Option<String>,
    /// Track duration in seconds.
    pub duration_seconds: f64,
    /// Current playback position in seconds.
    pub position_seconds: f64,
    /// Original BPM of the track.
    pub original_bpm: f64,
    /// Adjusted BPM (after pitch change).
    pub adjusted_bpm: f64,
    /// Pitch adjustment (-1.0 to 1.0).
    pub pitch: f64,
    /// Whether the deck is playing.
    pub is_playing: bool,
    /// Whether this deck is the master.
    pub is_master: bool,
    /// Whether sync is enabled.
    pub sync_enabled: bool,
    /// Hot cue positions (4 slots).
    pub hot_cues: [Option<f64>; 4],
    /// Cue point position.
    pub cue_point: Option<f64>,
    /// Beat phase (0.0 to 1.0).
    pub beat_phase: f64,
    /// Waveform data for display.
    pub waveform: Vec<f32>,
    /// Whether we are currently in cue preview mode (holding the cue button).
    /// This is tracked explicitly rather than relying on egui's button state
    /// because is_pointer_button_down_on() can lose track of the press.
    pub cue_preview_active: bool,
    /// Whether we've already handled the current cue button press.
    /// This prevents non-preview actions from firing repeatedly.
    cue_press_handled: bool,
}

impl DeckWidget {
    /// Returns whether the cue button is currently being held (for repaint requests).
    pub fn is_cue_held(&self) -> bool {
        self.cue_preview_active
    }

    /// Render the deck widget.
    pub fn render(
        &mut self,
        ui: &mut egui::Ui,
        deck_label: &str,
        deck_number: u8,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        let mut dropped_track_id: Option<i64> = None;

        // Get the rect we'll use for the deck
        let available_rect = ui.available_rect_before_wrap();
        let deck_rect = Rect::from_min_size(
            available_rect.min,
            egui::vec2(available_rect.width(), 400.0),
        );

        // Check if something is being dragged
        let is_dragging = ui.ctx().dragged_id().is_some();

        // Check if pointer is over our deck rect
        let pointer_over_deck = ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|pos| deck_rect.contains(pos));

        // Draw the deck frame background
        let fill_color = Color32::from_gray(25);
        ui.painter()
            .rect_filled(deck_rect, Rounding::same(8), fill_color);

        // Draw highlight border if dragging over this deck
        if is_dragging && pointer_over_deck {
            ui.painter().rect_stroke(
                deck_rect,
                8.0,
                Stroke::new(3.0, Color32::from_rgb(100, 200, 255)),
                egui::StrokeKind::Outside,
            );
        }

        // Check for drop: pointer was over deck and primary button just released
        if pointer_over_deck && ui.input(|i| i.pointer.primary_released()) {
            // Try to get the drag payload using the static DragAndDrop API
            if let Some(payload) = egui::DragAndDrop::take_payload::<TrackDragPayload>(ui.ctx()) {
                dropped_track_id = Some(payload.track_id);
            }
        }

        // Render deck contents inside the deck area
        let content_rect = deck_rect.shrink(12.0);
        let mut content_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(egui::Layout::top_down(egui::Align::LEFT)),
        );
        self.render_deck_contents(&mut content_ui, deck_label, deck_number, console_tx);

        // Consume the deck space
        ui.allocate_rect(deck_rect, egui::Sense::hover());

        // Send command if a track was dropped
        if let Some(track_id) = dropped_track_id {
            let _ = console_tx.send(ConsoleCommand::DjLoadTrack {
                deck: deck_number,
                track_id,
            });
        }
    }

    /// Render the internal deck contents.
    fn render_deck_contents(
        &mut self,
        ui: &mut egui::Ui,
        deck_label: &str,
        deck_number: u8,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        // Deck header with label and master indicator
        ui.horizontal(|ui| {
            ui.heading(format!("Deck {}", deck_label));
            if self.is_master {
                ui.label(
                    egui::RichText::new("MASTER")
                        .color(Color32::from_rgb(255, 200, 0))
                        .strong(),
                );
            }
            if self.sync_enabled {
                ui.label(
                    egui::RichText::new("SYNC")
                        .color(Color32::from_rgb(0, 200, 255))
                        .strong(),
                );
            }
        });

        ui.separator();

        // Track info
        if let Some(title) = &self.track_title {
            ui.label(egui::RichText::new(title).size(16.0).color(Color32::WHITE));
            if let Some(artist) = &self.track_artist {
                ui.label(egui::RichText::new(artist).size(14.0).color(Color32::GRAY));
            }
        } else {
            ui.label(
                egui::RichText::new("No track loaded")
                    .size(16.0)
                    .color(Color32::DARK_GRAY)
                    .italics(),
            );
        }

        ui.add_space(8.0);

        // Waveform display
        self.render_waveform(ui);

        ui.add_space(8.0);

        // Time and BPM display
        ui.horizontal(|ui| {
            // Time display
            let position_str = format_time(self.position_seconds);
            let duration_str = format_time(self.duration_seconds);
            let remaining = self.duration_seconds - self.position_seconds;
            let remaining_str = format!("-{}", format_time(remaining.max(0.0)));

            ui.label(
                egui::RichText::new(&position_str)
                    .size(24.0)
                    .monospace()
                    .color(Color32::WHITE),
            );
            ui.label(
                egui::RichText::new(format!(" / {} ", duration_str))
                    .size(14.0)
                    .monospace()
                    .color(Color32::GRAY),
            );
            ui.label(
                egui::RichText::new(&remaining_str)
                    .size(18.0)
                    .monospace()
                    .color(if remaining < 30.0 {
                        Color32::from_rgb(255, 100, 100)
                    } else {
                        Color32::GRAY
                    }),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // BPM display
                ui.label(
                    egui::RichText::new(format!("{:.1} BPM", self.adjusted_bpm))
                        .size(20.0)
                        .monospace()
                        .color(Color32::from_rgb(0, 255, 128)),
                );
            });
        });

        ui.add_space(8.0);

        // Transport controls
        ui.horizontal(|ui| {
            let button_size = Vec2::new(50.0, 40.0);
            let small_button_size = Vec2::new(40.0, 40.0);

            // Play/Pause button
            let play_text = if self.is_playing { "||" } else { ">" };
            let play_color = if self.is_playing {
                Color32::from_rgb(0, 200, 100)
            } else {
                Color32::WHITE
            };
            if ui
                .add_sized(
                    button_size,
                    egui::Button::new(egui::RichText::new(play_text).size(20.0).color(play_color)),
                )
                .clicked()
            {
                if self.is_playing {
                    // Currently playing, send pause
                    let _ = console_tx.send(ConsoleCommand::DjPause { deck: deck_number });
                } else {
                    // Currently paused, send play
                    let _ = console_tx.send(ConsoleCommand::DjPlay { deck: deck_number });
                }
                // State will be updated by DjDeckStateChanged event from the module
            }

            // Cue button - Pioneer CDJ-style behavior:
            // - When playing: click to jump to cue point and pause
            // - When paused AT cue point: HOLD to preview from cue, release to return
            // - When paused NOT at cue point: click to set new cue point
            let cue_color = if self.cue_preview_active {
                Color32::from_rgb(255, 100, 0) // Bright orange when previewing
            } else if self.cue_point.is_some() {
                Color32::from_rgb(255, 200, 0)
            } else {
                Color32::WHITE
            };
            let cue_response = ui.add_sized(
                button_size,
                egui::Button::new(egui::RichText::new("CUE").size(14.0).color(cue_color)),
            );

            // Check global pointer state - this is more reliable than is_pointer_button_down_on()
            // which can lose track of the press if the pointer moves slightly
            let primary_down = ui.input(|i| i.pointer.primary_down());
            let at_cue = is_at_cue_point(self.position_seconds, self.cue_point);

            // Reset press handled flag when mouse is released
            if !primary_down {
                self.cue_press_handled = false;
            }

            // Handle cue preview release - check FIRST before handling new presses
            // Release when: we're in preview mode AND mouse button is released
            if self.cue_preview_active && !primary_down {
                self.cue_preview_active = false;
                let _ = console_tx.send(ConsoleCommand::DjCuePreview {
                    deck: deck_number,
                    pressed: false,
                });
            }

            // Handle new button press - detect press on this button
            // but only when we haven't already handled this press
            let button_pressed = cue_response.is_pointer_button_down_on();
            let should_handle = button_pressed && !self.cue_press_handled;

            if should_handle {
                self.cue_press_handled = true;

                if self.is_playing {
                    // Playing: jump to cue point and pause
                    if let Some(cue_pos) = self.cue_point {
                        let _ = console_tx.send(ConsoleCommand::DjPause { deck: deck_number });
                        let _ = console_tx.send(ConsoleCommand::DjSeek {
                            deck: deck_number,
                            position_seconds: cue_pos,
                        });
                    }
                } else if at_cue {
                    // Paused AT cue point: start preview (will be held)
                    // We track this ourselves and use global pointer state for release
                    self.cue_preview_active = true;
                    let _ = console_tx.send(ConsoleCommand::DjCuePreview {
                        deck: deck_number,
                        pressed: true,
                    });
                } else {
                    // Paused NOT at cue point (or no cue): set new cue point
                    let _ = console_tx.send(ConsoleCommand::DjSetCue { deck: deck_number });
                }
            }

            ui.add_space(8.0);

            // Track search buttons (previous/next track)
            if ui
                .add_sized(
                    small_button_size,
                    egui::Button::new(egui::RichText::new("<<").size(16.0)),
                )
                .clicked()
            {
                let _ = console_tx.send(ConsoleCommand::DjPreviousTrack { deck: deck_number });
            }
            if ui
                .add_sized(
                    small_button_size,
                    egui::Button::new(egui::RichText::new(">>").size(16.0)),
                )
                .clicked()
            {
                let _ = console_tx.send(ConsoleCommand::DjNextTrack { deck: deck_number });
            }

            ui.add_space(8.0);

            // Sync button
            let sync_color = if self.sync_enabled {
                Color32::from_rgb(0, 200, 255)
            } else {
                Color32::GRAY
            };
            if ui
                .add_sized(
                    button_size,
                    egui::Button::new(egui::RichText::new("SYNC").size(12.0).color(sync_color)),
                )
                .clicked()
            {
                self.sync_enabled = !self.sync_enabled;
            }

            // Master button
            let master_color = if self.is_master {
                Color32::from_rgb(255, 200, 0)
            } else {
                Color32::GRAY
            };
            if ui
                .add_sized(
                    button_size,
                    egui::Button::new(egui::RichText::new("MST").size(12.0).color(master_color)),
                )
                .clicked()
            {
                self.is_master = !self.is_master;
            }
        });

        ui.add_space(8.0);

        // Hot cue buttons
        ui.horizontal(|ui| {
            ui.label("Hot Cues:");
            for i in 0..4 {
                let has_cue = self.hot_cues[i].is_some();
                let color = if has_cue {
                    hot_cue_color(i)
                } else {
                    Color32::DARK_GRAY
                };
                if ui
                    .add_sized(
                        Vec2::new(40.0, 30.0),
                        egui::Button::new(
                            egui::RichText::new(format!("{}", i + 1)).size(16.0).color(
                                if has_cue {
                                    Color32::BLACK
                                } else {
                                    Color32::GRAY
                                },
                            ),
                        )
                        .fill(color),
                    )
                    .clicked()
                {
                    if has_cue {
                        // Jump to hot cue
                    } else {
                        // Set hot cue
                        self.hot_cues[i] = Some(self.position_seconds);
                    }
                }
            }
        });

        ui.add_space(8.0);

        // Pitch fader
        ui.horizontal(|ui| {
            ui.label("Pitch:");
            let pitch_percent = self.pitch * 100.0;
            ui.add(
                egui::Slider::new(&mut self.pitch, -0.5..=0.5)
                    .show_value(false)
                    .trailing_fill(true),
            );
            ui.label(
                egui::RichText::new(format!("{:+.1}%", pitch_percent))
                    .monospace()
                    .color(if self.pitch.abs() > 0.01 {
                        Color32::from_rgb(255, 200, 0)
                    } else {
                        Color32::GRAY
                    }),
            );
        });
    }

    /// Render the waveform display.
    fn render_waveform(&self, ui: &mut egui::Ui) {
        let available_width = ui.available_width();
        let height = 60.0;
        let (rect, _response) =
            ui.allocate_exact_size(Vec2::new(available_width, height), egui::Sense::hover());

        let painter = ui.painter_at(rect);

        // Background
        painter.rect_filled(rect, Rounding::same(4), Color32::from_gray(15));

        // Draw waveform
        if !self.waveform.is_empty() {
            let num_samples = self.waveform.len();
            let samples_per_pixel = num_samples as f32 / available_width;
            let mid_y = rect.center().y;

            for x in 0..available_width as usize {
                let sample_idx = (x as f32 * samples_per_pixel) as usize;
                if sample_idx < num_samples {
                    let amplitude = self.waveform[sample_idx].abs() * (height / 2.0);
                    let color = waveform_color(sample_idx as f64 / num_samples as f64);
                    painter.line_segment(
                        [
                            egui::pos2(rect.left() + x as f32, mid_y - amplitude),
                            egui::pos2(rect.left() + x as f32, mid_y + amplitude),
                        ],
                        Stroke::new(1.0, color),
                    );
                }
            }
        } else {
            // Empty waveform placeholder
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No waveform",
                egui::FontId::proportional(12.0),
                Color32::DARK_GRAY,
            );
        }

        // Playhead position
        if self.duration_seconds > 0.0 {
            let progress = (self.position_seconds / self.duration_seconds) as f32;
            let playhead_x = rect.left() + (progress * available_width);
            painter.line_segment(
                [
                    egui::pos2(playhead_x, rect.top()),
                    egui::pos2(playhead_x, rect.bottom()),
                ],
                Stroke::new(2.0, Color32::WHITE),
            );
        }

        // Cue point marker
        if let Some(cue_pos) = self.cue_point {
            if self.duration_seconds > 0.0 {
                let cue_x =
                    rect.left() + ((cue_pos / self.duration_seconds) as f32 * available_width);
                painter.line_segment(
                    [
                        egui::pos2(cue_x, rect.top()),
                        egui::pos2(cue_x, rect.bottom()),
                    ],
                    Stroke::new(2.0, Color32::from_rgb(255, 200, 0)),
                );
            }
        }

        // Hot cue markers
        for (i, hot_cue) in self.hot_cues.iter().enumerate() {
            if let Some(pos) = hot_cue {
                if self.duration_seconds > 0.0 {
                    let x = rect.left() + ((*pos / self.duration_seconds) as f32 * available_width);
                    let marker_rect = Rect::from_center_size(
                        egui::pos2(x, rect.top() + 5.0),
                        Vec2::new(8.0, 10.0),
                    );
                    painter.rect_filled(marker_rect, Rounding::same(2), hot_cue_color(i));
                }
            }
        }

        // Beat phase indicator
        if self.is_playing {
            let beat_indicator_width = 4.0;
            let beat_x = rect.right() - 10.0 - (self.beat_phase as f32 * 20.0);
            let beat_rect = Rect::from_center_size(
                egui::pos2(beat_x, rect.bottom() - 5.0),
                Vec2::new(beat_indicator_width, 6.0),
            );
            painter.rect_filled(beat_rect, Rounding::same(1), Color32::from_rgb(0, 255, 128));
        }
    }
}

/// Format seconds as MM:SS.ss
fn format_time(seconds: f64) -> String {
    let mins = (seconds / 60.0).floor() as u32;
    let secs = seconds % 60.0;
    format!("{:02}:{:05.2}", mins, secs)
}

/// Get color for a hot cue slot.
fn hot_cue_color(slot: usize) -> Color32 {
    match slot {
        0 => Color32::from_rgb(255, 100, 100), // Red
        1 => Color32::from_rgb(100, 255, 100), // Green
        2 => Color32::from_rgb(100, 100, 255), // Blue
        3 => Color32::from_rgb(255, 255, 100), // Yellow
        _ => Color32::GRAY,
    }
}

/// Get color for waveform based on position.
fn waveform_color(progress: f64) -> Color32 {
    // Gradient from cyan to purple
    let r = (100.0 + progress * 155.0) as u8;
    let g = (200.0 - progress * 100.0) as u8;
    let b = 255;
    Color32::from_rgb(r, g, b)
}

/// Check if playhead position is approximately at the cue point.
fn is_at_cue_point(position: f64, cue_point: Option<f64>) -> bool {
    match cue_point {
        Some(cue) => (position - cue).abs() < 0.1, // 100ms tolerance
        None => false,
    }
}
