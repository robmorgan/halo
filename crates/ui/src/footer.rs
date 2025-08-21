use crate::utils::theme::Theme;
use eframe::egui::{Align, CornerRadius, Direction, Layout, RichText};
use halo_core::ConsoleCommand;
use tokio::sync::mpsc;

pub fn render(
    ui: &mut eframe::egui::Ui,
    _console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    state: &crate::state::ConsoleState,
    fps: u32,
) {
    let theme = Theme::default();
    let fixture_count = state.fixtures.len();
    let bpm = state.bpm;
    let rhythm_state = &state.rhythm_state;

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
