//! Direct-manipulation editor for a track's cue lanes: tall rows sharing
//! the zoomed view's frame→x mapping, with drag-to-create (snapped to the
//! beat grid), drag-to-move, edge resize, click/shift multi-select, and a
//! Cmd-drag rubber band. All positions are recomputed through the
//! [`FrameMap`] every frame, so editing stays correct while the timeline
//! plays underneath.

use std::collections::HashSet;

use eframe::egui;

use super::zoomed::ZoomSpan;
use super::{FrameMap, GridMarks, LANES, overlay_plan, palette};
use halo_light::cues::{ALL_LANES, CueSet};

/// Editor lane row height in points (~3× the perform strip's rows).
const EDIT_ROW_H: f32 = 44.0;
const STRIP_HEIGHT: f32 = 3.0 * EDIT_ROW_H + 2.0;
/// Pointer distance to a cue edge that counts as a resize grab.
const EDGE_GRAB_PX: f32 = 5.0;
/// Smallest musical cue duration, in beats (0.1 s without a grid).
const MIN_DUR_BEATS: f64 = 0.25;
const BAR_INSET_Y: f32 = 4.0;
const MIN_BAR_W: f32 = 2.0;
/// Intensity of a freshly drawn cue.
const CREATE_INTENSITY: f32 = 0.8;

pub struct LanesEditorParams<'a> {
    pub marks: &'a GridMarks,
    pub position_frames: f64,
    pub total_frames: usize,
    pub sample_rate: u32,
    /// Snap creates/moves/resizes to the nearest beat.
    pub snap: bool,
}

/// Drag state carried between frames.
#[derive(Default)]
pub struct EditorInteraction {
    drag: Option<DragKind>,
}

enum DragKind {
    /// Draw a new cue: it exists from the first frame and is resized
    /// between the anchor and the pointer.
    Create {
        id: u64,
        anchor: f64,
    },
    /// Move every selected cue by the pointer delta (original starts are
    /// kept so per-frame clamping never accumulates).
    Move {
        pointer_start: f64,
        orig: Vec<(u64, f64)>,
    },
    ResizeL {
        id: u64,
    },
    ResizeR {
        id: u64,
    },
    RubberBand {
        anchor_frame: f64,
        anchor_row: usize,
    },
}

enum Hit {
    EdgeL(u64),
    EdgeR(u64),
    Body(u64),
    Empty,
}

/// Nearest beat when snapping is on (and a grid exists); the raw frame
/// otherwise. Always non-negative.
pub fn snap_frame(marks: &GridMarks, snap: bool, frame: f64) -> f64 {
    let frame = frame.max(0.0);
    if !snap || !marks.is_usable() {
        return frame;
    }
    match marks.beat_at_or_before(frame) {
        Some(i) => {
            let a = marks.frame(i);
            let b = if i + 1 < marks.len() {
                marks.frame(i + 1)
            } else {
                a
            };
            if frame - a <= b - frame { a } else { b }
        }
        // Before the first beat: the first beat is the only grid point.
        None => marks.frame(0).min(frame).max(0.0),
    }
}

