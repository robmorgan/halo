use eframe::egui::{Align, Color32, CornerRadius, Direction, Layout, RichText};
use halo_core::ConsoleCommand;
use tokio::sync::mpsc;

use crate::utils::theme::Theme;

pub fn render(
    ui: &mut eframe::egui::Ui,
    _console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    state: &crate::state::ConsoleState,
) {
    let theme = Theme::default();
    let fixture_count = state.fixtures.len();
    let bpm = state.bpm;
    let rhythm_state = &state.rhythm_state;
    let active_effects_count = state.active_effects_count;

    ui.painter().rect_filled(
        ui.available_rect_before_wrap(),
        CornerRadius::same(0),
        theme.bg_color,
    );

    ui.horizontal(|ui| {
        ui.add_space(12.0);
        // Show status message if available, otherwise empty
        if let Some(ref message) = state.status_message {
            let status_text = if let Some((current, total)) = state.status_progress {
                let percentage = if total > 0 {
                    (current as f32 / total as f32 * 100.0) as u32
                } else {
                    0
                };
                format!("{} ({}/{} - {}%)", message, current, total, percentage)
            } else {
                message.clone()
            };
            ui.label(
                RichText::new(status_text)
                    .size(12.0)
                    .color(Color32::from_rgb(100, 180, 255)), // Light blue for status
            );
        }

        ui.with_layout(
            Layout::centered_and_justified(Direction::LeftToRight),
            |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} Fixtures | {} Active Effects | {:.1} BPM | Beat {:.2} | Phase {:.2}",
                        fixture_count,
                        active_effects_count,
                        bpm,
                        rhythm_state.beat_phase,
                        rhythm_state.bar_phase
                    ))
                    .size(12.0)
                    .color(theme.text_dim),
                );
            },
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("Halo v0.5").size(12.0).color(theme.text_dim));
        });
    });
}
