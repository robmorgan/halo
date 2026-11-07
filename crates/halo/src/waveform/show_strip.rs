//! L3 show strip: the three role lanes (Look / Energy / Accent) under
//! the zoomed waveform in the deck's waveform well, sharing its frame→x
//! mapping so everything scrolls in lockstep (the deck draws the playhead
//! through both). Read-only — editing lives in the Prepare view's
//! `show_editor`.

use eframe::egui;
use halo_light::cues::LANE_COUNT;

use super::zoomed::ZoomSpan;
use super::{FrameMap, GridMarks, lane_rows, palette};
use crate::show_preview::{ACCENT_LANE, ShowPreview};

/// Row heights, top to bottom: look / energy / accent.
const ROW_H: [f32; LANE_COUNT] = [14.0, 14.0, 14.0];
/// Full strip height: three rows between four 1 pt seams (above, between
/// and below them).
pub const STRIP_HEIGHT: f32 = ROW_H[0] + ROW_H[1] + ROW_H[2] + 4.0;
/// Vertical inset of an accent bar within its row.
const BAR_INSET_Y: f32 = 2.0;
/// Bars/blocks never collapse below this width at wide zooms.
const MIN_BAR_W: f32 = 2.0;
/// Extra dim applied to every lane when the deck isn't driving lighting.
const INACTIVE_DIM: f32 = 0.30;
/// Width of the left label gutter in points.
const LABEL_GUTTER_W: f32 = 48.0;
/// Space before a lane's caption.
const LABEL_PAD: f32 = 8.0;
/// Look blocks narrower than this skip their name label.
const MIN_LABEL_W: f32 = 28.0;

pub struct ShowStripParams<'a> {
    pub show: &'a ShowPreview,
    pub marks: &'a GridMarks,
    pub position_frames: f64,
    pub total_frames: usize,
    pub sample_rate: u32,
    /// This deck currently drives the lighting rig; dim everything when
    /// false.
    pub lighting_active: bool,
    /// The live programmer is replacing rig output. In the L3 stack
    /// (programmer replace > energy scale > look) that displaces the
    /// look, so the look row tints and its blocks render hollow.
    pub programmer_override: bool,
}

