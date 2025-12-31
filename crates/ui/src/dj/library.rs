//! Library browser for DJ track selection.

use eframe::egui::{self, Color32, RichText, Rounding, Vec2};
use halo_core::ConsoleCommand;
use log;
use tokio::sync::mpsc;

/// Drag payload for a track being dragged from the library.
#[derive(Clone, Debug)]
pub struct TrackDragPayload {
    /// The track ID being dragged.
    pub track_id: i64,
    /// Track title for display during drag.
    pub title: String,
}

/// A track entry in the library.
#[derive(Clone)]
pub struct TrackEntry {
    /// Track ID from database.
    pub id: i64,
    /// Track title.
    pub title: String,
    /// Track artist.
    pub artist: Option<String>,
    /// Duration in seconds.
    pub duration_seconds: f64,
    /// BPM (if analyzed).
    pub bpm: Option<f64>,
}

/// Library browser state.
#[derive(Default)]
pub struct LibraryBrowser {
    /// Search query.
    search_query: String,
    /// Currently selected track index.
    selected_index: Option<usize>,
    /// List of tracks (populated from database).
    tracks: Vec<TrackEntry>,
    /// Sort column.
    sort_column: SortColumn,
    /// Sort ascending.
    sort_ascending: bool,
}

/// Column to sort by.
#[derive(Default, Clone, Copy, PartialEq)]
enum SortColumn {
    #[default]
    Title,
    Artist,
    Bpm,
    Duration,
}

impl LibraryBrowser {
    /// Render the library browser.
    pub fn render(
        &mut self,
        ui: &mut egui::Ui,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        // Search bar
        ui.horizontal(|ui| {
            ui.label("Search:");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .desired_width(ui.available_width() - 60.0)
                    .hint_text("Search tracks..."),
            );
            if response.changed() {
                // Filter tracks based on search
                self.filter_tracks();
            }
            if ui.button("Clear").clicked() {
                self.search_query.clear();
                self.filter_tracks();
            }
        });

        ui.add_space(8.0);

