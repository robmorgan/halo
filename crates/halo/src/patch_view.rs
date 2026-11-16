//! The PATCH view below its action row: the patch sheet on the left and
//! the universe overview on the right.
//!
//! The overview has a pad per universe (patched/512 and an occupancy
//! strip), the universe showing as a 32×16 channel map, and a well for
//! the selected fixture. Fixtures on the map are keys spanning their
//! footprint, wrapping across rows: drag one to change its start address
//! a channel at a time, or onto a pad to move it to that universe.
//! Overlapping channels are hatched in `warn`.
//!
//! Every edit goes to the draft rig the caller passes; APPLY and REVERT
//! stay with the action row in `app.rs`. The address logic is
//! `halo_light::fixture`'s ([`Rig::occupancy`], [`Rig::first_free`],
//! [`Rig::conflicts`]).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use eframe::egui::{
    self, Align2, Color32, CursorIcon, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui,
    UiBuilder, Vec2, pos2, vec2,
};
use halo_light::fixture::{ALL_KINDS, Conflict, Conflicts, Fixture, Occupancy, Rig, Span};
use halo_light::fixture_library::FixtureLibrary;

use crate::footer::StatusHelp;
use crate::key::{self, Key};
use crate::theme::{self, Face, Top};

/// The map: 32 channels a row, 16 rows.
const COLS: u16 = 32;
const ROWS: u16 = 16;
/// Room left of the map for each row's first address.
const GUTTER: f32 = 38.0;
/// A universe pad, at most.
const PAD_W: f32 = 104.0;
/// The selection well's height.
const WELL_H: f32 = 44.0;
/// A sheet row's height.
const ROW_H: f32 = 28.0;
/// A selected, dragged or conflicting block's edge, and its wash.
const EDGE_STRONG: f32 = 1.5;
/// A block's profile name: its gap after the fixture's, and the least
/// room worth showing it in.
const PROFILE_GAP: f32 = 6.0;
const PROFILE_MIN_ROOM: f32 = 28.0;
const WASH: f32 = 0.08;

/// What the PATCH view remembers between frames, for the session.
pub struct PatchView {
    /// The universe the map shows.
    universe: u8,
    selected: Option<u32>,
    drag: Option<Drag>,
    /// Scroll the sheet to this fixture's row next frame.
    scroll_to: Option<u32>,
}

impl Default for PatchView {
    fn default() -> Self {
        PatchView {
            universe: 1,
            selected: None,
            drag: None,
            scroll_to: None,
        }
    }
}

/// A fixture being dragged on the map.
#[derive(Clone, Copy)]
struct Drag {
    id: u32,
    /// Channels from the fixture's start to the cell it was grabbed by.
    grab: u16,
    from_universe: u8,
    from_address: u16,
}

impl PatchView {
    /// Select `f` and show its universe.
    fn select(&mut self, f: &Fixture) {
        self.selected = Some(f.id);
        self.universe = f.universe;
    }
}

/// The sheet and the overview, filling what's left of `ui`. Returns
/// whether the draft changed.
pub fn show(
    ui: &mut Ui,
    view: &mut PatchView,
    rig: &mut Rig,
    library: &FixtureLibrary,
    conflicts: &Conflicts,
) -> bool {
    if view.selected.is_some_and(|id| !rig.ids().any(|i| i == id)) {
        view.selected = None;
    }
    let full = ui.available_rect_before_wrap();
    let gap = theme::SPACE_12;
    // The design's 5:6, keeping the sheet's columns and a usable map.
    let sheet_w = ((full.width() - gap) * 5.0 / 11.0)
        .max(SHEET_MIN_W)
        .min(full.width() - gap - OVERVIEW_MIN_W)
        .max(0.0);
    let sheet = Rect::from_min_size(full.min, vec2(sheet_w, full.height()));
    let overview = Rect::from_min_max(pos2(sheet.right() + gap, full.top()), full.max);

    let mut changed = false;
    ui.scope_builder(UiBuilder::new().max_rect(sheet), |ui| {
        changed |= sheet_ui(ui, view, rig, library, conflicts);
    });
    ui.scope_builder(UiBuilder::new().max_rect(overview), |ui| {
        changed |= overview_ui(ui, view, rig, library, conflicts);
    });
    ui.advance_cursor_after_rect(full);
    changed
}

// ---- The patch sheet ----

/// Column widths left to right; PROFILE takes what's left.
const COL_FIXTURE: f32 = 56.0;
const COL_KIND: f32 = 100.0;
const COL_MODE: f32 = 76.0;
const COL_CH: f32 = 28.0;
const COL_UNIV: f32 = 36.0;
const COL_ADDR: f32 = 44.0;
const COL_GRID: f32 = 30.0;
const COL_REMOVE: f32 = 24.0;
const COL_GAP: f32 = 8.0;
const SHEET_PAD: f32 = 8.0;
const PROFILE_MIN_W: f32 = 200.0;
/// Everything but PROFILE, gaps and padding included.
const SHEET_FIXED_W: f32 = COL_FIXTURE
    + COL_KIND
    + COL_MODE
    + COL_CH
    + COL_UNIV
    + COL_ADDR
    + 2.0 * COL_GRID
    + COL_REMOVE
    + 9.0 * COL_GAP
    + 2.0 * SHEET_PAD;
const SHEET_MIN_W: f32 = SHEET_FIXED_W + PROFILE_MIN_W;
const OVERVIEW_MIN_W: f32 = 420.0;