/// Paint the three role lanes.
pub fn paint_show_strip(ui: &mut egui::Ui, params: ShowStripParams<'_>, span: &ZoomSpan) {
    let desired_size = egui::vec2(ui.available_width(), STRIP_HEIGHT);
    let (response, painter) = ui.allocate_painter(desired_size, egui::Sense::hover());
    let rect = response.rect;
    let painter = painter.with_clip_rect(rect);

    // Black seams round every row, each row on the lane ground.
    painter.rect_filled(rect, 0.0, crate::theme::QUIET_SEAM);
    let rows = lane_rows(rect.shrink2(egui::vec2(0.0, 1.0)), ROW_H);
    for row in &rows {
        painter.rect_filled(*row, 0.0, palette::LANE_BG);
    }

    let dim = |color: egui::Color32, alpha: f32| {
        let alpha = if params.lighting_active {
            alpha
        } else {
            alpha * INACTIVE_DIM
        };
        color.gamma_multiply(alpha)
    };

    let loaded = params.total_frames > 0;

    if loaded {
        let span_frames = span.span_frames(params.marks, params.sample_rate);
        let map = FrameMap::new(rect, params.position_frames, span_frames);
        let total = params.total_frames as f64;

        // --- Look row: color-script blocks holding until the next event.
        let rr = rows[0];
        if params.programmer_override {
            painter.rect_filled(rr, 0.0, egui::Color32::WHITE.gamma_multiply(0.06));
        }
        let looks = &params.show.looks;
        let visible = looks.visible(map.start_frame(), map.end_frame());
        let active = looks.active_at(params.position_frames).map(|e| e.id);
        // The last visible block runs to the first event past the window
        // (which `visible` excludes), or to the end of the track.
        let end_past_window = looks
            .events()
            .iter()
            .find(|e| e.frame >= map.end_frame())
            .map_or(total, |e| e.frame);
        for (i, ev) in visible.iter().enumerate() {
            let color = ev.look.def().color;
            let block_color = egui::Color32::WHITE;
            let x0 = map.x(ev.frame);
            let block_end = visible.get(i + 1).map_or(end_past_window, |n| n.frame);
            let x1 = map.x(block_end).max(x0 + MIN_BAR_W);
            let block = egui::Rect::from_min_max(
                egui::pos2(x0, rr.top() + 1.0),
                egui::pos2(x1, rr.bottom() - 1.0),
            );
            let is_active = active == Some(ev.id) && !params.programmer_override;
            let fill = if is_active { 0.10 } else { 0.05 };
            if params.programmer_override {
                painter.rect_stroke(
                    block,
                    0.0,
                    egui::Stroke::new(1.0_f32, dim(block_color, 0.25)),
                    egui::StrokeKind::Inside,
                );
            } else {
                painter.rect_filled(block, 0.0, dim(block_color, fill));
            }
            if is_active {
                painter.rect_stroke(
                    block,
                    0.0,
                    egui::Stroke::new(1.0_f32, dim(block_color, 0.35)),
                    egui::StrokeKind::Inside,
                );
            }
            // Solid left-edge tick: the event marker itself.
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(x0, rr.top() + 1.0),
                    egui::pos2(x0 + 2.0, rr.bottom() - 1.0),
                ),
                0.0,
                dim(color, 0.9),
            );
            if x1 - x0 >= MIN_LABEL_W {
                // Carried-in blocks keep their name readable: the label
                // pins to the visible edge (past the gutter) instead of
                // sitting at an off-screen block start.
                let label_x = (x0 + 5.0).max(rect.left() + LABEL_GUTTER_W + 5.0);
                crate::theme::paint_text(
                    &painter,
                    egui::pos2(label_x, rr.center().y),
                    egui::Align2::LEFT_CENTER,
                    ev.look.def().name,
                    crate::theme::NOTE_TEXT,
                    dim(egui::Color32::WHITE, 0.75),
                );
            }
        }

        // --- Energy row: the envelope polyline with an under-curve fill.
        let rr = rows[1];
        let inset = 2.0;
        let y_of = |v: f32| rr.bottom() - inset - v.clamp(0.0, 1.0) * (rr.height() - 2.0 * inset);
        let energy = &params.show.energy;
        if energy.points().is_empty() {
            painter.line_segment(
                [
                    egui::pos2(rect.left(), y_of(1.0)),
                    egui::pos2(rect.right(), y_of(1.0)),
                ],
                egui::Stroke::new(1.0_f32, dim(palette::ENERGY, 0.25)),
            );
        } else {
            // Sample points: window edges plus every breakpoint inside.
            let mut xs: Vec<(f32, f32)> = vec![(rect.left(), energy.value_at(map.start_frame()))];
            for p in energy.points() {
                if p.frame > map.start_frame() && p.frame < map.end_frame() {
                    xs.push((map.x(p.frame), p.value));
                }
            }
            xs.push((rect.right(), energy.value_at(map.end_frame())));

            // Under-curve fill as one convex trapezoid per segment —
            // epaint's filled paths assume convexity, which a
            // multi-breakpoint envelope doesn't satisfy.
            let fill = dim(palette::ENERGY, 0.15);
            let mut mesh = egui::Mesh::default();
            for w in xs.windows(2) {
                let (x0, v0) = w[0];
                let (x1, v1) = w[1];
                let i = mesh.vertices.len() as u32;
                for (x, y) in [
                    (x0, y_of(v0)),
                    (x1, y_of(v1)),
                    (x1, rr.bottom() - inset),
                    (x0, rr.bottom() - inset),
                ] {
                    mesh.colored_vertex(egui::pos2(x, y), fill);
                }
                mesh.add_triangle(i, i + 1, i + 2);
                mesh.add_triangle(i, i + 2, i + 3);
            }
            painter.add(mesh);
            let line: Vec<egui::Pos2> = xs.iter().map(|&(x, v)| egui::pos2(x, y_of(v))).collect();
            painter.add(egui::Shape::line(
                line,
                egui::Stroke::new(1.5_f32, dim(palette::ENERGY, 0.9)),
            ));
        }

        // --- Accent row: one-shot bars, legacy rendering.
        let rr = rows[2];
        for c in params
            .show
            .accents
            .visible(ACCENT_LANE, map.start_frame(), map.end_frame())
        {
            let x0 = map.x(c.start_frame);
            let x1 = map.x(c.end_frame()).max(x0 + MIN_BAR_W);
            let bar = egui::Rect::from_min_max(
                egui::pos2(x0, rr.top() + BAR_INSET_Y),
                egui::pos2(x1, rr.bottom() - BAR_INSET_Y),
            );
            let alpha = 0.55 + 0.45 * c.intensity.clamp(0.0, 1.0);
            painter.rect_filled(bar, 2.0, dim(palette::ACCENT, alpha));
        }
    }

    // Left label gutter, painted over the content so the frame→x mapping
    // stays full-width and pixel-identical to the zoomed view above.
    for (row, label) in rows.iter().zip(["LOOK", "NRG", "ACC"]) {
        let gutter = egui::Rect::from_min_max(
            row.left_top(),
            egui::pos2(row.left() + LABEL_GUTTER_W, row.bottom()),
        );
        painter.rect_filled(gutter, 0.0, palette::LANE_BG);
        crate::theme::paint_text(
            &painter,
            egui::pos2(row.left() + LABEL_PAD, row.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            crate::theme::QUIET_CAPTION,
            crate::theme::TEXT_MUTED,
        );
    }
}
