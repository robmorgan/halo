//! The PATCH view below its action row: the patch sheet on the left and
//! the universe overview on the right.
//!
//! The overview has a pad per universe (patched/512 and an occupancy
//! strip), the universe showing as a 32×16 channel map, and a well for
//! the selected fixture. Fixtures on the map are keys spanning their
//! footprint, wrapping across rows: drag one to change its start address
//! a channel at a time, or onto a pad to move it to that universe.
//! Overlapping channels are hatched in `warn`. Clearing a fixture's ADDR
//! (or UNPATCH) unpatches it: its row greys with a `latch` red line, and
//! it stays on the programmer's grid, remembering its address, but is off
//! the map and sends nothing.
//!
//! A switch above the overview flips it to LAYOUT ([`layout`]): the
//! 8 × 8 stage the programmer's fixture grid mirrors, where dragging a
//! fixture places it by hand.
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

mod layout;

/// The map: 32 channels a row, 16 rows.
const COLS: u16 = 32;
const ROWS: u16 = 16;
/// A run this many cells wide holds a block's name and range.
const LABEL_MIN_CELLS: u16 = 6;
/// Room left of the map for each row's first address.
const GUTTER: f32 = 38.0;
/// A universe pad, at most.
const PAD_W: f32 = 104.0;
/// The selection well's height.
const WELL_H: f32 = 44.0;
/// The least room the well's PROFILE is shown in.
const WELL_PROFILE_MIN_W: f32 = 64.0;
/// A sheet row's height.
const ROW_H: f32 = 28.0;
/// A selected, dragged or conflicting block's edge, and its wash.
const EDGE_STRONG: f32 = 1.5;
/// A block's profile name: its gap after the fixture's, and the least
/// room worth showing it in.
const PROFILE_GAP: f32 = 6.0;
const PROFILE_MIN_ROOM: f32 = 28.0;
const WASH: f32 = 0.08;
/// The map's accessible name.
const MAP_LABEL: &str = "Channel map";

/// What the PATCH view remembers between frames, for the session.
pub struct PatchView {
    /// The universe the map shows.
    universe: u8,
    selected: Option<u32>,
    drag: Option<Drag>,
    /// Scroll the sheet to this fixture's row next frame.
    scroll_to: Option<u32>,
    /// The ADDR field being typed in: its fixture and text.
    addr_edit: Option<(u32, String)>,
    /// The selected fixture's row in the sheet last frame, to follow it
    /// when an edit re-sorts it.
    sheet_row: Option<(u32, usize)>,
    /// What the overview shows.
    overview: Overview,
    /// A fixture being placed on the stage.
    stage_drag: Option<layout::StageDrag>,
}

/// The overview's two views.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Overview {
    /// The channel map, a universe at a time.
    Map,
    /// The stage layout.
    Layout,
}