/// Paints and edits the lanes in place. Returns `true` when a mutating
/// gesture completed this frame — the caller persists the cue set then.
pub fn lanes_editor(
    ui: &mut egui::Ui,
    params: LanesEditorParams<'_>,
    span: &ZoomSpan,
    cues: &mut CueSet,
    selection: &mut HashSet<u64>,
    ix: &mut EditorInteraction,
) -> bool {
    let desired_size = egui::vec2(ui.available_width(), STRIP_HEIGHT);
    let (response, painter) = ui.allocate_painter(desired_size, egui::Sense::click_and_drag());
    let rect = response.rect;
    let painter = painter.with_clip_rect(rect);

    painter.rect_filled(rect, 4.0, palette::LANE_BG);

    let row_rect = |row: usize| {
        let top = rect.top() + row as f32 * (EDIT_ROW_H + 1.0);
        egui::Rect::from_min_size(
            egui::pos2(rect.left(), top),
            egui::vec2(rect.width(), EDIT_ROW_H),
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
    if !loaded {
        for (row, &(_, label, color)) in LANES.iter().enumerate() {
            let rr = row_rect(row);
            painter.text(
                egui::pos2(rr.left() + 6.0, rr.top() + 4.0),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::monospace(9.0),
                color.gamma_multiply(0.4),
            );
        }
        return false;
    }

    let span_frames = span.span_frames(params.marks, params.sample_rate);
    let map = FrameMap::new(rect, params.position_frames, span_frames);
    let total = params.total_frames as f64;
    let frame_at = |x: f32| map.start_frame() + (x - rect.left()) as f64 / map.px_per_frame();
    let snap = |frame: f64| snap_frame(params.marks, params.snap, frame).min(total);
    let min_dur = if params.marks.is_usable() && params.marks.median_beat_frames() > 0.0 {
        MIN_DUR_BEATS * params.marks.median_beat_frames()
    } else {
        0.1 * params.sample_rate.max(1) as f64
    };

    let row_at =
        |y: f32| (((y - rect.top()) / (EDIT_ROW_H + 1.0)).floor() as isize).clamp(0, 2) as usize;
    let hit_test = |cues: &CueSet, pos: egui::Pos2| -> (usize, Hit) {
        let row = row_at(pos.y);
        let lane = ALL_LANES[row];
        // Edges win over bodies; later (topmost-drawn) cues win ties.
        let mut hit = Hit::Empty;
        for c in cues.visible(lane, map.start_frame(), map.end_frame()) {
            let x0 = map.x(c.start_frame);
            let x1 = map.x(c.end_frame()).max(x0 + MIN_BAR_W);
            if (pos.x - x0).abs() <= EDGE_GRAB_PX {
                hit = Hit::EdgeL(c.id);
            } else if (pos.x - x1).abs() <= EDGE_GRAB_PX {
                hit = Hit::EdgeR(c.id);
            } else if pos.x > x0 && pos.x < x1 {
                hit = Hit::Body(c.id);
            }
        }
        (row, hit)
    };

    // Hover cursor feedback (only while not mid-drag).
    if ix.drag.is_none()
        && let Some(pos) = response.hover_pos()
    {
        match hit_test(cues, pos).1 {
            Hit::EdgeL(_) | Hit::EdgeR(_) => {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            Hit::Body(_) => ui.ctx().set_cursor_icon(egui::CursorIcon::Grab),
            Hit::Empty => {}
        }
    }

    let mut committed = false;
    let modifiers = ui.input(|i| i.modifiers);

    if response.drag_started()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let (row, hit) = hit_test(cues, pos);
        let lane = ALL_LANES[row];
        ix.drag = match hit {
            Hit::Body(id) => {
                if !selection.contains(&id) {
                    selection.clear();
                    selection.insert(id);
                }
                let mut orig: Vec<(u64, f64)> = selection
                    .iter()
                    .filter_map(|&id| cues.find(id).map(|(_, c)| (id, c.start_frame)))
                    .collect();
                orig.sort_by(|a, b| a.1.total_cmp(&b.1));
                Some(DragKind::Move {
                    pointer_start: frame_at(pos.x),
                    orig,
                })
            }
            Hit::EdgeL(id) => {
                selection.clear();
                selection.insert(id);
                Some(DragKind::ResizeL { id })
            }
            Hit::EdgeR(id) => {
                selection.clear();
                selection.insert(id);
                Some(DragKind::ResizeR { id })
            }
            Hit::Empty if modifiers.command => Some(DragKind::RubberBand {
                anchor_frame: frame_at(pos.x),
                anchor_row: row,
            }),
            Hit::Empty => {
                let anchor = snap(frame_at(pos.x));
                cues.insert(lane, anchor, min_dur, CREATE_INTENSITY)
                    .map(|id| {
                        selection.clear();
                        selection.insert(id);
                        DragKind::Create { id, anchor }
                    })
            }
        };
    }

    if response.dragged()
        && let Some(pos) = response.interact_pointer_pos()
    {
        match &ix.drag {
            Some(DragKind::Create { id, anchor }) => {
                let p = snap(frame_at(pos.x));
                let (lo, hi) = if p < *anchor {
                    (p, *anchor)
                } else {
                    (*anchor, p)
                };
                cues.resize(*id, lo, hi.max(lo + min_dur));
            }
            Some(DragKind::Move {
                pointer_start,
                orig,
            }) => {
                if let Some(&(_, first_start)) = orig.first() {
                    let raw_delta = frame_at(pos.x) - pointer_start;
                    let delta = snap(first_start + raw_delta) - first_start;
                    // Order matters so grouped cues don't clamp against a
                    // not-yet-moved neighbor: lead with the travel edge.
                    if delta >= 0.0 {
                        for &(id, start) in orig.iter().rev() {
                            cues.move_cue(id, start + delta);
                        }
                    } else {
                        for &(id, start) in orig.iter() {
                            cues.move_cue(id, start + delta);
                        }
                    }
                }
            }
            Some(DragKind::ResizeL { id }) => {
                if let Some((_, c)) = cues.find(*id) {
                    let end = c.end_frame();
                    cues.resize(*id, snap(frame_at(pos.x)).min(end - min_dur), end);
                }
            }
            Some(DragKind::ResizeR { id }) => {
                if let Some((_, c)) = cues.find(*id) {
                    let start = c.start_frame;
                    cues.resize(*id, start, snap(frame_at(pos.x)).max(start + min_dur));
                }
            }
            Some(DragKind::RubberBand { .. }) | None => {}
        }
    }

    if response.drag_stopped() {
        match ix.drag.take() {
            Some(DragKind::RubberBand {
                anchor_frame,
                anchor_row,
            }) => {
                if let Some(pos) = response.interact_pointer_pos() {
                    let f0 = anchor_frame.min(frame_at(pos.x));
                    let f1 = anchor_frame.max(frame_at(pos.x));
                    let r0 = anchor_row.min(row_at(pos.y));
                    let r1 = anchor_row.max(row_at(pos.y));
                    if !modifiers.shift {
                        selection.clear();
                    }
                    for &lane in &ALL_LANES[r0..=r1] {
                        for c in cues.visible(lane, f0, f1) {
                            if c.end_frame() > f0 && c.start_frame < f1 {
                                selection.insert(c.id);
                            }
                        }
                    }
                }
            }
            Some(_) => committed = true,
            None => {}
        }
    }

    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        match hit_test(cues, pos).1 {
            Hit::Body(id) | Hit::EdgeL(id) | Hit::EdgeR(id) => {
                if modifiers.shift {
                    if !selection.remove(&id) {
                        selection.insert(id);
                    }
                } else {
                    selection.clear();
                    selection.insert(id);
                }
            }
            Hit::Empty => selection.clear(),
        }
    }

    // Drop selection entries whose cues no longer exist.
    selection.retain(|&id| cues.find(id).is_some());

    // --- painting ---

    // Beat/downbeat ticks across the whole strip, density-adaptive.
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
        for i in visible {
            let is_downbeat = params.marks.is_downbeat(i);
            let stroke = if is_downbeat {
                let bar = params.marks.bar_number(i);
                if bar == 0 || !(bar - 1).is_multiple_of(stride) {
                    continue;
                }
                egui::Stroke::new(1.0_f32, palette::TICK_DOWNBEAT.gamma_multiply(0.35))
            } else {
                if !plan.draw_beats {
                    continue;
                }
                egui::Stroke::new(1.0_f32, palette::TICK_BEAT.gamma_multiply(0.12))
            };
            let x = map.x(params.marks.frame(i));
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                stroke,
            );
        }
    }

    for (row, &(lane, label, color)) in LANES.iter().enumerate() {
        let rr = row_rect(row);
        for c in cues.visible(lane, map.start_frame(), map.end_frame()) {
            let x0 = map.x(c.start_frame);
            let x1 = map.x(c.end_frame()).max(x0 + MIN_BAR_W);
            let bar = egui::Rect::from_min_max(
                egui::pos2(x0, rr.top() + BAR_INSET_Y),
                egui::pos2(x1, rr.bottom() - BAR_INSET_Y),
            );
            painter.rect_filled(
                bar,
                3.0,
                color.gamma_multiply(0.45 + 0.45 * c.intensity.clamp(0.0, 1.0)),
            );
            if selection.contains(&c.id) {
                painter.rect_stroke(
                    bar,
                    3.0,
                    egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
                    egui::StrokeKind::Outside,
                );
            }
        }
        painter.text(
            egui::pos2(rr.left() + 6.0, rr.top() + 4.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::monospace(9.0),
            color.gamma_multiply(0.6),
        );
    }

    // Live rubber-band rectangle.
    if let (
        Some(DragKind::RubberBand {
            anchor_frame,
            anchor_row,
        }),
        Some(pos),
    ) = (&ix.drag, response.interact_pointer_pos())
    {
        let x0 = map.x(*anchor_frame);
        let r0 = row_rect(*anchor_row.min(&row_at(pos.y)));
        let r1 = row_rect(*anchor_row.max(&row_at(pos.y)));
        let band = egui::Rect::from_min_max(
            egui::pos2(x0.min(pos.x), r0.top()),
            egui::pos2(x0.max(pos.x), r1.bottom()),
        );
        painter.rect_filled(
            band,
            0.0,
            egui::Color32::from_rgba_premultiplied(60, 90, 140, 40),
        );
        painter.rect_stroke(
            band,
            0.0,
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(120, 160, 220)),
            egui::StrokeKind::Inside,
        );
    }

    // Centered playhead, continuing the zoomed view's.
    let center_x = rect.center().x;
    painter.line_segment(
        [
            egui::pos2(center_x, rect.top()),
            egui::pos2(center_x, rect.bottom()),
        ],
        egui::Stroke::new(2.0_f32, palette::PLAYHEAD),
    );

    committed
}
