use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};

/// Helper method for vertical faders with generic values
pub fn render_vertical_fader(
    ui: &mut egui::Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    height: f32,
) -> bool {
    let mut changed = false;

    // Display value
    let display_value = if max <= 2.0 {
        format!("{:.2}", *value)
    } else if max <= 100.0 {
        format!("{:.1}%", *value)
    } else if min == 0.0 && max == 360.0 {
        format!("{}°", value.round())
    } else {
        format!("{}", value.round())
    };

    ui.label(display_value);

    // Create a custom vertical slider
    let slider_width = 36.0;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(slider_width, height), Sense::click_and_drag());

    if response.dragged() {
        let mouse_pos = response
            .interact_pointer_pos()
            .unwrap_or(Pos2::new(0.0, 0.0));
        let normalized = 1.0 - ((mouse_pos.y - rect.min.y) / height).clamp(0.0, 1.0);
        *value = min + normalized * (max - min);
        changed = true;
    }

    // Draw the slider background
    ui.painter()
        .rect_filled(rect, 4.0, Color32::from_rgb(30, 30, 30));

    // Draw the fill
    let fill_height = ((*value - min) / (max - min) * height).clamp(0.0, height);
    let fill_rect = Rect::from_min_size(
        Pos2::new(rect.min.x, rect.max.y - fill_height),
        Vec2::new(slider_width, fill_height),
    );

    // Choose appropriate slider color
    let fill_color = Color32::from_rgb(0, 150, 255);

    ui.painter().rect_filled(fill_rect, 4.0, fill_color);

    // Draw tick marks
    for i in 0..=4 {
        let y = rect.min.y + i as f32 * (height / 4.0);
        ui.painter().line_segment(
            [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
            Stroke::new(1.0, Color32::from_rgb(70, 70, 70)),
        );
    }

    changed
}
