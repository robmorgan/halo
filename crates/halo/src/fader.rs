//! Bar-style fader widget (physical-mixer look) for the channel volume
//! faders and the decks' tempo faders.
//!
//! A raised cap (`face-cap`, a 1px edge, a line of sheen and an amber
//! indicator line across its middle) rides a slot cut into the panel.
//! Faders draw evenly spaced notches down the side as a position scale.
//! Interaction is absolute positioning — the cap
//! jumps to the pointer, like a real fader — with double-click to reset.
//!
//! Pure UI: callers load an atomic into a local `f32`, pass a `&mut`, and
//! store back on `Response::changed()`, the same pattern the knobs use.

use std::ops::RangeInclusive;

use eframe::egui::{self, Color32, Rect, Response, Sense, Stroke, Ui, Vec2, Widget};

use crate::theme::{self, Face};

/// Thickness of the groove the cap rides in, in points.
const GROOVE: f32 = 4.0;
/// Cap thickness along the travel axis, in points.
const CAP_THICKNESS: f32 = 9.0;
/// How far the cap extends across the travel axis (each side of center).
const CAP_HALF_SPAN: f32 = 10.0;
/// Notch tick length (each side of the groove) and gap from it.
const NOTCH_LEN: f32 = 4.0;
const NOTCH_GAP: f32 = 3.0;
/// Corner radius of the cap.
const CAP_RADIUS: u8 = 2;

/// Notch (tick) layout drawn alongside the track.
pub enum Notches {
    None,
    /// `n` evenly-spaced intervals → n+1 ticks across the travel. (channel)
    Even(u32),
}

pub struct Fader<'a> {
    value: &'a mut f32,
    range: RangeInclusive<f32>,
    vertical: bool,
    size: Vec2,
    notches: Notches,
    default: f32,
    accent: Color32,
    /// Groove thickness across the travel axis.
    groove: f32,
    /// Cap size as (span across the travel axis, thickness along it).
    cap: Vec2,
}

impl<'a> Fader<'a> {
    pub fn new(value: &'a mut f32, range: RangeInclusive<f32>, accent: Color32) -> Self {
        let default = *range.start();
        Self {
            value,
            range,
            vertical: true,
            size: Vec2::new(24.0, 120.0),
            notches: Notches::None,
            default,
            accent,
            groove: GROOVE,
            cap: Vec2::new(CAP_HALF_SPAN * 2.0, CAP_THICKNESS),
        }
    }

    pub fn vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self
    }

    pub fn size(mut self, size: impl Into<Vec2>) -> Self {
        self.size = size.into();
        self
    }

    pub fn notches(mut self, notches: Notches) -> Self {
        self.notches = notches;
        self
    }

    pub fn default_value(mut self, default: f32) -> Self {
        self.default = default;
        self
    }

    /// Groove thickness across the travel axis (default 4.0).
    pub fn groove_width(mut self, width: f32) -> Self {
        self.groove = width;
        self
    }

    /// Cap size: span across the travel axis × thickness along it
    /// (default 20×9, the mixer look).
    pub fn cap_size(mut self, span: f32, thickness: f32) -> Self {
        self.cap = Vec2::new(span, thickness);
        self
    }

    fn span(&self) -> f32 {
        *self.range.end() - *self.range.start()
    }

    /// Normalize a value to 0..=1 across the range.
    fn norm(&self, v: f32) -> f32 {
        ((v - *self.range.start()) / self.span()).clamp(0.0, 1.0)
    }
}