impl Default for PatchView {
    fn default() -> Self {
        PatchView {
            universe: 1,
            selected: None,
            drag: None,
            scroll_to: None,
            addr_edit: None,
            sheet_row: None,
            overview: Overview::Map,
            stage_drag: None,
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
    /// Open the overview on the stage layout.
    pub fn show_layout(&mut self) {
        self.overview = Overview::Layout;
    }

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
    + COL_REMOVE
    + 7.0 * COL_GAP
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
        "FIXTURE", "KIND", "PROFILE", "MODE", "CH", "UNIV", "ADDR", "",
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
    let order = sheet_order(rig);
    // An edit that re-sorted the selected fixture: keep it in view.
    let row_now = view.selected.and_then(|id| {
        let fixtures = rig.iter().collect::<Vec<_>>();
        order
            .iter()
            .position(|&i| fixtures[i].id == id)
            .map(|p| (id, p))
    });
    if let (Some((id, p)), Some((was_id, was))) = (row_now, view.sheet_row)
        && id == was_id
        && p != was
    {
        view.scroll_to = Some(id);
    }
    view.sheet_row = row_now;
    let mut changed = false;
    let mut remove = None;
    let mut select = None;
    ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let fixtures = rig.fixtures_mut();
                for (i, &at) in order.iter().enumerate() {
                    let f = &mut fixtures[at];
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
                                // Its editors' ids follow the fixture, not
                                // the row, so a re-sort mid-edit can't hand
                                // a drag or focus to another fixture.
                                .id_salt(("patch-row", f.id))
                                .max_rect(cells)
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = COL_GAP;
                                // Number fields take their column's width,
                                // under egui's 40 minimum.
                                ui.spacing_mut().interact_size.x = COL_UNIV;
                                sheet_row(
                                    ui,
                                    f,
                                    &widths,
                                    library,
                                    &profiles,
                                    conflicts.get(f.id),
                                    &names,
                                    selected,
                                    &mut view.addr_edit,
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

/// The sheet's rows: indices into the rig, by universe then start
/// address (an unpatched fixture's remembered one), ties in rig order.
fn sheet_order(rig: &Rig) -> Vec<usize> {
    let fixtures: Vec<&Fixture> = rig.iter().collect();
    let mut order: Vec<usize> = (0..fixtures.len()).collect();
    order.sort_by_key(|&i| (fixtures[i].universe, fixtures[i].start_address));
    order
}

/// One fixture's row of editors. Returns whether any was used, which
/// selects the fixture.
#[allow(clippy::too_many_arguments)]
fn sheet_row(
    ui: &mut Ui,
    f: &mut Fixture,
    widths: &[f32; 8],
    library: &FixtureLibrary,
    profiles: &[(&String, String)],
    conflict: Option<&Conflict>,
    names: &Labels,
    selected: bool,
    addr_edit: &mut Option<(u32, String)>,
    changed: &mut bool,
    remove: &mut Option<u32>,
) -> bool {
    let h = theme::SIZE_KEY_SMALL;
    // Unpatched, the whole row greys, but its editors still work.
    if f.unpatched {
        ui.multiply_opacity(UNPATCHED_OPACITY);
    }
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
    // The kind's tick; unpatched, a red line at full strength instead.
    let tick = Rect::from_center_size(pos2(rect.left() + 1.0, rect.center().y), vec2(2.0, 14.0));
    if f.unpatched {
        let mut full = painter.clone();
        full.set_opacity(1.0);
        full.rect_filled(tick, 0.0, theme::LATCH);
    } else {
        let [r, g, b] = f.kind.color();
        painter.rect_filled(tick, 0.0, Color32::from_rgb(r, g, b));
    }
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
        None if f.unpatched => format!("{} is unpatched — type an ADDR to patch it", f.label),
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
    note(&r.response.accessible_name(format!("{} kind", f.label)));

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
    note(&r.response.accessible_name(format!("{} profile", f.label)));

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
    note(&r.response.accessible_name(format!("{} mode", f.label)));

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

    // UNIV: a DMX number, the one place JetBrains Mono appears.
    ui.scope(|ui| {
        ui.style_mut().drag_value_text_style = egui::TextStyle::Monospace;
        let r = ui
            .add_sized(
                [widths[5], h],
                egui::DragValue::new(&mut f.universe).range(1..=32),
            )
            .accessible_name(format!("{} universe", f.label));
        *changed |= r.changed();
        note(&r);
    });

    // ADDR: typed, so clearing it can unpatch the fixture.
    let r = addr_field(ui, f, widths[6], conflict.is_some(), addr_edit, changed);
    note(&r);

    // Remove: the × glyph, so it doesn't read as a decrement beside the
    // number fields.
    let r = ui
        .add(Key::new("").small().width(widths[7]))
        .accessible_name(format!("Remove {}", f.label));
    let color = if r.hovered() {
        theme::TEXT_STRONG
    } else {
        theme::TEXT
    };
    theme::paint_cross(ui.painter(), r.rect.center(), theme::CROSS_MINI, color);
    if r.status_help(format!("Remove {} from the rig", f.label))
        .clicked()
    {
        *remove = Some(f.id);
    }
    touched
}

/// A fixture's start address as a text field. Typing a number and
/// pressing Enter (or leaving the field) patches it there; leaving it
/// empty unpatches it, keeping the old address for PATCH. While typing,
/// the text lives in `edit`.
fn addr_field(
    ui: &mut Ui,
    f: &mut Fixture,
    w: f32,
    bad: bool,
    edit: &mut Option<(u32, String)>,
    changed: &mut bool,
) -> Response {
    let mut text = match edit {
        Some((id, text)) if *id == f.id => text.clone(),
        _ if f.unpatched => String::new(),
        _ => f.start_address.to_string(),
    };
    let (rect, _) = ui.allocate_exact_size(vec2(w, theme::SIZE_KEY_SMALL), Sense::hover());
    // The face goes under the text once the field's state is known.
    let face = ui.painter().add(Shape::Noop);
    let r = ui
        .put(
            rect,
            egui::TextEdit::singleline(&mut text)
                .font(egui::TextStyle::Monospace)
                .text_color(if bad { theme::WARN } else { theme::TEXT })
                .frame(egui::Frame::NONE)
                .align(Align2::CENTER_CENTER)
                .char_limit(3)
                .desired_width(w),
        )
        .accessible_name(format!("{} address", f.label));
    let visuals = ui.style().interact(&r);
    ui.painter().set(
        face,
        egui::epaint::RectShape::new(
            rect,
            visuals.corner_radius,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            StrokeKind::Inside,
        ),
    );
    text.retain(|c| c.is_ascii_digit());
    if r.lost_focus() {
        *edit = None;
        // Esc leaves it as it was.
        if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            *changed |= commit_addr(f, &text);
        }
    } else if r.has_focus() {
        *edit = Some((f.id, text));
    }
    let help = if f.unpatched {
        format!(
            "Type a start address to patch {} (it was {}.{:03})",
            f.label, f.universe, f.start_address
        )
    } else {
        format!("{}'s start address — clear it to unpatch {0}", f.label)
    };
    r.status_help(help)
}

/// What an ADDR field's text does to its fixture: empty unpatches it,
/// keeping its address; a number patches it there, clamped to 1–512;
/// anything else changes nothing. Returns whether it changed.
fn commit_addr(f: &mut Fixture, text: &str) -> bool {
    if text.trim().is_empty() {
        let changed = !f.unpatched;
        f.unpatched = true;
        return changed;
    }
    let Ok(a) = text.trim().parse::<u16>() else {
        return false;
    };
    let a = a.clamp(1, 512);
    let changed = f.unpatched || a != f.start_address;
    f.start_address = a;
    f.unpatched = false;
    changed
}

/// How much an unpatched fixture's row shows through.
const UNPATCHED_OPACITY: f32 = 0.4;

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
        .patched()
        .map(|f| f.universe)
        .chain([view.universe])
        .collect();
    let spans: Vec<(u32, u8, Option<Span>)> = rig
        .patched()
        .map(|f| (f.id, f.universe, f.span(library)))
        .collect();

    // The view switch, then on the map its universe pads.
    let top = Rect::from_min_size(rect.min, vec2(rect.width(), theme::SIZE_PAD));
    let switch = ui
        .scope_builder(UiBuilder::new().max_rect(top), |ui| {
            overview_switch(ui, view)
        })
        .inner;
    let pads_rect = Rect::from_min_max(pos2(switch.right() + theme::SPACE_12, top.top()), top.max);
    let pad_w = (pads_rect.width() / universes.len() as f32).min(PAD_W);
    let mut pads: Vec<(u8, Rect)> = Vec::new();
    ui.scope_builder(UiBuilder::new().max_rect(pads_rect), |ui| {
        if view.overview != Overview::Map {
            return;
        }
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
            let r = r.accessible_name(format!("Universe {u}"));
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
    let middle = Rect::from_min_max(
        pos2(rect.left(), top.bottom() + theme::SPACE_8),
        pos2(rect.right(), well_rect.top() - theme::SPACE_8),
    );
    let changed = match view.overview {
        Overview::Map => map_ui(ui, middle, view, rig, library, conflicts, &occupancy, &pads),
        Overview::Layout => layout::stage_ui(ui, middle, view, rig, conflicts),
    };
    let changed = changed | selection_ui(ui, well_rect, view, rig, library, conflicts);
    ui.advance_cursor_after_rect(rect);
    changed
}

/// MAP | LAYOUT, a key each with its icon, the one showing pressed in.
/// Returns the strip's rect.
fn overview_switch(ui: &mut Ui, view: &mut PatchView) -> Rect {
    let views = [
        (
            Overview::Map,
            theme::ICON_MAP,
            "MAP",
            "Map",
            "Channel map — drag fixtures to change their addresses",
        ),
        (
            Overview::Layout,
            theme::ICON_LAYOUT,
            "LAYOUT",
            "Layout",
            "Stage layout — optional; drag fixtures to mirror the physical rig",
        ),
    ];
    let ctx = ui.ctx().clone();
    let keys = views.map(|(o, icon, word, ..)| {
        let label = format!("{icon}  {word}");
        let w = theme::KEY_LABEL.width(&ctx, &label) + 2.0 * theme::SPACE_12;
        Key::new(label)
            .size(vec2(w, theme::SIZE_PAD))
            .pressed(view.overview == o)
    });
    let responses = key::segmented(ui, keys);
    let rect = responses[0].rect.union(responses[1].rect);
    for ((o, _, _, name, help), r) in views.into_iter().zip(responses) {
        if r.accessible_name(name).status_help(help).clicked() && view.overview != o {
            view.overview = o;
            // A drag in the view left behind ends where it is.
            view.drag = None;
            view.stage_drag = None;
        }
    }
    rect
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

/// The run a block's tick and labels go on: its first, where it's
/// addressed, unless that's too short for them and a later run is
/// longer, then the longest (the first of those).
fn label_run(segs: &[Seg]) -> usize {
    let widest = (0..segs.len())
        .max_by_key(|&i| (segs[i].cells(), std::cmp::Reverse(i)))
        .unwrap_or(0);
    let room = LABEL_MIN_CELLS.min(segs.get(widest).map_or(0, |s| s.cells()));
    if segs.first().is_some_and(|s| s.cells() >= room) {
        0
    } else {
        widest
    }
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
        .patched()
        .filter(|f| f.universe == universe)
        .filter_map(|f| f.span(library).map(|s| (f, s)))
        .collect();
    shown.sort_by_key(|(f, _)| drag.is_some_and(|d| d.id == f.id));

    // Interaction: one sense over the whole map.
    let id = ui.scope_id().with("patch-map");
    let response = ui.interact(grid.rect, id, Sense::click_and_drag());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Grid, true, MAP_LABEL));
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
        let labelled = label_run(&segs);
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
            if i == labelled {
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
    let unpatched = f.unpatched;
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

    let mut edit = None;
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_12;
            let field = |ui: &mut Ui, caption: &str, color: Color32, value: egui::RichText| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    // Elided, never wrapped a letter to a line.
                    ui.add(egui::Label::new(theme::CAPTION.rich(caption).color(color)).truncate());
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
            if unpatched {
                field(
                    ui,
                    "ADDRESS",
                    theme::TEXT_MUTED,
                    theme::KEY_LABEL.rich("UNPATCHED").color(theme::TEXT_MUTED),
                )
                .status_help(format!("Remembered: {range}"));
            } else {
                field(
                    ui,
                    "ADDRESS",
                    theme::TEXT_MUTED,
                    theme::MONO_TEXT.rich(&range).color(theme::TEXT_STRONG),
                );
            }
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
                let verb = if unpatched { "Patch it at" } else { "Move to" };
                let next_help = match free {
                    Some(a) => format!(
                        "{verb} {universe}.{a:03} — the first free run of {footprint} channels"
                    ),
                    None => format!("No free run of {footprint} channels on universe {universe}"),
                };
                if ui
                    .add(
                        Key::new("NEXT FREE")
                            .small()
                            .enabled(free.is_some_and(|a| unpatched || a != addr)),
                    )
                    .status_help(next_help)
                    .clicked()
                    && let Some(a) = free
                {
                    edit = Some(Edit::PatchAt(a));
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
                                    Key::new("−")
                                        .small()
                                        .width(26.0)
                                        .enabled(!unpatched && addr > 1),
                                    Key::new("+")
                                        .small()
                                        .width(26.0)
                                        .enabled(!unpatched && addr < max),
                                ],
                            )
                        },
                    )
                    .inner;
                if down.status_help("Move one channel down").clicked() {
                    edit = Some(Edit::PatchAt(addr - 1));
                }
                if up.status_help("Move one channel up").clicked() {
                    edit = Some(Edit::PatchAt(addr + 1));
                }
                let (key, patch_help) = if unpatched {
                    ("PATCH", format!("Patch {label} on {range}, where it was"))
                } else {
                    (
                        "UNPATCH",
                        format!(
                            "Unpatch {label}: no DMX, keeping its address and its place on the grid"
                        ),
                    )
                };
                if ui
                    .add(Key::new(key).small())
                    .status_help(patch_help)
                    .clicked()
                {
                    edit = Some(Edit::Unpatched(!unpatched));
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
                // Too narrow (beside a conflict, say), it's left out: the
                // sheet row shows it too.
                if ui.available_width() >= WELL_PROFILE_MIN_W {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        field(
                            ui,
                            "PROFILE",
                            theme::TEXT_MUTED,
                            theme::BODY_TEXT.rich(&profile).color(theme::TEXT),
                        );
                    });
                }
            });
        },
    );
    let Some(edit) = edit else { return false };
    let Some(f) = rig.fixtures_mut().iter_mut().find(|f| f.id == id) else {
        return false;
    };
    match edit {
        Edit::PatchAt(a) => {
            f.start_address = a.clamp(1, max);
            f.unpatched = false;
        }
        Edit::Unpatched(off) => f.unpatched = off,
    }
    true
}