fn sheet_ui(
    ui: &mut Ui,
    view: &mut PatchView,
    rig: &mut Rig,
    library: &FixtureLibrary,
    conflicts: &Conflicts,
) -> bool {
    // Stable, name-sorted profile list for the combos.
    let mut profiles: Vec<(&String, String)> = library
        .profiles
        .iter()
        .map(|(id, p)| (id, p.to_string()))
        .collect();
    profiles.sort_by(|a, b| a.1.cmp(&b.1));

    let rect = ui.max_rect();
    theme::paint_well(ui.painter(), rect, theme::RADIUS_CONTROL);
    let profile_w = (rect.width() - SHEET_FIXED_W).max(PROFILE_MIN_W.min(rect.width()));
    let widths = [
        COL_FIXTURE,
        COL_KIND,
        profile_w,
        COL_MODE,
        COL_CH,
        COL_UNIV,
        COL_ADDR,
        COL_GRID,
        COL_GRID,
        COL_REMOVE,
    ];

    // The header stays put above the rows.
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), ROW_H));
    ui.painter().hline(
        header.x_range(),
        header.bottom() - 0.5,
        Stroke::new(1.0, theme::EDGE_PANEL),
    );
    let mut x = header.left() + SHEET_PAD;
    for (h, w) in [
        "FIXTURE", "KIND", "PROFILE", "MODE", "CH", "UNIV", "ADDR", "COL", "ROW", "",
    ]
    .iter()
    .zip(widths)
    {
        let (pos, align) = if *h == "CH" {
            (pos2(x + w, header.center().y), Align2::RIGHT_CENTER)
        } else {
            (pos2(x, header.center().y), Align2::LEFT_CENTER)
        };
        theme::paint_text(
            ui.painter(),
            pos,
            align,
            h,
            theme::CAPTION,
            theme::TEXT_MUTED,
        );
        x += w + COL_GAP;
    }

    let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
    let names = labels(rig);
    let mut changed = false;
    let mut remove = None;
    let mut select = None;
    ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for (i, f) in rig.fixtures_mut().iter_mut().enumerate() {
                    let (row, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
                    let selected = view.selected == Some(f.id);
                    if selected {
                        ui.painter().rect_filled(row, 0.0, theme::SELECTION);
                    } else if i % 2 == 1 {
                        ui.painter().rect_filled(row, 0.0, theme::QUIET_HAIRLINE);
                    }
                    if view.scroll_to == Some(f.id) {
                        ui.scroll_to_rect(row, None);
                        view.scroll_to = None;
                    }
                    let cells = row.shrink2(vec2(SHEET_PAD, 0.0));
                    let touched = ui
                        .scope_builder(
                            UiBuilder::new()
                                .max_rect(cells)
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = COL_GAP;
                                // Number fields take their column's width,
                                // under egui's 40 minimum.
                                ui.spacing_mut().interact_size.x = COL_GRID;
                                sheet_row(
                                    ui,
                                    f,
                                    &widths,
                                    library,
                                    &profiles,
                                    conflicts.get(f.id),
                                    &names,
                                    selected,
                                    &mut changed,
                                    &mut remove,
                                )
                            },
                        )
                        .inner;
                    if touched {
                        select = Some(f.id);
                    }
                }
            });
    });
    if let Some(id) = select
        && let Some(f) = rig.iter().find(|f| f.id == id)
    {
        view.select(f);
    }
    if let Some(id) = remove {
        rig.fixtures_mut().retain(|f| f.id != id);
        changed = true;
    }
    changed
}

