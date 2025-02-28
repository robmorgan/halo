use eframe::egui::{Align, CornerRadius, Direction, Layout, RichText};

use halo_fixtures::Fixture;

use crate::utils::Theme;

pub fn render(ui: &mut eframe::egui::Ui, fps: u32, fixtures: Vec<Fixture>) {
    let theme = Theme::default();

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
                    RichText::new(format!("{} Fixtures | 42 Parameters", fixtures.len()))
                        .size(12.0)
                        .color(theme.text_dim),
                );
            },
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("Halo v1.0").size(12.0).color(theme.text_dim));
        });
    });
}