/// What the selection well does to its fixture.
enum Edit {
    /// Patch it (if it isn't) at this start address.
    PatchAt(u16),
    Unpatched(bool),
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
    fn a_block_is_labelled_on_its_first_run_when_it_fits() {
        // A 192-channel bar from 130: 31 cells, five full rows, then 1.
        assert_eq!(label_run(&segments(130, 321)), 0);
        // From 1: six full rows.
        assert_eq!(label_run(&segments(1, 192)), 0);
        // 8 from 30: 3 cells, then 5 below, the roomier.
        assert_eq!(label_run(&segments(30, 37)), 1);
        // 8 from 27: 6 cells, enough, then 2.
        assert_eq!(label_run(&segments(27, 34)), 0);
        // Within a row, and a single channel.
        assert_eq!(label_run(&segments(89, 96)), 0);
        assert_eq!(label_run(&segments(151, 151)), 0);
    }

    #[test]
    fn the_sheet_sorts_by_universe_then_address() {
        let library = FixtureLibrary::new();
        let mut rig = halo_light::fixture::default_rig(&library);
        // Out of order: PB1 onto universe 1 at 162, P1 (77) past ST2.
        for f in rig.fixtures_mut() {
            match f.label.as_str() {
                "PB1" => (f.universe, f.start_address) = (1, 162),
                "P1" => f.start_address = 140,
                _ => {}
            }
        }
        let all: Vec<&Fixture> = rig.iter().collect();
        let sorted: Vec<(u8, u16)> = sheet_order(&rig)
            .into_iter()
            .map(|i| (all[i].universe, all[i].start_address))
            .collect();
        assert!(sorted.is_sorted());
        assert_eq!(sorted.len(), all.len());
        let labels: Vec<&str> = sheet_order(&rig)
            .into_iter()
            .map(|i| all[i].label.as_str())
            .collect();
        let at = |l: &str| labels.iter().position(|x| *x == l).unwrap();
        assert!(at("ST2") < at("P1") && at("P1") < at("PB1"));
        assert!(at("PB1") < at("PB2"), "universe 1 before 2");
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
    fn clearing_addr_unpatches_and_a_number_patches() {
        let library = FixtureLibrary::new();
        let mut f = halo_light::fixture::default_rig(&library)
            .iter()
            .find(|f| f.label == "P3")
            .unwrap()
            .clone();
        assert_eq!(f.start_address, 93);
        // Cleared: unpatched, the address kept for PATCH.
        assert!(commit_addr(&mut f, ""));
        assert!(f.unpatched);
        assert_eq!(f.start_address, 93);
        assert!(!commit_addr(&mut f, " "), "already unpatched");
        // A number patches it there; past 512 clamps.
        assert!(commit_addr(&mut f, "120"));
        assert_eq!((f.unpatched, f.start_address), (false, 120));
        assert!(!commit_addr(&mut f, "120"));
        assert!(commit_addr(&mut f, "999"));
        assert_eq!(f.start_address, 512);
        assert!(commit_addr(&mut f, "0"));
        assert_eq!(f.start_address, 1);
        // Garbage changes nothing.
        assert!(!commit_addr(&mut f, "x"));
        assert_eq!((f.unpatched, f.start_address), (false, 1));
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

    // ---- The view in a kittest harness ----
    //
    // `KITTEST_RECORD=open cargo test -p halo patch_view` records each
    // of these and shows the video when it finishes.

    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT as _, Queryable as _};

    /// What the harness's ui draws from: the view, a draft rig and the
    /// library, as `app.rs` passes them.
    struct Patch {
        view: PatchView,
        rig: Rig,
        library: FixtureLibrary,
    }

    impl Patch {
        fn fixture(&self, label: &str) -> &Fixture {
            self.rig.iter().find(|f| f.label == label).unwrap()
        }
    }

    /// The PATCH view on the default rig, themed as the app is.
    fn harness() -> Harness<'static, Patch> {
        let library = FixtureLibrary::new();
        let rig = halo_light::fixture::default_rig(&library);
        Harness::builder()
            .with_size(vec2(1400.0, 720.0))
            .with_context_setup(crate::app::apply_theme)
            .build_ui_state(
                |ui, p: &mut Patch| {
                    let conflicts = p.rig.conflicts(&p.library);
                    show(ui, &mut p.view, &mut p.rig, &p.library, &conflicts);
                },
                Patch {
                    view: PatchView::default(),
                    rig,
                    library,
                },
            )
    }

