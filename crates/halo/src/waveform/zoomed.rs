//! Zoomed scrolling waveform: playhead fixed at horizontal center, the
//! track scrolls underneath. 3-band envelopes are tessellated per frame
//! from the pyramid level closest to one bucket per pixel (the content
//! moves every frame, so a texture would need constant re-upload; a few
//! thousand triangles at the 30 fps repaint cap is cheaper). The ≥1px
//! bucket rule bounds the tessellation by the panel's pixel width.
//! Beat/downbeat edge ticks, loop overlay, and drag-to-scrub.

use eframe::egui;
use timestretch::{BandPeaks, PeakLevel};

use super::{FrameMap, GridMarks, overlay_plan, paint_placeholder, palette};

/// View height in points.
const VIEW_HEIGHT: f32 = 160.0;
/// Edge tick heights in points.
const TICK_BEAT_PX: f32 = 8.0;
const TICK_DOWNBEAT_PX: f32 = 14.0;
/// Scroll distance (points) per zoom step on wheel/trackpad zoom.
const SCROLL_PER_ZOOM_STEP: f32 = 40.0;
/// Cue marker triangle size in points.
const CUE_TRI_W: f32 = 9.0;
const CUE_TRI_H: f32 = 7.0;

/// Zoom presets: bars when a grid exists, seconds otherwise. Same index
/// into both tables so toggling grids keeps a comparable span.
const BAR_PRESETS: [f64; 5] = [1.0, 2.0, 4.0, 8.0, 16.0];
const SEC_PRESETS: [f64; 5] = [2.0, 4.0, 8.0, 16.0, 32.0];
const DEFAULT_PRESET: usize = 2;

/// Visible-span state for the zoomed view.
pub struct ZoomSpan {
    idx: usize,
    /// Accumulated scroll distance toward the next wheel-zoom step.
    scroll_accum: f32,
}

impl Default for ZoomSpan {
    fn default() -> Self {
        Self {
            idx: DEFAULT_PRESET,
            scroll_accum: 0.0,
        }
    }
}

impl ZoomSpan {
    pub fn zoom_in(&mut self) {
        self.idx = self.idx.saturating_sub(1);
    }

    pub fn zoom_out(&mut self) {
        self.idx = (self.idx + 1).min(BAR_PRESETS.len() - 1);
    }

    /// Label for the zoom control, e.g. "4 BARS" or "8 s".
    pub fn label(&self, has_grid: bool) -> String {
        if has_grid {
            let bars = BAR_PRESETS[self.idx];
            if bars == 1.0 {
                "1 BAR".to_string()
            } else {
                format!("{bars:.0} BARS")
            }
        } else {
            format!("{:.0} s", SEC_PRESETS[self.idx])
        }
    }

    /// Visible span in source frames.
    pub(crate) fn span_frames(&self, marks: &GridMarks, sample_rate: u32) -> f64 {
        let beat = marks.median_beat_frames();
        if marks.is_usable() && beat > 0.0 {
            BAR_PRESETS[self.idx] * 4.0 * beat
        } else {
            SEC_PRESETS[self.idx] * sample_rate.max(1) as f64
        }
    }

    /// Step the zoom from accumulated scroll input; scrolling up zooms in.
    fn apply_scroll(&mut self, delta_y: f32) {
        self.scroll_accum += delta_y;
        while self.scroll_accum >= SCROLL_PER_ZOOM_STEP {
            self.zoom_in();
            self.scroll_accum -= SCROLL_PER_ZOOM_STEP;
        }
        while self.scroll_accum <= -SCROLL_PER_ZOOM_STEP {
            self.zoom_out();
            self.scroll_accum += SCROLL_PER_ZOOM_STEP;
        }
    }
}

/// Drag lifecycle of the zoomed view, for audible scrubbing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrubGesture {
    /// Drag began this frame: grab the platter.
    Grab,
    /// Pointer moved while dragging: relative scrub distance in source
    /// frames (content follows the pointer, so dragging right moves the
    /// position backward).
    Drag(f64),
    /// The drag ended this frame; the scrub voice carries its own
    /// momentum, so no velocity payload is needed.
    Release,
}

/// Translucent ghost playhead for the sync-align slide-in: drawn
/// `offset_frames` from the centered playhead, fading with `alpha`.
pub struct GhostPlayhead {
    pub offset_frames: f64,
    pub alpha: f32,
}