/// One fixture's row of editors. Returns whether any was used, which
/// selects the fixture.
#[allow(clippy::too_many_arguments)]
fn sheet_row(
    ui: &mut Ui,
    f: &mut Fixture,
    widths: &[f32; 10],
    library: &FixtureLibrary,
    profiles: &[(&String, String)],
    conflict: Option<&Conflict>,
    names: &Labels,
    selected: bool,
    changed: &mut bool,
    remove: &mut Option<u32>,
) -> bool {
    let h = theme::SIZE_KEY_SMALL;
    let mut touched = false;
    let mut note = |r: &Response| touched |= r.clicked() || r.changed() || r.drag_started();

    // FIXTURE: the kind's tick, ⚠ when in conflict, the label.
    let (rect, label) = ui.allocate_exact_size(vec2(widths[0], h), Sense::click());
    let color = if conflict.is_some() {
        theme::WARN
    } else if selected || label.hovered() {
        theme::TEXT_STRONG
    } else {
        theme::TEXT
    };
    let painter = ui.painter();
    let [r, g, b] = f.kind.color();
    painter.rect_filled(
        Rect::from_center_size(pos2(rect.left() + 1.0, rect.center().y), vec2(2.0, 14.0)),
        0.0,
        Color32::from_rgb(r, g, b),
    );
    let text = if conflict.is_some() {
        format!("⚠ {}", f.label)
    } else {
        f.label.clone()
    };
    let galley = theme::truncated(ui.ctx(), &text, theme::BODY_TEXT, color, rect.width() - 8.0);
    painter.galley(
        pos2(rect.left() + 8.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    let help = match conflict {
        Some(c) => conflict_text(f, c, names),
        None => format!("Show {} in the universe overview", f.label),
    };
    note(&label.status_help(help));

    // KIND
    let r = cell(ui, widths[1], |ui| {
        egui::ComboBox::from_id_salt(("patch-kind", f.id))
            .selected_text(f.kind.group_label())
            .width(widths[1])
            .truncate()
            .show_ui(ui, |ui| {
                for kind in ALL_KINDS {
                    if ui
                        .selectable_label(f.kind == kind, kind.group_label())
                        .clicked()
                        && f.kind != kind
                    {
                        f.kind = kind;
                        *changed = true;
                    }
                }
            })
    });
    note(&r.response);

    // PROFILE
    let profile_text = library
        .get(&f.profile_id)
        .map(|p| p.to_string())
        .unwrap_or_else(|| format!("? {}", f.profile_id));
    let r = cell(ui, widths[2], |ui| {
        egui::ComboBox::from_id_salt(("patch-profile", f.id))
            .selected_text(profile_text)
            .width(widths[2])
            .truncate()
            .show_ui(ui, |ui| {
                for (pid, name) in profiles {
                    if ui.selectable_label(&f.profile_id == *pid, name).clicked()
                        && &f.profile_id != *pid
                    {
                        f.profile_id = (*pid).clone();
                        // The new profile's first mode.
                        f.mode.clear();
                        *changed = true;
                    }
                }
            })
    });
    note(&r.response);

    // MODE: the profile's DMX modes, one for most fixtures.
    let modes: Vec<String> = library
        .get(&f.profile_id)
        .map(|p| p.modes.iter().map(|m| m.name.clone()).collect())
        .unwrap_or_default();
    let current = library
        .layout(f)
        .map_or_else(|| "—".to_string(), |m| m.name.clone());
    let r = cell(ui, widths[3], |ui| {
        egui::ComboBox::from_id_salt(("patch-mode", f.id))
            .selected_text(&current)
            .width(widths[3])
            .truncate()
            .show_ui(ui, |ui| {
                for name in &modes {
                    if ui.selectable_label(&current == name, name).clicked() && &current != name {
                        f.mode = name.clone();
                        *changed = true;
                    }
                }
            })
    });
    note(&r.response);

    // CH: the footprint, read-only.
    let (rect, _) = ui.allocate_exact_size(vec2(widths[4], h), Sense::hover());
    let fp = library.layout(f).map(|m| m.footprint().to_string());
    theme::paint_text(
        ui.painter(),
        rect.right_center(),
        Align2::RIGHT_CENTER,
        fp.as_deref().unwrap_or("—"),
        theme::MONO_TEXT,
        theme::TEXT_MUTED,
    );

    // UNIV, ADDR: DMX numbers, the one place JetBrains Mono appears.
    ui.scope(|ui| {
        ui.style_mut().drag_value_text_style = egui::TextStyle::Monospace;
        let r = ui.add_sized(
            [widths[5], h],
            egui::DragValue::new(&mut f.universe).range(1..=32),
        );
        *changed |= r.changed();
        note(&r);
        if conflict.is_some() {
            ui.visuals_mut().override_text_color = Some(theme::WARN);
        }
        let r = ui.add_sized(
            [widths[6], h],
            egui::DragValue::new(&mut f.start_address).range(1..=512),
        );
        *changed |= r.changed();
        note(&r);
    });

    // COL, ROW on the programmer's grid.
    for (v, w) in [(&mut f.col, widths[7]), (&mut f.row, widths[8])] {
        let r = ui.add_sized([w, h], egui::DragValue::new(v).range(0..=15));
        *changed |= r.changed();
        note(&r);
    }

    if ui
        .add(Key::new("−").small().width(widths[9]))
        .status_help("Unpatch")
        .clicked()
    {
        *remove = Some(f.id);
    }
    touched
}

/// A column's cell, exactly `w` wide: its own box, so a combo's text
/// truncates to the column instead of widening it.
fn cell<R>(ui: &mut Ui, w: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let (rect, _) = ui.allocate_exact_size(vec2(w, theme::SIZE_KEY_SMALL), Sense::hover());
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(rect.expand(2.0)));
            ui.spacing_mut().item_spacing.x = theme::SPACE_4;
            add(ui)
        },
    )
    .inner
}

/// Each fixture's label, by id.
type Labels = HashMap<u32, String>;

fn labels(rig: &Rig) -> Labels {
    rig.iter().map(|f| (f.id, f.label.clone())).collect()
}

/// Why `f` is in conflict, in a sentence: "P3 overlaps P4 on 089–096".
fn conflict_text(f: &Fixture, c: &Conflict, rig: &Labels) -> String {
    if let Some((first, last)) = c.range() {
        format!(
            "{} overlaps {} on {first:03}–{last:03}",
            f.label,
            names(c, rig)
        )
    } else if c.past_end > 0 {
        format!("{} runs {} channels past 512", f.label, c.past_end)
    } else {
        format!("{}'s profile isn't in the fixture library", f.label)
    }
}

/// The labels of the fixtures `c` overlaps, in rig order.
fn names(c: &Conflict, rig: &Labels) -> String {
    c.overlaps
        .iter()
        .map(|o| rig.get(&o.other).cloned().unwrap_or_default())
        .collect::<Vec<_>>()
        .join(", ")
}

// ---- The universe overview ----

