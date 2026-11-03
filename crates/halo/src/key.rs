//! The key: the one button Halo's performance surface is made of.
//!
//! A raised `face-key` with a line of sheen and a 1px dark edge, 30 high
//! with 12 either side of a bold spaced label. Hover lightens the face and
//! never grows it; holding pushes it in. A key can light its whole face in
//! a state colour (set hot cues, a held CUE, a playing PLAY) or carry an
//! LED before its label (latched toggles).
//!
//! The widget only draws: it allocates a click-sensing rect and returns the
//! plain `Response`, so callers keep their own press / release / right-click
//! handling exactly as with `egui::Button`.

// Small keys, `pressed` and the audition face arrive with the toolbar and
// Prepare redesign steps; drop this once they are used.
#![allow(dead_code)]

use eframe::egui::{self, Color32, Response, Sense, Ui, Vec2, Widget, WidgetInfo, WidgetType};

use crate::theme::{self, Face, TypeStyle};

/// A state colour a key's face lights in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lit {
    /// Amber: set hot cues, a held CUE.
    Accent,
    /// Green: a playing PLAY.
    Play,
    /// Yellow: the Prepare audition player.
    Audition,
}

impl Lit {
    fn face(self) -> Face {
        match self {
            Lit::Accent => Face::Accent,
            Lit::Play => Face::Play,
            Lit::Audition => Face::Audition,
        }
    }

    /// The dark label colour on the lit face.
    fn on(self) -> Color32 {
        match self {
            Lit::Accent => theme::ACCENT_ON,
            Lit::Play => theme::PLAY_ON,
            Lit::Audition => theme::AUDITION_ON,
        }
    }
}

/// `brightness(0.92)` on a lit face that is held down.
const LIT_HELD_TINT: Color32 = Color32::from_gray(235);

#[must_use = "add it with `ui.add(...)`"]
pub struct Key {
    label: String,
    style: TypeStyle,
    size: Option<Vec2>,
    width: Option<f32>,
    small: bool,
    lit: Option<Lit>,
    lit_when_held: Option<Lit>,
    color: Option<Color32>,
    led: Option<(bool, Color32)>,
    pressed: bool,
}

impl Key {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style: theme::KEY_LABEL,
            size: None,
            width: None,
            small: false,
            lit: None,
            lit_when_held: None,
            color: None,
            led: None,
            pressed: false,
        }
    }

    /// Fixed size; otherwise as wide as the label (12 either side) × 30.
    pub fn size(mut self, size: impl Into<Vec2>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// Fixed width at the key's usual height.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// A 22-high key with 8 either side (toolbar and list actions).
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    /// Light the whole face.
    pub fn lit(mut self, lit: Option<Lit>) -> Self {
        self.lit = lit;
        self
    }

    /// Light the face only while the pointer holds it (momentary CUE).
    pub fn lit_when_held(mut self, lit: Lit) -> Self {
        self.lit_when_held = Some(lit);
        self
    }

    /// The label's colour on an unlit key (CUE's amber, PLAY's green).
    pub fn label_color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// An LED before the label, lit in `color` when `on`.
    pub fn led(mut self, on: bool, color: Color32) -> Self {
        self.led = Some((on, color));
        self
    }

    /// Draw it held regardless of the pointer.
    pub fn pressed(mut self, pressed: bool) -> Self {
        self.pressed = pressed;
        self
    }

    /// The label's type style (default `key-label`).
    pub fn text_style(mut self, style: TypeStyle) -> Self {
        self.style = style;
        self
    }
}

impl Widget for Key {
    fn ui(self, ui: &mut Ui) -> Response {
        let (height, side, gap) = if self.small {
            (theme::SIZE_KEY_SMALL, theme::SPACE_8, theme::SPACE_6)
        } else {
            (theme::SIZE_KEY, theme::SPACE_12, theme::SPACE_8)
        };
        let ctx = ui.ctx().clone();
        let label_w = if self.label.is_empty() {
            0.0
        } else {
            self.style.width(&ctx, &self.label)
        };
        let led_w = if self.led.is_some() {
            theme::SIZE_LED + if label_w > 0.0 { gap } else { 0.0 }
        } else {
            0.0
        };
        let content_w = led_w + label_w;
        let size = self.size.unwrap_or_else(|| {
            let w = self.width.unwrap_or((content_w + 2.0 * side).max(height));
            egui::vec2(w, height)
        });

        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let enabled = response.enabled();
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, &self.label));
        if !ui.is_rect_visible(rect) {
            return response;
        }

        let held = enabled && (self.pressed || response.is_pointer_button_down_on());
        let hovered = enabled && response.hovered();
        let lit = if !enabled {
            None
        } else if held {
            self.lit.or(self.lit_when_held)
        } else {
            self.lit
        };
        let radius = theme::RADIUS_CONTROL;
        let painter = ui.painter();

        let label_color = if !enabled {
            painter.rect_filled(rect, radius, theme::KEY_LO);
            theme::paint_edge(painter, rect, radius, theme::EDGE);
            theme::TEXT_DISABLED
        } else if let Some(lit) = lit {
            if held {
                theme::paint_pressed(painter, rect, radius, lit.face(), LIT_HELD_TINT);
            } else {
                theme::paint_lit(painter, rect, radius, lit.face(), Color32::WHITE);
                if hovered {
                    // brightness(1.05): the faintest lift of light.
                    painter.rect_filled(rect, radius, theme::LIP);
                }
            }
            lit.on()
        } else {
            if held {
                theme::paint_pressed(painter, rect, radius, Face::KeyDown, Color32::WHITE);
            } else if hovered {
                theme::paint_raised(painter, rect, radius, Face::KeyHover);
            } else {
                theme::paint_raised(painter, rect, radius, Face::Key);
            }
            self.color.unwrap_or(if held || hovered {
                theme::TEXT_STRONG
            } else {
                theme::TEXT
            })
        };

        let mut x = rect.center().x - content_w / 2.0;
        if let Some((on, color)) = self.led {
            let center = egui::pos2(x + theme::SIZE_LED / 2.0, rect.center().y);
            theme::paint_led(painter, center, on && enabled, color);
            x += led_w;
        }
        if label_w > 0.0 {
            let galley = self.style.galley(&ctx, &self.label, label_color);
            let pos = egui::pos2(x, rect.center().y - galley.size().y / 2.0);
            painter.galley(pos.round(), galley, label_color);
        }
        response
    }
}
