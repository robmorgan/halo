//! The key: the one button Halo's performance surface is made of.
//!
//! A raised `face-key` with a line of sheen and a 1px dark edge, 30 high
//! with 12 either side of a bold spaced label. Hover lightens the face and
//! never grows it; holding pushes it in. A key can light in a state colour
//! (a held CUE, a playing PLAY), outlined and labelled in it, or carry an
//! LED before its label (latched toggles).
//!
//! Keys can also be joined into one strip with [`segmented`], and drawn
//! in the decks' quieter skin with [`Key::quiet`]. [`latch`] is the
//! rectangle latch beside them that fills red when on, and [`Checkbox`]
//! a small key face that lights with a tick.
//!
//! The widget only draws: it allocates a click-sensing rect and returns the
//! plain `Response`, so callers keep their own press / release / right-click
//! handling exactly as with `egui::Button`.

use eframe::egui::{
    self, Color32, CornerRadius, Rect, Response, Role, Sense, Stroke, Ui, Vec2, Widget, WidgetInfo,
};

use crate::theme::{self, Face, Skin, Top, TypeStyle};

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
    /// The outline and label a lit key takes.
    fn color(self) -> Color32 {
        match self {
            Lit::Accent => theme::ACCENT,
            Lit::Play => theme::TEXT_STRONG,
            Lit::Audition => theme::AUDITION,
        }
    }
}

/// A lit key's outline, inside its edge, and the wash of its colour over
/// the face.
const LIT_OUTLINE: f32 = 1.5;
const LIT_WASH: f32 = 0.08;
/// A [`Key::pulse`]: one breath in and out per period (seconds), never
/// fading below `PULSE_MIN`, redrawn on the programmer preview's slower
/// tick so an idle deck doesn't pay for 30 fps.
const PULSE_PERIOD: f64 = 2.8;
const PULSE_MIN: f32 = 0.15;
const PULSE_TICK: std::time::Duration = std::time::Duration::from_millis(80);
/// A [`Key::strip`]: 2 high, 8 in from either side, 4 up from the bottom.
const STRIP_H: f32 = 2.0;
const STRIP_INSET: f32 = 8.0;
const STRIP_BOTTOM: f32 = 4.0;
/// A [`Key::corner_label`]: in from the right edge, down from the top.
const CORNER_INSET: Vec2 = egui::vec2(6.0, 4.0);

#[must_use = "add it with `ui.add(...)`"]
pub struct Key {
    label: String,
    /// `None`: the skin's key label.
    style: Option<TypeStyle>,
    skin: Skin,
    size: Option<Vec2>,
    width: Option<f32>,
    small: bool,
    lit: Option<Lit>,
    lit_when_held: Option<Lit>,
    color: Option<Color32>,
    led: Option<(bool, Color32)>,
    strip: Option<Color32>,
    corner: Option<(String, Color32)>,
    pulse: Option<Color32>,
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
            style: None,
            skin: theme::SKIN_SYSTEM,
            size: None,
            width: None,
            small: false,
            lit: None,
            lit_when_held: None,
            color: None,
            led: None,
            strip: None,
            corner: None,
            pulse: None,
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

    /// An outline that slowly pulses in `color` while the key is enabled
    /// and unlit (a loaded deck's PLAY, waiting to start).
    pub fn pulse(mut self, color: Option<Color32>) -> Self {
        self.pulse = color;
        self
    }

    /// A small `pad-number` label in the top-right corner (a hot cue
    /// pad's slot number), leaving the centre for the key's own label.
    pub fn corner_label(mut self, label: impl Into<String>, color: Color32) -> Self {
        self.corner = Some((label.into(), color));
        self
    }