fn overview_ui(
    ui: &mut Ui,
    view: &mut PatchView,
    rig: &mut Rig,
    library: &FixtureLibrary,
    conflicts: &Conflicts,
) -> bool {
    let rect = ui.max_rect();
    let occupancy = rig.occupancy(library);
    let universes: BTreeSet<u8> = rig
        .iter()
        .map(|f| f.universe)
        .chain([view.universe])
        .collect();
    let spans: Vec<(u32, u8, Option<Span>)> = rig
        .iter()
        .map(|f| (f.id, f.universe, f.span(library)))
        .collect();

    // Pads.
    let pads_rect = Rect::from_min_size(rect.min, vec2(rect.width(), theme::SIZE_PAD));
    let pad_w = (rect.width() / universes.len() as f32).min(PAD_W);
    let mut pads: Vec<(u8, Rect)> = Vec::new();
    ui.scope_builder(UiBuilder::new().max_rect(pads_rect), |ui| {
        let keys = universes
            .iter()
            .map(|&u| {
                Key::new("")
                    .size(vec2(pad_w, theme::SIZE_PAD))
                    .pressed(u == view.universe)
            })
            .collect();
        let responses = key::segmented_vec(ui, keys);
        for (&u, r) in universes.iter().zip(responses) {
            let used = occupancy.get(&u).map_or(0, |o| o.used());
            let bad = spans
                .iter()
                .any(|(id, fu, _)| *fu == u && conflicts.contains(id));
            let showing = u == view.universe;
            paint_pad(
                ui,
                &r,
                u,
                showing,
                used,
                bad,
                &spans,
                view.selected,
                conflicts,
            );
            pads.push((u, r.rect));
            let help = format!(
                "Universe {u} — {used} of 512 channels patched{}",
                if bad { ", with conflicts" } else { "" }
            );
            if r.status_help(help).clicked() {
                view.universe = u;
            }
        }
    });

    let well_rect = Rect::from_min_max(pos2(rect.left(), rect.bottom() - WELL_H), rect.max);
    let map_well = Rect::from_min_max(
        pos2(rect.left(), pads_rect.bottom() + theme::SPACE_8),
        pos2(rect.right(), well_rect.top() - theme::SPACE_8),
    );
    let changed = map_ui(
        ui, map_well, view, rig, library, conflicts, &occupancy, &pads,
    );
    let changed = changed | selection_ui(ui, well_rect, view, rig, library, conflicts);
    ui.advance_cursor_after_rect(rect);
    changed
}

/// The text, count and occupancy strip over a universe pad's key.
#[allow(clippy::too_many_arguments)]
fn paint_pad(
    ui: &Ui,
    r: &Response,
    u: u8,
    showing: bool,
    used: u16,
    bad: bool,
    spans: &[(u32, u8, Option<Span>)],
    selected: Option<u32>,
    conflicts: &Conflicts,
) {
    let inner = r.rect.shrink2(vec2(theme::SPACE_8, 0.0));
    let painter = ui.painter();
    let top = r.rect.center().y - 11.5;
    let name = if r.hovered() || showing {
        theme::TEXT_STRONG
    } else {
        theme::TEXT
    };
    theme::paint_text(
        painter,
        pos2(inner.left(), top),
        Align2::LEFT_TOP,
        format!("U{u}"),
        theme::KEY_LABEL,
        name,
    );
    theme::paint_text(
        painter,
        pos2(inner.right(), top),
        Align2::RIGHT_TOP,
        format!("{used}/512"),
        theme::NUMERALS_SM,
        if bad { theme::WARN } else { theme::TEXT_MUTED },
    );
    let strip = Rect::from_min_size(pos2(inner.left(), top + 19.0), vec2(inner.width(), 4.0));
    painter.rect_filled(strip, 0.0, theme::SEGMENT_OFF);
    let px = strip.width() / 512.0;
    for (id, fu, span) in spans {
        let Some(span) = span.filter(|_| *fu == u) else {
            continue;
        };
        let left = strip.left() + f32::from(span.start - 1) * px;
        let width = (f32::from(span.end - span.start + 1) * px).max(1.0);
        let color = if conflicts.contains(id) {
            theme::WARN
        } else if Some(*id) == selected {
            theme::ACCENT
        } else {
            theme::TEXT_MUTED
        };
        painter.rect_filled(
            Rect::from_min_size(pos2(left, strip.top()), vec2(width, strip.height())),
            0.0,
            color,
        );
    }
}

/// One row's run of a fixture's channels on the map, columns inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Seg {
    row: u16,
    c0: u16,
    c1: u16,
}

impl Seg {
    fn cells(self) -> u16 {
        self.c1 - self.c0 + 1
    }
}

/// Channels `start..=end` cut into one run per map row.
fn segments(start: u16, end: u16) -> Vec<Seg> {
    let mut out = Vec::new();
    let (mut c, last) = (start - 1, end - 1);
    while c <= last {
        let c0 = c % COLS;
        let c1 = (COLS - 1).min(c0 + (last - c));
        out.push(Seg {
            row: c / COLS,
            c0,
            c1,
        });
        c += c1 - c0 + 1;
    }
    out
}

/// How a block's run joins the runs above and below: no gap where they
/// touch, and no edge where the other covers this one's columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Join {
    touch_up: bool,
    touch_down: bool,
    open_up: bool,
    open_down: bool,
}

fn join(seg: Seg, prev: Option<Seg>, next: Option<Seg>) -> Join {
    let touches = |o: Option<Seg>| o.is_some_and(|o| o.c1 >= seg.c0 && o.c0 <= seg.c1);
    let covers = |o: Option<Seg>| o.is_some_and(|o| o.c0 <= seg.c0 && o.c1 >= seg.c1);
    Join {
        touch_up: touches(prev),
        touch_down: touches(next),
        open_up: covers(prev),
        open_down: covers(next),
    }
}

/// The map's geometry: cell `(row, col)` to screen and back.
#[derive(Clone, Copy)]
struct Grid {
    rect: Rect,
}

impl Grid {
    fn cell(&self) -> Vec2 {
        vec2(
            self.rect.width() / f32::from(COLS),
            self.rect.height() / f32::from(ROWS),
        )
    }

    fn seg_rect(&self, s: Seg) -> Rect {
        let c = self.cell();
        Rect::from_min_max(
            self.rect.min + vec2(f32::from(s.c0) * c.x, f32::from(s.row) * c.y),
            self.rect.min + vec2(f32::from(s.c1 + 1) * c.x, f32::from(s.row + 1) * c.y),
        )
    }

