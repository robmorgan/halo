//! The programmer surface in the footer: fixture grid + lane overrides on
//! the left, the parameter views (Intensity / Color / Position / Beam /
//! Pixel FX) with their beat-synced effect panels in the middle, and the
//! CLEAR / STORE / PREVIEW / HIGHLIGHT / ENERGY action column on the right.
//!
//! Everything here edits real state, but values and effects are
//! visual-only until the fixture engine consumes them.

use std::collections::HashSet;

use eframe::egui;
use halo_light::cues::LANE_COUNT;
use halo_light::fixture::{ALL_KINDS, FixtureKind, Rig};
use halo_light::programmer::{
    self, ALL_INTERVALS, ALL_VIEWS, ALL_WAVEFORMS, COLOR_PRESETS, Distribution, EffectConfig,
    LaneOutput, LaneSource, PIXEL_EFFECTS, PanTiltTarget, ParamView, Programmer, ProgrammerParams,
    effect_value,
};

use crate::fader::{Fader, Notches};
use crate::footer::StatusHelp;
use crate::key::{Key, Lit};
use crate::knob::{Knob, KnobArc};
use crate::theme;

/// Everything the programmer panel reads and writes.
pub struct ProgrammerCtx<'a> {
    pub rig: &'a Rig,
    pub selection: &'a mut HashSet<u32>,
    pub overrides: &'a mut Programmer,
    pub params: &'a mut ProgrammerParams,
    pub outputs: &'a [LaneOutput; LANE_COUNT],
    pub can_store: bool,
    pub deck_name: &'a str,
    /// Musical time in beats (beat index + intra-beat phase) driving the
    /// effect previews.
    pub beat_t: f64,
}

const ACTION_COL_W: f32 = 112.0;
const FADER_W: f32 = 20.0;
/// Fixed width of the parameter column so the effects panel never shifts
/// when the view changes (sized for the widest view: Color's four faders
/// plus the swatch column).
const PARAMS_W: f32 = 300.0;

/// A small key for one choice of a set; the current choice is selected.
fn choice(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    ui.add(Key::new(label).small().selected(selected))
}

/// A spaced caption over a programmer group.
fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(theme::CAPTION.rich(text).color(theme::TEXT_MUTED));
}

