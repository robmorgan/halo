//! Rotary knob widget (Traktor/Rekordbox style) for the mixer.
//!
//! A knob sweeps 270° — from −135° (≈7:30) to +135° (≈4:30), measured
//! clockwise from 12 o'clock — with a dim background track, an accent
//! value arc that shows how much is applied, and a pointer at the current
//! value. Bipolar knobs fill the arc outward from a center detent (EQ/trim
//! unity); unipolar knobs fill from the min end (master).
//!
//! Drawn as Halo's design system has it: a lit cap seated in a well, the
//! track in `segment-off`, the value arc in amber and a `text-strong`
//! pointer.
//!
//! The widget is pure UI: callers load an atomic into a local `f32`, pass a
//! `&mut` to it, and store back on `Response::changed()` — the same pattern
//! the mixer sliders use.

use std::f32::consts::PI;
use std::ops::RangeInclusive;

use eframe::egui::{self, Color32, Pos2, Response, Sense, Stroke, Ui, Vec2, Widget};

use crate::theme;

/// Total sweep, from `-SWEEP/2` to `+SWEEP/2` about 12 o'clock.
const SWEEP: f32 = 1.5 * PI; // 270°
const HALF_SWEEP: f32 = SWEEP / 2.0;
/// Vertical drag distance (points) to traverse the full range.
const PIXELS_FOR_FULL_TRAVEL: f32 = 200.0;
/// Fine-adjust multiplier while Shift is held.
const FINE: f32 = 0.25;
/// The cap's radius as a fraction of the diameter.
const CAP_RATIO: f32 = 0.29;

/// How the value arc is drawn.
pub enum KnobArc {
    /// Arc grows from the min end of the sweep. (master)
    Unipolar,
    /// Arc grows from a center detent outward either way. (EQ, trim, filter)
    Bipolar { center: f32 },
}

pub struct Knob<'a> {
    value: &'a mut f32,
    range: RangeInclusive<f32>,
    arc: KnobArc,
    /// Double-click reset target.
    default: f32,
    diameter: f32,
    accent: Color32,
}

impl<'a> Knob<'a> {
    pub fn new(value: &'a mut f32, range: RangeInclusive<f32>, accent: Color32) -> Self {
        let default = *range.start();
        Self {
            value,
            range,
            arc: KnobArc::Unipolar,
            default,
            diameter: theme::SIZE_KNOB,
            accent,
        }
    }

    pub fn arc(mut self, arc: KnobArc) -> Self {
        self.arc = arc;
        self
    }

    pub fn default_value(mut self, default: f32) -> Self {
        self.default = default;
        self
    }

    pub fn diameter(mut self, diameter: f32) -> Self {
        self.diameter = diameter;
        self
    }

    fn span(&self) -> f32 {
        *self.range.end() - *self.range.start()
    }

    /// Normalize a value to `0..=1` across the range.
    fn norm(&self, v: f32) -> f32 {
        ((v - *self.range.start()) / self.span()).clamp(0.0, 1.0)
    }

    /// Sweep angle (radians, clockwise from 12 o'clock) for a value.
    fn angle_of(&self, v: f32) -> f32 {
        -HALF_SWEEP + self.norm(v) * SWEEP
    }
}

/// Unit direction for a sweep angle (egui y-down: θ=0 → up, +θ → right).
fn dir(angle: f32) -> Vec2 {
    Vec2::new(angle.sin(), -angle.cos())
}

impl Widget for Knob<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) =
            ui.allocate_exact_size(Vec2::splat(self.diameter), Sense::click_and_drag());

        let span = self.span();

        // Drag: vertical, up = increase. Shift = fine.
        if response.dragged() {
            let dy = -response.drag_delta().y;
            if dy != 0.0 {
                let fine = if ui.input(|i| i.modifiers.shift_only()) {
                    FINE
                } else {
                    1.0
                };
                let next = (*self.value + dy / PIXELS_FOR_FULL_TRAVEL * span * fine)
                    .clamp(*self.range.start(), *self.range.end());
                if next != *self.value {
                    *self.value = next;
                    response.mark_changed();
                }
            }
        }

        // Double-click resets to default.
        if response.double_clicked() && *self.value != self.default {
            *self.value = self.default;
            response.mark_changed();
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let center = rect.center();
            let d = self.diameter;
            let ring = d / 2.0 - (d / 12.0).max(4.0);
            let width = (d / 20.0).max(2.0);
            let cap_r = d * CAP_RATIO;
            // Arc at 85% until hovered or dragged.
            let accent = if response.hovered() || response.dragged() {
                self.accent
            } else {
                self.accent.gamma_multiply(0.85)
            };

            // The seat: a flat well disc.
            painter.circle_filled(center, d / 2.0 - 0.5, theme::WELL);

            // Track across the full sweep, then the value arc.
            round_arc(
                painter,
                center,
                ring,
                -HALF_SWEEP,
                HALF_SWEEP,
                width,
                theme::SEGMENT_OFF,
            );
            let cur = self.angle_of(*self.value);
            let from = match self.arc {
                KnobArc::Unipolar => -HALF_SWEEP,
                KnobArc::Bipolar { center: c } => self.angle_of(c),
            };
            if (cur - from).abs() > 1e-4 {
                round_arc(
                    painter,
                    center,
                    ring,
                    from.min(cur),
                    from.max(cur),
                    width,
                    accent,
                );
            }

            // The cap: a matte disc with a 1px edge.
            painter.circle_filled(center, cap_r, theme::CAP);
            painter.circle_stroke(center, cap_r, Stroke::new(1.0, theme::EDGE));

            // Pointer from near the centre to just inside the cap's rim.
            let dv = dir(cur);
            painter.line_segment(
                [center + dv * (d * 0.1), center + dv * (cap_r - d * 0.06)],
                Stroke::new((d * 0.037).max(1.6), theme::TEXT_STRONG),
            );
        }

        response
    }
}

/// Polyline approximating the arc from `a0` to `a1` (radians) at `radius`.
fn arc_points(center: egui::Pos2, radius: f32, a0: f32, a1: f32) -> Vec<egui::Pos2> {
    // ~one segment per 6° keeps the curve smooth at these sizes.
    let steps = (((a1 - a0).abs() / (PI / 30.0)).ceil() as usize).max(1);
    (0..=steps)
        .map(|i| {
            let a = a0 + (a1 - a0) * (i as f32 / steps as f32);
            center + dir(a) * radius
        })
        .collect()
}

/// An arc stroke with round ends.
fn round_arc(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    a0: f32,
    a1: f32,
    width: f32,
    color: Color32,
) {
    painter.add(egui::Shape::line(
        arc_points(center, radius, a0, a1),
        Stroke::new(width, color),
    ));
    for a in [a0, a1] {
        painter.circle_filled(center + dir(a) * radius, width / 2.0, color);
    }
}
