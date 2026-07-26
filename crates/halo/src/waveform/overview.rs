//! Full-track overview strip, CDJ-3000 style: a bottom-anchored 3-band
//! amplitude silhouette ("side-on" view) with the played portion dimmed,
//! hot cue markers, loop region, position cursor, a hover seek-preview,
//! and click-to-seek.

use eframe::egui;

use super::peaks::{BandPeaks, PeakLevel};
use super::{GridMarks, paint_placeholder, palette};

/// Strip height in points.
const STRIP_HEIGHT: f32 = 56.0;
/// Texture height in pixels (2x the strip for retina crispness).
const TEX_HEIGHT: usize = 112;
/// Perceptual lift applied to column heights (amp^gamma): keeps quiet
/// intros/breakdowns visible in the silhouette. 1.0 = linear.
const OVERVIEW_GAMMA: f32 = 0.85;
/// Fraction of the strip height the silhouette may use; the headroom above
/// holds the hot cue markers.
const WAVE_HEIGHT_FRAC: f32 = 0.60;
/// Hot cue marker triangle size in points.
const CUE_MARKER_W: f32 = 8.0;
const CUE_MARKER_H: f32 = 6.0;

/// The full track pre-rendered once per load from the coarsest pyramid
/// level as a bottom-anchored silhouette (column height = band peak),
/// bands overlaid per column. Drawn twice per frame: full-width with a
/// white tint, then UV-clipped to the playhead with a grey tint that dims
/// the played part (CDJ-style).
pub struct OverviewTexture {
    tex: egui::TextureHandle,
}

impl OverviewTexture {
    pub fn from_peaks(ctx: &egui::Context, peaks: &BandPeaks) -> Self {
        Self {
            tex: ctx.load_texture(
                "waveform_overview",
                render_level(peaks.coarsest()),
                egui::TextureOptions::LINEAR,
            ),
        }
    }
}

/// Rasterizes a peak level into a transparent-background image, one
/// bottom-anchored column per bucket (CDJ-3000 "side-on" silhouette:
/// height = band peak, everything rises from the baseline). Bands paint
/// in high → mid → low order — low on top — so kick-heavy passages read
/// blue and highs surface only where the lows drop out (CDJ RGB
/// semantics; in a dense master the high band's *peak* is near full scale
/// everywhere and would bury the image if on top).
fn render_level(level: &PeakLevel) -> egui::ColorImage {
    let width = level.num_buckets().max(1);
    let mut image = egui::ColorImage::new([width, TEX_HEIGHT], egui::Color32::TRANSPARENT);
    let band_colors = [palette::BAND_LOW, palette::BAND_MID, palette::BAND_HIGH];
    for x in 0..level.num_buckets() {
        for (band, &color) in band_colors.iter().enumerate().rev() {
            let pos = level.pos[band][x].clamp(0.0, 1.0);
            let neg = level.neg[band][x].clamp(-1.0, 0.0);
            let amp = pos.max(-neg).powf(OVERVIEW_GAMMA);
            let top_f = (1.0 - amp * WAVE_HEIGHT_FRAC) * TEX_HEIGHT as f32;
            let top = top_f.ceil().clamp(0.0, TEX_HEIGHT as f32) as usize;
            for y in top..TEX_HEIGHT {
                image.pixels[y * width + x] = color;
            }
            // Anti-aliased top edge: the partial pixel above the solid run
            // gets coverage-scaled alpha instead of a hard step.
            let coverage = top as f32 - top_f;
            if coverage > 0.0 && top > 0 {
                image.pixels[(top - 1) * width + x] = color.linear_multiply(coverage);
            }
        }
    }
    image
}

pub struct OverviewParams<'a> {
    pub texture: Option<&'a OverviewTexture>,
    /// Playback position as a fraction of the track (0..1).
    pub progress: f32,
    pub total_frames: usize,
    pub loop_region: Option<(usize, usize)>,
    pub loop_in: Option<usize>,
    /// Hot cue slots (source frames); markers draw above the wave for each
    /// defined slot. Pass `&[]` for players without hot cues.
    pub hot_cues: &'a [Option<usize>],
    /// Beat grid, for the bar numbers along the top edge.
    pub marks: &'a GridMarks,
}

