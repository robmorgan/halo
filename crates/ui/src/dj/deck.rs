//! Deck widget for DJ playback display and control.

use std::sync::Arc;

use eframe::egui::{self, Color32, Rect, Rounding, Stroke, Vec2};
use halo_core::ConsoleCommand;
use tokio::sync::mpsc;

use super::waveform_texture::WaveformTexture;
use super::TrackDragPayload;

/// Waveform zoom levels (visible duration in seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaveformZoomLevel {
    Overview,
    Seconds16,
    #[default]
    Seconds8,
    Seconds4,
    Seconds2,
    Seconds1,
}

impl WaveformZoomLevel {
    /// Get the visible duration in seconds for this zoom level.
    /// Returns None for Overview mode (full track).
    pub fn visible_duration(&self) -> Option<f64> {
        match self {
            Self::Overview => None,
            Self::Seconds16 => Some(16.0),
            Self::Seconds8 => Some(8.0),
            Self::Seconds4 => Some(4.0),
            Self::Seconds2 => Some(2.0),
            Self::Seconds1 => Some(1.0),
        }
    }

    /// Get display label for UI.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Overview => "OVERVIEW",
            Self::Seconds16 => "16s",
            Self::Seconds8 => "8s",
            Self::Seconds4 => "4s",
            Self::Seconds2 => "2s",
            Self::Seconds1 => "1s",
        }
    }

    /// Zoom in one level (returns self if already at max zoom).
    pub fn zoom_in(&self) -> Self {
        match self {
            Self::Overview => Self::Seconds16,
            Self::Seconds16 => Self::Seconds8,
            Self::Seconds8 => Self::Seconds4,
            Self::Seconds4 => Self::Seconds2,
            Self::Seconds2 | Self::Seconds1 => Self::Seconds1,
        }
    }

    /// Zoom out one level (returns self if already at min zoom).
    pub fn zoom_out(&self) -> Self {
        match self {
            Self::Overview | Self::Seconds16 => Self::Overview,
            Self::Seconds8 => Self::Seconds16,
            Self::Seconds4 => Self::Seconds8,
            Self::Seconds2 => Self::Seconds4,
            Self::Seconds1 => Self::Seconds2,
        }
    }

    /// Check if this is a zoomed view (not overview).
    pub fn is_zoomed(&self) -> bool {
        !matches!(self, Self::Overview)
    }
}

/// Visual state for a single deck.
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
    /// Whether the deck is waiting for quantized sync start.
    pub waiting_for_quantized_start: bool,
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
    /// Waveform data for display (Arc for zero-copy sharing from state).
    pub waveform: Arc<Vec<f32>>,
    /// 3-band frequency data for colored waveform (low, mid, high).
    /// Arc for zero-copy sharing. None for legacy waveforms without frequency analysis.
    pub waveform_colors: Option<Arc<Vec<(f32, f32, f32)>>>,
    /// Beat positions in seconds (from beat grid analysis).
    pub beat_positions: Vec<f64>,
    /// First beat offset in seconds.
    pub first_beat_offset: f64,
    /// Whether we are currently in cue preview mode (holding the cue button).
    /// This is tracked explicitly rather than relying on egui's button state
    /// because is_pointer_button_down_on() can lose track of the press.
    pub cue_preview_active: bool,
    /// Whether we've already handled the current cue button press.
    /// This prevents non-preview actions from firing repeatedly.
    cue_press_handled: bool,
    /// Master Tempo (key lock) enabled.
    pub master_tempo_enabled: bool,
    /// Tempo range setting (0=±6%, 1=±10%, 2=±16%, 3=±25%, 4=±50%).
    pub tempo_range: u8,
    /// Current waveform zoom level (CDJ-style scrolling view).
    pub waveform_zoom_level: WaveformZoomLevel,
    // Loop state
    /// Loop IN point in seconds.
    pub loop_in: Option<f64>,
    /// Loop OUT point in seconds.
    pub loop_out: Option<f64>,
    /// Whether loop is currently active.
    pub loop_active: bool,
    /// Number of beats in the current loop (supports 1/32 to 512 beats).
    pub loop_beat_count: f64,
    /// Cached GPU texture for waveform rendering.
    waveform_texture: WaveformTexture,
}