/// Returns true when STORE was pressed.
pub fn programmer_panel(ui: &mut egui::Ui, cx: &mut ProgrammerCtx<'_>) -> bool {
    let mut store = false;
    // A section you change here is taken from the deck's look until CLEAR.
    let before = programmer::Sections::of(cx.params);
    ui.horizontal_top(|ui| {
        // Left: group selects, rig grid, lane override row — the groups
        // row and the view tabs sit on the same top line.
        ui.vertical(|ui| {
            groups_row(ui, cx.rig, cx.selection);
            ui.add_space(theme::SPACE_8);
            theme::well(ui, theme::SPACE_8, |ui| {
                fixture_grid(ui, cx.rig, cx.selection, cx.outputs, &cx.params.held);
            });
            ui.add_space(theme::SPACE_8);
            ui.horizontal(|ui| {
                for (i, o) in cx.overrides.iter_mut().enumerate() {
                    lane_controls(ui, o, i);
                    ui.add_space(theme::SPACE_8);
                }
            });
        });
        programmer_divider(ui);

        // Middle: view tabs + the active view + its effect panel.
        let center_w = (ui.available_width() - ACTION_COL_W - 20.0).max(300.0);
        ui.vertical(|ui| {
            ui.set_width(center_w);
            ui.horizontal(|ui| {
                for (view, label) in ALL_VIEWS {
                    if choice(ui, cx.params.view == view, label).clicked() {
                        cx.params.view = view;
                    }
                }
            });
            ui.add_space(theme::SPACE_8);
            ui.horizontal_top(|ui| {
                let beat_t = cx.beat_t;
                // Fixed-width parameter column: the effects panel to its
                // right stays put across view changes. The pad after it is
                // measured, so even a view that overflows the nominal
                // width can't push the panel around between frames.
                let params_left = ui.cursor().left();
                ui.vertical(|ui| {
                    ui.set_width(PARAMS_W);
                    // Intensity and colour belong to the fixtures selected
                    // as they're set: the whole rig when none are.
                    let targets = programmer::targets(cx.rig, cx.selection);
                    let selection = &*cx.selection;
                    ui.horizontal_top(|ui| match cx.params.view {
                        ParamView::Intensity => intensity_view(ui, cx.params, &targets, selection),
                        ParamView::Color => color_view(ui, cx.params, &targets, selection),
                        ParamView::Position => position_view(ui, &mut cx.params.position),
                        ParamView::Beam => beam_view(ui, &mut cx.params.beam),
                        // Pixel effects are pre-baked patterns; the
                        // standard effects panel doesn't apply.
                        ParamView::PixelFx => {
                            let bars = programmer::pixel_targets(cx.rig, selection);
                            pixel_view(ui, cx.params, &bars);
                        }
                    });
                });
                let pad = params_left + PARAMS_W + 10.0 - ui.cursor().left();
                if pad > 0.0 {
                    ui.add_space(pad);
                }
                let effect = match cx.params.view {
                    ParamView::Intensity => Some((
                        &mut cx.params.intensity.effect,
                        crate::waveform::palette::LANE_LIGHTING,
                    )),
                    ParamView::Color => {
                        Some((&mut cx.params.color.effect, theme::LOOK_AMBER_SWEEP))
                    }
                    ParamView::Position => Some((&mut cx.params.position.effect, POSITION_ACCENT)),
                    ParamView::Beam => Some((&mut cx.params.beam.effect, theme::LOOK_STROBE_WHITE)),
                    ParamView::PixelFx => None,
                };
                if let Some((cfg, accent)) = effect {
                    effect_panel(ui, cfg, accent, beat_t);
                }
            });
        });
        programmer_divider(ui);

        // Right: stacked actions.
        store = action_column(ui, cx);
    });
    let after = programmer::Sections::of(cx.params);
    cx.params.owned.mark(&before, &after);
    store
}

/// Accent of the Position view's effect, pad dot and faders.
const POSITION_ACCENT: egui::Color32 = theme::LOOK_UV_VIOLET;

/// The groove between the programmer's columns: a 1px `edge` line with a
/// 1px `lip` beside it, as tall as the column beside it.
fn programmer_divider(ui: &mut egui::Ui) {
    let height = ui.min_rect().height().max(theme::SIZE_KEY);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, height), egui::Sense::hover());
    let x = rect.left() + 3.5;
    ui.painter()
        .vline(x, rect.y_range(), egui::Stroke::new(1.0, theme::EDGE));
    ui.painter()
        .vline(x + 1.0, rect.y_range(), egui::Stroke::new(1.0, theme::LIP));
}

/// Group-select row: click selects exactly that kind's fixtures,
/// shift-click unions, ALL selects everything.
fn groups_row(ui: &mut egui::Ui, rig: &Rig, selection: &mut HashSet<u32>) {
    let shift = ui.input(|i| i.modifiers.shift);
    ui.horizontal(|ui| {
        let all: HashSet<u32> = rig.ids().collect();
        let all_selected = !all.is_empty() && selection.len() == all.len();
        if choice(ui, all_selected, "ALL").clicked() {
            *selection = all;
        }
        for kind in ALL_KINDS {
            let ids: HashSet<u32> = rig.ids_of_kind(kind).collect();
            let active = !ids.is_empty() && ids.is_subset(selection);
            if choice(ui, active, kind.group_label()).clicked() {
                if shift {
                    selection.extend(&ids);
                } else {
                    *selection = ids;
                }
            }
        }
        ui.add_space(10.0);
        let n = selection.len();
        // Where a dimmer or colour edit goes.
        let caption = if n == 0 {
            "→ whole rig".to_string()
        } else {
            format!("→ {n} selected")
        };
        ui.label(theme::NOTE_TEXT.rich(caption).color(theme::TEXT_MUTED));
    });
}

