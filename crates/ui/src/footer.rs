use eframe::egui::{Align, CornerRadius, Direction, Layout, RichText};
use std::sync::{Arc, Mutex};

use halo_core::LightingConsole;
use halo_fixtures::{Fixture, FixtureType};

use crate::utils::Theme;

pub fn render(ui: &mut eframe::egui::Ui, console: &Arc<Mutex<LightingConsole>>, fps: u32) {
    let theme = Theme::default();
    let fixture_count;
    let clock;
    {
        let mut console = console.lock().unwrap();
        fixture_count = console.fixtures.clone().len();
        clock = console.link_state.get_clock_state();
        drop(console);
    }

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
                        fixture_count, clock.tempo, clock.beats, clock.phase
                    ))
                    .size(12.0)
                    .color(theme.text_dim),
                );
            },
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("Halo v0.2").size(12.0).color(theme.text_dim));
        });
    });
}