    /// The centre of channel `ch`'s cell on the map.
    fn channel_pos(harness: &Harness<'_, Patch>, ch: u16) -> Pos2 {
        let grid = Grid {
            rect: harness.get_by_label(MAP_LABEL).rect(),
        };
        let c = grid.cell();
        let i = ch - 1;
        grid.rect.min
            + vec2(
                (f32::from(i % COLS) + 0.5) * c.x,
                (f32::from(i / COLS) + 0.5) * c.y,
            )
    }

    /// Press at `from` and move to `to` over a few frames, still held.
    fn press_and_move(harness: &mut Harness<'_, Patch>, from: Pos2, to: Pos2) {
        harness.hover_at(from);
        harness.run();
        harness.drag_at(from);
        harness.run();
        for i in 1..=12 {
            harness.hover_at(from.lerp(to, i as f32 / 12.0));
            harness.run();
        }
    }

    /// Move from `from` to `to` over a few frames, the button still held.
    fn press_and_move_held(harness: &mut Harness<'_, Patch>, from: Pos2, to: Pos2) {
        for i in 1..=12 {
            harness.hover_at(from.lerp(to, i as f32 / 12.0));
            harness.run();
        }
    }

    #[test]
    fn dragging_a_block_readdresses_its_fixture() {
        let mut harness = harness();
        let before = harness.state().rig.clone();
        let p3 = harness.state().fixture("P3").clone();
        assert_eq!((p3.universe, p3.start_address), (1, 93));

        // Grab P3 by its first channel and drop it on 161, a free row.
        let from = channel_pos(&harness, 93);
        let to = channel_pos(&harness, 161);
        press_and_move(&mut harness, from, to);
        let p = harness.state();
        assert!(p.view.drag.is_some_and(|d| d.id == p3.id));
        assert_eq!(p.fixture("P3").start_address, 161, "moves while held");
        harness.drop_at(to);
        harness.run();

        let p = harness.state();
        let moved = p.fixture("P3");
        assert_eq!((moved.universe, moved.start_address), (1, 161));
        assert!(p.view.drag.is_none());
        assert_eq!(p.view.selected, Some(p3.id));
        assert!(p.rig.conflicts(&p.library).is_empty());
        // Its sheet row follows it, from between P2 and P4 to after PY2
        // (153).
        let y = |label: &str| {
            harness
                .get_by_label(&format!("{label} address"))
                .rect()
                .top()
        };
        assert!(y("P3") > y("PY2"), "re-sorted below PY2");
        assert!(y("P2") < y("P4"));
        // Nothing else moved.
        for (f, was) in p.rig.iter().zip(before.iter()) {
            if f.id != p3.id {
                assert_eq!(
                    (f.universe, f.start_address),
                    (was.universe, was.start_address)
                );
            }
        }
    }

