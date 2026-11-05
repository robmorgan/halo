//! The key: the one button Halo's performance surface is made of.
//!
//! A raised `face-key` with a line of sheen and a 1px dark edge, 30 high
//! with 12 either side of a bold spaced label. Hover lightens the face and
//! never grows it; holding pushes it in. A key can light its whole face in
//! a state colour (set hot cues, a held CUE, a playing PLAY) or carry an
//! LED before its label (latched toggles).
//!
//! Keys can also be joined into one strip with [`segmented`].
//!
//! The widget only draws: it allocates a click-sensing rect and returns the
//! plain `Response`, so callers keep their own press / release / right-click
//! handling exactly as with `egui::Button`.

use eframe::egui::{
    self, Color32, CornerRadius, Rect, Response, Sense, Stroke, Ui, Vec2, Widget, WidgetInfo,
    WidgetType,
};

use crate::theme::{self, Face, Top, TypeStyle};

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
    selected: bool,
    pressed: bool,
    enabled: bool,
    joint: Option<Joint>,
}

/// Where a key sits in a joined strip: which ends it rounds, and whether a
/// seam separates it from the key before it.
#[derive(Clone, Copy, Debug)]
struct Joint {
    first: bool,
    last: bool,
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
            selected: false,
            pressed: false,
            enabled: true,
            joint: None,
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

    /// The current choice of a set (the view tab): a `selection` wash over
    /// the face under a `text-strong` label.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Keep the key pushed in (`face-key-down` with `shade`): the chosen
    /// segment of a choice, such as the current view.
    pub fn pressed(mut self, pressed: bool) -> Self {
        self.pressed = pressed;
        self
    }

    /// Whether the key takes clicks; disabled it draws flat `key-lo`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
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
        if !self.enabled {
            return ui.add_enabled(
                false,
                Key {
                    enabled: true,
                    ..self
                },
            );
        }
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

        let held = enabled && response.is_pointer_button_down_on();
        let hovered = enabled && response.hovered();
        let lit = if !enabled {
            None
        } else if held {
            self.lit.or(self.lit_when_held)
        } else {
            self.lit
        };
        // On its own a key is rounded all round with its own edge; in a
        // strip it rounds only the strip's ends, and the strip draws one
        // edge round every key.
        let r = theme::RADIUS_CONTROL;
        let corners = match self.joint {
            None => CornerRadius::same(r),
            Some(Joint { first, last }) => {
                let (w, e) = (if first { r } else { 0 }, if last { r } else { 0 });
                CornerRadius {
                    nw: w,
                    sw: w,
                    ne: e,
                    se: e,
                }
            }
        };
        let painter = ui.painter();
        if self.joint.is_none() {
            theme::paint_edge(painter, rect, corners, theme::EDGE);
        }
        let face = |face, tint, top| theme::paint_key_face(painter, rect, corners, face, tint, top);

        let label_color = if !enabled {
            painter.rect_filled(rect, corners, theme::KEY_LO);
            theme::TEXT_DISABLED
        } else if let Some(lit) = lit {
            if held || self.pressed {
                face(lit.face(), LIT_HELD_TINT, Top::Shade);
            } else {
                face(lit.face(), Color32::WHITE, Top::SheenLit);
                if hovered {
                    // brightness(1.05): the faintest lift of light.
                    painter.rect_filled(rect, corners, theme::LIP);
                }
            }
            lit.on()
        } else {
            if held || self.pressed {
                face(Face::KeyDown, Color32::WHITE, Top::Shade);
            } else if hovered {
                face(Face::KeyHover, Color32::WHITE, Top::Sheen);
            } else {
                face(Face::Key, Color32::WHITE, Top::Sheen);
            }
            if self.selected {
                painter.rect_filled(rect, corners, theme::SELECTION);
            }
            self.color
                .unwrap_or(if held || hovered || self.selected || self.pressed {
                    theme::TEXT_STRONG
                } else {
                    theme::TEXT
                })
        };

        // The seam before every key but a strip's first: a 1px `edge` line
        // with a 1px `lip` of light beside it.
        if let Some(Joint { first: false, .. }) = self.joint {
            let x = rect.left();
            painter.vline(x + 0.5, rect.y_range(), Stroke::new(1.0, theme::EDGE));
            painter.vline(x + 1.5, rect.y_range(), Stroke::new(1.0, theme::LIP));
        }

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

/// Keys joined into one strip (SegmentedButton): one `edge` round the
/// whole row, `radius-control` at its two ends only, and a seam between
/// neighbours. Each key keeps its own states (rest, hover, held, lit,
/// pressed, disabled) and its own `Response`, in order.
pub fn segmented<const N: usize>(ui: &mut Ui, keys: [Key; N]) -> [Response; N] {
    let responses = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let mut i = 0;
            keys.map(|mut key| {
                key.joint = Some(Joint {
                    first: i == 0,
                    last: i + 1 == N,
                });
                i += 1;
                ui.add(key)
            })
        })
        .inner;
    let strip = responses
        .iter()
        .fold(Rect::NOTHING, |strip, r| strip.union(r.rect));
    if ui.is_rect_visible(strip) {
        theme::paint_edge(ui.painter(), strip, theme::RADIUS_CONTROL, theme::EDGE);
    }
    responses
}