/// Fader height that fills the footer space left below the view tabs
/// (the ~40 pt reserve holds the readout + caption under each fader).
fn fill_fader_height(ui: &egui::Ui) -> f32 {
    (ui.available_height() - 40.0).clamp(140.0, 420.0)
}

/// Spec for one labeled vertical fader column.
struct FaderCol<'a> {
    label: &'a str,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
    accent: egui::Color32,
    unit: &'a str,
    height: f32,
}

/// One labeled vertical fader with a live value readout. Returns whether
/// it moved.
fn fader_col(ui: &mut egui::Ui, value: &mut f32, spec: FaderCol<'_>) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.set_width(48.0);
        ui.vertical_centered(|ui| {
            changed = ui
                .add(
                    Fader::new(value, spec.range, spec.accent)
                        .size((FADER_W, spec.height))
                        .notches(Notches::Even(4))
                        .default_value(spec.default),
                )
                .changed();
            ui.label(
                theme::NUMERALS_SM
                    .rich(format!("{value:.0}{}", spec.unit))
                    .color(theme::TEXT_STRONG),
            );
            caption(ui, spec.label);
        });
    });
    changed
}

fn intensity_view(
    ui: &mut egui::Ui,
    p: &mut ProgrammerParams,
    targets: &[u32],
    selection: &HashSet<u32>,
) {
    let accent = crate::waveform::palette::LANE_LIGHTING;
    let height = fill_fader_height(ui);
    let spec = |label, default| FaderCol {
        label,
        range: 0.0..=100.0,
        default,
        accent,
        unit: "%",
        height,
    };
    let mut dimmer = p.shown_dimmer(selection);
    if fader_col(ui, &mut dimmer, spec("DIMMER", 100.0)) {
        p.hold_dimmer(targets, dimmer);
    }
    fader_col(ui, &mut p.intensity.strobe, spec("STROBE", 0.0));
}

fn color_view(
    ui: &mut egui::Ui,
    p: &mut ProgrammerParams,
    targets: &[u32],
    selection: &HashSet<u32>,
) {
    const ACCENTS: [(usize, &str, egui::Color32); 4] = [
        (0, "R", theme::FIXTURE_R),
        (1, "G", theme::FIXTURE_G),
        (2, "B", theme::FIXTURE_B),
        (3, "W", theme::FIXTURE_W),
    ];
    let height = fill_fader_height(ui);
    let mut rgbw = p.shown_rgbw(selection);
    let mut changed = false;
    let mut picked = None;
    for (i, label, accent) in ACCENTS {
        changed |= fader_col(
            ui,
            &mut rgbw[i],
            FaderCol {
                label,
                range: 0.0..=100.0,
                default: 0.0,
                accent,
                unit: "",
                height,
            },
        );
    }
    ui.add_space(6.0);
    ui.vertical(|ui| {
        // Mixed-color swatch (white adds broadly to all channels).
        let mix = |c: f32| (((c + rgbw[3] * 0.9) / 100.0).clamp(0.0, 1.0) * 255.0) as u8;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 22.0), egui::Sense::hover());
        ui.painter().rect_filled(
            rect,
            theme::RADIUS_CONTROL,
            egui::Color32::from_rgb(mix(rgbw[0]), mix(rgbw[1]), mix(rgbw[2])),
        );
        theme::paint_edge(ui.painter(), rect, theme::RADIUS_CONTROL, theme::EDGE);
        ui.add_space(4.0);
        // Preset swatches set the faders instantly.
        egui::Grid::new("color_presets")
            .spacing([4.0, 4.0])
            .min_col_width(18.0)
            .show(ui, |ui| {
                for (i, &(name, preset)) in COLOR_PRESETS.iter().enumerate() {
                    if color_swatch(ui, preset, false).status_help(name).clicked() {
                        picked = Some(preset);
                    }
                    if i % 5 == 4 {
                        ui.end_row();
                    }
                }
            });
    });
    if let Some(preset) = picked {
        rgbw = preset;
        changed = true;
    }
    if changed {
        p.hold_rgbw(targets, rgbw);
    }
}