    #[test]
    fn escape_mid_drag_puts_the_fixture_back() {
        let mut harness = harness();
        let from = channel_pos(&harness, 93);
        let to = channel_pos(&harness, 200);
        press_and_move(&mut harness, from, to);
        assert_ne!(harness.state().fixture("P3").start_address, 93);

        harness.key_press(egui::Key::Escape);
        harness.run();
        harness.drop_at(to);
        harness.run();

        let p = harness.state();
        assert_eq!(p.fixture("P3").start_address, 93);
        assert!(p.view.drag.is_none());
    }

    // ---- LAYOUT ----

    /// The harness, on LAYOUT.
    fn layout_harness() -> Harness<'static, Patch> {
        let mut harness = harness();
        harness.get_by_label("Layout").click();
        harness.run();
        harness
    }

    /// The centre of `cell` on the stage.
    fn cell_pos(harness: &Harness<'_, Patch>, col: u8, row: u8) -> Pos2 {
        let stage = harness.get_by_label(layout::STAGE_LABEL).rect();
        let (w, h) = (
            stage.width() / f32::from(halo_light::fixture::STAGE_COLS),
            stage.height() / f32::from(halo_light::fixture::STAGE_ROWS),
        );
        stage.min + vec2((f32::from(col) + 0.5) * w, (f32::from(row) + 0.5) * h)
    }

    fn cell(col: u8, row: u8) -> Option<halo_light::fixture::Cell> {
        Some(halo_light::fixture::Cell::new(col, row))
    }

    #[test]
    fn the_switch_shows_the_stage_in_place_of_the_map() {
        let mut harness = harness();
        assert!(harness.query_by_label("Universe 1").is_some());
        assert!(harness.query_by_label(layout::STAGE_LABEL).is_none());
        harness.get_by_label("Layout").click();
        harness.run();
        assert!(harness.query_by_label("Universe 1").is_none(), "no pads");
        assert!(harness.query_by_label(MAP_LABEL).is_none());
        harness.get_by_label(layout::STAGE_LABEL);
        harness.get_by_label("Arranged automatically");
        harness.get_by_label("Map").click();
        harness.run();
        harness.get_by_label("Universe 1");
    }

    #[test]
    fn dragging_a_fixture_places_it_by_hand() {
        let mut harness = layout_harness();
        // S1 is arranged top left; drop it bottom left.
        let from = cell_pos(&harness, 0, 0);
        let to = cell_pos(&harness, 0, 7);
        press_and_move(&mut harness, from, to);
        harness.drop_at(to);
        harness.run();
        let p = harness.state();
        let s1 = p.fixture("S1");
        assert_eq!(s1.pos, cell(0, 7));
        assert_eq!(p.view.selected, Some(s1.id), "selected, as on the map");
        assert_eq!(p.rig.placed(), 1);
        // The other spots keep their cells.
        assert_eq!(p.fixture("S2").pos, None);
        harness.get_by_label("1 fixture placed by hand");
    }

    #[test]
    fn dropping_on_a_fixture_swaps_the_two() {
        let mut harness = layout_harness();
        // S1 (0, 0) onto S2 (1, 0).
        let from = cell_pos(&harness, 0, 0);
        let to = cell_pos(&harness, 1, 0);
        press_and_move(&mut harness, from, to);
        harness.drop_at(to);
        harness.run();
        let p = harness.state();
        assert_eq!(p.fixture("S1").pos, cell(1, 0));
        assert_eq!(p.fixture("S2").pos, cell(0, 0));
    }

    #[test]
    fn passing_over_a_fixture_leaves_it_where_it_was() {
        let mut harness = layout_harness();
        // S1 across S2 (1, 0) and on to an empty cell.
        let from = cell_pos(&harness, 0, 0);
        let over = cell_pos(&harness, 1, 0);
        press_and_move(&mut harness, from, over);
        assert_eq!(harness.state().fixture("S2").pos, cell(0, 0), "making room");
        let to = cell_pos(&harness, 3, 7);
        press_and_move_held(&mut harness, over, to);
        harness.drop_at(to);
        harness.run();
        let p = harness.state();
        assert_eq!(p.fixture("S1").pos, cell(3, 7));
        assert_eq!(p.fixture("S2").pos, None, "back, still automatic");
        assert_eq!(p.rig.placed(), 1);
    }

    #[test]
    fn escape_mid_drag_puts_both_back() {
        let mut harness = layout_harness();
        let from = cell_pos(&harness, 0, 0);
        let to = cell_pos(&harness, 1, 0);
        press_and_move(&mut harness, from, to);
        harness.key_press(egui::Key::Escape);
        harness.run();
        harness.drop_at(to);
        harness.run();
        let p = harness.state();
        assert_eq!((p.fixture("S1").pos, p.fixture("S2").pos), (None, None));
        assert!(p.view.stage_drag.is_none());
    }

    #[test]
    fn arrow_keys_nudge_the_focused_fixture() {
        let mut harness = layout_harness();
        // P1 sits at (0, 1), P2 at (1, 1); (0, 2) is free.
        let p1 = cell_pos(&harness, 0, 1);
        harness.hover_at(p1);
        harness.run();
        harness.drag_at(p1);
        harness.run();
        harness.drop_at(p1);
        harness.run();
        harness.key_press(egui::Key::ArrowDown);
        harness.run();
        assert_eq!(harness.state().fixture("P1").pos, cell(0, 2));
        // Left at the edge stays put; up and right lands on P2, swapping.
        harness.key_press(egui::Key::ArrowLeft);
        harness.run();
        assert_eq!(harness.state().fixture("P1").pos, cell(0, 2));
        harness.key_press(egui::Key::ArrowUp);
        harness.run();
        harness.key_press(egui::Key::ArrowRight);
        harness.run();
        let p = harness.state();
        assert_eq!(p.fixture("P1").pos, cell(1, 1));
        assert_eq!(p.fixture("P2").pos, cell(0, 1));
        assert_eq!(p.view.selected, Some(p.fixture("P1").id));
    }

    #[test]
    fn auto_arrange_clears_every_hand_placed_cell() {
        let mut harness = layout_harness();
        let disabled = |h: &Harness<'_, Patch>| {
            h.get_by_label("AUTO ARRANGE")
                .accesskit_node()
                .is_disabled()
        };
        assert!(disabled(&harness), "nothing placed yet");
        harness.state_mut().rig.fixtures_mut()[0].pos = cell(7, 7);
        harness.state_mut().rig.fixtures_mut()[3].pos = cell(6, 7);
        harness.run();
        harness.get_by_label("2 fixtures placed by hand");
        assert!(!disabled(&harness));
        harness.get_by_label("AUTO ARRANGE").click();
        harness.run();
        assert_eq!(harness.state().rig.placed(), 0);
        harness.get_by_label("Arranged automatically");
        assert!(disabled(&harness));
    }
}