    /// A 2px colour strip along the bottom of the face (a hot cue pad's
    /// slot colour, `segment-off` when empty).
    pub fn strip(mut self, color: Color32) -> Self {
        self.strip = Some(color);
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

    /// The label's type style (default: the skin's key label).
    pub fn text_style(mut self, style: TypeStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Draw in the decks' quieter skin: flatter faces, fainter sheen and
    /// lip, 600-weight labels and 5px LEDs.
    pub fn quiet(mut self) -> Self {
        self.skin = theme::SKIN_QUIET;
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
        let skin = self.skin;
        let style = self.style.unwrap_or(skin.key_label);
        let label_w = if self.label.is_empty() {
            0.0
        } else {
            style.width(&ctx, &self.label)
        };
        let led_w = if self.led.is_some() {
            skin.led + if label_w > 0.0 { gap } else { 0.0 }
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
        response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, &self.label));
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
            painter.rect_filled(rect, corners, skin.key_disabled);
            theme::TEXT_DISABLED
        } else if let Some(lit) = lit {
            let color = lit.color();
            if held || self.pressed {
                face(Face::KeyDown, Color32::WHITE, Top::Shade);
            } else if hovered {
                face(skin.key_hover, Color32::WHITE, Top::Sheen(skin.sheen));
            } else {
                face(skin.key, Color32::WHITE, Top::Sheen(skin.sheen));
            }
            painter.rect_filled(rect, corners, color.gamma_multiply(LIT_WASH));
            painter.rect_stroke(
                rect,
                corners,
                Stroke::new(LIT_OUTLINE, color),
                egui::StrokeKind::Inside,
            );
            color
        } else {
            if held || self.pressed {
                face(Face::KeyDown, Color32::WHITE, Top::Shade);
            } else if hovered {
                face(skin.key_hover, Color32::WHITE, Top::Sheen(skin.sheen));
            } else {
                face(skin.key, Color32::WHITE, Top::Sheen(skin.sheen));
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
            painter.vline(x + 1.5, rect.y_range(), Stroke::new(1.0, skin.lip));
        }

        if let Some(color) = self.pulse.filter(|_| enabled && lit.is_none()) {
            let t = ui.input(|i| i.time);
            let k = (0.5 - 0.5 * (t * std::f64::consts::TAU / PULSE_PERIOD).cos()) as f32;
            painter.rect_stroke(
                rect,
                corners,
                Stroke::new(
                    LIT_OUTLINE,
                    color.gamma_multiply(PULSE_MIN + (1.0 - PULSE_MIN) * k),
                ),
                egui::StrokeKind::Inside,
            );
            ui.ctx().request_repaint_after(PULSE_TICK);
        }

        if let Some((label, color)) = &self.corner {
            theme::paint_text(
                painter,
                egui::pos2(rect.right() - CORNER_INSET.x, rect.top() + CORNER_INSET.y),
                egui::Align2::RIGHT_TOP,
                label,
                theme::PAD_NUMBER,
                if enabled {
                    *color
                } else {
                    theme::TEXT_DISABLED
                },
            );
        }

        if let Some(color) = self.strip.filter(|_| enabled) {
            let bottom = rect.bottom() - STRIP_BOTTOM;
            let strip = Rect::from_x_y_ranges(
                rect.left() + STRIP_INSET..=rect.right() - STRIP_INSET,
                bottom - STRIP_H..=bottom,
            );
            painter.rect_filled(strip, CornerRadius::same(1), color);
        }

        let mut x = rect.center().x - content_w / 2.0;
        if let Some((on, color)) = self.led {
            let center = egui::pos2(x + skin.led / 2.0, rect.center().y);
            theme::paint_led_sized(painter, center, on && enabled, color, skin.led);
            x += led_w;
        }
        if label_w > 0.0 {
            let galley = style.galley(&ctx, &self.label, label_color);
            let pos = egui::pos2(x, rect.center().y - galley.size().y / 2.0);
            painter.galley(pos.round(), galley, label_color);
        }
        if response.has_focus() {
            theme::paint_focus(painter, rect);
        }
        response
    }
}

/// Keys joined into one strip (SegmentedButton): one `edge` round the
/// whole row, `radius-control` at its two ends only, and a seam between
/// neighbours. Each key keeps its own states (rest, hover, held, lit,
/// pressed, disabled) and its own `Response`, in order.
pub fn segmented<const N: usize>(ui: &mut Ui, keys: [Key; N]) -> [Response; N] {
    let strip = |ui: &mut Ui| {
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
    };
    // Always a row: in a row already, keep its alignment; otherwise
    // start one.
    let responses = if ui.layout().is_horizontal() {
        ui.scope(strip).inner
    } else {
        ui.horizontal(strip).inner
    };
    let strip = responses
        .iter()
        .fold(Rect::NOTHING, |strip, r| strip.union(r.rect));
    if ui.is_rect_visible(strip) {
        theme::paint_edge(ui.painter(), strip, theme::RADIUS_CONTROL, theme::EDGE);
    }
    responses
}

/// A lit latch at rest, a shade under its hovered face.
const LATCH_REST_TINT: Color32 = Color32::from_gray(232);

/// A latch drawn as a rectangle round its label (A.CUE, Q), `size`
/// across. Off it is a key's face, outlined, with a `text-muted` label; on
/// it lights red with a dark label. The whole rect takes the click, which
/// flips `on`.
pub fn latch(ui: &mut Ui, on: &mut bool, label: &str, size: Vec2) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let enabled = response.enabled();
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, *on, label));
    if response.clicked() {
        *on = !*on;
    }
    if ui.is_rect_visible(rect) {
        let hovered = enabled && response.hovered();
        let painter = ui.painter();
        let corners = CornerRadius::same(theme::RADIUS_CONTROL);
        let label_color = if !enabled {
            painter.rect_stroke(
                rect,
                corners,
                Stroke::new(1.0, theme::EDGE.gamma_multiply(0.5)),
                egui::StrokeKind::Inside,
            );
            theme::TEXT_DISABLED
        } else if *on {
            let tint = if hovered {
                Color32::WHITE
            } else {
                LATCH_REST_TINT
            };
            theme::paint_face(painter, rect, corners, Face::Latch, tint);
            theme::GROUND
        } else {
            let (face, edge, label) = if hovered {
                (Face::QuietKeyHover, theme::TEXT_MUTED, theme::TEXT)
            } else {
                (Face::QuietKey, theme::EDGE, theme::TEXT_MUTED)
            };
            theme::paint_face(painter, rect, corners, face, Color32::WHITE);
            painter.rect_stroke(
                rect,
                corners,
                Stroke::new(1.0, edge),
                egui::StrokeKind::Inside,
            );
            label
        };
        theme::paint_text(
            painter,
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            theme::TEXT_TOGGLE,
            label_color,
        );
    }
    response
}