        // Column headers
        ui.horizontal(|ui| {
            let header_style = RichText::new("").size(12.0).color(Color32::GRAY);

            if ui
                .selectable_label(
                    self.sort_column == SortColumn::Title,
                    RichText::new("Title").size(12.0).color(Color32::GRAY),
                )
                .clicked()
            {
                self.toggle_sort(SortColumn::Title);
            }

            ui.add_space(80.0);

            if ui
                .selectable_label(
                    self.sort_column == SortColumn::Artist,
                    RichText::new("Artist").size(12.0).color(Color32::GRAY),
                )
                .clicked()
            {
                self.toggle_sort(SortColumn::Artist);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .selectable_label(
                        self.sort_column == SortColumn::Duration,
                        RichText::new("Time").size(12.0).color(Color32::GRAY),
                    )
                    .clicked()
                {
                    self.toggle_sort(SortColumn::Duration);
                }

                ui.add_space(20.0);

                if ui
                    .selectable_label(
                        self.sort_column == SortColumn::Bpm,
                        RichText::new("BPM").size(12.0).color(Color32::GRAY),
                    )
                    .clicked()
                {
                    self.toggle_sort(SortColumn::Bpm);
                }
            });
        });

        ui.separator();

        // Track list
        let mut double_clicked_track_id: Option<i64> = None;

        // Reserve space for bottom controls (buttons + spacing)
        let bottom_height = 40.0;
        let available_height = ui.available_height() - bottom_height;

        egui::ScrollArea::vertical()
            .max_height(available_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.tracks.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.label(
                            RichText::new("No tracks in library")
                                .size(14.0)
                                .color(Color32::DARK_GRAY)
                                .italics(),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Import tracks using File > Import Music Folder")
                                .size(12.0)
                                .color(Color32::DARK_GRAY),
                        );
                    });
                } else {
                    // Clone filtered tracks to avoid borrow issues
                    let filtered_tracks: Vec<TrackEntry> = self
                        .get_filtered_tracks()
                        .iter()
                        .map(|t| (*t).clone())
                        .collect();
                    let current_selected = self.selected_index;
                    let mut new_selected = current_selected;

                    for (idx, track) in filtered_tracks.iter().enumerate() {
                        let is_selected = current_selected == Some(idx);
                        let track_id = track.id;
                        let track_title = track.title.clone();

                        let fill_color = if is_selected {
                            Color32::from_rgb(60, 80, 120)
                        } else if idx % 2 == 0 {
                            Color32::from_gray(30)
                        } else {
                            Color32::from_gray(25)
                        };

                        // Create drag payload
                        let payload = TrackDragPayload {
                            track_id,
                            title: track_title.clone(),
                        };

                        // Allocate space for the row first
                        let desired_size = Vec2::new(ui.available_width(), 40.0);
                        let (rect, base_response) =
                            ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());

                        // Paint background
                        if ui.is_rect_visible(rect) {
                            let visuals = if base_response.hovered() {
                                Color32::from_rgb(70, 90, 130)
                            } else {
                                fill_color
                            };
                            ui.painter().rect_filled(rect, Rounding::same(2), visuals);

                            let text_rect = rect.shrink2(Vec2::new(8.0, 4.0));

                            // Title
                            ui.painter().text(
                                text_rect.left_top(),
                                egui::Align2::LEFT_TOP,
                                &track.title,
                                egui::FontId::proportional(13.0),
                                Color32::WHITE,
                            );

                            // Artist
                            if let Some(artist) = &track.artist {
                                ui.painter().text(
                                    text_rect.left_top() + Vec2::new(0.0, 16.0),
                                    egui::Align2::LEFT_TOP,
                                    artist,
                                    egui::FontId::proportional(11.0),
                                    Color32::GRAY,
                                );
                            }

                            // BPM
                            let bpm_text = track
                                .bpm
                                .map(|b| format!("{:.0}", b))
                                .unwrap_or_else(|| "---".to_string());
                            let bpm_color = if track.bpm.is_some() {
                                Color32::from_rgb(0, 200, 100)
                            } else {
                                Color32::DARK_GRAY
                            };
                            ui.painter().text(
                                egui::pos2(text_rect.right() - 60.0, text_rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                &bpm_text,
                                egui::FontId::monospace(12.0),
                                bpm_color,
                            );

                            // Duration
                            ui.painter().text(
                                egui::pos2(text_rect.right(), text_rect.center().y),
                                egui::Align2::RIGHT_CENTER,
                                format_duration(track.duration_seconds),
                                egui::FontId::monospace(12.0),
                                Color32::GRAY,
                            );
                        }

                        // Handle drag - set payload when dragging
                        if base_response.drag_started() {
                            base_response.dnd_set_drag_payload(payload);
                        }

                        // Show drag preview while dragging
                        if base_response.dragged() {
                            // Paint a preview at cursor
                            if let Some(pointer_pos) = ui.ctx().pointer_hover_pos() {
                                let preview_rect = egui::Rect::from_min_size(
                                    pointer_pos + Vec2::new(10.0, 10.0),
                                    Vec2::new(200.0, 30.0),
                                );
                                ui.painter().rect_filled(
                                    preview_rect,
                                    Rounding::same(4),
                                    Color32::from_rgba_unmultiplied(60, 80, 120, 200),
                                );
                                ui.painter().text(
                                    preview_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    &track.title,
                                    egui::FontId::proportional(12.0),
                                    Color32::WHITE,
                                );
                            }
                        }

                        // Select on mouse down (not clicked, which requires no movement)
                        // This makes selection feel more responsive
                        if base_response.is_pointer_button_down_on() {
                            new_selected = Some(idx);
                        }

                        // Handle double-click to load to Deck A
                        if base_response.double_clicked() {
                            double_clicked_track_id = Some(track_id);
                        }

                        ui.add_space(2.0);
                    }

                    // Update selection after loop
                    self.selected_index = new_selected;
                }
            });

        // Handle double-click load (send command after ScrollArea to avoid borrow issues)
        if let Some(track_id) = double_clicked_track_id {
            let _ = console_tx.send(ConsoleCommand::DjLoadTrack { deck: 0, track_id });
        }

        ui.add_space(8.0);

        // Bottom controls
        ui.horizontal(|ui| {
            let selected_track_id = self.selected_track().map(|t| t.id);

            if ui
                .add_sized(
                    Vec2::new(80.0, 24.0),
                    egui::Button::new(RichText::new("Load A").size(11.0)),
                )
                .clicked()
            {
                eprintln!(
                    "DEBUG: Load A button clicked, selected_track_id={:?}",
                    selected_track_id
                );
                if let Some(track_id) = selected_track_id {
                    log::info!(
                        "UI: Load A clicked - sending DjLoadTrack deck=0, track_id={}",
                        track_id
                    );
                    eprintln!(
                        "DEBUG: Sending DjLoadTrack command for track_id={}",
                        track_id
                    );
                    let _ = console_tx.send(ConsoleCommand::DjLoadTrack { deck: 0, track_id });
                } else {
                    log::warn!("UI: Load A clicked but no track selected");
                    eprintln!("DEBUG: No track selected!");
                }
            }

            if ui
                .add_sized(
                    Vec2::new(80.0, 24.0),
                    egui::Button::new(RichText::new("Load B").size(11.0)),
                )
                .clicked()
            {
                if let Some(track_id) = selected_track_id {
                    log::info!(
                        "UI: Load B clicked - sending DjLoadTrack deck=1, track_id={}",
                        track_id
                    );
                    let _ = console_tx.send(ConsoleCommand::DjLoadTrack { deck: 1, track_id });
                } else {
                    log::warn!("UI: Load B clicked but no track selected");
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} tracks", self.tracks.len()))
                        .size(11.0)
                        .color(Color32::GRAY),
                );
            });
        });
    }

    /// Toggle sort on a column.
    fn toggle_sort(&mut self, column: SortColumn) {
        if self.sort_column == column {
            self.sort_ascending = !self.sort_ascending;
        } else {
            self.sort_column = column;
            self.sort_ascending = true;
        }
        self.sort_tracks();
    }

    /// Sort tracks by current column.
    fn sort_tracks(&mut self) {
        match self.sort_column {
            SortColumn::Title => {
                self.tracks.sort_by(|a, b| {
                    let cmp = a.title.to_lowercase().cmp(&b.title.to_lowercase());
                    if self.sort_ascending {
                        cmp
                    } else {
                        cmp.reverse()
                    }
                });
            }
            SortColumn::Artist => {
                self.tracks.sort_by(|a, b| {
                    let a_artist = a.artist.as_deref().unwrap_or("");
                    let b_artist = b.artist.as_deref().unwrap_or("");
                    let cmp = a_artist.to_lowercase().cmp(&b_artist.to_lowercase());
                    if self.sort_ascending {
                        cmp
                    } else {
                        cmp.reverse()
                    }
                });
            }
            SortColumn::Bpm => {
                self.tracks.sort_by(|a, b| {
                    let a_bpm = a.bpm.unwrap_or(0.0);
                    let b_bpm = b.bpm.unwrap_or(0.0);
                    let cmp = a_bpm
                        .partial_cmp(&b_bpm)
                        .unwrap_or(std::cmp::Ordering::Equal);
                    if self.sort_ascending {
                        cmp
                    } else {
                        cmp.reverse()
                    }
                });
            }
            SortColumn::Duration => {
                self.tracks.sort_by(|a, b| {
                    let cmp = a
                        .duration_seconds
                        .partial_cmp(&b.duration_seconds)
                        .unwrap_or(std::cmp::Ordering::Equal);
                    if self.sort_ascending {
                        cmp
                    } else {
                        cmp.reverse()
                    }
                });
            }
        }
    }

    /// Filter tracks based on search query.
    fn filter_tracks(&mut self) {
        // In a real implementation, this would query the database
        // For now, tracks are pre-loaded and we just update selected_index
        self.selected_index = None;
    }

    /// Get tracks filtered by search query.
    fn get_filtered_tracks(&self) -> Vec<&TrackEntry> {
        if self.search_query.is_empty() {
            self.tracks.iter().collect()
        } else {
            let query = self.search_query.to_lowercase();
            self.tracks
                .iter()
                .filter(|t| {
                    t.title.to_lowercase().contains(&query)
                        || t.artist
                            .as_ref()
                            .map(|a| a.to_lowercase().contains(&query))
                            .unwrap_or(false)
                })
                .collect()
        }
    }

    /// Get the currently selected track.
    pub fn selected_track(&self) -> Option<&TrackEntry> {
        self.selected_index.and_then(|idx| {
            let filtered = self.get_filtered_tracks();
            filtered.get(idx).copied()
        })
    }

    /// Set the track list (called when library is updated).
    pub fn set_tracks(&mut self, tracks: Vec<TrackEntry>) {
        self.tracks = tracks;
        self.sort_tracks();
        self.selected_index = None;
    }
}

/// Format duration as MM:SS.
fn format_duration(seconds: f64) -> String {
    let mins = (seconds / 60.0).floor() as u32;
    let secs = (seconds % 60.0).floor() as u32;
    format!("{}:{:02}", mins, secs)
}
