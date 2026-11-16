//! The PATCH overview's LAYOUT: the 8 × 8 stage the programmer's fixture
//! grid mirrors, upstage at the top and the audience below.
//!
//! Each fixture is a key in its cell ([`fixture::layout`]): placed by
//! hand, or arranged automatically by kind, kinds sharing a row where
//! they fit. Dragging one moves
//! it cell by cell under the pointer and places it there; over another
//! fixture, that one shows in the dragged one's old cell, and dropping
//! swaps them. Esc puts both back. Arrow keys nudge the focused fixture
//! a cell, swapping the same way. AUTO ARRANGE clears every hand-placed
//! cell. All of it edits the draft rig, like the rest of the view.

use eframe::egui::{
    self, Align, Align2, CursorIcon, EventFilter, Layout, Modifiers, Pos2, Rect, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};
use halo_light::fixture::{self, Cell, Conflicts, Rig, STAGE_COLS, STAGE_ROWS};

use super::{EDGE_STRONG, PatchView, UNPATCHED_OPACITY, WASH};
use crate::footer::StatusHelp;
use crate::key::Key;
use crate::theme;

/// The stage's accessible name.
pub(super) const STAGE_LABEL: &str = "Stage layout, upstage at the top";

/// The header row: STAGE LAYOUT, the count, AUTO ARRANGE.
const HEAD_H: f32 = theme::SIZE_KEY_SMALL;
/// The AUDIENCE caption's row under the stage.
const AUDIENCE_H: f32 = 12.0;
/// A fixture's key inside its cell.
const TOKEN_INSET: Vec2 = vec2(4.0, 6.0);
/// The label's start, past the kind's tick.
const LABEL_LEFT: f32 = 8.0;

/// A fixture being placed on the stage.
#[derive(Clone, Copy)]
pub(super) struct StageDrag {
    id: u32,
    /// The cell it was picked up from, and its hand-placed cell then.
    from: Cell,
    from_pos: Option<Cell>,
    /// The cell it's over.
    over: Cell,
    /// The fixture moved into `from` to make room, and its hand-placed
    /// cell before.
    displaced: Option<(u32, Option<Cell>)>,
}