pub struct ZoomedParams<'a> {
    pub peaks: Option<&'a BandPeaks>,
    pub marks: &'a GridMarks,
    pub position_frames: f64,
    pub total_frames: usize,
    pub sample_rate: u32,
    pub loop_region: Option<(usize, usize)>,
    pub loop_in: Option<usize>,
    /// Hot cue slots (source frames); markers draw at the top edge for each
    /// defined slot. Pass `&[]` for views without hot cues.
    pub hot_cues: &'a [Option<usize>],
    /// CDJ cue point (source frames), drawn as an unnumbered marker.
    pub cue_point: Option<usize>,
    /// Sync-align slide-in animation, if one is running.
    pub ghost: Option<GhostPlayhead>,
}

/// Paint the zoomed view. Reports the drag lifecycle while the user
/// scrubs: the grab as [`ScrubGesture::Grab`], pointer deltas as
/// [`ScrubGesture::Drag`], and the drop as [`ScrubGesture::Release`].
pub fn paint_zoomed(
    ui: &mut egui::Ui,
    params: ZoomedParams<'_>,
    span: &mut ZoomSpan,
) -> Option<ScrubGesture> {
    let desired_size = egui::vec2(ui.available_width(), VIEW_HEIGHT);
    let (response, painter) = ui.allocate_painter(desired_size, egui::Sense::drag());
    let rect = response.rect;

    painter.rect_filled(rect, 4.0, palette::BACKGROUND);

    let (Some(peaks), true) = (params.peaks, params.total_frames > 0) else {
        paint_placeholder(&painter, rect);
        return None;
    };

    if response.hovered() {
        span.apply_scroll(ui.input(|i| i.smooth_scroll_delta.y));
    }

    let span_frames = span.span_frames(params.marks, params.sample_rate);
    let map = FrameMap::new(rect, params.position_frames, span_frames);

    // 3-band envelopes from the pyramid level nearest one bucket per pixel.
    let px_per_sec = (map.px_per_frame() * params.sample_rate as f64) as f32;
    let level = peaks.level(peaks.level_index_for(px_per_sec));
    let frames_per_bucket = params.sample_rate as f64 / level.buckets_per_sec;
    let first_bucket = (map.start_frame() / frames_per_bucket).floor().max(0.0) as usize;
    let last_bucket =
        ((map.end_frame() / frames_per_bucket).ceil() as usize).min(level.num_buckets());
    if first_bucket < last_bucket {
        paint_band_envelopes(
            &painter.with_clip_rect(rect),
            level,
            first_bucket..last_bucket,
            frames_per_bucket,
            &map,
            rect,
            params.total_frames,
        );
    }

    // Loop overlay: fill plus full-height boundary lines where in view.
    if let Some((start, end)) = params.loop_region {
        let x0 = map.x(start as f64);
        let x1 = map.x(end as f64);
        if x1 > rect.left() && x0 < rect.right() {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(x0.max(rect.left()), rect.top()),
                    egui::pos2(x1.min(rect.right()), rect.bottom()),
                ),
                0.0,
                palette::LOOP_FILL,
            );
        }
        for x in [x0, x1] {
            if x >= rect.left() && x <= rect.right() {
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    egui::Stroke::new(1.5_f32, palette::LOOP_EDGE),
                );
            }
        }
    } else if let Some(start) = params.loop_in {
        let x = map.x(start as f64);
        if x >= rect.left() && x <= rect.right() {
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.5_f32, palette::LOOP_EDGE),
            );
        }
    }

    // Beat/downbeat edge ticks (top and bottom), density-adaptive.
    if params.marks.is_usable() {
        let visible = params
            .marks
            .visible_range(map.start_frame(), map.end_frame());
        let downbeats = visible
            .clone()
            .filter(|&i| params.marks.is_downbeat(i))
            .count();
        let plan = overlay_plan(rect.width(), visible.len(), downbeats);
        let stride = plan.downbeat_stride as u32;
        // Honest display: a low-confidence grid draws dimmed, and the
        // counter row says why.
        let (beat_color, downbeat_color) = if params.marks.low_confidence() {
            (
                palette::TICK_BEAT.gamma_multiply(super::LOW_CONFIDENCE_TICK_DIM),
                palette::TICK_DOWNBEAT.gamma_multiply(super::LOW_CONFIDENCE_TICK_DIM),
            )
        } else {
            (palette::TICK_BEAT, palette::TICK_DOWNBEAT)
        };
        // Bar-number labels need more room than ticks (~34 px vs 6), so they
        // thin on their own power-of-two stride on top of the tick stride.
        let bar_px = (params.marks.median_beat_frames() * 4.0 * map.px_per_frame()) as f32;
        let mut label_stride = stride;
        if bar_px > 0.0 {
            while bar_px * (label_stride as f32) < 34.0 && label_stride < (1 << 16) {
                label_stride *= 2;
            }
        }
        for i in visible {
            let is_downbeat = params.marks.is_downbeat(i);
            let (height, stroke) = if is_downbeat {
                let bar = params.marks.bar_number(i);
                if bar == 0 || !(bar - 1).is_multiple_of(stride) {
                    continue;
                }
                if (bar - 1).is_multiple_of(label_stride) {
                    painter.text(
                        egui::pos2(map.x(params.marks.frame(i)) + 3.0, rect.top() + 1.0),
                        egui::Align2::LEFT_TOP,
                        bar,
                        egui::FontId::monospace(9.0),
                        palette::TEXT_DIM,
                    );
                }
                (TICK_DOWNBEAT_PX, egui::Stroke::new(2.0_f32, downbeat_color))
            } else {
                if !plan.draw_beats {
                    continue;
                }
                (TICK_BEAT_PX, egui::Stroke::new(1.0_f32, beat_color))
            };
            let x = map.x(params.marks.frame(i));
            painter.line_segment(
                [
                    egui::pos2(x, rect.top()),
                    egui::pos2(x, rect.top() + height),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(x, rect.bottom() - height),
                    egui::pos2(x, rect.bottom()),
                ],
                stroke,
            );
        }
    }

    // Elastic lead-in: mark where the track actually starts. Gated on a
    // negative position so ordinary near-head playback (where the viewport
    // routinely straddles frame 0) stays unadorned.
    if params.position_frames < 0.0 {
        let x = map.x(0.0);
        if x >= rect.left() && x <= rect.right() {
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.0_f32, palette::TEXT_DIM),
            );
        }
    }

    // Cue markers: the CDJ cue point (unnumbered) plus the hot cue slots,
    // drawn over the ticks but under the playhead.
    let draw_marker = |frame: usize, label: Option<usize>| {
        let x = map.x(frame as f64);
        if x >= rect.left() && x <= rect.right() {
            cue_marker(&painter, rect, x, label);
        }
    };
    if let Some(frame) = params.cue_point {
        draw_marker(frame, None);
    }
    for (slot, cue) in params.hot_cues.iter().enumerate() {
        if let Some(frame) = cue {
            draw_marker(*frame, Some(slot + 1));
        }
    }

    // Ghost playhead: the pre-align position gliding into the centered
    // playhead after a sync-aligned start. Drawn center-relative so it
    // converges exactly, and under the real playhead so it merges into it.
    if let Some(g) = &params.ghost {
        let x = rect.center().x + (g.offset_frames * map.px_per_frame()) as f32;
        if x >= rect.left() && x <= rect.right() {
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(2.0_f32, palette::PLAYHEAD.gamma_multiply(g.alpha)),
            );
        }
    }

    // Fixed centered playhead — the one full-height line in this view.
    let center_x = rect.center().x;
    painter.line_segment(
        [
            egui::pos2(center_x, rect.top()),
            egui::pos2(center_x, rect.bottom()),
        ],
        egui::Stroke::new(2.0_f32, palette::PLAYHEAD),
    );

    // Drag-to-scrub: content follows the pointer. `drag_started` must be
    // checked before `dragged` (both are true on the first frame; the
    // first-frame delta is ~0 and safely dropped).
    if response.drag_started() {
        return Some(ScrubGesture::Grab);
    }
    if response.drag_stopped() {
        return Some(ScrubGesture::Release);
    }
    if response.dragged() {
        let dx = response.drag_delta().x;
        return Some(ScrubGesture::Drag(-(dx as f64) / map.px_per_frame()));
    }
    None
}