/// Small clickable color swatch for a preset.
fn color_swatch(ui: &mut egui::Ui, rgbw: [f32; 4], selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
    let mix = |c: f32| (((c + rgbw[3] * 0.9) / 100.0).clamp(0.0, 1.0) * 255.0) as u8;
    ui.painter().rect_filled(
        rect,
        theme::RADIUS_CONTROL,
        egui::Color32::from_rgb(mix(rgbw[0]), mix(rgbw[1]), mix(rgbw[2])),
    );
    theme::paint_edge(ui.painter(), rect, theme::RADIUS_CONTROL, theme::EDGE);
    if selected {
        ui.painter().rect_stroke(
            rect.expand(1.0),
            theme::RADIUS_CONTROL,
            egui::Stroke::new(1.5_f32, theme::TEXT_STRONG),
            egui::StrokeKind::Outside,
        );
    }
    resp
}

fn position_view(ui: &mut egui::Ui, p: &mut halo_light::programmer::PositionParams) {
    let accent = POSITION_ACCENT;
    let height = fill_fader_height(ui);
    for (value, label) in [(&mut p.pan, "PAN"), (&mut p.tilt, "TILT")] {
        fader_col(
            ui,
            value,
            FaderCol {
                label,
                range: 0.0..=360.0,
                default: 180.0,
                accent,
                unit: "°",
                height,
            },
        );
    }
    ui.add_space(6.0);
    ui.vertical(|ui| {
        // XY pad: pan on x, tilt on y (up = more tilt); the dot drags.
        const PAD: f32 = 96.0;
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(PAD, PAD), egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect.expand(1.0));
        theme::paint_well(&painter, rect, theme::RADIUS_CONTROL);
        for f in [0.25, 0.5, 0.75] {
            let x = rect.left() + rect.width() * f;
            let y = rect.top() + rect.height() * f;
            let stroke = egui::Stroke::new(
                1.0_f32,
                if f == 0.5 {
                    theme::KEY
                } else {
                    theme::SEGMENT_OFF
                },
            );
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                stroke,
            );
        }
        if (resp.dragged() || resp.clicked())
            && let Some(pos) = resp.interact_pointer_pos()
        {
            p.pan = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0) * 360.0;
            p.tilt = (1.0 - (pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0) * 360.0;
        }
        let dot = egui::pos2(
            rect.left() + rect.width() * (p.pan / 360.0),
            rect.top() + rect.height() * (1.0 - p.tilt / 360.0),
        );
        painter.circle_filled(dot, 4.5, accent);
        painter.circle_stroke(dot, 6.0, egui::Stroke::new(1.0_f32, theme::TEXT_STRONG));

        // Which axes the effect drives.
        ui.add_space(theme::SPACE_8);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_4;
            for (t, label) in [
                (PanTiltTarget::Both, "BOTH"),
                (PanTiltTarget::Pan, "PAN"),
                (PanTiltTarget::Tilt, "TILT"),
            ] {
                if choice(ui, p.target == t, label)
                    .status_help("Apply the effect to these axes")
                    .clicked()
                {
                    p.target = t;
                }
            }
        });
    });
}

fn beam_view(ui: &mut egui::Ui, p: &mut halo_light::programmer::BeamParams) {
    ui.vertical(|ui| {
        caption(ui, "GOBO");
        ui.add_space(theme::SPACE_6);
        egui::Grid::new("gobo_grid")
            .spacing([theme::SPACE_8, theme::SPACE_8])
            .show(ui, |ui| {
                // OPEN, then G1–G8, three to a row.
                for g in 0..=8u8 {
                    let label = if g == 0 {
                        "OPEN".to_string()
                    } else {
                        format!("G{g}")
                    };
                    if ui
                        .add(
                            Key::new(label)
                                .size([theme::SIZE_PAD, theme::SIZE_KEY])
                                .selected(p.gobo == g),
                        )
                        .clicked()
                    {
                        p.gobo = g;
                    }
                    if g % 3 == 2 {
                        ui.end_row();
                    }
                }
            });
    });
}