/// The stage in `rect`. Returns whether the draft changed.
pub(super) fn stage_ui(
    ui: &mut Ui,
    rect: Rect,
    view: &mut PatchView,
    rig: &mut Rig,
    conflicts: &Conflicts,
) -> bool {
    if view
        .stage_drag
        .is_some_and(|d| !rig.ids().any(|id| id == d.id))
    {
        view.stage_drag = None;
    }
    let painter = ui.painter().clone();
    theme::paint_well(&painter, rect, theme::RADIUS_CONTROL);
    let inner = rect.shrink(theme::SPACE_8);
    let mut changed = false;

    // Header: what's placed by hand, and AUTO ARRANGE.
    let head = Rect::from_min_size(inner.min, vec2(inner.width(), HEAD_H));
    theme::paint_text(
        &painter,
        head.left_center(),
        Align2::LEFT_CENTER,
        "STAGE LAYOUT",
        theme::CAPTION,
        theme::TEXT_MUTED,
    );
    let placed = rig.placed();
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(head)
            .layout(Layout::right_to_left(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let help = if placed == 0 {
                "Every fixture is arranged automatically"
            } else {
                "Clear every hand-placed position and arrange the rig automatically"
            };
            if ui
                .add(Key::new("AUTO ARRANGE").small().enabled(placed > 0))
                .status_help(help)
                .clicked()
            {
                for f in rig.fixtures_mut() {
                    f.pos = None;
                }
                view.stage_drag = None;
                changed = true;
            }
            let count = match placed {
                0 => "Arranged automatically".to_string(),
                1 => "1 fixture placed by hand".to_string(),
                n => format!("{n} fixtures placed by hand"),
            };
            ui.label(theme::NOTE_TEXT.rich(count).color(theme::TEXT_MUTED));
        },
    );

    // The stage, and the audience below it.
    let audience = Rect::from_min_max(pos2(inner.left(), inner.bottom() - AUDIENCE_H), inner.max);
    let stage = Stage {
        rect: Rect::from_min_max(
            pos2(inner.left(), head.bottom() + theme::SPACE_6),
            pos2(inner.right(), audience.top() - theme::SPACE_6),
        ),
    };
    let cell = stage.cell();
    for i in 1..STAGE_COLS {
        let x = stage.rect.left() + f32::from(i) * cell.x;
        painter.vline(
            x,
            stage.rect.y_range(),
            Stroke::new(1.0, theme::QUIET_BEAT_LINE),
        );
    }
    for i in 1..STAGE_ROWS {
        let y = stage.rect.top() + f32::from(i) * cell.y;
        painter.hline(
            stage.rect.x_range(),
            y,
            Stroke::new(1.0, theme::QUIET_BEAT_LINE),
        );
    }
    painter.rect_stroke(
        stage.rect,
        0.0,
        Stroke::new(1.0, theme::EDGE_PANEL),
        StrokeKind::Inside,
    );
    theme::paint_text(
        &painter,
        audience.center(),
        Align2::CENTER_CENTER,
        "AUDIENCE",
        theme::CAPTION,
        theme::TEXT_MUTED,
    );

    // Dragging: one sense over the stage, under the fixtures' keys,
    // which take the clicks.
    let id = ui.scope_id().with("patch-stage");
    let response = ui.interact(stage.rect, id, Sense::drag());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Grid, true, STAGE_LABEL));
    let mut focus = None;
    if response.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
        && let Some(id) = occupant(rig, stage.cell_at(origin), None)
        && let Some(f) = rig.iter().find(|f| f.id == id)
    {
        let from = stage.cell_at(origin);
        view.stage_drag = Some(StageDrag {
            id,
            from,
            from_pos: f.pos,
            over: from,
            displaced: None,
        });
        view.select(f);
        view.scroll_to = Some(id);
        focus = Some(id);
    }
    if let Some(mut d) = view.stage_drag {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            put_back(rig, &mut d);
            set_pos(rig, d.id, d.from_pos);
            view.stage_drag = None;
            changed = true;
        } else {
            if response.dragged()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let to = stage.cell_at(pos);
                if to != d.over {
                    move_to(rig, &mut d, to);
                    view.stage_drag = Some(d);
                    changed = true;
                }
            }
            if response.drag_stopped() {
                view.stage_drag = None;
            }
        }
    }

    // The fixtures, each a key in its cell.
    let cells = fixture::layout(rig);
    let drag = view.stage_drag;
    let placing = drag
        .and_then(|d| rig.iter().find(|f| f.id == d.id))
        .map(|f| {
            format!(
                "Placing {} — release to drop; dropping on another fixture swaps them",
                f.label
            )
        });
    let mut hovered = false;
    let mut nudge = None;
    let mut select = None;
    for f in rig.iter() {
        let Some(&at) = cells.get(&f.id) else {
            continue;
        };
        let r = stage.cell_rect(at).shrink2(TOKEN_INSET);
        let dragging = drag.is_some_and(|d| d.id == f.id);
        let selected = view.selected == Some(f.id);
        let bad = conflicts.contains(&f.id);
        let token = ui
            .scope_builder(
                UiBuilder::new()
                    .id_salt(("stage-fixture", f.id))
                    .max_rect(r),
                |ui| {
                    if f.unpatched {
                        ui.multiply_opacity(UNPATCHED_OPACITY);
                    }
                    let token = ui.add(Key::new("").size(r.size()).pressed(dragging));
                    paint_token(ui, r, f, dragging || selected, bad);
                    token
                },
            )
            .inner;
        let token = token.accessible_name(format!(
            "{}, {}, column {}, row {}",
            f.label,
            if f.pos.is_some() {
                "placed by hand"
            } else {
                "arranged automatically"
            },
            at.col + 1,
            at.row + 1
        ));
        hovered |= token.hovered();
        if focus == Some(f.id) || token.clicked() {
            token.request_focus();
        }
        if token.clicked() {
            select = Some(f.id);
        }
        if token.has_focus() {
            // The arrows nudge it instead of moving focus.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    token.id,
                    EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                );
            });
            let step = ui.input_mut(|i| {
                [
                    (egui::Key::ArrowLeft, (-1, 0)),
                    (egui::Key::ArrowRight, (1, 0)),
                    (egui::Key::ArrowUp, (0, -1)),
                    (egui::Key::ArrowDown, (0, 1)),
                ]
                .into_iter()
                .find(|&(key, _)| i.consume_key(Modifiers::NONE, key))
                .map(|(_, step)| step)
            });
            if let Some(step) = step {
                nudge = Some((f.id, at, step));
            }
        }
        let help = placing.clone().unwrap_or_else(|| {
            format!(
                "{} on {}.{:03} — drag to place it on the stage, arrow keys to nudge",
                f.label, f.universe, f.start_address
            )
        });
        token.status_help(help);
    }
    if let Some(id) = select
        && let Some(f) = rig.iter().find(|f| f.id == id)
    {
        view.select(f);
        view.scroll_to = Some(id);
    }
    if let Some((id, at, (dx, dy))) = nudge {
        let to = Cell::new(
            (i16::from(at.col) + dx).clamp(0, i16::from(STAGE_COLS) - 1) as u8,
            (i16::from(at.row) + dy).clamp(0, i16::from(STAGE_ROWS) - 1) as u8,
        );
        if to != at {
            if let Some(other) = occupant(rig, to, Some(id)) {
                set_pos(rig, other, Some(at));
            }
            set_pos(rig, id, Some(to));
            if let Some(f) = rig.iter().find(|f| f.id == id) {
                view.select(f);
            }
            changed = true;
        }
    }

    if drag.is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    } else if hovered {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }
    response.status_help(placing.unwrap_or_else(|| {
        "Layout is optional: drag fixtures to mirror your stage — anything you leave alone is \
         arranged automatically"
            .to_string()
    }));
    changed
}