impl Default for DeckWidget {
    fn default() -> Self {
        Self {
            track_title: None,
            track_artist: None,
            duration_seconds: 0.0,
            position_seconds: 0.0,
            original_bpm: 0.0,
            adjusted_bpm: 0.0,
            pitch: 0.0,
            is_playing: false,
            waiting_for_quantized_start: false,
            is_master: false,
            sync_enabled: false,
            hot_cues: [None; 4],
            cue_point: None,
            beat_phase: 0.0,
            waveform: Arc::new(Vec::new()),
            waveform_colors: None,
            beat_positions: Vec::new(),
            first_beat_offset: 0.0,
            cue_preview_active: false,
            cue_press_handled: false,
            master_tempo_enabled: false,
            tempo_range: 1,
            waveform_zoom_level: WaveformZoomLevel::default(),
            loop_in: None,
            loop_out: None,
            loop_active: false,
            loop_beat_count: 4.0,
            waveform_texture: WaveformTexture::default(),
        }
    }
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

        // Waveform display with zoom controls
        ui.horizontal(|ui| {
            // Zoom out button
            if ui
                .add_enabled(
                    !matches!(self.waveform_zoom_level, WaveformZoomLevel::Overview),
                    egui::Button::new("-").min_size(Vec2::new(24.0, 20.0)),
                )
                .on_hover_text("Zoom out")
                .clicked()
            {
                self.waveform_zoom_level = self.waveform_zoom_level.zoom_out();
            }

            // Clickable zoom level label
            let label_color = if self.waveform_zoom_level.is_zoomed() {
                Color32::from_rgb(0, 200, 255)
            } else {
                Color32::GRAY
            };
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(self.waveform_zoom_level.label())
                            .size(10.0)
                            .color(label_color),
                    )
                    .min_size(Vec2::new(60.0, 20.0)),
                )
                .on_hover_text("Toggle overview/zoom")
                .clicked()
            {
                if self.waveform_zoom_level.is_zoomed() {
                    self.waveform_zoom_level = WaveformZoomLevel::Overview;
                } else {
                    self.waveform_zoom_level = WaveformZoomLevel::Seconds8;
                }
            }