/// Paint the overview strip. Returns the click-to-seek target as a track
/// fraction, if clicked.
pub fn paint_overview(ui: &mut egui::Ui, params: OverviewParams<'_>) -> Option<f32> {
    let desired_size = egui::vec2(ui.available_width(), STRIP_HEIGHT);
    let (response, painter) = ui.allocate_painter(desired_size, egui::Sense::click());
    let rect = response.rect;

    painter.rect_filled(rect, 4.0, palette::BACKGROUND);

    let Some(texture) = params.texture else {
        paint_placeholder(&painter, rect);
        return None;
    };

    let progress = params.progress.clamp(0.0, 1.0);
    let cursor_x = rect.left() + rect.width() * progress;

    // Baseline the silhouette sits on, visible through silent passages.
    painter.line_segment(
        [
            egui::pos2(rect.left(), rect.bottom() - 1.0),
            egui::pos2(rect.right(), rect.bottom() - 1.0),
        ],
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(50, 50, 62)),
    );

    // Unplayed across the full width, played dimmed on top (UV-clipped).
    painter.image(
        texture.tex.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    if progress > 0.0 {
        painter.image(
            texture.tex.id(),
            egui::Rect::from_min_max(rect.left_top(), egui::pos2(cursor_x, rect.bottom())),
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(progress, 1.0)),
            palette::PLAYED_TINT,
        );
    }

    let frac_x = |frame: usize| {
        rect.left() + rect.width() * (frame as f32 / params.total_frames.max(1) as f32)
    };
    let frac_x_f = |frame: f64| {
        rect.left() + rect.width() * (frame / params.total_frames.max(1) as f64) as f32
    };

    // Bar numbers along the top edge, thinned to keep labels ≥ ~40 px apart.
    if params.marks.is_usable() {
        let bars = params.marks.downbeat_count();
        if bars > 0 {
            let mut stride = 1u32;
            while rect.width() * stride as f32 / (bars as f32) < 40.0 && stride < (1 << 16) {
                stride *= 2;
            }
            for i in (0..params.marks.len()).filter(|&i| params.marks.is_downbeat(i)) {
                let bar = params.marks.bar_number(i);
                if bar == 0 || !(bar - 1).is_multiple_of(stride) {
                    continue;
                }
                painter.text(
                    egui::pos2(frac_x_f(params.marks.frame(i)) + 2.0, rect.top() + 1.0),
                    egui::Align2::LEFT_TOP,
                    bar,
                    egui::FontId::monospace(8.0),
                    palette::TEXT_DIM,
                );
            }
        }
    }

    // Loop region / staged loop-in.
    if let Some((start, end)) = params.loop_region {
        let (x0, x1) = (frac_x(start), frac_x(end));
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, rect.bottom())),
            0.0,
            palette::LOOP_FILL,
        );
        for x in [x0, x1] {
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.0_f32, palette::LOOP_EDGE),
            );
        }
    } else if let Some(start) = params.loop_in {
        let x = frac_x(start);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0_f32, palette::LOOP_EDGE),
        );
    }

    // Hot cue markers: numbered amber triangles pointing down at the wave
    // ceiling, in the headroom the 60% height cap reserves.
    if params.total_frames > 0 {
        let ceiling_y = rect.bottom() - WAVE_HEIGHT_FRAC * rect.height();
        for (slot, cue) in params.hot_cues.iter().enumerate() {
            let Some(frame) = cue else { continue };
            let x = frac_x(*frame);
            let tip = egui::pos2(x, ceiling_y - 1.0);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(x - CUE_MARKER_W / 2.0, tip.y - CUE_MARKER_H),
                    egui::pos2(x + CUE_MARKER_W / 2.0, tip.y - CUE_MARKER_H),
                    tip,
                ],
                palette::LOOP_EDGE,
                egui::Stroke::NONE,
            ));
            painter.text(
                egui::pos2(x, tip.y - CUE_MARKER_H - 1.0),
                egui::Align2::CENTER_BOTTOM,
                format!("{}", slot + 1),
                egui::FontId::proportional(8.0),
                palette::LOOP_EDGE,
            );
        }
    }

    // Hover seek-preview: a ghost of the position cursor under the pointer
    // shows where a click will land.
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if let Some(hover) = response.hover_pos() {
        let x = hover.x.clamp(rect.left(), rect.right());
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(
                1.0_f32,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 90),
            ),
        );
    }

    // Position cursor.
    painter.line_segment(
        [
            egui::pos2(cursor_x, rect.top()),
            egui::pos2(cursor_x, rect.bottom()),
        ],
        egui::Stroke::new(1.0_f32, palette::CURSOR),
    );

    // Click-to-seek.
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        return Some(((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
    }
    None
}
