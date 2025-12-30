//! DJ panel UI components.
//!
//! Provides a dual-deck DJ interface with:
//! - Deck displays (waveform, transport, BPM)
//! - Track browser
//! - Hot cue buttons
//! - Pitch/tempo controls

mod deck;
mod library;

use eframe::egui;
use halo_core::ConsoleCommand;
use tokio::sync::mpsc;

use crate::state::ConsoleState;

pub use deck::DeckWidget;
pub use library::LibraryBrowser;

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
        if !self.library_requested || state.dj_tracks.len() != self.last_track_count {
            let _ = console_tx.send(ConsoleCommand::DjQueryLibrary);
            self.library_requested = true;
            self.last_track_count = state.dj_tracks.len();
        }

        // Update library browser with tracks from state
        if !state.dj_tracks.is_empty() {
            let tracks: Vec<library::TrackEntry> = state
                .dj_tracks
                .iter()
                .map(|t| library::TrackEntry {
                    id: t.id,
                    title: t.title.clone(),
                    artist: t.artist.clone(),
                    duration_seconds: t.duration_seconds,
                    bpm: t.bpm,
                })
                .collect();
            self.library.set_tracks(tracks);
        }

        // Left side panel for library browser
        egui::SidePanel::left("dj_library_panel")
            .resizable(true)
            .default_width(300.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Library");
                ui.separator();
                self.library.render(ui);
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
                    self.deck_a.render(ui, "A");
                });

                ui.add_space(20.0);

                // Deck B
                ui.vertical(|ui| {
                    ui.set_width(deck_width);
                    self.deck_b.render(ui, "B");
                });
            });
        });
    }
}
