//! Bar-style fader widget (physical-mixer look) for the channel volume
//! faders and the crossfader.
//!
//! A rectangular cap rides a grooved track. Channel faders draw evenly
//! spaced notches down the side as a position scale; the crossfader draws a
//! single center notch and tints its cap from grey (center) to the accent
//! color at the extremes. Interaction is absolute positioning — the cap
//! jumps to the pointer, like a real fader — with double-click to reset.
//!
//! Pure UI: callers load an atomic into a local `f32`, pass a `&mut`, and
//! store back on `Response::changed()`, the same pattern the knobs use.

use std::ops::RangeInclusive;

use eframe::egui::{self, Color32, Rect, Response, Sense, Ui, Vec2, Widget};

/// Thickness of the groove the cap rides in, in points.
const GROOVE: f32 = 4.0;
/// Cap thickness along the travel axis, in points.
const CAP_THICKNESS: f32 = 9.0;
/// How far the cap extends across the travel axis (each side of center).
const CAP_HALF_SPAN: f32 = 10.0;
/// Notch tick length (each side of the groove) and gap from it.
const NOTCH_LEN: f32 = 4.0;
const NOTCH_GAP: f32 = 3.0;

/// Notch (tick) layout drawn alongside the track.
pub enum Notches {
    None,
    /// `n` evenly-spaced intervals → n+1 ticks across the travel. (channel)
    Even(u32),
    /// A single tick at the value center. (crossfader)
    Center,
}

pub struct Fader<'a> {
    value: &'a mut f32,
    range: RangeInclusive<f32>,
    vertical: bool,
    size: Vec2,
    notches: Notches,
    default: f32,
    accent: Color32,
    /// Some(center): draw an accent fill along the groove from `center` to the
    /// cap as it moves off center (like the bipolar EQ knobs).
    center_fill: Option<f32>,
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
            center_fill: None,
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

    pub fn center_fill(mut self, center: f32) -> Self {
        self.center_fill = Some(center);
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
            let visuals = ui.visuals();
            let center = rect.center();
            let track_col = visuals.weak_text_color().gamma_multiply(0.5);

            // Groove along the travel axis.
            let groove = if self.vertical {
                Rect::from_center_size(center, Vec2::new(self.groove, (hi - lo).abs() + self.cap.y))
            } else {
                Rect::from_center_size(center, Vec2::new((hi - lo).abs() + self.cap.y, self.groove))
            };
            painter.rect_filled(groove, 2.0, visuals.extreme_bg_color);

            // Notches: short ticks perpendicular to the groove.
            let half_groove = self.groove / 2.0;
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
                let stroke = egui::Stroke::new(1.0, track_col);
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
                Notches::Center => tick(painter, 0.5),
            }

            let t = self.norm(*self.value);
            let p = lo + (hi - lo) * t;
            let active = response.hovered() || response.dragged();

            // Center-origin fill: an accent bar from the center value to the
            // cap, showing how far it's pushed off center (like the EQ knobs).
            if let Some(c) = self.center_fill {
                let pc = lo + (hi - lo) * self.norm(c);
                if (p - pc).abs() > 0.5 {
                    let fill = if self.vertical {
                        Rect::from_two_pos(
                            egui::pos2(center.x - half_groove, pc),
                            egui::pos2(center.x + half_groove, p),
                        )
                    } else {
                        Rect::from_two_pos(
                            egui::pos2(pc, center.y - half_groove),
                            egui::pos2(p, center.y + half_groove),
                        )
                    };
                    let fill_col = if active {
                        self.accent
                    } else {
                        self.accent.gamma_multiply(0.9)
                    };
                    painter.rect_filled(fill, 2.0, fill_col);
                }
            }

            // Cap at the current value (always accent).
            let cap = if self.vertical {
                Rect::from_center_size(egui::pos2(center.x, p), self.cap)
            } else {
                Rect::from_center_size(egui::pos2(p, center.y), Vec2::new(self.cap.y, self.cap.x))
            };
            let cap_col = if active {
                self.accent
            } else {
                self.accent.gamma_multiply(0.9)
            };
            painter.rect_filled(cap, 2.0, cap_col);
            painter.rect_stroke(
                cap,
                2.0,
                egui::Stroke::new(1.0, visuals.extreme_bg_color),
                egui::StrokeKind::Inside,
            );
        }

        response
    }
}
