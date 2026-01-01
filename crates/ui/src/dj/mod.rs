//! DJ panel UI components.
//!
//! Provides a dual-deck DJ interface with:
//! - Deck displays (waveform, transport, BPM)
//! - Track browser
//! - Hot cue buttons
//! - Pitch/tempo controls

mod deck;
mod library;

pub use deck::DeckWidget;
use eframe::egui;
use halo_core::ConsoleCommand;
pub use library::{LibraryBrowser, TrackDragPayload};
use tokio::sync::mpsc;

use crate::state::ConsoleState;

/// State for the DJ panel.
pub struct DjPanel {
    /// Deck A widget state.
    deck_a: DeckWidget,
    /// Deck B widget state.
    deck_b: DeckWidget,
    /// Library browser state.
    library: LibraryBrowser,
    /// Whether the library panel is expanded.
    library_expanded: bool,
    /// Whether we've requested the library.
    library_requested: bool,
    /// Last known track count to detect changes.
    last_track_count: usize,
}

impl Default for DjPanel {
    fn default() -> Self {
        Self {
            deck_a: DeckWidget::default(),
            deck_b: DeckWidget::default(),
            library: LibraryBrowser::default(),
            library_expanded: false,
            library_requested: false,
            last_track_count: 0,
        }
    }
}

impl DjPanel {
    /// Render the DJ panel.
    pub fn render(
        &mut self,
        ctx: &egui::Context,
        state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        // Request library on first render or after import
        let track_count_changed = state.dj_tracks.len() != self.last_track_count;
        if !self.library_requested || track_count_changed {
            let _ = console_tx.send(ConsoleCommand::DjQueryLibrary);
            self.library_requested = true;
        }

        // Update library browser with tracks from state when tracks change
        if track_count_changed && !state.dj_tracks.is_empty() {
            let tracks: Vec<library::TrackEntry> = state
                .dj_tracks
                .iter()
                .map(|t| library::TrackEntry {
                    id: t.id,
                    title: t.title.clone(),
                    artist: t.artist.clone(),
                    duration_seconds: t.duration_seconds,
                    bpm: t.bpm,
                    file_path: t.file_path.clone(),
                })
                .collect();
            self.library.set_tracks(tracks);
            self.last_track_count = state.dj_tracks.len();
        } else {
            // Sync BPM values for tracks that have been analyzed
            // (when track count is the same but BPM values may have changed)
            self.library.update_track_bpms(&state.dj_tracks);
        }

        // Sync deck state from console state
        self.deck_a.track_title = state.dj_deck_a.track_title.clone();
        self.deck_a.track_artist = state.dj_deck_a.track_artist.clone();
        self.deck_a.duration_seconds = state.dj_deck_a.duration_seconds;
        self.deck_a.position_seconds = state.dj_deck_a.position_seconds;
        self.deck_a.adjusted_bpm = state.dj_deck_a.bpm.unwrap_or(120.0);
        self.deck_a.is_playing = state.dj_deck_a.is_playing;
        self.deck_a.cue_point = state.dj_deck_a.cue_point;
        if self.deck_a.waveform.len() != state.dj_deck_a.waveform.len() {
            self.deck_a.waveform = state.dj_deck_a.waveform.clone();
            self.deck_a.waveform_colors = state.dj_deck_a.waveform_colors.clone();
        }
        if self.deck_a.beat_positions.len() != state.dj_deck_a.beat_positions.len() {
            self.deck_a.beat_positions = state.dj_deck_a.beat_positions.clone();
            self.deck_a.first_beat_offset = state.dj_deck_a.first_beat_offset;
        }

        self.deck_b.track_title = state.dj_deck_b.track_title.clone();
        self.deck_b.track_artist = state.dj_deck_b.track_artist.clone();
        self.deck_b.duration_seconds = state.dj_deck_b.duration_seconds;
        self.deck_b.position_seconds = state.dj_deck_b.position_seconds;
        self.deck_b.adjusted_bpm = state.dj_deck_b.bpm.unwrap_or(120.0);
        self.deck_b.is_playing = state.dj_deck_b.is_playing;
        self.deck_b.cue_point = state.dj_deck_b.cue_point;
        if self.deck_b.waveform.len() != state.dj_deck_b.waveform.len() {
            self.deck_b.waveform = state.dj_deck_b.waveform.clone();
            self.deck_b.waveform_colors = state.dj_deck_b.waveform_colors.clone();
        }
        if self.deck_b.beat_positions.len() != state.dj_deck_b.beat_positions.len() {
            self.deck_b.beat_positions = state.dj_deck_b.beat_positions.clone();
            self.deck_b.first_beat_offset = state.dj_deck_b.first_beat_offset;
        }

        // Sync Master Tempo state
        self.deck_a.master_tempo_enabled = state.dj_deck_a.master_tempo_enabled;
        self.deck_a.tempo_range = state.dj_deck_a.tempo_range;
        self.deck_b.master_tempo_enabled = state.dj_deck_b.master_tempo_enabled;
        self.deck_b.tempo_range = state.dj_deck_b.tempo_range;

        // Sync Loop state
        self.deck_a.loop_in = state.dj_deck_a.loop_in;
        self.deck_a.loop_out = state.dj_deck_a.loop_out;
        self.deck_a.loop_active = state.dj_deck_a.loop_active;
        self.deck_a.loop_beat_count = state.dj_deck_a.loop_beat_count;
        self.deck_b.loop_in = state.dj_deck_b.loop_in;
        self.deck_b.loop_out = state.dj_deck_b.loop_out;
        self.deck_b.loop_active = state.dj_deck_b.loop_active;
        self.deck_b.loop_beat_count = state.dj_deck_b.loop_beat_count;

        // Left side panel for library browser
        egui::SidePanel::left("dj_library_panel")
            .resizable(true)
            .default_width(300.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Library");
                ui.separator();
                self.library.render(ui, console_tx);
            });

        // Main content area with two decks
        egui::CentralPanel::default().show(ctx, |ui| {
            // Top area: Both decks side by side
            let available_width = ui.available_width();
            let deck_width = (available_width - 20.0) / 2.0;

            ui.horizontal(|ui| {
                // Deck A
                ui.vertical(|ui| {
                    ui.set_width(deck_width);
                    self.deck_a.render(ui, "A", 0, console_tx);
                });

                ui.add_space(20.0);

                // Deck B
                ui.vertical(|ui| {
                    ui.set_width(deck_width);
                    self.deck_b.render(ui, "B", 1, console_tx);
                });
            });
        });

        // Request continuous repaints while cue button is held on either deck
        // This ensures hold detection works properly in egui's repaint model
        if self.deck_a.is_cue_held() || self.deck_b.is_cue_held() {
            ctx.request_repaint();
        }
    }
}
