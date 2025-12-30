//! Library browser for DJ track selection.

use eframe::egui::{self, Color32, RichText, Rounding, Vec2};

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
    pub fn render(&mut self, ui: &mut egui::Ui) {
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
        egui::ScrollArea::vertical()
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
                            RichText::new("Import tracks using File > Import Folder")
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

                        let frame = egui::Frame::default()
                            .fill(if is_selected {
                                Color32::from_rgb(60, 80, 120)
                            } else if idx % 2 == 0 {
                                Color32::from_gray(30)
                            } else {
                                Color32::from_gray(25)
                            })
                            .corner_radius(Rounding::same(2))
                            .inner_margin(egui::Margin::symmetric(8, 4));

                        let frame_response = frame.show(ui, |ui| {
                            ui.set_min_width(ui.available_width());

                            ui.horizontal(|ui| {
                                // Title and artist
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(&track.title)
                                            .size(13.0)
                                            .color(Color32::WHITE),
                                    );
                                    if let Some(artist) = &track.artist {
                                        ui.label(
                                            RichText::new(artist).size(11.0).color(Color32::GRAY),
                                        );
                                    }
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        // Duration
                                        ui.label(
                                            RichText::new(format_duration(track.duration_seconds))
                                                .size(12.0)
                                                .monospace()
                                                .color(Color32::GRAY),
                                        );

                                        ui.add_space(20.0);

                                        // BPM
                                        if let Some(bpm) = track.bpm {
                                            ui.label(
                                                RichText::new(format!("{:.0}", bpm))
                                                    .size(12.0)
                                                    .monospace()
                                                    .color(Color32::from_rgb(0, 200, 100)),
                                            );
                                        } else {
                                            ui.label(
                                                RichText::new("---")
                                                    .size(12.0)
                                                    .monospace()
                                                    .color(Color32::DARK_GRAY),
                                            );
                                        }
                                    },
                                );
                            });
                        });

                        // Handle click on frame
                        if frame_response
                            .response
                            .interact(egui::Sense::click())
                            .clicked()
                        {
                            new_selected = Some(idx);
                        }

                        // Handle double-click to load
                        if frame_response
                            .response
                            .interact(egui::Sense::click())
                            .double_clicked()
                        {
                            // TODO: Send load command to deck
                        }

                        ui.add_space(2.0);
                    }

                    // Update selection after loop
                    self.selected_index = new_selected;
                }
            });

        ui.add_space(8.0);

        // Bottom controls
        ui.horizontal(|ui| {
            if ui
                .add_sized(
                    Vec2::new(80.0, 24.0),
                    egui::Button::new(RichText::new("Load A").size(11.0)),
                )
                .clicked()
            {
                // TODO: Load selected track to Deck A
            }

            if ui
                .add_sized(
                    Vec2::new(80.0, 24.0),
                    egui::Button::new(RichText::new("Load B").size(11.0)),
                )
                .clicked()
            {
                // TODO: Load selected track to Deck B
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