/// A checkbox's state. Mixed is for a box over others, some of them on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    Off,
    On,
    // No box over others yet.
    #[allow(dead_code)]
    Mixed,
}

impl Check {
    pub fn from_bool(on: bool) -> Self {
        if on { Check::On } else { Check::Off }
    }

    /// What a click makes it: a mixed box turns everything on, an on box
    /// everything off.
    pub fn clicked(self) -> Self {
        match self {
            Check::On => Check::Off,
            Check::Off | Check::Mixed => Check::On,
        }
    }

    fn lit(self) -> bool {
        self != Check::Off
    }
}

/// A checkbox: an 18px `face-key` box with its label beside it, and an
/// optional hint under the label. On or mixed, the box keeps its face
/// under an accent wash and outline, with the tick or minus in accent.
/// The whole row takes the click; like [`Key`] it only draws, and the
/// caller applies [`Check::clicked`].
pub struct Checkbox {
    state: Check,
    label: String,
    hint: Option<String>,
    enabled: bool,
}

/// The box, across and down.
const CHECK_BOX: f32 = 18.0;
/// Between the box and its label.
const CHECK_GAP: f32 = 10.0;
/// Between the label and its hint.
const CHECK_HINT_GAP: f32 = 3.0;
/// The tick's and the minus's line.
const CHECK_GLYPH_W: f32 = 2.0;

impl Checkbox {
    pub fn new(state: Check, label: impl Into<String>) -> Self {
        Self {
            state,
            label: label.into(),
            hint: None,
            enabled: true,
        }
    }

