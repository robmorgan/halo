//! Trigger lanes: three rows (Lighting / Pixels / FX) under the zoomed
//! waveform, sharing its frame→x mapping and centered playhead so the
//! bars scroll in lockstep with the audio.

use eframe::egui;
use halo_light::cues::{CueSet, LANE_COUNT};
use halo_light::programmer::{LaneOutput, LaneSource};

use super::zoomed::ZoomSpan;
use super::{FrameMap, GridMarks, LANES, palette};

/// Height of each lane row in points.
const LANE_ROW_H: f32 = 14.0;
/// Full strip height: three rows plus two 1 pt separators.
const STRIP_HEIGHT: f32 = 3.0 * LANE_ROW_H + 2.0;
/// Vertical inset of a trigger bar within its row.
const BAR_INSET_Y: f32 = 2.5;
/// Bars never collapse below this width at wide zooms.
const MIN_BAR_W: f32 = 2.0;
/// Extra dim applied to every lane when the deck isn't driving lighting.
const INACTIVE_DIM: f32 = 0.30;

pub struct LanesParams<'a> {
    pub cues: &'a CueSet,
    pub marks: &'a GridMarks,
    pub position_frames: f64,
    pub total_frames: usize,
    pub sample_rate: u32,
    /// This deck currently drives the lighting rig; dim everything when
    /// false.
    pub lighting_active: bool,
    /// Resolved lighting output, Some only for the active lighting deck:
    /// a programmer-overridden lane tints its row and renders its cue
    /// bars hollow ("this would be playing, but you've taken over").
    pub outputs: Option<&'a [LaneOutput; LANE_COUNT]>,
}

/// Paint the three trigger lanes.
pub fn paint_lanes(ui: &mut egui::Ui, params: LanesParams<'_>, span: &ZoomSpan) {
    let desired_size = egui::vec2(ui.available_width(), STRIP_HEIGHT);
    let (response, painter) = ui.allocate_painter(desired_size, egui::Sense::hover());
    let rect = response.rect;
    let painter = painter.with_clip_rect(rect);

    painter.rect_filled(rect, 4.0, palette::LANE_BG);

    let dim = |color: egui::Color32, alpha: f32| {
        let alpha = if params.lighting_active {
            alpha
        } else {
            alpha * INACTIVE_DIM
        };
        color.gamma_multiply(alpha)
    };

    let row_rect = |row: usize| {
        let top = rect.top() + row as f32 * (LANE_ROW_H + 1.0);
        egui::Rect::from_min_size(
            egui::pos2(rect.left(), top),
            egui::vec2(rect.width(), LANE_ROW_H),
        )
    };

    for row in 1..LANES.len() {
        let y = row_rect(row).top() - 0.5;
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0_f32, palette::LANE_SEPARATOR),
        );
    }

    let loaded = params.total_frames > 0;

    if loaded {
        let span_frames = span.span_frames(params.marks, params.sample_rate);
        let map = FrameMap::new(rect, params.position_frames, span_frames);

        for (row, &(lane, _, color)) in LANES.iter().enumerate() {
            let rr = row_rect(row);
            let overridden = params
                .outputs
                .is_some_and(|o| o[row].source == LaneSource::Programmer);
            if overridden {
                painter.rect_filled(rr, 0.0, color.gamma_multiply(0.10));
            }
            for c in params
                .cues
                .visible(lane, map.start_frame(), map.end_frame())
            {
                let x0 = map.x(c.start_frame);
                let x1 = map.x(c.end_frame()).max(x0 + MIN_BAR_W);
                let bar = egui::Rect::from_min_max(
                    egui::pos2(x0, rr.top() + BAR_INSET_Y),
                    egui::pos2(x1, rr.bottom() - BAR_INSET_Y),
                );
                let alpha = 0.55 + 0.45 * c.intensity.clamp(0.0, 1.0);
                if overridden {
                    painter.rect_stroke(
                        bar,
                        2.0,
                        egui::Stroke::new(1.0_f32, dim(color, alpha)),
                        egui::StrokeKind::Inside,
                    );
                } else {
                    painter.rect_filled(bar, 2.0, dim(color, alpha));
                }
            }
        }
    }

    // Labels over the bars (no reserved gutter, so the mapping stays
    // full-width and pixel-identical to the zoomed view above), with a
    // backing wash for legibility.
    for (row, &(_, label, color)) in LANES.iter().enumerate() {
        let rr = row_rect(row);
        let galley = painter.layout_no_wrap(
            label.to_owned(),
            egui::FontId::monospace(8.0),
            dim(color, 0.5),
        );
        let pos = egui::pos2(rr.left() + 4.0, rr.center().y - galley.size().y / 2.0);
        let backing = egui::Rect::from_min_size(pos, galley.size()).expand2(egui::vec2(2.0, 0.0));
        painter.rect_filled(backing, 2.0, palette::LANE_BG.gamma_multiply(0.8));
        painter.galley(pos, galley, color);
    }

    // Continue the zoomed view's centered playhead through the strip.
    if loaded {
        let center_x = rect.center().x;
        painter.line_segment(
            [
                egui::pos2(center_x, rect.top()),
                egui::pos2(center_x, rect.bottom()),
            ],
            egui::Stroke::new(2.0_f32, palette::PLAYHEAD),
        );
    }
}