/// Picking an effect or colour brings `bars` up, so they show it without
/// a lane.
fn pixel_view(ui: &mut egui::Ui, params: &mut ProgrammerParams, bars: &[u32]) {
    let p = &mut params.pixel;
    let mut picked = false;
    ui.vertical(|ui| {
        caption(ui, "EFFECT");
        ui.add_space(theme::SPACE_6);
        egui::ScrollArea::vertical()
            .id_salt("pixel_fx_list")
            .max_height(118.0)
            .show(ui, |ui| {
                for (i, name) in PIXEL_EFFECTS.iter().enumerate() {
                    if ui.selectable_label(p.effect == i, *name).clicked() {
                        p.effect = i;
                        picked = true;
                    }
                }
            });
    });
    ui.add_space(10.0);
    ui.vertical(|ui| {
        caption(ui, "COLOR");
        ui.add_space(theme::SPACE_6);
        egui::Grid::new("pixel_colors")
            .spacing([4.0, 4.0])
            .min_col_width(18.0)
            .show(ui, |ui| {
                for (i, &(name, rgbw)) in COLOR_PRESETS.iter().enumerate() {
                    if color_swatch(ui, rgbw, p.color == i)
                        .status_help(name)
                        .clicked()
                    {
                        p.color = i;
                        picked = true;
                    }
                    if i % 5 == 4 {
                        ui.end_row();
                    }
                }
            });
        ui.add_space(6.0);
        ui.label(
            theme::NOTE_TEXT
                .rich(format!(
                    "{} · {}",
                    PIXEL_EFFECTS[p.effect], COLOR_PRESETS[p.color].0
                ))
                .color(theme::TEXT_MUTED),
        );
    });
    if picked {
        params.bring_up(bars);
    }
}

/// The shared per-parameter effects panel with a beat-synced preview.
fn effect_panel(ui: &mut egui::Ui, cfg: &mut EffectConfig, accent: egui::Color32, beat_t: f64) {
    // The panel is embedded in a horizontal row; force its own content
    // back to a vertical stack.
    theme::well(ui, theme::SPACE_8, |ui| {
        ui.vertical(|ui| {
            ui.set_width(EFFECT_W);
            ui.spacing_mut().item_spacing = egui::vec2(theme::SPACE_4, theme::SPACE_6);
            caption(ui, "EFFECT");
            ui.horizontal_wrapped(|ui| {
                for (wf, label) in ALL_WAVEFORMS {
                    if choice(ui, cfg.waveform == wf, label).clicked() {
                        cfg.waveform = wf;
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (iv, label) in ALL_INTERVALS {
                    if choice(ui, cfg.interval == iv, label).clicked() {
                        cfg.interval = iv;
                    }
                }
            });
            ui.spacing_mut().slider_width = 100.0;
            ui.horizontal(|ui| {
                caption(ui, "RATIO");
                ui.add(
                    egui::Slider::new(&mut cfg.ratio, 0.0..=2.0)
                        .fixed_decimals(2)
                        .handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.5 }),
                );
            });
            ui.horizontal(|ui| {
                caption(ui, "PHASE");
                ui.add(
                    egui::Slider::new(&mut cfg.phase_deg, 0.0..=360.0)
                        .fixed_decimals(0)
                        .suffix("°")
                        .handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.5 }),
                );
            });
            ui.horizontal(|ui| {
                caption(ui, "DIST");
                let is_all = cfg.distribution == Distribution::All;
                if choice(ui, is_all, "ALL").clicked() {
                    cfg.distribution = Distribution::All;
                }
                let is_step = matches!(cfg.distribution, Distribution::Step(_));
                if choice(ui, is_step, "STEP").clicked() {
                    cfg.distribution = Distribution::Step(2);
                }
                let is_wave = matches!(cfg.distribution, Distribution::Wave(_));
                if choice(ui, is_wave, "WAVE").clicked() {
                    cfg.distribution = Distribution::Wave(45);
                }
                match &mut cfg.distribution {
                    Distribution::Step(n) => {
                        ui.add(egui::DragValue::new(n).range(1..=32));
                    }
                    Distribution::Wave(offset) => {
                        ui.add(egui::DragValue::new(offset).range(0..=360).suffix("°"));
                    }
                    Distribution::All => {}
                }
            });
            effect_preview(ui, cfg, accent, beat_t);
            // A latch: an LED in the effect's colour; the label is the
            // state.
            let label = if cfg.applied { "APPLIED" } else { "APPLY" };
            if ui
                .add(
                    Key::new(label)
                        .width(EFFECT_W)
                        .led(cfg.applied, accent)
                        .label_color(if cfg.applied {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_UNLIT
                        }),
                )
                .status_help("Latch this effect (visual-only until the fixture engine lands)")
                .clicked()
            {
                cfg.applied = !cfg.applied;
            }
        });
    });
}

