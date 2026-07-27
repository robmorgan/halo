//! Direct-manipulation editor for the L3 show lanes: tall Look / Energy /
//! Accent rows sharing the zoomed view's frame→x mapping. Look events
//! drag as blocks (create-then-slide on empty space, armed palette look),
//! energy breakpoints drag in both axes, accents keep the legacy
//! draw/move/resize gestures. All positions are recomputed through the
//! [`FrameMap`] every frame, so editing stays correct while the timeline
//! plays underneath.
//!
//! Rubber-band selection is dropped for the preview (shift-click still
//! multi-selects); it can return with the full L3 landing.

use std::collections::HashSet;

use eframe::egui;

use super::zoomed::ZoomSpan;
use super::{FrameMap, GridMarks, lane_row_at, lane_rows, overlay_plan, palette, snap_frame};
use crate::show_preview::{ACCENT_LANE, LookId, ShowPreview, ShowSel};

/// Row heights, top to bottom: look / energy / accent. Energy is tallest
/// for y-drag resolution; accent keeps the legacy editor height.
const ROW_H: [f32; 3] = [36.0, 56.0, 44.0];
const STRIP_HEIGHT: f32 = ROW_H[0] + ROW_H[1] + ROW_H[2] + 2.0;
/// Pointer distance to an accent edge that counts as a resize grab.
const EDGE_GRAB_PX: f32 = 5.0;
/// Pointer distance to an energy breakpoint that counts as a grab.
const POINT_GRAB_PX: f32 = 6.0;
/// Smallest musical accent duration, in beats (0.1 s without a grid).
const MIN_DUR_BEATS: f64 = 0.25;
/// Minimum separation between look events, in beats (0.5 s w/o a grid).
const LOOK_SEP_BEATS: f64 = 1.0;
const BAR_INSET_Y: f32 = 4.0;
const MIN_BAR_W: f32 = 2.0;
/// Intensity of a freshly drawn accent.
const CREATE_INTENSITY: f32 = 0.8;
/// Vertical inset of the energy envelope within its row.
const ENERGY_INSET: f32 = 5.0;
/// Energy breakpoint dot radius.
const POINT_R: f32 = 3.0;

pub struct ShowEditorParams<'a> {
    pub marks: &'a GridMarks,
    pub position_frames: f64,
    pub total_frames: usize,
    pub sample_rate: u32,
    /// Snap creates/moves/resizes to the nearest beat.
    pub snap: bool,
    /// Palette look a fresh look event is created with.
    pub armed_look: LookId,
}

/// Drag state carried between frames.
#[derive(Default)]
pub struct ShowEditorInteraction {
    drag: Option<ShowDrag>,
}

enum ShowDrag {
    /// Slide a look event; `grab_offset` keeps the block under the
    /// pointer instead of jumping its start to it.
    LookMove {
        id: u64,
        grab_offset: f64,
    },
    /// Drag a breakpoint in both axes.
    EnergyMove {
        id: u64,
    },
    /// Draw a new accent between the anchor and the pointer.
    AccentCreate {
        id: u64,
        anchor: f64,
    },
    /// Move every selected accent by the pointer delta (original starts
    /// are kept so per-frame clamping never accumulates).
    AccentMove {
        pointer_start: f64,
        orig: Vec<(u64, f64)>,
    },
    AccentResizeL {
        id: u64,
    },
    AccentResizeR {
        id: u64,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Row {
    Look = 0,
    Energy = 1,
    Accent = 2,
}

enum Hit {
    LookBody(u64),
    EnergyPoint(u64),
    AccentEdgeL(u64),
    AccentEdgeR(u64),
    AccentBody(u64),
    Empty(Row),
}

/// Paints and edits the show lanes in place. Returns `true` when a
/// mutating gesture completed this frame — the caller syncs the show to
/// its sibling views then.
pub fn show_editor(
    ui: &mut egui::Ui,
    params: ShowEditorParams<'_>,
    span: &ZoomSpan,
    show: &mut ShowPreview,
    selection: &mut HashSet<ShowSel>,
    ix: &mut ShowEditorInteraction,
) -> bool {
    let desired_size = egui::vec2(ui.available_width(), STRIP_HEIGHT);
    let (response, painter) = ui.allocate_painter(desired_size, egui::Sense::click_and_drag());
    let rect = response.rect;
    let painter = painter.with_clip_rect(rect);

    painter.rect_filled(rect, 4.0, palette::LANE_BG);

    let rows = lane_rows(rect, ROW_H);
    for row in &rows[1..] {
        let y = row.top() - 0.5;
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0_f32, palette::LANE_SEPARATOR),
        );
    }