/// Paint the 3-band waveform as interpolated envelopes: per band, a mesh
/// of one trapezoid per adjacent-bucket pair spanning the positive to the
/// negative peak contour (sampled at bucket centers), plus 1px strokes
/// along both contours whose feathering anti-aliases the edges. Meshes
/// rather than filled paths because epaint's filled paths assume convexity,
/// which an envelope doesn't satisfy. Bands paint high → mid → low — low
/// on top: see overview::render_level.
fn paint_band_envelopes(
    painter: &egui::Painter,
    level: &PeakLevel,
    buckets: std::ops::Range<usize>,
    frames_per_bucket: f64,
    map: &FrameMap,
    rect: egui::Rect,
    total_frames: usize,
) {
    let center_y = rect.center().y;
    let half_height = rect.height() * 0.45;
    let band_colors = [palette::BAND_LOW, palette::BAND_MID, palette::BAND_HIGH];

    // Sample x positions at bucket centers, extended flat to the panel (or
    // track) edges so the outer half-buckets aren't extrapolated. `idx`
    // maps each sample back to its bucket (edge points reuse the
    // first/last bucket's values).
    let x_of = |b: usize| map.x((b as f64 + 0.5) * frames_per_bucket);
    let mut xs: Vec<f32> = Vec::with_capacity(buckets.len() + 2);
    let mut idx: Vec<usize> = Vec::with_capacity(buckets.len() + 2);
    let left_edge = rect.left().max(map.x(0.0));
    if left_edge < x_of(buckets.start) {
        xs.push(left_edge);
        idx.push(buckets.start);
    }
    for b in buckets.clone() {
        xs.push(x_of(b));
        idx.push(b);
    }
    let right_edge = rect.right().min(map.x(total_frames as f64));
    if right_edge > *xs.last().unwrap() {
        xs.push(right_edge);
        idx.push(buckets.end - 1);
    }

    for (band, &color) in band_colors.iter().enumerate().rev() {
        let y_pos: Vec<f32> = idx
            .iter()
            .map(|&b| center_y - level.pos[band][b].clamp(0.0, 1.0) * half_height)
            .collect();
        let y_neg: Vec<f32> = idx
            .iter()
            .map(|&b| center_y - level.neg[band][b].clamp(-1.0, 0.0) * half_height)
            .collect();
        let silent = |i: usize| y_pos[i] == center_y && y_neg[i] == center_y;

        // The contour strokes split at silent runs (so silence stays
        // transparent, not a center line) and are collected to paint over
        // the band's own mesh but under lower bands.
        let mut mesh = egui::Mesh::default();
        let mut strokes: Vec<egui::Shape> = Vec::new();
        let mut pos_line: Vec<egui::Pos2> = Vec::new();
        let mut neg_line: Vec<egui::Pos2> = Vec::new();
        let flush = |pos_line: &mut Vec<egui::Pos2>,
                     neg_line: &mut Vec<egui::Pos2>,
                     strokes: &mut Vec<egui::Shape>| {
            for line in [std::mem::take(pos_line), std::mem::take(neg_line)] {
                if line.len() >= 2 {
                    strokes.push(egui::Shape::line(line, egui::Stroke::new(1.0_f32, color)));
                }
            }
        };
        for i in 0..xs.len() - 1 {
            // A segment paints when either endpoint is audible, so decays
            // taper to nothing instead of snapping off.
            if silent(i) && silent(i + 1) {
                flush(&mut pos_line, &mut neg_line, &mut strokes);
                continue;
            }
            let base = mesh.vertices.len() as u32;
            for (x, y) in [
                (xs[i], y_pos[i]),
                (xs[i + 1], y_pos[i + 1]),
                (xs[i + 1], y_neg[i + 1]),
                (xs[i], y_neg[i]),
            ] {
                mesh.colored_vertex(egui::pos2(x, y), color);
            }
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base, base + 2, base + 3);
            if pos_line.is_empty() {
                pos_line.push(egui::pos2(xs[i], y_pos[i]));
                neg_line.push(egui::pos2(xs[i], y_neg[i]));
            }
            pos_line.push(egui::pos2(xs[i + 1], y_pos[i + 1]));
            neg_line.push(egui::pos2(xs[i + 1], y_neg[i + 1]));
        }
        flush(&mut pos_line, &mut neg_line, &mut strokes);
        if !mesh.is_empty() {
            painter.add(mesh);
        }
        painter.extend(strokes);
    }
}

/// CDJ-style cue marker: a down-pointing triangle hanging from the top
/// edge, a small square foot on the bottom edge at the same x, and an
/// optional hot-cue slot number beside the triangle.
fn cue_marker(painter: &egui::Painter, rect: egui::Rect, x: f32, label: Option<usize>) {
    let color = palette::CUE_MARKER;
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(x - CUE_TRI_W / 2.0, rect.top()),
            egui::pos2(x + CUE_TRI_W / 2.0, rect.top()),
            egui::pos2(x, rect.top() + CUE_TRI_H),
        ],
        color,
        egui::Stroke::NONE,
    ));
    painter.rect_filled(
        egui::Rect::from_center_size(egui::pos2(x, rect.bottom() - 2.0), egui::vec2(4.0, 4.0)),
        0.0,
        color,
    );
    if let Some(n) = label {
        painter.text(
            egui::pos2(x + CUE_TRI_W / 2.0 + 2.0, rect.top()),
            egui::Align2::LEFT_TOP,
            n,
            egui::FontId::proportional(8.0),
            color,
        );
    }
}