/// Width of the effect panel's content.
const EFFECT_W: f32 = 232.0;

/// Musical window the preview shows, in intervals. A few cycles at once
/// keeps the "now" dot travelling calmly (once per bar at the default
/// beat interval) instead of whipping across on every beat.
const PREVIEW_SPAN: f64 = 4.0;

/// A few intervals of the configured waveform with a dot riding the
/// actual musical phase, plus a fainter offset trace hinting Step/Wave
/// spread.
fn effect_preview(ui: &mut egui::Ui, cfg: &EffectConfig, accent: egui::Color32, beat_t: f64) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(EFFECT_W), 44.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect.expand(1.0));
    // A content well inside the effect tray, on the lanes' ground.
    theme::paint_content_well(&painter, rect, theme::LANE_GROUND);
    let mid = rect.center().y;
    painter.line_segment(
        [egui::pos2(rect.left(), mid), egui::pos2(rect.right(), mid)],
        egui::Stroke::new(1.0_f32, theme::LANE_SEPARATOR),
    );
    let inset = rect.shrink2(egui::vec2(3.0, 4.0));
    let y_of = |v: f32| inset.bottom() - v * inset.height();
    let curve = |phase_offset: f64| -> Vec<egui::Pos2> {
        (0..=192)
            .map(|i| {
                let frac = i as f64 / 192.0;
                egui::pos2(
                    inset.left() + inset.width() * frac as f32,
                    y_of(effect_value(cfg, frac * PREVIEW_SPAN + phase_offset)),
                )
            })
            .collect()
    };
    // Offset trace first (under the main one) when the effect spreads.
    let spread_offset = match cfg.distribution {
        Distribution::All => None,
        Distribution::Step(n) => Some(1.0 / n.max(2) as f64),
        Distribution::Wave(deg) => Some(deg as f64 / 360.0),
    };
    if let Some(off) = spread_offset {
        painter.add(egui::Shape::line(
            curve(off),
            egui::Stroke::new(1.0_f32, accent.gamma_multiply(0.3)),
        ));
    }
    painter.add(egui::Shape::line(
        curve(0.0),
        egui::Stroke::new(1.5_f32, accent),
    ));
    // The "now" dot, riding the deck's beat grid across the whole window.
    let t_now = (beat_t / cfg.interval.beats()).rem_euclid(PREVIEW_SPAN);
    let dot = egui::pos2(
        inset.left() + inset.width() * (t_now / PREVIEW_SPAN) as f32,
        y_of(effect_value(cfg, t_now)),
    );
    painter.circle_filled(dot, 3.0, theme::TEXT_STRONG);
}