            // Zoom in button
            if ui
                .add_enabled(
                    !matches!(self.waveform_zoom_level, WaveformZoomLevel::Seconds1),
                    egui::Button::new("+").min_size(Vec2::new(24.0, 20.0)),
                )
                .on_hover_text("Zoom in")
                .clicked()
            {
                self.waveform_zoom_level = self.waveform_zoom_level.zoom_in();
            }
        });

        // Render the appropriate waveform view
        if self.waveform_zoom_level.is_zoomed() {
            self.render_zoomed_waveform(ui, deck_number, console_tx);
        } else {
            self.render_waveform(ui, deck_number, console_tx);
        }

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
            let (play_text, play_color) = if self.waiting_for_quantized_start {
                ("SYNC", Color32::from_rgb(255, 200, 0)) // Yellow when waiting for sync
            } else if self.is_playing {
                ("||", Color32::from_rgb(0, 200, 100))
            } else {
                (">", Color32::WHITE)
            };
            if ui
                .add_sized(
                    button_size,
                    egui::Button::new(egui::RichText::new(play_text).size(14.0).color(play_color)),
                )
                .clicked()
            {
                if self.is_playing || self.waiting_for_quantized_start {
                    // Currently playing or waiting, send pause
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
                let _ = console_tx.send(ConsoleCommand::DjToggleSync { deck: deck_number });
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
                let _ = console_tx.send(ConsoleCommand::DjSetMaster { deck: deck_number });
            }

            ui.add_space(12.0);

            // Master Tempo button (key lock) - magenta when active like CDJ-3000
            let mt_color = if self.master_tempo_enabled {
                Color32::from_rgb(255, 0, 200) // Magenta
            } else {
                Color32::GRAY
            };
            if ui
                .add_sized(
                    Vec2::new(60.0, 30.0),
                    egui::Button::new(egui::RichText::new("M.TEMPO").size(11.0).color(mt_color)),
                )
                .on_hover_text("Master Tempo - tempo changes without pitch change")
                .clicked()
            {
                let _ = console_tx.send(ConsoleCommand::DjToggleMasterTempo { deck: deck_number });
            }

            // Tempo range selector
            let range_labels = ["±6%", "±10%", "±16%", "Wide"];
            let current_label = range_labels
                .get(self.tempo_range as usize)
                .unwrap_or(&"±10%");
            egui::ComboBox::from_id_salt(format!("tempo_range_{}", deck_number))
                .width(50.0)
                .selected_text(*current_label)
                .show_ui(ui, |ui| {
                    for (i, label) in range_labels.iter().enumerate() {
                        if ui
                            .selectable_value(&mut self.tempo_range, i as u8, *label)
                            .clicked()
                        {
                            let _ = console_tx.send(ConsoleCommand::DjSetTempoRange {
                                deck: deck_number,
                                range: i as u8,
                            });
                        }
                    }
                });
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

        // Loop controls (CDJ-3000 style)
        ui.horizontal(|ui| {
            ui.label("Loop:");

            let loop_button_size = Vec2::new(35.0, 30.0);

            // 4-beat loop button - green when active with 4 beats
            let loop_4_active = self.loop_active && (self.loop_beat_count - 4.0).abs() < 0.001;
            let loop_4_color = if loop_4_active {
                Color32::from_rgb(0, 200, 100) // Green
            } else {
                Color32::DARK_GRAY
            };
            if ui
                .add_sized(
                    loop_button_size,
                    egui::Button::new(egui::RichText::new("4").size(14.0).color(
                        if loop_4_active {
                            Color32::BLACK
                        } else {
                            Color32::WHITE
                        },
                    ))
                    .fill(loop_4_color),
                )
                .on_hover_text("Set 4-beat loop")
                .clicked()
            {
                let _ = console_tx.send(ConsoleCommand::DjSetLoop {
                    deck: deck_number,
                    beat_count: 4,
                });
            }

            // 8-beat loop button - green when active with 8 beats
            let loop_8_active = self.loop_active && (self.loop_beat_count - 8.0).abs() < 0.001;
            let loop_8_color = if loop_8_active {
                Color32::from_rgb(0, 200, 100) // Green
            } else {
                Color32::DARK_GRAY
            };
            if ui
                .add_sized(
                    loop_button_size,
                    egui::Button::new(egui::RichText::new("8").size(14.0).color(
                        if loop_8_active {
                            Color32::BLACK
                        } else {
                            Color32::WHITE
                        },
                    ))
                    .fill(loop_8_color),
                )
                .on_hover_text("Set 8-beat loop")
                .clicked()
            {
                let _ = console_tx.send(ConsoleCommand::DjSetLoop {
                    deck: deck_number,
                    beat_count: 8,
                });
            }

            ui.add_space(4.0);

            // Reloop/Exit button
            let has_loop = self.loop_in.is_some();
            let (exit_text, exit_color) = if self.loop_active {
                ("EXIT", Color32::from_rgb(255, 140, 0)) // Orange when active
            } else if has_loop {
                ("RELOOP", Color32::from_rgb(0, 150, 255)) // Blue when loop defined but inactive
            } else {
                ("RELOOP", Color32::DARK_GRAY) // Gray when no loop defined
            };

            if ui
                .add_sized(
                    Vec2::new(55.0, 30.0),
                    egui::Button::new(egui::RichText::new(exit_text).size(11.0).color(
                        if self.loop_active || has_loop {
                            Color32::BLACK
                        } else {
                            Color32::GRAY
                        },
                    ))
                    .fill(exit_color),
                )
                .on_hover_text(if self.loop_active {
                    "Exit loop"
                } else {
                    "Re-enable loop"
                })
                .clicked()
            {
                if has_loop {
                    let _ = console_tx.send(ConsoleCommand::DjToggleLoop { deck: deck_number });
                }
            }

            ui.add_space(8.0);

            // Beat jump / Loop halve-double buttons
            let jump_button_size = Vec2::new(35.0, 30.0);

            // Left button: Beat jump back OR halve loop
            let left_text = if self.loop_active { "/2" } else { "<<" };
            let left_tooltip = if self.loop_active {
                "Halve loop"
            } else {
                "Jump back 4 beats"
            };
            if ui
                .add_sized(
                    jump_button_size,
                    egui::Button::new(egui::RichText::new(left_text).size(14.0)),
                )
                .on_hover_text(left_tooltip)
                .clicked()
            {
                if self.loop_active {
                    let _ = console_tx.send(ConsoleCommand::DjHalveLoop { deck: deck_number });
                } else {
                    let _ = console_tx.send(ConsoleCommand::DjSeekBeats {
                        deck: deck_number,
                        beats: -4,
                    });
                }
            }

            // Right button: Beat jump forward OR double loop
            let right_text = if self.loop_active { "x2" } else { ">>" };
            let right_tooltip = if self.loop_active {
                "Double loop"
            } else {
                "Jump forward 4 beats"
            };
            if ui
                .add_sized(
                    jump_button_size,
                    egui::Button::new(egui::RichText::new(right_text).size(14.0)),
                )
                .on_hover_text(right_tooltip)
                .clicked()
            {
                if self.loop_active {
                    let _ = console_tx.send(ConsoleCommand::DjDoubleLoop { deck: deck_number });
                } else {
                    let _ = console_tx.send(ConsoleCommand::DjSeekBeats {
                        deck: deck_number,
                        beats: 4,
                    });
                }
            }
        });

        ui.add_space(8.0);

        // Beat Grid Editor
        egui::CollapsingHeader::new("Beat Grid")
            .id_salt(format!("beat_grid_{}", deck_label))
            .show(ui, |ui| {
                // First row: Set Downbeat and Beat Shift
                ui.horizontal(|ui| {
                    if ui.button("Set Downbeat").clicked() {
                        let _ =
                            console_tx.send(ConsoleCommand::DjSetDownbeat { deck: deck_number });
                    }
                    ui.separator();
                    if ui.button("◀ Beat").clicked() {
                        let _ = console_tx.send(ConsoleCommand::DjShiftBeatGrid {
                            deck: deck_number,
                            beats: -1,
                        });
                    }
                    if ui.button("Beat ▶").clicked() {
                        let _ = console_tx.send(ConsoleCommand::DjShiftBeatGrid {
                            deck: deck_number,
                            beats: 1,
                        });
                    }
                });

                // Second row: Fine nudge controls
                ui.horizontal(|ui| {
                    ui.label(format!("Offset: {:.1}ms", self.first_beat_offset * 1000.0));
                    ui.separator();

                    let nudge_size = Vec2::new(45.0, 24.0);
                    if ui.add_sized(nudge_size, egui::Button::new("-10")).clicked() {
                        let _ = console_tx.send(ConsoleCommand::DjNudgeBeatGrid {
                            deck: deck_number,
                            offset_ms: -10.0,
                        });
                    }
                    if ui.add_sized(nudge_size, egui::Button::new("-1")).clicked() {
                        let _ = console_tx.send(ConsoleCommand::DjNudgeBeatGrid {
                            deck: deck_number,
                            offset_ms: -1.0,
                        });
                    }
                    if ui.add_sized(nudge_size, egui::Button::new("+1")).clicked() {
                        let _ = console_tx.send(ConsoleCommand::DjNudgeBeatGrid {
                            deck: deck_number,
                            offset_ms: 1.0,
                        });
                    }
                    if ui.add_sized(nudge_size, egui::Button::new("+10")).clicked() {
                        let _ = console_tx.send(ConsoleCommand::DjNudgeBeatGrid {
                            deck: deck_number,
                            offset_ms: 10.0,
                        });
                    }
                });
            });

        ui.add_space(8.0);

        // Pitch fader
        ui.horizontal(|ui| {
            ui.label("Pitch:");
            let pitch_percent = self.pitch * 100.0;
            let slider_response = ui.add(
                egui::Slider::new(&mut self.pitch, -0.5..=0.5)
                    .show_value(false)
                    .trailing_fill(true),
            );
            // Double-click to reset pitch to 0%
            if slider_response.double_clicked() {
                self.pitch = 0.0;
                let _ = console_tx.send(ConsoleCommand::DjSetPitch {
                    deck: deck_number,
                    percent: 0.0,
                });
            } else if slider_response.changed() {
                // Send pitch change command when slider is dragged
                let _ = console_tx.send(ConsoleCommand::DjSetPitch {
                    deck: deck_number,
                    percent: self.pitch, // Decimal value: -0.5 = -50%
                });
            }
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
    fn render_waveform(
        &mut self,
        ui: &mut egui::Ui,
        deck_number: u8,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        let available_width = ui.available_width();
        let height = 60.0;
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(available_width, height), egui::Sense::click());

        // Handle needle drop (click to seek)
        if response.clicked() {
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                let x_offset = pointer_pos.x - rect.left();
                let progress = (x_offset / available_width).clamp(0.0, 1.0);
                let position_seconds = progress as f64 * self.duration_seconds;
                let _ = console_tx.send(ConsoleCommand::DjSeek {
                    deck: deck_number,
                    position_seconds,
                });
            }
        }

        // Handle scroll wheel zoom (scroll up to zoom in from overview)
        if response.hovered() {
            let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
            if scroll_delta > 0.0 {
                self.waveform_zoom_level = self.waveform_zoom_level.zoom_in();
            }
        }

        let painter = ui.painter_at(rect);

        // Background
        painter.rect_filled(rect, Rounding::same(4), Color32::from_gray(15));

        // GPU texture-based waveform rendering (update once, draw instantly)
        if !self.waveform.is_empty() {
            // Update texture only when waveform data changes
            // Use high resolution (up to 8000px) for crisp display in both overview and zoomed
            // views
            if self
                .waveform_texture
                .needs_update(&self.waveform, &self.waveform_colors)
            {
                let texture_width = self.waveform.len().min(8000);
                self.waveform_texture.update(
                    ui.ctx(),
                    &self.waveform,
                    &self.waveform_colors,
                    texture_width,
                );
            }

            // Draw the pre-rendered texture (O(1) CPU work)
            self.waveform_texture.draw_overview(ui, rect);
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

        // Draw beat grid markers (batched for performance)
        if self.duration_seconds > 0.0 && !self.beat_positions.is_empty() {
            let beat_interval = if self.adjusted_bpm > 0.0 {
                60.0 / self.adjusted_bpm
            } else {
                0.5 // Default if BPM unknown
            };

            let mut beat_shapes: Vec<egui::Shape> = Vec::with_capacity(self.beat_positions.len());

            for (idx, beat_pos) in self.beat_positions.iter().enumerate() {
                if *beat_pos >= 0.0 && *beat_pos <= self.duration_seconds {
                    let x = rect.left()
                        + ((*beat_pos / self.duration_seconds) as f32 * available_width);

                    // Check if downbeat (every 4 beats) for stronger visual
                    let beats_from_first = if beat_interval > 0.0 {
                        ((beat_pos - self.first_beat_offset) / beat_interval).round() as usize
                    } else {
                        idx
                    };
                    let is_downbeat = beats_from_first % 4 == 0;

                    let color = if is_downbeat {
                        // Brighter for downbeats
                        Color32::from_rgba_unmultiplied(255, 255, 255, 100)
                    } else {
                        // Subtle for regular beats
                        Color32::from_rgba_unmultiplied(255, 255, 255, 40)
                    };

                    beat_shapes.push(egui::Shape::line_segment(
                        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                        Stroke::new(1.0, color),
                    ));
                }
            }

            painter.extend(beat_shapes);
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

        // Shadow playhead (hover preview)
        if response.hovered() {
            if let Some(hover_pos) = response.hover_pos() {
                let hover_x = hover_pos.x.clamp(rect.left(), rect.right());
                painter.line_segment(
                    [
                        egui::pos2(hover_x, rect.top()),
                        egui::pos2(hover_x, rect.bottom()),
                    ],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 80)),
                );
            }
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

        // Loop region overlay
        if let (Some(loop_in), Some(loop_out)) = (self.loop_in, self.loop_out) {
            if self.duration_seconds > 0.0 {
                let start_x =
                    rect.left() + ((loop_in / self.duration_seconds) as f32 * available_width);
                let end_x =
                    rect.left() + ((loop_out / self.duration_seconds) as f32 * available_width);

                // Semi-transparent fill
                let fill_color = if self.loop_active {
                    Color32::from_rgba_unmultiplied(0, 200, 100, 40) // Green tint when active
                } else {
                    Color32::from_rgba_unmultiplied(100, 100, 100, 30) // Gray tint when inactive
                };
                let loop_rect = Rect::from_x_y_ranges(start_x..=end_x, rect.top()..=rect.bottom());
                painter.rect_filled(loop_rect, 0.0, fill_color);

                // IN/OUT boundary lines
                let line_color = if self.loop_active {
                    Color32::from_rgb(0, 255, 128) // Green
                } else {
                    Color32::from_rgb(100, 150, 255) // Blue
                };
                painter.line_segment(
                    [
                        egui::pos2(start_x, rect.top()),
                        egui::pos2(start_x, rect.bottom()),
                    ],
                    Stroke::new(2.0, line_color),
                );
                painter.line_segment(
                    [
                        egui::pos2(end_x, rect.top()),
                        egui::pos2(end_x, rect.bottom()),
                    ],
                    Stroke::new(2.0, line_color),
                );
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

    /// Render the zoomed waveform display (CDJ-style scrolling view).
    ///
    /// Shows approximately 8 seconds of audio with the playhead fixed at 1/3 from left.
    /// The waveform scrolls as the track plays, giving a "driving" feel like a CDJ-3000.
    fn render_zoomed_waveform(
        &mut self,
        ui: &mut egui::Ui,
        deck_number: u8,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        let available_width = ui.available_width();
        let height = 80.0; // Taller for zoomed view
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(available_width, height), egui::Sense::click());

        let painter = ui.painter_at(rect);

        // Background
        painter.rect_filled(rect, Rounding::same(4), Color32::from_gray(10));

        // Zoomed view parameters
        let zoom_window_seconds = self.waveform_zoom_level.visible_duration().unwrap_or(8.0);
        let playhead_position = 0.33; // Playhead at 1/3 from left (like CDJ-3000)

        // Calculate the time window to display
        let window_start = self.position_seconds - (zoom_window_seconds * playhead_position);
        let window_end = window_start + zoom_window_seconds;

        // Handle click to seek within visible window
        if response.clicked() {
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                let x_offset = pointer_pos.x - rect.left();
                let click_progress = x_offset / available_width;
                let click_time = window_start + (click_progress as f64 * zoom_window_seconds);
                let position_seconds = click_time.clamp(0.0, self.duration_seconds);
                let _ = console_tx.send(ConsoleCommand::DjSeek {
                    deck: deck_number,
                    position_seconds,
                });
            }
        }

        // Handle scroll wheel zoom
        if response.hovered() {
            let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
            if scroll_delta > 0.0 {
                self.waveform_zoom_level = self.waveform_zoom_level.zoom_in();
            } else if scroll_delta < 0.0 {
                self.waveform_zoom_level = self.waveform_zoom_level.zoom_out();
            }
        }

        // GPU texture-based waveform rendering (update once, scroll via UV coords)
        if !self.waveform.is_empty() && self.duration_seconds > 0.0 {
            // Update texture only when waveform data changes
            // Use same high resolution as overview (8000px) so texture is shared between views
            if self
                .waveform_texture
                .needs_update(&self.waveform, &self.waveform_colors)
            {
                let texture_width = self.waveform.len().min(8000);
                self.waveform_texture.update(
                    ui.ctx(),
                    &self.waveform,
                    &self.waveform_colors,
                    texture_width,
                );
            }

            // Draw zoomed portion of texture using UV offset (O(1) CPU work)
            self.waveform_texture.draw_zoomed(
                ui,
                rect,
                self.position_seconds,
                self.duration_seconds,
                zoom_window_seconds,
                playhead_position as f32,
            );
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

        // Draw beat grid markers (batched, only those in visible window)
        if self.duration_seconds > 0.0 && !self.beat_positions.is_empty() {
            let beat_interval = if self.adjusted_bpm > 0.0 {
                60.0 / self.adjusted_bpm
            } else {
                0.5
            };

            // Estimate visible beats for capacity (roughly 2 beats/sec at 120bpm)
            let estimated_visible_beats =
                (zoom_window_seconds * self.adjusted_bpm / 60.0).ceil() as usize + 2;
            let mut beat_shapes: Vec<egui::Shape> = Vec::with_capacity(estimated_visible_beats);

            for (idx, beat_pos) in self.beat_positions.iter().enumerate() {
                // Only draw beats within visible window
                if *beat_pos >= window_start && *beat_pos <= window_end {
                    let x_progress = (beat_pos - window_start) / zoom_window_seconds;
                    let x = rect.left() + (x_progress as f32 * available_width);

                    // Check if downbeat (every 4 beats)
                    let beats_from_first = if beat_interval > 0.0 {
                        ((beat_pos - self.first_beat_offset) / beat_interval).round() as usize
                    } else {
                        idx
                    };
                    let is_downbeat = beats_from_first % 4 == 0;

                    let color = if is_downbeat {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 120)
                    } else {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 50)
                    };

                    beat_shapes.push(egui::Shape::line_segment(
                        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                        Stroke::new(if is_downbeat { 2.0 } else { 1.0 }, color),
                    ));
                }
            }

            painter.extend(beat_shapes);
        }

        // Loop region overlay (only if visible in window)
        if let (Some(loop_in), Some(loop_out)) = (self.loop_in, self.loop_out) {
            // Check if loop region overlaps with visible window
            if loop_out >= window_start && loop_in <= window_end {
                // Clamp loop bounds to visible window
                let visible_start = loop_in.max(window_start);
                let visible_end = loop_out.min(window_end);

                let start_x_progress = (visible_start - window_start) / zoom_window_seconds;
                let end_x_progress = (visible_end - window_start) / zoom_window_seconds;

                let start_x = rect.left() + (start_x_progress as f32 * available_width);
                let end_x = rect.left() + (end_x_progress as f32 * available_width);

                // Semi-transparent fill
                let fill_color = if self.loop_active {
                    Color32::from_rgba_unmultiplied(0, 200, 100, 50) // Green tint when active
                } else {
                    Color32::from_rgba_unmultiplied(100, 100, 100, 35) // Gray tint when inactive
                };
                let loop_rect = Rect::from_x_y_ranges(start_x..=end_x, rect.top()..=rect.bottom());
                painter.rect_filled(loop_rect, 0.0, fill_color);

                // Draw IN boundary line if visible
                let line_color = if self.loop_active {
                    Color32::from_rgb(0, 255, 128) // Green
                } else {
                    Color32::from_rgb(100, 150, 255) // Blue
                };

                if loop_in >= window_start && loop_in <= window_end {
                    let in_x_progress = (loop_in - window_start) / zoom_window_seconds;
                    let in_x = rect.left() + (in_x_progress as f32 * available_width);
                    painter.line_segment(
                        [
                            egui::pos2(in_x, rect.top()),
                            egui::pos2(in_x, rect.bottom()),
                        ],
                        Stroke::new(2.0, line_color),
                    );
                    // "IN" label
                    painter.text(
                        egui::pos2(in_x + 3.0, rect.top() + 10.0),
                        egui::Align2::LEFT_CENTER,
                        "IN",
                        egui::FontId::proportional(9.0),
                        line_color,
                    );
                }

                // Draw OUT boundary line if visible
                if loop_out >= window_start && loop_out <= window_end {
                    let out_x_progress = (loop_out - window_start) / zoom_window_seconds;
                    let out_x = rect.left() + (out_x_progress as f32 * available_width);
                    painter.line_segment(
                        [
                            egui::pos2(out_x, rect.top()),
                            egui::pos2(out_x, rect.bottom()),
                        ],
                        Stroke::new(2.0, line_color),
                    );
                    // "OUT" label
                    painter.text(
                        egui::pos2(out_x - 3.0, rect.top() + 10.0),
                        egui::Align2::RIGHT_CENTER,
                        "OUT",
                        egui::FontId::proportional(9.0),
                        line_color,
                    );
                }
            }
        }

        // Fixed playhead position (the track scrolls, playhead stays fixed)
        let playhead_x = rect.left() + (playhead_position as f32 * available_width);

        // Draw playhead glow
        painter.line_segment(
            [
                egui::pos2(playhead_x, rect.top()),
                egui::pos2(playhead_x, rect.bottom()),
            ],
            Stroke::new(4.0, Color32::from_rgba_unmultiplied(255, 255, 255, 40)),
        );
        painter.line_segment(
            [
                egui::pos2(playhead_x, rect.top()),
                egui::pos2(playhead_x, rect.bottom()),
            ],
            Stroke::new(2.0, Color32::WHITE),
        );

        // Draw cue point marker if in visible window
        if let Some(cue_pos) = self.cue_point {
            if cue_pos >= window_start && cue_pos <= window_end {
                let x_progress = (cue_pos - window_start) / zoom_window_seconds;
                let cue_x = rect.left() + (x_progress as f32 * available_width);
                painter.line_segment(
                    [
                        egui::pos2(cue_x, rect.top()),
                        egui::pos2(cue_x, rect.bottom()),
                    ],
                    Stroke::new(2.0, Color32::from_rgb(255, 200, 0)),
                );
            }
        }

        // Draw hot cue markers if in visible window
        for (i, hot_cue) in self.hot_cues.iter().enumerate() {
            if let Some(pos) = hot_cue {
                if *pos >= window_start && *pos <= window_end {
                    let x_progress = (pos - window_start) / zoom_window_seconds;
                    let x = rect.left() + (x_progress as f32 * available_width);
                    let marker_rect = Rect::from_center_size(
                        egui::pos2(x, rect.top() + 8.0),
                        Vec2::new(12.0, 14.0),
                    );
                    painter.rect_filled(marker_rect, Rounding::same(2), hot_cue_color(i));
                    // Draw hot cue number
                    painter.text(
                        marker_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("{}", i + 1),
                        egui::FontId::proportional(9.0),
                        Color32::WHITE,
                    );
                }
            }
        }

        // Draw time markers at the edges
        let start_time = window_start.max(0.0);
        let end_time = window_end.min(self.duration_seconds);

        painter.text(
            egui::pos2(rect.left() + 4.0, rect.bottom() - 12.0),
            egui::Align2::LEFT_CENTER,
            format_time(start_time),
            egui::FontId::monospace(10.0),
            Color32::from_rgba_unmultiplied(255, 255, 255, 150),
        );

        painter.text(
            egui::pos2(rect.right() - 4.0, rect.bottom() - 12.0),
            egui::Align2::RIGHT_CENTER,
            format_time(end_time),
            egui::FontId::monospace(10.0),
            Color32::from_rgba_unmultiplied(255, 255, 255, 150),
        );

        // Shadow playhead (hover preview) - shows where you'll seek on click
        if response.hovered() {
            if let Some(hover_pos) = response.hover_pos() {
                let hover_x = hover_pos.x.clamp(rect.left(), rect.right());
                painter.line_segment(
                    [
                        egui::pos2(hover_x, rect.top()),
                        egui::pos2(hover_x, rect.bottom()),
                    ],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 80)),
                );

                // Show time at hover position
                let hover_progress = (hover_x - rect.left()) / available_width;
                let hover_time = window_start + (hover_progress as f64 * zoom_window_seconds);
                if hover_time >= 0.0 && hover_time <= self.duration_seconds {
                    painter.text(
                        egui::pos2(hover_x, rect.top() + 10.0),
                        egui::Align2::CENTER_CENTER,
                        format_time(hover_time),
                        egui::FontId::monospace(9.0),
                        Color32::from_rgba_unmultiplied(255, 255, 255, 200),
                    );
                }
            }
        }
    }
}

/// Format seconds as MM:SS.ss (handles negative values for countdown display).
fn format_time(seconds: f64) -> String {
    if seconds < 0.0 {
        let abs_seconds = seconds.abs();
        let mins = (abs_seconds / 60.0).floor() as u32;
        let secs = abs_seconds % 60.0;
        format!("-{:02}:{:05.2}", mins, secs)
    } else {
        let mins = (seconds / 60.0).floor() as u32;
        let secs = seconds % 60.0;
        format!("{:02}:{:05.2}", mins, secs)
    }
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

/// Check if playhead position is approximately at the cue point.
fn is_at_cue_point(position: f64, cue_point: Option<f64>) -> bool {
    match cue_point {
        Some(cue) => (position - cue).abs() < 0.1, // 100ms tolerance
        None => false,
    }
}