impl Widget for Fader<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(self.size, Sense::click_and_drag());

        // The cap center travels between these two points; inset from the
        // ends by half the cap so it never spills past the track.
        let inset = self.cap.y / 2.0;
        let (lo, hi) = if self.vertical {
            (rect.bottom() - inset, rect.top() + inset) // norm 0 = bottom
        } else {
            (rect.left() + inset, rect.right() - inset) // norm 0 = left
        };

        // Absolute positioning: cap follows the pointer along the travel axis.
        if (response.dragged() || response.clicked())
            && let Some(pos) = response.interact_pointer_pos()
        {
            let p = if self.vertical { pos.y } else { pos.x };
            let t = ((p - lo) / (hi - lo)).clamp(0.0, 1.0);
            let next = (*self.range.start() + t * self.span())
                .clamp(*self.range.start(), *self.range.end());
            if next != *self.value {
                *self.value = next;
                response.mark_changed();
            }
        }

        if response.double_clicked() && *self.value != self.default {
            *self.value = self.default;
            response.mark_changed();
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let center = rect.center();
            let enabled = response.enabled();
            let active = enabled && (response.hovered() || response.dragged());
            // Indicator and fill: 90% accent until hovered or dragged.
            let line_col = if !enabled {
                theme::TEXT_DISABLED
            } else if active {
                self.accent
            } else {
                self.accent.gamma_multiply(0.9)
            };
            let notch_col = theme::TEXT_MUTED.gamma_multiply(if enabled { 0.6 } else { 0.3 });

            // The slot: a well with a 1px edge along the whole travel.
            let half_groove = self.groove / 2.0;
            let slot = if self.vertical {
                Rect::from_center_size(center, Vec2::new(self.groove, rect.height() - 1.0))
            } else {
                Rect::from_center_size(center, Vec2::new(rect.width() - 1.0, self.groove))
            };
            theme::paint_well_edged(painter, slot, half_groove.min(2.0) as u8);

            // Notches: short ticks perpendicular to the groove.
            let tick = |painter: &egui::Painter, t: f32| {
                let p = lo + (hi - lo) * t;
                let (a, b, c, d) = if self.vertical {
                    (
                        egui::pos2(center.x - half_groove - NOTCH_GAP - NOTCH_LEN, p),
                        egui::pos2(center.x - half_groove - NOTCH_GAP, p),
                        egui::pos2(center.x + half_groove + NOTCH_GAP, p),
                        egui::pos2(center.x + half_groove + NOTCH_GAP + NOTCH_LEN, p),
                    )
                } else {
                    (
                        egui::pos2(p, center.y - half_groove - NOTCH_GAP - NOTCH_LEN),
                        egui::pos2(p, center.y - half_groove - NOTCH_GAP),
                        egui::pos2(p, center.y + half_groove + NOTCH_GAP),
                        egui::pos2(p, center.y + half_groove + NOTCH_GAP + NOTCH_LEN),
                    )
                };
                let stroke = Stroke::new(1.0, notch_col);
                painter.line_segment([a, b], stroke);
                painter.line_segment([c, d], stroke);
            };
            match self.notches {
                Notches::None => {}
                Notches::Even(n) => {
                    for i in 0..=n {
                        tick(painter, i as f32 / n as f32);
                    }
                }
            }

            let t = self.norm(*self.value);
            let p = lo + (hi - lo) * t;

            // The cap: face-cap, a 1px edge, sheen along its top and a 2px
            // indicator line across its middle. Disabled, it fades to 55%.
            let cap = if self.vertical {
                Rect::from_center_size(egui::pos2(center.x, p), self.cap)
            } else {
                Rect::from_center_size(egui::pos2(p, center.y), Vec2::new(self.cap.y, self.cap.x))
            };
            let fade = if enabled { 1.0 } else { 0.55 };
            theme::paint_edge(painter, cap, CAP_RADIUS, theme::EDGE.gamma_multiply(fade));
            theme::paint_face(
                painter,
                cap,
                CAP_RADIUS,
                Face::Cap,
                Color32::WHITE.gamma_multiply(fade),
            );
            painter.hline(
                (cap.left() + 2.0)..=(cap.right() - 2.0),
                cap.top() + 1.0,
                Stroke::new(1.0, theme::SHEEN_LIT.gamma_multiply(0.5 * fade)),
            );
            let indicator = Stroke::new(2.0, line_col);
            if self.vertical {
                painter.hline((cap.left() + 2.0)..=(cap.right() - 2.0), p, indicator);
            } else {
                painter.vline(p, (cap.top() + 2.0)..=(cap.bottom() - 2.0), indicator);
            }
        }

        response
    }
}