/// CLEAR / STORE / PREVIEW / HIGHLIGHT and the ENERGY fader, stacked on the
/// panel's right edge.
fn action_column(ui: &mut egui::Ui, cx: &mut ProgrammerCtx<'_>) -> bool {
    let mut store = false;
    ui.vertical(|ui| {
        ui.set_width(ACTION_COL_W);
        ui.spacing_mut().item_spacing.y = theme::SPACE_8;
        // CLEAR lights amber while any lane is latched: there's something
        // live to release.
        let armed = programmer::any_latched(cx.overrides) || cx.params.holds_any();
        if ui
            .add(
                Key::new("CLEAR")
                    .width(ACTION_COL_W)
                    .lit(armed.then_some(Lit::Accent)),
            )
            .status_help("Release latched lanes, held fixtures and edited parameters (Esc)")
            .clicked()
        {
            programmer::clear(cx.overrides);
            cx.params.release();
        }
        ui.add_enabled_ui(cx.can_store, |ui| {
            if ui
                .add(Key::new("STORE").width(ACTION_COL_W))
                .status_help(format!(
                    "Write the active lanes into deck {}'s track as cues at the current bar",
                    cx.deck_name
                ))
                .clicked()
            {
                store = true;
            }
        });
        ui.add_space(theme::SPACE_8);
        for (value, label, hint) in [
            (
                &mut cx.params.preview,
                "PREVIEW",
                "Blind: edit programmer values without sending them to the rig",
            ),
            (
                &mut cx.params.highlight,
                "HIGHLIGHT",
                "Snap the selected fixtures to full white for identification",
            ),
        ] {
            let key = Key::new(label)
                .width(ACTION_COL_W)
                .led(*value, theme::ACCENT)
                .label_color(if *value {
                    theme::TEXT_STRONG
                } else {
                    theme::TEXT_UNLIT
                });
            if ui.add(key).status_help(hint).clicked() {
                *value = !*value;
            }
        }
        ui.add_space(theme::SPACE_8);
        // The grand master on the deck's NRG curve: everything but
        // HIGHLIGHT follows it.
        ui.horizontal(|ui| {
            caption(ui, "ENERGY");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    theme::NOTE_TEXT
                        .rich(format!("{:.0}%", cx.params.energy * 100.0))
                        .color(theme::TEXT_MUTED),
                );
            });
        });
        ui.add(
            Fader::new(&mut cx.params.energy, 0.0..=1.0, theme::ENERGY)
                .vertical(false)
                .size([ACTION_COL_W, theme::SIZE_KEY])
                .default_value(1.0),
        )
        .status_help("Scale the whole rig with the track's NRG curve");
        ui.label(
            theme::NOTE_TEXT
                .rich(format!("out: deck {}", cx.deck_name))
                .color(theme::TEXT_MUTED),
        );
    });
    store
}

/// One lane's compact override cluster: colored label, latching ON,
/// momentary FLASH, intensity knob.
fn lane_controls(ui: &mut egui::Ui, o: &mut halo_light::programmer::LaneOverride, i: usize) {
    let (_, label, color) = crate::waveform::LANES[i];
    // The lane's code lights in its colour while the lane is overridden.
    ui.label(
        theme::CAPTION
            .rich(label)
            .color(if o.active() { color } else { theme::TEXT_MUTED }),
    );
    let latch = Key::new("ON")
        .led(o.latched, color)
        .label_color(if o.latched {
            theme::TEXT_STRONG
        } else {
            theme::TEXT_UNLIT
        });
    if ui
        .add(latch)
        .status_help("Latch this lane on until CLEAR")
        .clicked()
    {
        o.latched = !o.latched;
    }
    let flash = ui
        .add(Key::new("FLASH").lit_when_held(Lit::Accent))
        .status_help("Active while held");
    if flash.is_pointer_button_down_on() {
        o.flash_held = true;
    }
    let mut v = o.intensity;
    if ui
        .add(
            Knob::new(&mut v, 0.0..=1.0, color)
                .arc(KnobArc::Unipolar)
                .default_value(1.0)
                .diameter(theme::SIZE_KEY),
        )
        .on_hover_text(format!("Intensity: {:.0}%", o.intensity * 100.0))
        .changed()
    {
        o.intensity = v;
    }
}