    let row_label = |row: usize, color: egui::Color32, alpha: f32| {
        let label = ["LOOK", "ENERGY", "ACCENT"][row];
        painter.text(
            egui::pos2(rows[row].left() + 6.0, rows[row].top() + 4.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::monospace(9.0),
            color.gamma_multiply(alpha),
        );
    };
    let row_colors = [palette::TEXT_DIM, palette::ENERGY, palette::ACCENT];

    let loaded = params.total_frames > 0;
    if !loaded {
        for (row, &color) in row_colors.iter().enumerate() {
            row_label(row, color, 0.4);
        }
        return false;
    }

    let span_frames = span.span_frames(params.marks, params.sample_rate);
    let map = FrameMap::new(rect, params.position_frames, span_frames);
    let total = params.total_frames as f64;
    let frame_at = |x: f32| map.start_frame() + (x - rect.left()) as f64 / map.px_per_frame();
    let snap = |frame: f64| snap_frame(params.marks, params.snap, frame).min(total);
    let beat = if params.marks.is_usable() && params.marks.median_beat_frames() > 0.0 {
        params.marks.median_beat_frames()
    } else {
        0.0
    };
    let min_dur = if beat > 0.0 {
        MIN_DUR_BEATS * beat
    } else {
        0.1 * params.sample_rate.max(1) as f64
    };
    let look_sep = if beat > 0.0 {
        LOOK_SEP_BEATS * beat
    } else {
        0.5 * params.sample_rate.max(1) as f64
    };

    // Energy row value↔y mapping.
    let energy_row = rows[Row::Energy as usize];
    let y_of = |v: f32| {
        energy_row.bottom()
            - ENERGY_INSET
            - v.clamp(0.0, 1.0) * (energy_row.height() - 2.0 * ENERGY_INSET)
    };
    let value_at_y = |y: f32| {
        ((energy_row.bottom() - ENERGY_INSET - y) / (energy_row.height() - 2.0 * ENERGY_INSET))
            .clamp(0.0, 1.0)
    };

    // Block end of the look event at `idx` within the visible slice.
    let look_block_end = |show: &ShowPreview, visible_idx: usize| -> f64 {
        let visible = show.looks.visible(map.start_frame(), map.end_frame());
        visible.get(visible_idx + 1).map_or_else(
            || {
                show.looks
                    .events()
                    .iter()
                    .find(|e| e.frame >= map.end_frame())
                    .map_or(total, |e| e.frame)
            },
            |n| n.frame,
        )
    };

    let hit_test = |show: &ShowPreview, pos: egui::Pos2| -> Hit {
        match lane_row_at(&rows, pos.y) {
            0 => {
                let visible = show.looks.visible(map.start_frame(), map.end_frame());
                for (i, ev) in visible.iter().enumerate() {
                    let x0 = map.x(ev.frame);
                    let x1 = map.x(look_block_end(show, i)).max(x0 + MIN_BAR_W);
                    if pos.x >= x0 && pos.x < x1 {
                        return Hit::LookBody(ev.id);
                    }
                }
                Hit::Empty(Row::Look)
            }
            1 => {
                // Nearest breakpoint dot within grab range wins.
                let mut best: Option<(f32, u64)> = None;
                for p in show.energy.points() {
                    let dot = egui::pos2(map.x(p.frame), y_of(p.value));
                    let d = dot.distance(pos);
                    if d <= POINT_GRAB_PX && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, p.id));
                    }
                }
                best.map_or(Hit::Empty(Row::Energy), |(_, id)| Hit::EnergyPoint(id))
            }
            _ => {
                // Edges win over bodies; later (topmost-drawn) cues win.
                let mut hit = Hit::Empty(Row::Accent);
                for c in show
                    .accents
                    .visible(ACCENT_LANE, map.start_frame(), map.end_frame())
                {
                    let x0 = map.x(c.start_frame);
                    let x1 = map.x(c.end_frame()).max(x0 + MIN_BAR_W);
                    if (pos.x - x0).abs() <= EDGE_GRAB_PX {
                        hit = Hit::AccentEdgeL(c.id);
                    } else if (pos.x - x1).abs() <= EDGE_GRAB_PX {
                        hit = Hit::AccentEdgeR(c.id);
                    } else if pos.x > x0 && pos.x < x1 {
                        hit = Hit::AccentBody(c.id);
                    }
                }
                hit
            }
        }
    };

    // Hover cursor feedback (only while not mid-drag).
    if ix.drag.is_none()
        && let Some(pos) = response.hover_pos()
    {
        match hit_test(show, pos) {
            Hit::AccentEdgeL(_) | Hit::AccentEdgeR(_) => {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            Hit::LookBody(_) | Hit::EnergyPoint(_) | Hit::AccentBody(_) => {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            Hit::Empty(_) => {}
        }
    }

    let mut committed = false;
    let modifiers = ui.input(|i| i.modifiers);

    if response.drag_started()
        && let Some(pos) = response.interact_pointer_pos()
    {
        ix.drag = match hit_test(show, pos) {
            Hit::LookBody(id) => {
                if !selection.contains(&ShowSel::Look(id)) {
                    selection.clear();
                    selection.insert(ShowSel::Look(id));
                }
                show.looks.find(id).map(|ev| ShowDrag::LookMove {
                    id,
                    grab_offset: frame_at(pos.x) - ev.frame,
                })
            }
            Hit::EnergyPoint(id) => {
                selection.clear();
                selection.insert(ShowSel::Energy(id));
                Some(ShowDrag::EnergyMove { id })
            }
            Hit::AccentBody(id) => {
                if !selection.contains(&ShowSel::Accent(id)) {
                    selection.clear();
                    selection.insert(ShowSel::Accent(id));
                }
                let mut orig: Vec<(u64, f64)> = selection
                    .iter()
                    .filter_map(|&sel| match sel {
                        ShowSel::Accent(id) => {
                            show.accents.find(id).map(|(_, c)| (id, c.start_frame))
                        }
                        _ => None,
                    })
                    .collect();
                orig.sort_by(|a, b| a.1.total_cmp(&b.1));
                Some(ShowDrag::AccentMove {
                    pointer_start: frame_at(pos.x),
                    orig,
                })
            }
            Hit::AccentEdgeL(id) => {
                selection.clear();
                selection.insert(ShowSel::Accent(id));
                Some(ShowDrag::AccentResizeL { id })
            }
            Hit::AccentEdgeR(id) => {
                selection.clear();
                selection.insert(ShowSel::Accent(id));
                Some(ShowDrag::AccentResizeR { id })
            }
            Hit::Empty(Row::Look) => {
                // Create with the armed look, then slide.
                let anchor = snap(frame_at(pos.x));
                show.looks
                    .insert(anchor, params.armed_look, look_sep)
                    .map(|id| {
                        selection.clear();
                        selection.insert(ShowSel::Look(id));
                        ShowDrag::LookMove {
                            id,
                            grab_offset: 0.0,
                        }
                    })
            }
            Hit::Empty(Row::Energy) => {
                // Add a breakpoint under the pointer, then slide.
                let id = show.energy.insert(snap(frame_at(pos.x)), value_at_y(pos.y));
                selection.clear();
                selection.insert(ShowSel::Energy(id));
                Some(ShowDrag::EnergyMove { id })
            }
            Hit::Empty(Row::Accent) => {
                let anchor = snap(frame_at(pos.x));
                show.accents
                    .insert(ACCENT_LANE, anchor, min_dur, CREATE_INTENSITY)
                    .map(|id| {
                        selection.clear();
                        selection.insert(ShowSel::Accent(id));
                        ShowDrag::AccentCreate { id, anchor }
                    })
            }
        };
    }

    if response.dragged()
        && let Some(pos) = response.interact_pointer_pos()
    {
        match &ix.drag {
            Some(ShowDrag::LookMove { id, grab_offset }) => {
                show.looks
                    .move_event(*id, snap(frame_at(pos.x) - grab_offset), look_sep);
            }
            Some(ShowDrag::EnergyMove { id }) => {
                show.energy
                    .move_point(*id, snap(frame_at(pos.x)), value_at_y(pos.y));
            }
            Some(ShowDrag::AccentCreate { id, anchor }) => {
                let p = snap(frame_at(pos.x));
                let (lo, hi) = if p < *anchor {
                    (p, *anchor)
                } else {
                    (*anchor, p)
                };
                show.accents.resize(*id, lo, hi.max(lo + min_dur));
            }
            Some(ShowDrag::AccentMove {
                pointer_start,
                orig,
            }) => {
                if let Some(&(_, first_start)) = orig.first() {
                    let raw_delta = frame_at(pos.x) - pointer_start;
                    let delta = snap(first_start + raw_delta) - first_start;
                    // Order matters so grouped accents don't clamp against
                    // a not-yet-moved neighbor: lead with the travel edge.
                    if delta >= 0.0 {
                        for &(id, start) in orig.iter().rev() {
                            show.accents.move_cue(id, start + delta);
                        }
                    } else {
                        for &(id, start) in orig.iter() {
                            show.accents.move_cue(id, start + delta);
                        }
                    }
                }
            }
            Some(ShowDrag::AccentResizeL { id }) => {
                if let Some((_, c)) = show.accents.find(*id) {
                    let end = c.end_frame();
                    show.accents
                        .resize(*id, snap(frame_at(pos.x)).min(end - min_dur), end);
                }
            }
            Some(ShowDrag::AccentResizeR { id }) => {
                if let Some((_, c)) = show.accents.find(*id) {
                    let start = c.start_frame;
                    show.accents
                        .resize(*id, start, snap(frame_at(pos.x)).max(start + min_dur));
                }
            }
            None => {}
        }
    }

    if response.drag_stopped() && ix.drag.take().is_some() {
        committed = true;
    }

    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let sel = match hit_test(show, pos) {
            Hit::LookBody(id) => Some(ShowSel::Look(id)),
            Hit::EnergyPoint(id) => Some(ShowSel::Energy(id)),
            Hit::AccentBody(id) | Hit::AccentEdgeL(id) | Hit::AccentEdgeR(id) => {
                Some(ShowSel::Accent(id))
            }
            Hit::Empty(_) => None,
        };
        match sel {
            Some(sel) => {
                if modifiers.shift {
                    if !selection.remove(&sel) {
                        selection.insert(sel);
                    }
                } else {
                    selection.clear();
                    selection.insert(sel);
                }
            }
            None => selection.clear(),
        }
    }

    // Secondary click deletes an energy breakpoint outright.
    if response.secondary_clicked()
        && let Some(pos) = response.interact_pointer_pos()
        && let Hit::EnergyPoint(id) = hit_test(show, pos)
    {
        show.energy.remove(id);
        selection.remove(&ShowSel::Energy(id));
        committed = true;
    }

    // Drop selection entries whose items no longer exist.
    selection.retain(|&sel| match sel {
        ShowSel::Look(id) => show.looks.find(id).is_some(),
        ShowSel::Energy(id) => show.energy.find(id).is_some(),
        ShowSel::Accent(id) => show.accents.find(id).is_some(),
    });

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

    // Look blocks.
    let rr = rows[Row::Look as usize];
    let visible: Vec<_> = show
        .looks
        .visible(map.start_frame(), map.end_frame())
        .to_vec();
    for (i, ev) in visible.iter().enumerate() {
        let color = ev.look.def().color;
        let x0 = map.x(ev.frame);
        let x1 = map.x(look_block_end(show, i)).max(x0 + MIN_BAR_W);
        let block = egui::Rect::from_min_max(
            egui::pos2(x0, rr.top() + BAR_INSET_Y),
            egui::pos2(x1, rr.bottom() - BAR_INSET_Y),
        );
        painter.rect_filled(block, 3.0, color.gamma_multiply(0.30));
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x0, block.top()),
                egui::pos2(x0 + 2.5, block.bottom()),
            ),
            0.0,
            color.gamma_multiply(0.95),
        );
        // Pin carried-in block names to the visible edge, clear of the
        // row label.
        let label_x = (x0 + 7.0).max(rect.left() + 52.0);
        painter.text(
            egui::pos2(label_x, block.center().y),
            egui::Align2::LEFT_CENTER,
            ev.look.def().name,
            egui::FontId::monospace(9.0),
            egui::Color32::WHITE.gamma_multiply(0.8),
        );
        if selection.contains(&ShowSel::Look(ev.id)) {
            painter.rect_stroke(
                block,
                3.0,
                egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
                egui::StrokeKind::Outside,
            );
        }
    }

    // Energy envelope: fill, line, then draggable dots.
    let energy = &show.energy;
    if energy.points().is_empty() {
        painter.line_segment(
            [
                egui::pos2(rect.left(), y_of(1.0)),
                egui::pos2(rect.right(), y_of(1.0)),
            ],
            egui::Stroke::new(1.0_f32, palette::ENERGY.gamma_multiply(0.3)),
        );
    } else {
        let mut xs: Vec<(f32, f32)> = vec![(rect.left(), energy.value_at(map.start_frame()))];
        for p in energy.points() {
            if p.frame > map.start_frame() && p.frame < map.end_frame() {
                xs.push((map.x(p.frame), p.value));
            }
        }
        xs.push((rect.right(), energy.value_at(map.end_frame())));

        // One convex trapezoid per segment — epaint's filled paths assume
        // convexity, which a multi-breakpoint envelope doesn't satisfy.
        let fill = palette::ENERGY.gamma_multiply(0.12);
        let mut mesh = egui::Mesh::default();
        for w in xs.windows(2) {
            let (x0, v0) = w[0];
            let (x1, v1) = w[1];
            let i = mesh.vertices.len() as u32;
            for (x, y) in [
                (x0, y_of(v0)),
                (x1, y_of(v1)),
                (x1, energy_row.bottom() - ENERGY_INSET),
                (x0, energy_row.bottom() - ENERGY_INSET),
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
            egui::Stroke::new(1.5_f32, palette::ENERGY.gamma_multiply(0.9)),
        ));
        for p in energy.points() {
            let dot = egui::pos2(map.x(p.frame), y_of(p.value));
            if dot.x < rect.left() || dot.x > rect.right() {
                continue;
            }
            painter.circle_filled(dot, POINT_R, palette::ENERGY);
            if selection.contains(&ShowSel::Energy(p.id)) {
                painter.circle_stroke(
                    dot,
                    POINT_R + 1.5,
                    egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
                );
            }
        }
    }

    // Accent bars.
    let rr = rows[Row::Accent as usize];
    for c in show
        .accents
        .visible(ACCENT_LANE, map.start_frame(), map.end_frame())
    {
        let x0 = map.x(c.start_frame);
        let x1 = map.x(c.end_frame()).max(x0 + MIN_BAR_W);
        let bar = egui::Rect::from_min_max(
            egui::pos2(x0, rr.top() + BAR_INSET_Y),
            egui::pos2(x1, rr.bottom() - BAR_INSET_Y),
        );
        painter.rect_filled(
            bar,
            3.0,
            palette::ACCENT.gamma_multiply(0.45 + 0.45 * c.intensity.clamp(0.0, 1.0)),
        );
        if selection.contains(&ShowSel::Accent(c.id)) {
            painter.rect_stroke(
                bar,
                3.0,
                egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
                egui::StrokeKind::Outside,
            );
        }
    }

    for (row, &color) in row_colors.iter().enumerate() {
        row_label(row, color, 0.6);
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