    /// A block's run with its padding: 1 either side, 2 above and below
    /// unless it joins the run there.
    fn block_rect(&self, s: Seg, j: Join) -> Rect {
        let r = self.seg_rect(s);
        Rect::from_min_max(
            r.min + vec2(1.0, if j.touch_up { 0.0 } else { 2.0 }),
            r.max - vec2(1.0, if j.touch_down { 0.0 } else { 2.0 }),
        )
    }

    /// The 1-based channel under `pos`, clamped to the map.
    fn channel_at(&self, pos: Pos2) -> u16 {
        let c = self.cell();
        let col = ((pos.x - self.rect.left()) / c.x).floor().clamp(0.0, 31.0) as u16;
        let row = ((pos.y - self.rect.top()) / c.y).floor().clamp(0.0, 15.0) as u16;
        row * COLS + col + 1
    }
}

#[allow(clippy::too_many_arguments)]
fn map_ui(
    ui: &mut Ui,
    well: Rect,
    view: &mut PatchView,
    rig: &mut Rig,
    library: &FixtureLibrary,
    conflicts: &Conflicts,
    occupancy: &BTreeMap<u8, Occupancy>,
    pads: &[(u8, Rect)],
) -> bool {
    let universe = view.universe;
    let painter = ui.painter().clone();
    theme::paint_well(&painter, well, theme::RADIUS_CONTROL);
    let inner = Rect::from_min_max(
        well.min + vec2(0.0, theme::SPACE_8),
        well.max - vec2(theme::SPACE_8, theme::SPACE_8),
    );

    // Header: the universe, and what's patched and free on it.
    let used = occupancy.get(&universe).map_or(0, |o| o.used());
    let head_y = inner.top() + 8.0;
    theme::paint_text(
        &painter,
        pos2(inner.left() + GUTTER, head_y),
        Align2::LEFT_CENTER,
        format!("UNIVERSE {universe}"),
        theme::CAPTION,
        theme::TEXT_MUTED,
    );
    let mut x = inner.right();
    for (i, (text, style, color)) in [
        ("FREE".to_string(), theme::CAPTION, theme::TEXT_MUTED),
        (
            (512 - used).to_string(),
            theme::NUMERALS_SM,
            theme::TEXT_STRONG,
        ),
        ("PATCHED".to_string(), theme::CAPTION, theme::TEXT_MUTED),
        (used.to_string(), theme::NUMERALS_SM, theme::TEXT_STRONG),
    ]
    .into_iter()
    .enumerate()
    {
        let r = theme::paint_text(
            &painter,
            pos2(x, head_y),
            Align2::RIGHT_CENTER,
            &text,
            style,
            color,
        );
        // 6 within a pair, 12 between the two.
        x = r.left() - if i == 1 { 12.0 } else { 6.0 };
    }

    let grid = Grid {
        rect: Rect::from_min_max(
            pos2(inner.left() + GUTTER, inner.top() + 16.0 + 4.0 + 12.0 + 4.0),
            inner.max,
        ),
    };
    let cell = grid.cell();

    // Ruler and gutter.
    for n in [1u16, 8, 16, 24, 32] {
        theme::paint_text(
            &painter,
            pos2(
                grid.rect.left() + (f32::from(n) - 0.5) * cell.x,
                grid.rect.top() - 10.0,
            ),
            Align2::CENTER_CENTER,
            n,
            theme::WAVE_NUMBER,
            theme::TEXT_MUTED,
        );
    }
    for r in 0..ROWS {
        theme::paint_text(
            &painter,
            pos2(
                grid.rect.left() - 6.0,
                grid.rect.top() + (f32::from(r) + 0.5) * cell.y,
            ),
            Align2::RIGHT_CENTER,
            format!("{:03}", r * COLS + 1),
            theme::MONO_TEXT.sized(11.0),
            theme::TEXT_MUTED,
        );
    }

    // Grid lines, every 8th channel stronger.
    for i in 1..COLS {
        let x = grid.rect.left() + f32::from(i) * cell.x;
        let color = if i % 8 == 0 {
            theme::QUIET_DOWNBEAT_LINE
        } else {
            theme::QUIET_BEAT_LINE
        };
        painter.vline(x, grid.rect.y_range(), Stroke::new(1.0, color));
    }
    for i in 1..ROWS {
        let y = grid.rect.top() + f32::from(i) * cell.y;
        painter.hline(grid.rect.x_range(), y, Stroke::new(1.0, theme::EDGE_PANEL));
    }
    painter.rect_stroke(
        grid.rect,
        0.0,
        Stroke::new(1.0, theme::EDGE_PANEL),
        StrokeKind::Inside,
    );

    // The fixtures showing, the dragged one last so it draws on top.
    let drag = view.drag;
    let mut shown: Vec<(&Fixture, Span)> = rig
        .iter()
        .filter(|f| f.universe == universe)
        .filter_map(|f| f.span(library).map(|s| (f, s)))
        .collect();
    shown.sort_by_key(|(f, _)| drag.is_some_and(|d| d.id == f.id));

    // Interaction: one sense over the whole map.
    let id = ui.scope_id().with("patch-map");
    let response = ui.interact(grid.rect, id, Sense::click_and_drag());
    let hit = |pos: Pos2| {
        let ch = grid.channel_at(pos);
        // The topmost block, the selected one first where they overlap.
        shown
            .iter()
            .filter(|(_, s)| s.start <= ch && ch <= s.end)
            .max_by_key(|(f, _)| Some(f.id) == view.selected)
            .map(|(f, s)| (f.id, ch - s.start))
    };
    let hovered = response
        .hover_pos()
        .filter(|p| grid.rect.contains(*p))
        .and_then(hit);
    let pressed = if response.drag_started() {
        ui.input(|i| i.pointer.press_origin()).and_then(hit)
    } else {
        None
    };
    let clicked = if response.clicked() {
        response.interact_pointer_pos().and_then(hit)
    } else {
        None
    };

    // The origin of a moved fixture, dashed.
    if let Some(d) = drag
        && d.from_universe == universe
        && let Some((f, s)) = shown.iter().find(|(f, _)| f.id == d.id)
        && (f.universe != d.from_universe || f.start_address != d.from_address)
    {
        let from = Span {
            start: d.from_address,
            end: (u32::from(d.from_address) + u32::from(s.footprint) - 1).min(512) as u16,
            footprint: s.footprint,
        };
        let segs = segments(from.start, from.end);
        for (i, &s) in segs.iter().enumerate() {
            let j = join(
                s,
                i.checked_sub(1).map(|i| segs[i]),
                segs.get(i + 1).copied(),
            );
            dashed_rect(&painter, grid.block_rect(s, j));
        }
    }

    // Blocks, then the overlaps hatched over them, then the labels.
    let mut labels = Vec::new();
    for (f, span) in &shown {
        let bad = conflicts.contains(&f.id);
        let selected = view.selected == Some(f.id);
        let dragging = drag.is_some_and(|d| d.id == f.id);
        let hover = hovered.is_some_and(|(id, _)| id == f.id);
        let segs = segments(span.start, span.end);
        let widest = (0..segs.len())
            .max_by_key(|&i| (segs[i].cells(), std::cmp::Reverse(i)))
            .unwrap_or(0);
        let (edge, width) = if dragging || selected {
            (theme::ACCENT, EDGE_STRONG)
        } else if bad {
            (theme::WARN, EDGE_STRONG)
        } else {
            (theme::EDGE, 1.0)
        };
        for (i, &s) in segs.iter().enumerate() {
            let j = join(
                s,
                i.checked_sub(1).map(|i| segs[i]),
                segs.get(i + 1).copied(),
            );
            let r = grid.block_rect(s, j);
            let (face, top) = if dragging {
                (Face::KeyDown, Top::Shade)
            } else if hover {
                (Face::KeyHover, Top::Sheen(theme::SKIN_SYSTEM.sheen))
            } else {
                (Face::Key, Top::Sheen(theme::SKIN_SYSTEM.sheen))
            };
            theme::paint_key_face(
                &painter,
                r,
                theme::RADIUS_CONTROL,
                face,
                Color32::WHITE,
                top,
            );
            if dragging || selected {
                painter.rect_filled(r, theme::RADIUS_CONTROL, theme::ACCENT.gamma_multiply(WASH));
            } else if bad {
                painter.rect_filled(r, theme::RADIUS_CONTROL, theme::WARN.gamma_multiply(WASH));
            }
            block_edges(&painter, r, j, Stroke::new(width, edge));
            if i == widest {
                let [cr, cg, cb] = f.kind.color();
                let inset = width;
                painter.rect_filled(
                    Rect::from_min_size(
                        r.min + vec2(inset, inset),
                        vec2(2.0, r.height() - 2.0 * inset),
                    ),
                    0.0,
                    Color32::from_rgb(cr, cg, cb),
                );
                labels.push((*f, *span, s, r, bad, selected || dragging));
            }
        }
    }
    if let Some(occ) = occupancy.get(&universe) {
        for (first, last) in occ.overlaps() {
            for s in segments(first, last) {
                let r = grid.seg_rect(s).shrink2(vec2(0.0, 2.0));
                hatch(&painter, r);
            }
        }
    }
    for (f, span, s, r, bad, lit) in labels {
        let cells = s.cells();
        let name = if bad {
            theme::WARN
        } else if lit {
            theme::ACCENT
        } else {
            theme::TEXT
        };
        let sub = if drag.is_some_and(|d| d.id == f.id) {
            theme::TEXT_STRONG
        } else {
            theme::TEXT_MUTED
        };
        let range = (cells >= 2).then(|| {
            if cells >= 3 {
                format!("{:03}–{:03}", span.start, span.end)
            } else {
                format!("{:03}", span.start)
            }
        });
        let clip = painter.with_clip_rect(r.shrink(1.0));
        let left = r.left() + 7.0;
        let width = r.right() - left - 2.0;
        let ctx = ui.ctx();
        let name_g = theme::truncated(ctx, &f.label, theme::KEY_LABEL, name, width);
        // The profile after the name, as much as fits.
        let room = width - name_g.size().x - PROFILE_GAP;
        let profile_g = library
            .get(&f.profile_id)
            .filter(|_| room >= PROFILE_MIN_ROOM)
            .map(|p| theme::truncated(ctx, &p.to_string(), theme::NOTE_TEXT, sub, room));
        let sub_style = theme::MONO_TEXT.sized(10.0);
        let sub_g = range.map(|t| theme::truncated(ctx, &t, sub_style, sub, width));
        let line_h = name_g.size().y;
        let total = line_h + sub_g.as_ref().map_or(0.0, |g| g.size().y - 2.0);
        let top = r.center().y - total / 2.0;
        // A conflict's labels sit on plates, clear of the hatching.
        let plate = |at: Pos2, size: Vec2| {
            if bad {
                clip.rect_filled(
                    Rect::from_min_size(at - vec2(2.0, 0.0), size + vec2(4.0, 0.0)),
                    0.0,
                    theme::KEY_LO,
                );
            }
        };
        let name_w = name_g.size().x;
        let mut first = name_g.size();
        if let Some(g) = &profile_g {
            first.x = name_w + PROFILE_GAP + g.size().x;
        }
        plate(pos2(left, top), first);
        clip.galley(pos2(left, top), name_g, name);
        if let Some(g) = profile_g {
            let y = top + (line_h - g.size().y) / 2.0;
            clip.galley(pos2(left + name_w + PROFILE_GAP, y), g, sub);
        }
        if let Some(g) = sub_g {
            let at = pos2(left, top + line_h - 2.0);
            plate(at, g.size());
            clip.galley(at, g, sub);
        }
    }

    // Press, drag, release.
    let mut changed = false;
    if let Some((id, grab)) = pressed
        && let Some(f) = rig.iter().find(|f| f.id == id)
    {
        view.drag = Some(Drag {
            id,
            grab,
            from_universe: f.universe,
            from_address: f.start_address,
        });
        view.selected = Some(id);
        view.scroll_to = Some(id);
    }
    if let Some(d) = view.drag {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            // Put it back where it was picked up.
            if let Some(f) = rig.fixtures_mut().iter_mut().find(|f| f.id == d.id) {
                f.universe = d.from_universe;
                f.start_address = d.from_address;
            }
            view.universe = d.from_universe;
            view.drag = None;
            changed = true;
        } else if response.dragged()
            && let Some(pos) = response.interact_pointer_pos()
            && let Some(f) = rig.fixtures_mut().iter_mut().find(|f| f.id == d.id)
        {
            let footprint = library.layout(f).map_or(1, |m| m.footprint().max(1)) as u16;
            if let Some(&(u, _)) = pads.iter().find(|(_, r)| r.contains(pos)) {
                if u != f.universe {
                    f.universe = u;
                    view.universe = u;
                    changed = true;
                }
            } else {
                let max = 513u16.saturating_sub(footprint).max(1);
                let addr = grid.channel_at(pos).saturating_sub(d.grab).clamp(1, max);
                if addr != f.start_address {
                    f.start_address = addr;
                    changed = true;
                }
            }
        }
        if response.drag_stopped() {
            view.drag = None;
        }
    }
    if let Some((id, _)) = clicked {
        view.selected = Some(id);
        view.scroll_to = Some(id);
    }

    if view.drag.is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    } else if hovered.is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }
    let help = match view.drag.and_then(|d| rig.iter().find(|f| f.id == d.id)) {
        Some(f) => format!(
            "Moving {} to {}.{:03} — release to drop",
            f.label, f.universe, f.start_address
        ),
        None => "Drag a fixture to change its address — drag it onto a universe to move it there"
            .to_string(),
    };
    response.status_help(help);
    changed
}