/// Clickable rig grid, laid out to mirror the stage: cells glow with
/// their lane's current output level and carry the same white-ring
/// programmer-override language as the LEDs and lane strips. Click
/// selects, shift-click toggles, press-drag paints, background click
/// clears. Selection is the target for future per-fixture effects/colors;
/// it is not consumed by the output path yet.
pub fn fixture_grid(
    ui: &mut egui::Ui,
    rig: &Rig,
    selection: &mut HashSet<u32>,
    outputs: &[LaneOutput; LANE_COUNT],
    held: &std::collections::BTreeMap<u32, programmer::Held>,
) {
    const CELL_W: f32 = 40.0;
    const CELL_H: f32 = 28.0;
    const GAP: f32 = 4.0;
    let (cols, rows) = rig.extent();
    let size = egui::vec2(
        cols as f32 * (CELL_W + GAP) - GAP,
        rows as f32 * (CELL_H + GAP) - GAP,
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect.expand(2.0));

    let shift = ui.input(|i| i.modifiers.shift);
    let pointer = response.interact_pointer_pos();
    let clicked = response.clicked();
    let painting = response.drag_started() || response.dragged();
    let mut background_click = clicked;

    for f in rig.iter() {
        let min =
            rect.min + egui::vec2(f.col as f32 * (CELL_W + GAP), f.row as f32 * (CELL_H + GAP));
        let cell = egui::Rect::from_min_size(min, egui::vec2(CELL_W, CELL_H));
        let lane = outputs[f.kind.lane() as usize];
        // A fixture the programmer holds shows its own level.
        let held_dimmer = held.get(&f.id).and_then(|h| h.dimmer);
        let (level, programmed) = match held_dimmer {
            Some(dimmer) => (dimmer / 100.0, true),
            None => (lane.level, lane.source == LaneSource::Programmer),
        };
        let selected = selection.contains(&f.id);

        let alpha = 0.15 + 0.85 * level.clamp(0.0, 1.0);
        let [r, g, b] = f.kind.color();
        painter.rect_filled(
            cell,
            theme::RADIUS_CONTROL,
            egui::Color32::from_rgb(r, g, b).gamma_multiply(alpha),
        );
        if programmed {
            painter.rect_stroke(
                cell.shrink(1.5),
                2.0,
                egui::Stroke::new(1.0_f32, theme::TEXT_STRONG.gamma_multiply(0.55)),
                egui::StrokeKind::Inside,
            );
        }
        if selected {
            painter.rect_stroke(
                cell,
                theme::RADIUS_CONTROL,
                egui::Stroke::new(1.5_f32, theme::TEXT_STRONG),
                egui::StrokeKind::Outside,
            );
        }
        // Bright fills (strobes glowing white, smoke grey) need dark text.
        let bright = matches!(f.kind, FixtureKind::Strobe | FixtureKind::Smoke) && alpha > 0.5;
        theme::paint_text(
            &painter,
            cell.center(),
            egui::Align2::CENTER_CENTER,
            &f.label,
            theme::PAD_NUMBER.sized(10.0),
            if bright {
                theme::WELL
            } else {
                theme::TEXT_STRONG
            },
        );

        if pointer.is_some_and(|p| cell.contains(p)) {
            if clicked {
                background_click = false;
                if shift {
                    if !selection.remove(&f.id) {
                        selection.insert(f.id);
                    }
                } else {
                    selection.clear();
                    selection.insert(f.id);
                }
            } else if painting {
                selection.insert(f.id);
            }
        }
    }
    if background_click {
        selection.clear();
    }
}
