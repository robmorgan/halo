//! CDJ-style beat counter: `BAR n.b` readout plus a 4-segment indicator
//! with the current beat-in-bar lit. Doubles as a live check of the
//! detected grid — wrong downbeats are immediately visible here.

use eframe::egui;

use super::GridMarks;
use crate::theme;

/// Segment size and gap in points.
const SEG_SIZE: egui::Vec2 = egui::vec2(16.0, 10.0);
const SEG_GAP: f32 = 4.0;
const SEGMENTS: u8 = 4;
/// Padding inside the segments' well.
const WELL_PAD: f32 = 3.0;
/// The four beat steps in their well.
pub const BEAT_STEPS_SIZE: egui::Vec2 = egui::vec2(
    SEGMENTS as f32 * SEG_SIZE.x + (SEGMENTS - 1) as f32 * SEG_GAP + 2.0 * WELL_PAD,
    SEG_SIZE.y + 2.0 * WELL_PAD,
);

/// The four beat steps in a well at `rect` (`BEAT_STEPS_SIZE`), `beat`
/// (1–4) lit.
pub fn paint_beat_steps(painter: &egui::Painter, rect: egui::Rect, beat: Option<u8>) {
    theme::paint_well(painter, rect, theme::RADIUS_CONTROL);
    for seg in 1..=SEGMENTS {
        let x = rect.left() + WELL_PAD + (seg - 1) as f32 * (SEG_SIZE.x + SEG_GAP);
        let seg_rect = egui::Rect::from_min_size(egui::pos2(x, rect.top() + WELL_PAD), SEG_SIZE);
        let color = if beat == Some(seg) {
            theme::ACCENT
        } else {
            theme::SEGMENT_OFF
        };
        painter.rect_filled(seg_rect, 0.0, color);
    }
}

/// Paint the readout + segments, the BAR caption in `caption`. Shows
/// placeholders before the first beat; paints nothing without a usable
/// grid. `hint` adds the low-confidence note after the steps (the deck
/// header has no room for it and shows it as a tooltip instead).
pub fn paint_beat_counter(
    ui: &mut egui::Ui,
    marks: &GridMarks,
    position_frames: f64,
    caption: theme::TypeStyle,
    hint: bool,
) {
    if !marks.is_usable() {
        return;
    }
    let bar_beat = marks.bar_beat(position_frames);
    let ctx = ui.ctx().clone();

    ui.label(caption.rich("BAR").color(theme::TEXT_MUTED));

    // The value right-aligned in a column as wide as "888.8", so the
    // digits never move as bars tick over.
    let text = match bar_beat {
        Some((bar, beat)) => format!("{bar}.{beat}"),
        None => "-.-".to_string(),
    };
    let cell_w = theme::NUMERALS_SM.width(&ctx, "888.8");
    let galley = theme::NUMERALS_SM.galley(&ctx, &text, theme::TEXT_STRONG);
    let (cell, _) =
        ui.allocate_exact_size(egui::vec2(cell_w, galley.size().y), egui::Sense::hover());
    ui.painter().galley(
        egui::pos2(cell.right() - galley.size().x, cell.top()),
        galley,
        theme::TEXT_STRONG,
    );

    // The steps sit in a well, the current beat lit.
    let (rect, _) = ui.allocate_exact_size(BEAT_STEPS_SIZE, egui::Sense::hover());
    paint_beat_steps(ui.painter(), rect, bar_beat.map(|(_, beat)| beat));

    // Honest-display hint: the waveform ticks are drawn dimmed for this
    // grid, and this says why.
    if hint && marks.low_confidence() {
        ui.label(
            theme::NOTE_TEXT
                .rich("grid: low confidence")
                .color(theme::TEXT_MUTED),
        );
    }
}
