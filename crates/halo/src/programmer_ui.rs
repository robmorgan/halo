//! The programmer surface in the footer: fixture grid + lane overrides on
//! the left, the parameter views (Intensity / Color / Position / Beam /
//! Pixel FX) with their beat-synced effect panels in the middle, and the
//! CLEAR / STORE / PREVIEW / HIGHLIGHT action column on the right.
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
use crate::knob::{Knob, KnobArc};

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

const ACTION_COL_W: f32 = 96.0;
const FADER_W: f32 = 20.0;
/// Fixed width of the parameter column so the effects panel never shifts
/// when the view changes (sized for the widest view: Color's four faders
/// plus the swatch column).
const PARAMS_W: f32 = 300.0;

/// Returns true when STORE was pressed.
pub fn programmer_panel(ui: &mut egui::Ui, cx: &mut ProgrammerCtx<'_>) -> bool {
    let mut store = false;
    ui.horizontal_top(|ui| {
        // Left: group selects, rig grid, lane override row — the groups
        // row and the view tabs sit on the same top line.
        ui.vertical(|ui| {
            groups_row(ui, cx.rig, cx.selection);
            ui.add_space(6.0);
            fixture_grid(ui, cx.rig, cx.selection, cx.outputs);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for (i, o) in cx.overrides.iter_mut().enumerate() {
                    lane_controls(ui, o, i);
                    ui.add_space(4.0);
                }
            });
        });
        ui.separator();

        // Middle: view tabs + the active view + its effect panel.
        let center_w = (ui.available_width() - ACTION_COL_W - 20.0).max(300.0);
        ui.vertical(|ui| {
            ui.set_width(center_w);
            ui.horizontal(|ui| {
                for (view, label) in ALL_VIEWS {
                    if ui
                        .selectable_label(
                            cx.params.view == view,
                            egui::RichText::new(label).size(10.0),
                        )
                        .clicked()
                    {
                        cx.params.view = view;
                    }
                }
            });
            ui.add_space(6.0);
            ui.horizontal_top(|ui| {
                let beat_t = cx.beat_t;
                // Fixed-width parameter column: the effects panel to its
                // right stays put across view changes. The pad after it is
                // measured, so even a view that overflows the nominal
                // width can't push the panel around between frames.
                let params_left = ui.cursor().left();
                ui.vertical(|ui| {
                    ui.set_width(PARAMS_W);
                    ui.horizontal_top(|ui| match cx.params.view {
                        ParamView::Intensity => intensity_view(ui, &mut cx.params.intensity),
                        ParamView::Color => color_view(ui, &mut cx.params.color),
                        ParamView::Position => position_view(ui, &mut cx.params.position),
                        ParamView::Beam => beam_view(ui, &mut cx.params.beam),
                        // Pixel effects are pre-baked patterns; the
                        // standard effects panel doesn't apply.
                        ParamView::PixelFx => pixel_view(ui, &mut cx.params.pixel),
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
                    ParamView::Color => Some((
                        &mut cx.params.color.effect,
                        egui::Color32::from_rgb(240, 200, 90),
                    )),
                    ParamView::Position => Some((
                        &mut cx.params.position.effect,
                        egui::Color32::from_rgb(140, 120, 255),
                    )),
                    ParamView::Beam => Some((
                        &mut cx.params.beam.effect,
                        egui::Color32::from_rgb(225, 235, 250),
                    )),
                    ParamView::PixelFx => None,
                };
                if let Some((cfg, accent)) = effect {
                    effect_panel(ui, cfg, accent, beat_t);
                }
            });
        });
        ui.separator();

        // Right: stacked actions.
        store = action_column(ui, cx);
    });
    store
}

/// Group-select row: click selects exactly that kind's fixtures,
/// shift-click unions, ALL selects everything.
fn groups_row(ui: &mut egui::Ui, rig: &Rig, selection: &mut HashSet<u32>) {
    let shift = ui.input(|i| i.modifiers.shift);
    ui.horizontal(|ui| {
        let all: HashSet<u32> = rig.ids().collect();
        let all_selected = !all.is_empty() && selection.len() == all.len();
        if ui
            .selectable_label(all_selected, egui::RichText::new("ALL").size(10.0))
            .clicked()
        {
            *selection = all;
        }
        for kind in ALL_KINDS {
            let ids: HashSet<u32> = rig.ids_of_kind(kind).collect();
            let active = !ids.is_empty() && ids.is_subset(selection);
            if ui
                .selectable_label(active, egui::RichText::new(kind.group_label()).size(10.0))
                .clicked()
            {
                if shift {
                    selection.extend(&ids);
                } else {
                    *selection = ids;
                }
            }
        }
        ui.add_space(10.0);
        let n = selection.len();
        let caption = if n == 0 {
            "no selection = whole lanes".to_string()
        } else {
            format!("{n} selected")
        };
        ui.label(egui::RichText::new(caption).weak().size(9.0));
    });
}

/// Fader height that fills the footer space left below the view tabs
/// (the ~34 pt reserve holds the readout + label under each fader).
fn fill_fader_height(ui: &egui::Ui) -> f32 {
    (ui.available_height() - 34.0).clamp(140.0, 420.0)
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

/// One labeled vertical fader with a live value readout.
fn fader_col(ui: &mut egui::Ui, value: &mut f32, spec: FaderCol<'_>) {
    ui.vertical(|ui| {
        ui.set_width(38.0);
        ui.vertical_centered(|ui| {
            ui.add(
                Fader::new(value, spec.range, spec.accent)
                    .size((FADER_W, spec.height))
                    .notches(Notches::Even(4))
                    .default_value(spec.default),
            );
            ui.label(
                egui::RichText::new(format!("{value:.0}{}", spec.unit))
                    .monospace()
                    .size(9.0),
            );
            ui.label(egui::RichText::new(spec.label).weak().size(9.0));
        });
    });
}

fn intensity_view(ui: &mut egui::Ui, p: &mut halo_light::programmer::IntensityParams) {
    let accent = crate::waveform::palette::LANE_LIGHTING;
    let height = fill_fader_height(ui);
    for (value, label, default) in [
        (&mut p.dimmer, "DIMMER", 100.0),
        (&mut p.strobe, "STROBE", 0.0),
    ] {
        fader_col(
            ui,
            value,
            FaderCol {
                label,
                range: 0.0..=100.0,
                default,
                accent,
                unit: "%",
                height,
            },
        );
    }
}

fn color_view(ui: &mut egui::Ui, p: &mut halo_light::programmer::ColorParams) {
    const ACCENTS: [(usize, &str, egui::Color32); 4] = [
        (0, "R", egui::Color32::from_rgb(235, 70, 60)),
        (1, "G", egui::Color32::from_rgb(80, 205, 95)),
        (2, "B", egui::Color32::from_rgb(75, 125, 255)),
        (3, "W", egui::Color32::from_rgb(235, 235, 240)),
    ];
    let height = fill_fader_height(ui);
    for (i, label, accent) in ACCENTS {
        fader_col(
            ui,
            &mut p.rgbw[i],
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
        let mix = |c: f32| (((c + p.rgbw[3] * 0.9) / 100.0).clamp(0.0, 1.0) * 255.0) as u8;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 22.0), egui::Sense::hover());
        ui.painter().rect_filled(
            rect,
            3.0,
            egui::Color32::from_rgb(mix(p.rgbw[0]), mix(p.rgbw[1]), mix(p.rgbw[2])),
        );
        ui.add_space(4.0);
        // Preset swatches set the faders instantly.
        egui::Grid::new("color_presets")
            .spacing([4.0, 4.0])
            .min_col_width(18.0)
            .show(ui, |ui| {
                for (i, &(name, rgbw)) in COLOR_PRESETS.iter().enumerate() {
                    if color_swatch(ui, rgbw, false).on_hover_text(name).clicked() {
                        p.rgbw = rgbw;
                    }
                    if i % 5 == 4 {
                        ui.end_row();
                    }
                }
            });
    });
}

