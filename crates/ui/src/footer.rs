use crate::utils::theme::Theme;
use eframe::egui::{Align, CornerRadius, Direction, Layout, RichText};
use halo_core::ConsoleCommand;
use tokio::sync::mpsc;

pub fn render(
    ui: &mut eframe::egui::Ui,
    _console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    fps: u32,
) {
    let theme = Theme::default();
    // TODO: Get state from the UI state instead of console
    let fixture_count = 0; // Placeholder
    let bpm = 120.0; // Placeholder
    let rhythm_state = halo_core::RhythmState {
        beat_phase: 0.0,
        bar_phase: 0.0,
        phrase_phase: 0.0,
        beats_per_bar: 4,
        bars_per_phrase: 4,
        last_tap_time: None,
        tap_count: 0,
    };

    ui.painter().rect_filled(
        ui.available_rect_before_wrap(),
        CornerRadius::same(0),
        theme.bg_color,
    );

    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(
            RichText::new(format!("FPS: {}", fps))
                .size(12.0)
                .color(theme.text_dim),
        );

        ui.with_layout(
            Layout::centered_and_justified(Direction::LeftToRight),
            |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} Fixtures | 42 Parameters | {:.1} BPM | Beat {:.2} | Phase {:.2}",
                        fixture_count, bpm, rhythm_state.beat_phase, rhythm_state.bar_phase
                    ))
                    .size(12.0)
                    .color(theme.text_dim),
                );
            },
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("Halo v0.3").size(12.0).color(theme.text_dim));
        });
    });
}