/// A fixture key's tick, outline and labels, over the key's face.
fn paint_token(ui: &Ui, r: Rect, f: &fixture::Fixture, lit: bool, bad: bool) {
    let painter = ui.painter();
    let (edge, width) = if lit {
        (Some(theme::ACCENT), EDGE_STRONG)
    } else if bad {
        (Some(theme::WARN), EDGE_STRONG)
    } else {
        (None, 1.0)
    };
    if let Some(edge) = edge {
        painter.rect_filled(r, theme::RADIUS_CONTROL, edge.gamma_multiply(WASH));
        painter.rect_stroke(
            r,
            theme::RADIUS_CONTROL,
            Stroke::new(width, edge),
            StrokeKind::Inside,
        );
    }
    let [cr, cg, cb] = f.kind.color();
    painter.rect_filled(
        Rect::from_min_size(
            r.min + vec2(width, width),
            vec2(2.0, r.height() - 2.0 * width),
        ),
        0.0,
        egui::Color32::from_rgb(cr, cg, cb),
    );
    let name = if bad {
        theme::WARN
    } else if lit {
        theme::ACCENT
    } else {
        theme::TEXT
    };
    let left = r.left() + LABEL_LEFT;
    let room = r.right() - left - 3.0;
    let ctx = ui.ctx();
    let label = theme::truncated(ctx, &f.label, theme::KEY_LABEL, name, room);
    let address = format!("{}.{:03}", f.universe, f.start_address);
    let address = theme::truncated(
        ctx,
        &address,
        theme::MONO_TEXT.sized(10.0),
        theme::TEXT_MUTED,
        room,
    );
    let gap = 2.0;
    let total = label.size().y + gap + address.size().y;
    let top = r.center().y - total / 2.0;
    let clip = painter.with_clip_rect(r.shrink(1.0));
    let label_h = label.size().y;
    clip.galley(pos2(left, top), label, name);
    clip.galley(pos2(left, top + label_h + gap), address, theme::TEXT_MUTED);
}

/// The fixture on `cell`, leaving `skip` out.
fn occupant(rig: &Rig, cell: Cell, skip: Option<u32>) -> Option<u32> {
    fixture::layout(rig)
        .into_iter()
        .find(|&(id, c)| c == cell && Some(id) != skip)
        .map(|(id, _)| id)
}

fn set_pos(rig: &mut Rig, id: u32, pos: Option<Cell>) {
    if let Some(f) = rig.fixtures_mut().iter_mut().find(|f| f.id == id) {
        f.pos = pos;
    }
}

/// Return the fixture that made room to where it was.
fn put_back(rig: &mut Rig, d: &mut StageDrag) {
    if let Some((id, pos)) = d.displaced.take() {
        set_pos(rig, id, pos);
    }
}

/// Move the dragged fixture onto `to`: back where it started puts it as
/// it was; onto another fixture moves that one into its old cell, until
/// the drag moves on.
fn move_to(rig: &mut Rig, d: &mut StageDrag, to: Cell) {
    put_back(rig, d);
    if to == d.from {
        set_pos(rig, d.id, d.from_pos);
    } else {
        // Who's on `to` with the dragged fixture back on its own cell.
        set_pos(rig, d.id, Some(d.from));
        if let Some(other) = occupant(rig, to, Some(d.id)) {
            let before = rig.iter().find(|f| f.id == other).and_then(|f| f.pos);
            d.displaced = Some((other, before));
            set_pos(rig, other, Some(d.from));
        }
        set_pos(rig, d.id, Some(to));
    }
    d.over = to;
}

/// The stage's geometry: cell to screen and back.
#[derive(Clone, Copy)]
struct Stage {
    rect: Rect,
}

impl Stage {
    fn cell(&self) -> Vec2 {
        vec2(
            self.rect.width() / f32::from(STAGE_COLS),
            self.rect.height() / f32::from(STAGE_ROWS),
        )
    }

    fn cell_rect(&self, c: Cell) -> Rect {
        let size = self.cell();
        Rect::from_min_size(
            self.rect.min + vec2(f32::from(c.col) * size.x, f32::from(c.row) * size.y),
            size,
        )
    }

    /// The cell under `pos`, clamped to the stage.
    fn cell_at(&self, pos: Pos2) -> Cell {
        let size = self.cell();
        let col = ((pos.x - self.rect.left()) / size.x)
            .floor()
            .clamp(0.0, f32::from(STAGE_COLS - 1));
        let row = ((pos.y - self.rect.top()) / size.y)
            .floor()
            .clamp(0.0, f32::from(STAGE_ROWS - 1));
        Cell::new(col as u8, row as u8)
    }
}