/// A block's edge, leaving out the sides where it runs on into the row
/// above or below.
fn block_edges(painter: &egui::Painter, r: Rect, j: Join, stroke: Stroke) {
    let h = stroke.width / 2.0;
    let r = r.shrink(h);
    painter.vline(r.left(), r.y_range(), stroke);
    painter.vline(r.right(), r.y_range(), stroke);
    if !j.open_up {
        painter.hline(r.x_range(), r.top(), stroke);
    }
    if !j.open_down {
        painter.hline(r.x_range(), r.bottom(), stroke);
    }
}

/// Where a dragged fixture came from: a dashed `text-disabled` outline.
fn dashed_rect(painter: &egui::Painter, r: Rect) {
    let r = r.shrink(0.5);
    let points = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    painter.extend(Shape::dashed_line(
        &points,
        Stroke::new(1.0, theme::TEXT_DISABLED),
        3.0,
        3.0,
    ));
}

/// Channels patched twice: `warn` stripes at 135° and a 1px `warn` edge.
fn hatch(painter: &egui::Painter, r: Rect) {
    let clip = painter.with_clip_rect(r);
    let stroke = Stroke::new(1.5, theme::WARN.gamma_multiply(0.85));
    let mut x = r.left() - r.height();
    while x < r.right() {
        clip.line_segment([pos2(x, r.bottom()), pos2(x + r.height(), r.top())], stroke);
        x += 6.0;
    }
    clip.rect_stroke(r, 0.0, Stroke::new(1.0, theme::WARN), StrokeKind::Inside);
}