/// Small clickable color swatch for a preset.
fn color_swatch(ui: &mut egui::Ui, rgbw: [f32; 4], selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
    let mix = |c: f32| (((c + rgbw[3] * 0.9) / 100.0).clamp(0.0, 1.0) * 255.0) as u8;
    ui.painter().rect_filled(
        rect,
        3.0,
        egui::Color32::from_rgb(mix(rgbw[0]), mix(rgbw[1]), mix(rgbw[2])),
    );
    if selected {
        ui.painter().rect_stroke(
            rect,
            3.0,
            egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
            egui::StrokeKind::Outside,
        );
    }
    resp
}

fn position_view(ui: &mut egui::Ui, p: &mut halo_light::programmer::PositionParams) {
    let accent = egui::Color32::from_rgb(140, 120, 255);
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
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(16, 16, 22));
        for f in [0.25, 0.5, 0.75] {
            let x = rect.left() + rect.width() * f;
            let y = rect.top() + rect.height() * f;
            let stroke = egui::Stroke::new(
                1.0_f32,
                if f == 0.5 {
                    egui::Color32::from_rgb(45, 45, 55)
                } else {
                    egui::Color32::from_rgb(28, 28, 36)
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
        painter.circle_stroke(dot, 6.0, egui::Stroke::new(1.0_f32, egui::Color32::WHITE));

        // Which axes the effect drives.
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for (t, label) in [
                (PanTiltTarget::Both, "BOTH"),
                (PanTiltTarget::Pan, "PAN"),
                (PanTiltTarget::Tilt, "TILT"),
            ] {
                if ui
                    .selectable_label(p.target == t, egui::RichText::new(label).size(9.0))
                    .on_hover_text("Apply the effect to these axes")
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
        ui.label(egui::RichText::new("GOBO").weak().size(9.0));
        ui.add_space(2.0);
        egui::Grid::new("gobo_grid")
            .spacing([4.0, 4.0])
            .show(ui, |ui| {
                for g in 1..=8u8 {
                    if ui
                        .add_sized(
                            [34.0, 30.0],
                            egui::SelectableLabel::new(
                                p.gobo == g,
                                egui::RichText::new(format!("G{g}")).monospace().size(10.0),
                            ),
                        )
                        .clicked()
                    {
                        p.gobo = g;
                    }
                    if g == 4 {
                        ui.end_row();
                    }
                }
            });
    });
}

fn pixel_view(ui: &mut egui::Ui, p: &mut halo_light::programmer::PixelFxParams) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new("EFFECT").weak().size(9.0));
        egui::ScrollArea::vertical()
            .id_salt("pixel_fx_list")
            .max_height(118.0)
            .show(ui, |ui| {
                for (i, name) in PIXEL_EFFECTS.iter().enumerate() {
                    if ui
                        .selectable_label(p.effect == i, egui::RichText::new(*name).size(10.0))
                        .clicked()
                    {
                        p.effect = i;
                    }
                }
            });
    });
    ui.add_space(10.0);
    ui.vertical(|ui| {
        ui.label(egui::RichText::new("COLOR").weak().size(9.0));
        ui.add_space(2.0);
        egui::Grid::new("pixel_colors")
            .spacing([4.0, 4.0])
            .min_col_width(18.0)
            .show(ui, |ui| {
                for (i, &(name, rgbw)) in COLOR_PRESETS.iter().enumerate() {
                    if color_swatch(ui, rgbw, p.color == i)
                        .on_hover_text(name)
                        .clicked()
                    {
                        p.color = i;
                    }
                    if i % 5 == 4 {
                        ui.end_row();
                    }
                }
            });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(format!(
                "{} · {}",
                PIXEL_EFFECTS[p.effect], COLOR_PRESETS[p.color].0
            ))
            .weak()
            .size(9.0),
        );
    });
}