    /// A note under the label: sentence case, an em dash for the aside.
    #[allow(dead_code)]
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Disabled it's dimmed, never hidden.
    #[allow(dead_code)]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Widget for Checkbox {
    fn ui(self, ui: &mut Ui) -> Response {
        if !self.enabled {
            return ui.add_enabled(
                false,
                Checkbox {
                    enabled: true,
                    ..self
                },
            );
        }
        let ctx = ui.ctx().clone();
        let label = self.label.to_uppercase();
        let label_size = theme::TEXT_TOGGLE.galley(&ctx, &label, theme::TEXT).size();
        let hint = self
            .hint
            .as_ref()
            .map(|hint| theme::NOTE_TEXT.galley(&ctx, hint, theme::TEXT_MUTED));
        let label_w = label_size.x - theme::TEXT_TOGGLE.spacing;
        let text_w = hint.as_ref().map_or(label_w, |g| label_w.max(g.size().x));
        // The label's first line sits on the key row's centre line; a hint
        // adds its own line under it.
        let line_y = theme::SIZE_KEY / 2.0;
        let height = hint.as_ref().map_or(theme::SIZE_KEY, |g| {
            line_y + label_size.y / 2.0 + CHECK_HINT_GAP + g.size().y + line_y - label_size.y / 2.0
        });
        let size = egui::vec2(CHECK_BOX + CHECK_GAP + text_w, height);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let enabled = response.enabled();
        response.widget_info(|| {
            WidgetInfo::selected(Role::CheckBox, enabled, self.state == Check::On, &label)
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }

        let held = enabled && response.is_pointer_button_down_on();
        let hovered = enabled && response.hovered();
        let line = rect.top() + line_y;
        let check = Rect::from_center_size(
            egui::pos2(rect.left() + CHECK_BOX / 2.0, line),
            Vec2::splat(CHECK_BOX),
        );
        let corners = CornerRadius::same(theme::RADIUS_CONTROL);
        let painter = ui.painter();
        theme::paint_edge(painter, check, corners, theme::EDGE);
        let lit = enabled && self.state.lit();
        if !enabled {
            painter.rect_filled(check, corners, theme::KEY_LO);
        } else {
            let (face, top) = if held {
                (Face::KeyDown, Top::Shade)
            } else if hovered {
                (Face::KeyHover, Top::Sheen(theme::SHEEN))
            } else {
                (Face::Key, Top::Sheen(theme::SHEEN))
            };
            theme::paint_key_face(painter, check, corners, face, Color32::WHITE, top);
            if lit {
                painter.rect_filled(check, corners, theme::ACCENT.gamma_multiply(LIT_WASH));
                painter.rect_stroke(
                    check,
                    corners,
                    Stroke::new(LIT_OUTLINE, theme::ACCENT),
                    egui::StrokeKind::Inside,
                );
            }
        }
        let glyph = if enabled {
            theme::ACCENT
        } else {
            theme::TEXT_DISABLED
        };
        let at = |x: f32, y: f32| check.min + egui::vec2(x, y);
        match self.state {
            Check::Off => {}
            Check::On => {
                let points = square_ends([at(4.5, 9.0), at(7.5, 12.0), at(13.5, 6.0)]);
                painter.add(egui::Shape::line(
                    points.to_vec(),
                    Stroke::new(CHECK_GLYPH_W, glyph),
                ));
            }
            // A filled bar: square ends by construction.
            Check::Mixed => {
                painter.rect_filled(
                    Rect::from_center_size(check.center(), egui::vec2(8.0, CHECK_GLYPH_W)),
                    0.0,
                    glyph,
                );
            }
        }

        let label_color = if !enabled {
            theme::TEXT_DISABLED
        } else if lit || hovered {
            theme::TEXT_STRONG
        } else {
            theme::TEXT_UNLIT
        };
        let text_x = check.right() + CHECK_GAP;
        theme::paint_text(
            painter,
            egui::pos2(text_x, line),
            egui::Align2::LEFT_CENTER,
            &label,
            theme::TEXT_TOGGLE,
            label_color,
        );
        if let Some(hint) = &self.hint {
            let color = if enabled {
                theme::TEXT_MUTED
            } else {
                theme::TEXT_DISABLED
            };
            theme::paint_text(
                painter,
                egui::pos2(text_x, line + label_size.y / 2.0 + CHECK_HINT_GAP),
                egui::Align2::LEFT_TOP,
                hint,
                theme::NOTE_TEXT,
                color,
            );
        }
        if response.has_focus() {
            theme::paint_focus(painter, check);
        }
        response
    }
}

/// A polyline's two ends pushed out by half its line, so they end square
/// (egui's strokes end flat at the points).
fn square_ends<const N: usize>(mut points: [egui::Pos2; N]) -> [egui::Pos2; N] {
    let half = CHECK_GLYPH_W / 2.0;
    if N >= 2 {
        let start = (points[0] - points[1]).normalized() * half;
        points[0] += start;
        let end = (points[N - 1] - points[N - 2]).normalized() * half;
        points[N - 1] += end;
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_turns_mixed_on_and_on_off() {
        assert_eq!(Check::Off.clicked(), Check::On);
        assert_eq!(Check::Mixed.clicked(), Check::On);
        assert_eq!(Check::On.clicked(), Check::Off);
    }

    #[test]
    fn the_ticks_ends_reach_out_half_its_line() {
        let [start, _, end] = square_ends([
            egui::pos2(4.5, 9.0),
            egui::pos2(7.5, 12.0),
            egui::pos2(13.5, 6.0),
        ]);
        let half = CHECK_GLYPH_W / 2.0;
        assert!((start.distance(egui::pos2(4.5, 9.0)) - half).abs() < 1e-4);
        assert!((end.distance(egui::pos2(13.5, 6.0)) - half).abs() < 1e-4);
        assert!(start.x < 4.5 && end.x > 13.5);
    }
}