// ---- The selection well ----

fn selection_ui(
    ui: &mut Ui,
    rect: Rect,
    view: &mut PatchView,
    rig: &mut Rig,
    library: &FixtureLibrary,
    conflicts: &Conflicts,
) -> bool {
    theme::paint_well(ui.painter(), rect, theme::RADIUS_CONTROL);
    let inner = rect.shrink2(vec2(theme::SPACE_12, theme::SPACE_4));
    let Some(f) = view.selected.and_then(|id| rig.iter().find(|f| f.id == id)) else {
        theme::paint_text(
            ui.painter(),
            inner.left_center(),
            Align2::LEFT_CENTER,
            "No fixture selected",
            theme::BODY_TEXT,
            theme::TEXT_MUTED,
        );
        return false;
    };
    let span = f.span(library);
    let footprint = span.map_or(0, |s| s.footprint);
    let range = span.map_or_else(
        || "—".to_string(),
        |s| format!("{u}.{:03} – {u}.{:03}", s.start, s.end, u = f.universe),
    );
    let profile = library
        .get(&f.profile_id)
        .map_or_else(|| format!("? {}", f.profile_id), |p| p.to_string());
    let others = labels(rig);
    let problem = conflicts.get(f.id).map(|c| {
        if let Some((first, last)) = c.range() {
            (
                "OVERLAPS",
                format!("{} · {first:03}–{last:03}", names(c, &others)),
            )
        } else if c.past_end > 0 {
            ("PAST 512", format!("+{} CH", c.past_end))
        } else {
            ("PROFILE", "NOT IN LIBRARY".to_string())
        }
    });
    let help = conflicts.get(f.id).map(|c| conflict_text(f, c, &others));
    let free = rig.first_free(library, f.universe, footprint, Some(f.id));
    let (id, label, universe, addr) = (f.id, f.label.clone(), f.universe, f.start_address);
    let max = 513u16.saturating_sub(footprint).max(1);

    let mut moved = None;
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_12;
            let field = |ui: &mut Ui, caption: &str, color: Color32, value: egui::RichText| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.label(theme::CAPTION.rich(caption).color(color));
                    ui.add(egui::Label::new(value).truncate())
                })
                .response
            };
            let divider = |ui: &mut Ui| {
                let (r, _) = ui.allocate_exact_size(vec2(1.0, 24.0), Sense::hover());
                ui.painter().vline(
                    r.left() + 0.5,
                    r.y_range(),
                    Stroke::new(1.0, theme::TRAY_DIVIDER),
                );
            };
            field(
                ui,
                "FIXTURE",
                theme::TEXT_MUTED,
                theme::NUMERALS_MD.rich(&label).color(theme::TEXT_STRONG),
            );
            divider(ui);
            field(
                ui,
                "ADDRESS",
                theme::TEXT_MUTED,
                theme::MONO_TEXT.rich(&range).color(theme::TEXT_STRONG),
            );
            divider(ui);
            field(
                ui,
                "CHANNELS",
                theme::TEXT_MUTED,
                theme::NUMERALS_MD
                    .rich(footprint.to_string())
                    .color(theme::TEXT_STRONG),
            );
            divider(ui);
            // The actions sit at the right; the profile takes the room
            // between.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_8;
                let next_help = match free {
                    Some(a) => format!(
                        "Move to {universe}.{a:03} — the first free run of {footprint} channels"
                    ),
                    None => format!("No free run of {footprint} channels on universe {universe}"),
                };
                if ui
                    .add(
                        Key::new("NEXT FREE")
                            .small()
                            .enabled(free.is_some_and(|a| a != addr)),
                    )
                    .status_help(next_help)
                    .clicked()
                {
                    moved = free;
                }
                // Its own left-to-right box, so − stays left of +.
                let [down, up] = ui
                    .allocate_ui_with_layout(
                        vec2(52.0, theme::SIZE_KEY_SMALL),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            key::segmented(
                                ui,
                                [
                                    Key::new("−").small().width(26.0).enabled(addr > 1),
                                    Key::new("+").small().width(26.0).enabled(addr < max),
                                ],
                            )
                        },
                    )
                    .inner;
                if down.status_help("Move one channel down").clicked() {
                    moved = Some(addr - 1);
                }
                if up.status_help("Move one channel up").clicked() {
                    moved = Some(addr + 1);
                }
                if let Some((caption, text)) = &problem {
                    let r = field(
                        ui,
                        caption,
                        theme::WARN,
                        theme::MONO_TEXT.rich(text).color(theme::WARN),
                    );
                    if let Some(help) = &help {
                        r.status_help(help.clone());
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    field(
                        ui,
                        "PROFILE",
                        theme::TEXT_MUTED,
                        theme::BODY_TEXT.rich(&profile).color(theme::TEXT),
                    );
                });
            });
        },
    );
    if let Some(a) = moved
        && let Some(f) = rig.fixtures_mut().iter_mut().find(|f| f.id == id)
    {
        f.start_address = a.clamp(1, max);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_footprint_wraps_into_one_run_per_row() {
        // 8 from 89: within row 2.
        assert_eq!(
            segments(89, 96),
            [Seg {
                row: 2,
                c0: 24,
                c1: 31
            }]
        );
        // 9 from 30: the end of row 0 and the start of row 1.
        assert_eq!(
            segments(30, 38),
            [
                Seg {
                    row: 0,
                    c0: 29,
                    c1: 31
                },
                Seg {
                    row: 1,
                    c0: 0,
                    c1: 5
                },
            ]
        );
        // A 192-channel bar from 1: six whole rows.
        let bar = segments(1, 192);
        assert_eq!(bar.len(), 6);
        assert!(bar.iter().all(|s| (s.c0, s.c1) == (0, 31)));
        // The last channel.
        assert_eq!(
            segments(512, 512),
            [Seg {
                row: 15,
                c0: 31,
                c1: 31
            }]
        );
    }

    #[test]
    fn runs_join_where_they_touch_and_open_where_covered() {
        let segs = segments(1, 192);
        let j = join(segs[1], Some(segs[0]), Some(segs[2]));
        assert!(j.touch_up && j.touch_down && j.open_up && j.open_down);
        // 30–38: row 0's columns 29–31 sit above row 1's 0–5, apart.
        let segs = segments(30, 38);
        let j = join(segs[1], Some(segs[0]), None);
        assert!(!j.touch_up && !j.open_up && !j.touch_down);
        // 20–60: row 0 (19–31) and row 1 (0–27) touch, but neither
        // covers the other, so both keep their edge there.
        let segs = segments(20, 60);
        let j = join(segs[1], Some(segs[0]), None);
        assert!(j.touch_up && !j.open_up);
        let j = join(segs[0], None, Some(segs[1]));
        assert!(j.touch_down && !j.open_down);
    }

    #[test]
    fn the_map_reads_channels_row_by_row() {
        let grid = Grid {
            rect: Rect::from_min_size(Pos2::ZERO, vec2(320.0, 160.0)),
        };
        assert_eq!(grid.channel_at(pos2(0.5, 0.5)), 1);
        assert_eq!(grid.channel_at(pos2(315.0, 0.5)), 32);
        assert_eq!(grid.channel_at(pos2(5.0, 15.0)), 33);
        assert_eq!(grid.channel_at(pos2(319.0, 159.0)), 512);
        // Off the map clamps to its edge.
        assert_eq!(grid.channel_at(pos2(-40.0, 500.0)), 481);
    }
}