/// The shared per-parameter effects panel with a beat-synced preview.
fn effect_panel(ui: &mut egui::Ui, cfg: &mut EffectConfig, accent: egui::Color32, beat_t: f64) {
    // The panel is embedded in a horizontal row; force its own content
    // back to a vertical stack.
    egui::Frame::group(ui.style())
        .inner_margin(6.0)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(216.0);
                ui.label(egui::RichText::new("EFFECT").weak().size(9.0));
                ui.horizontal(|ui| {
                    for (wf, label) in ALL_WAVEFORMS {
                        if ui
                            .selectable_label(
                                cfg.waveform == wf,
                                egui::RichText::new(label).size(9.0),
                            )
                            .clicked()
                        {
                            cfg.waveform = wf;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    for (iv, label) in ALL_INTERVALS {
                        if ui
                            .selectable_label(
                                cfg.interval == iv,
                                egui::RichText::new(label).size(9.0),
                            )
                            .clicked()
                        {
                            cfg.interval = iv;
                        }
                    }
                });
                ui.spacing_mut().slider_width = 100.0;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("RATIO").weak().size(9.0));
                    ui.add(
                        egui::Slider::new(&mut cfg.ratio, 0.0..=2.0)
                            .fixed_decimals(2)
                            .handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.5 }),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("PHASE").weak().size(9.0));
                    ui.add(
                        egui::Slider::new(&mut cfg.phase_deg, 0.0..=360.0)
                            .fixed_decimals(0)
                            .suffix("°")
                            .handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.5 }),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("DIST").weak().size(9.0));
                    let is_all = cfg.distribution == Distribution::All;
                    if ui
                        .selectable_label(is_all, egui::RichText::new("ALL").size(9.0))
                        .clicked()
                    {
                        cfg.distribution = Distribution::All;
                    }
                    let is_step = matches!(cfg.distribution, Distribution::Step(_));
                    if ui
                        .selectable_label(is_step, egui::RichText::new("STEP").size(9.0))
                        .clicked()
                    {
                        cfg.distribution = Distribution::Step(2);
                    }
                    let is_wave = matches!(cfg.distribution, Distribution::Wave(_));
                    if ui
                        .selectable_label(is_wave, egui::RichText::new("WAVE").size(9.0))
                        .clicked()
                    {
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
                ui.add_space(4.0);
                effect_preview(ui, cfg, accent, beat_t);
                ui.add_space(4.0);
                let apply = egui::Button::new(
                    egui::RichText::new(if cfg.applied { "APPLIED" } else { "APPLY" })
                        .size(10.0)
                        .strong()
                        .color(if cfg.applied {
                            egui::Color32::from_rgb(15, 15, 18)
                        } else {
                            egui::Color32::from_rgb(220, 220, 225)
                        }),
                )
                .fill(if cfg.applied {
                    accent
                } else {
                    egui::Color32::from_rgb(42, 42, 46)
                });
                if ui
                    .add_sized([ui.available_width(), 20.0], apply)
                    .on_hover_text("Latch this effect (visual-only until the fixture engine lands)")
                    .clicked()
                {
                    cfg.applied = !cfg.applied;
                }
            });
        });
}

/// Musical window the preview shows, in intervals. A few cycles at once
/// keeps the "now" dot travelling calmly (once per bar at the default
/// beat interval) instead of whipping across on every beat.
const PREVIEW_SPAN: f64 = 4.0;

/// A few intervals of the configured waveform with a dot riding the
/// actual musical phase, plus a fainter offset trace hinting Step/Wave
/// spread.
fn effect_preview(ui: &mut egui::Ui, cfg: &EffectConfig, accent: egui::Color32, beat_t: f64) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(204.0), 44.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, egui::Color32::from_rgb(12, 12, 16));
    let mid = rect.center().y;
    painter.line_segment(
        [egui::pos2(rect.left(), mid), egui::pos2(rect.right(), mid)],
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(30, 30, 38)),
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
    painter.circle_filled(dot, 3.0, egui::Color32::WHITE);
}

/// CLEAR / STORE / PREVIEW / HIGHLIGHT, stacked on the panel's right edge.
fn action_column(ui: &mut egui::Ui, cx: &mut ProgrammerCtx<'_>) -> bool {
    let mut store = false;
    ui.vertical(|ui| {
        ui.set_width(ACTION_COL_W);
        let armed = programmer::any_latched(cx.overrides);
        let clear_btn = egui::Button::new(egui::RichText::new("CLEAR").size(12.0).strong().color(
            if armed {
                egui::Color32::WHITE
            } else {
                egui::Color32::from_rgb(200, 200, 205)
            },
        ))
        .fill(if armed {
            egui::Color32::from_rgb(130, 32, 32)
        } else {
            egui::Color32::from_rgb(42, 42, 46)
        });
        if ui
            .add_sized([ACTION_COL_W, 26.0], clear_btn)
            .on_hover_text("Release all latched lanes (Esc)")
            .clicked()
        {
            programmer::clear(cx.overrides);
        }
        ui.add_space(4.0);
        ui.add_enabled_ui(cx.can_store, |ui| {
            if ui
                .add_sized(
                    [ACTION_COL_W, 26.0],
                    egui::Button::new(egui::RichText::new("STORE").size(12.0)),
                )
                .on_hover_text(format!(
                    "Write the active lanes into deck {}'s track as cues at the current bar",
                    cx.deck_name
                ))
                .clicked()
            {
                store = true;
            }
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
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
            if ui
                .add_sized(
                    [ACTION_COL_W, 22.0],
                    egui::SelectableLabel::new(*value, egui::RichText::new(label).size(10.0)),
                )
                .on_hover_text(hint)
                .clicked()
            {
                *value = !*value;
            }
            ui.add_space(4.0);
        }
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(format!("out: deck {}", cx.deck_name))
                .weak()
                .size(9.0),
        );
    });
    store
}

/// One lane's compact override cluster: colored label, latching ON,
/// momentary FLASH, intensity knob.
fn lane_controls(ui: &mut egui::Ui, o: &mut halo_light::programmer::LaneOverride, i: usize) {
    let (_, label, color) = crate::waveform::LANES[i];
    ui.label(
        egui::RichText::new(label)
            .monospace()
            .size(10.0)
            .color(if o.active() {
                color
            } else {
                color.gamma_multiply(0.5)
            }),
    );
    if ui
        .selectable_label(o.latched, egui::RichText::new("ON").size(10.0))
        .on_hover_text("Latch this lane on until CLEAR")
        .clicked()
    {
        o.latched = !o.latched;
    }
    let flash = ui
        .add(egui::Button::new(egui::RichText::new("FLASH").size(10.0)))
        .on_hover_text("Active while held");
    if flash.is_pointer_button_down_on() {
        o.flash_held = true;
    }
    let mut v = o.intensity;
    if ui
        .add(
            Knob::new(&mut v, 0.0..=1.0, color)
                .arc(KnobArc::Unipolar)
                .default_value(1.0)
                .diameter(18.0),
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
        let out = outputs[f.kind.lane() as usize];
        let selected = selection.contains(&f.id);

        let alpha = 0.15 + 0.85 * out.level.clamp(0.0, 1.0);
        let [r, g, b] = f.kind.color();
        painter.rect_filled(
            cell,
            4.0,
            egui::Color32::from_rgb(r, g, b).gamma_multiply(alpha),
        );
        if out.source == LaneSource::Programmer {
            painter.rect_stroke(
                cell.shrink(1.5),
                3.0,
                egui::Stroke::new(1.0_f32, egui::Color32::WHITE.gamma_multiply(0.55)),
                egui::StrokeKind::Inside,
            );
        }
        if selected {
            painter.rect_stroke(
                cell,
                4.0,
                egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
                egui::StrokeKind::Outside,
            );
        }
        // Bright fills (strobes glowing white, smoke grey) need dark text.
        let bright = matches!(f.kind, FixtureKind::Strobe | FixtureKind::Smoke) && alpha > 0.5;
        painter.text(
            cell.center(),
            egui::Align2::CENTER_CENTER,
            &f.label,
            egui::FontId::monospace(9.0),
            if bright {
                egui::Color32::from_rgb(20, 20, 24)
            } else {
                egui::Color32::from_rgb(230, 230, 235)
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
