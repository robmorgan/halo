//! Halo's design system ("Halo, as designed") as code: the tokens from the
//! system's `tokens.json` under their role names, the type styles, the fonts,
//! and the paint helpers every widget is drawn with.
//!
//! The look is lit from above. A raised thing (key, cap, panel) is a
//! vertical gradient face, a 1px line of sheen along its top inside edge and
//! a 1px near-black edge around it. A recessed thing (readout, meter, slot)
//! is a flat well with shade inside its top and a 1px lip of light below.
//! Never hand-mix a new grey: every colour here is a token.

// The full token table lands before every consumer does (later redesign
// steps); drop this once each token has a use.
#![allow(dead_code)]

use std::sync::{Arc, LazyLock};

use eframe::egui::epaint::RectShape;
use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, RichText, Stroke,
    StrokeKind, TextureHandle, Vec2,
};

/// A translucent token, premultiplied the way `Color32` stores colours.
const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    const fn premultiply(c: u8, a: u8) -> u8 {
        ((c as u16 * a as u16 + 127) / 255) as u8
    }
    Color32::from_rgba_premultiplied(premultiply(r, a), premultiply(g, a), premultiply(b, a), a)
}

// ---------------------------------------------------------------- surfaces
pub const GROUND: Color32 = Color32::from_rgb(0x0a, 0x0a, 0x0b);
pub const PANEL: Color32 = Color32::from_rgb(0x1e, 0x1f, 0x21);
pub const PANEL_DEEP: Color32 = Color32::from_rgb(0x18, 0x19, 0x1b);
pub const WELL: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0c);
pub const KEY_HI: Color32 = Color32::from_rgb(0x35, 0x36, 0x3a);
pub const KEY: Color32 = Color32::from_rgb(0x2a, 0x2b, 0x2e);
pub const KEY_LO: Color32 = Color32::from_rgb(0x23, 0x24, 0x27);
pub const KEY_DOWN: Color32 = Color32::from_rgb(0x1f, 0x20, 0x23);
pub const CAP_HI: Color32 = Color32::from_rgb(0x46, 0x47, 0x4c);
pub const CAP: Color32 = Color32::from_rgb(0x2c, 0x2d, 0x31);
pub const CAP_LO: Color32 = Color32::from_rgb(0x16, 0x17, 0x19);
pub const SEGMENT_OFF: Color32 = Color32::from_rgb(0x1c, 0x1d, 0x20);
pub const LED_OFF: Color32 = Color32::from_rgb(0x0c, 0x0c, 0x0d);

// ---------------------------------------------------------------- light
pub const EDGE: Color32 = rgba(0, 0, 0, 0xeb);
pub const EDGE_PANEL: Color32 = rgba(0, 0, 0, 0xd9);
pub const SHEEN: Color32 = rgba(0xff, 0xff, 0xff, 0x14);
pub const SHEEN_PANEL: Color32 = rgba(0xff, 0xff, 0xff, 0x0e);
pub const SHEEN_LIT: Color32 = rgba(0xff, 0xff, 0xff, 0x6b);
pub const LIP: Color32 = rgba(0xff, 0xff, 0xff, 0x0b);
pub const SHADE: Color32 = rgba(0, 0, 0, 0xbf);

// ---------------------------------------------------------------- text
pub const TEXT_STRONG: Color32 = Color32::from_rgb(0xec, 0xec, 0xec);
pub const TEXT: Color32 = Color32::from_rgb(0xc8, 0xc8, 0xc8);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8a, 0x8a, 0x8a);
pub const TEXT_UNLIT: Color32 = Color32::from_rgb(0xa3, 0xa3, 0xa6);
pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0x5a, 0x5b, 0x5f);

// ---------------------------------------------------------------- states
pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0xaa, 0x28);
pub const ACCENT_HI: Color32 = Color32::from_rgb(0xff, 0xc7, 0x67);
pub const ACCENT_LO: Color32 = Color32::from_rgb(0xe3, 0x8f, 0x1c);
pub const ACCENT_ON: Color32 = Color32::from_rgb(0x2e, 0x24, 0x10);
pub const ACCENT_GLOW: Color32 = rgba(0xff, 0xaa, 0x28, 0x8c);
pub const PLAY: Color32 = Color32::from_rgb(0x5a, 0xdc, 0x78);
pub const PLAY_HI: Color32 = Color32::from_rgb(0x8c, 0xee, 0xa4);
pub const PLAY_LO: Color32 = Color32::from_rgb(0x3d, 0xbf, 0x5f);
pub const PLAY_ON: Color32 = Color32::from_rgb(0x20, 0x38, 0x28);
pub const AUDITION: Color32 = Color32::from_rgb(0xff, 0xd7, 0x46);
pub const AUDITION_HI: Color32 = Color32::from_rgb(0xff, 0xe5, 0x8a);
pub const AUDITION_LO: Color32 = Color32::from_rgb(0xe6, 0xbd, 0x2a);
pub const AUDITION_ON: Color32 = Color32::from_rgb(0x3a, 0x32, 0x1a);

// ---------------------------------------------------------------- signals
pub const MUSICAL_KEY: Color32 = Color32::from_rgb(0x78, 0xbe, 0xff);
pub const WARN: Color32 = Color32::from_rgb(0xe6, 0x5a, 0x46);
pub const METER_GREEN: Color32 = Color32::from_rgb(0x6e, 0xc8, 0x6e);
pub const METER_RED: Color32 = Color32::from_rgb(0xe6, 0x50, 0x50);
pub const SELECTION: Color32 = rgba(0xff, 0xaa, 0x28, 0x2e);

// ---------------------------------------------------------------- content
pub const WAVE_GROUND: Color32 = Color32::from_rgb(0x0a, 0x0a, 0x0f);
pub const WAVE_ANNOTATION: Color32 = Color32::from_rgb(0x8a, 0x8a, 0xa0);
pub const PLAYHEAD: Color32 = Color32::from_rgb(0xe6, 0x28, 0x28);
pub const LANE_LIGHTING: Color32 = Color32::from_rgb(0x50, 0xa5, 0xff);

// ---------------------------------------------------------------- shape
pub const RADIUS_CONTROL: u8 = 3;
pub const RADIUS_PANEL: u8 = 4;
pub const RADIUS_WINDOW: u8 = 6;

pub const SPACE_2: f32 = 2.0;
pub const SPACE_4: f32 = 4.0;
pub const SPACE_6: f32 = 6.0;
pub const SPACE_8: f32 = 8.0;
pub const SPACE_12: f32 = 12.0;
pub const SPACE_16: f32 = 16.0;
pub const SPACE_24: f32 = 24.0;

pub const SIZE_KEY: f32 = 30.0;
pub const SIZE_KEY_SMALL: f32 = 22.0;
pub const SIZE_PAD: f32 = 40.0;
pub const SIZE_TRANSPORT: f32 = 52.0;
pub const SIZE_KNOB: f32 = 60.0;
pub const SIZE_KNOB_SMALL: f32 = 40.0;
pub const SIZE_LED: f32 = 7.0;
pub const SIZE_NUMERALS: f32 = 30.0;

// ---------------------------------------------------------------- type

/// Font file names registered in [`install_fonts`].
const INTER_REGULAR: &str = "Inter-Regular";
const INTER_SEMIBOLD: &str = "Inter-SemiBold";
const INTER_BOLD: &str = "Inter-Bold";
const INTER_TABULAR: &str = "InterTabular-SemiBold";
const JETBRAINS_MONO: &str = "JetBrainsMono-Regular";
/// egui's bundled icon fallbacks, kept behind Inter in every chain.
const EMOJI_FALLBACKS: [&str; 2] = ["NotoEmoji-Regular", "emoji-icon-font"];

static SEMIBOLD: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("semibold".into()));
static BOLD: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("bold".into()));
static NUMERALS: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("numerals".into()));

/// egui has no font weights, only families of font files: one per weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    /// Inter 400: reading text.
    Regular,
    /// Inter 600: titles.
    SemiBold,
    /// Inter 700: labels and captions.
    Bold,
    /// Inter 600 with tabular figures frozen in: every live number.
    Numerals,
    /// JetBrains Mono: DMX addresses only.
    Mono,
}

impl Weight {
    pub fn family(self) -> FontFamily {
        match self {
            Self::Regular => FontFamily::Proportional,
            Self::SemiBold => SEMIBOLD.clone(),
            Self::Bold => BOLD.clone(),
            Self::Numerals => NUMERALS.clone(),
            Self::Mono => FontFamily::Monospace,
        }
    }
}

/// One of the system's type styles: size, weight and letter-spacing.
#[derive(Clone, Copy, Debug)]
pub struct TypeStyle {
    pub size: f32,
    pub weight: Weight,
    /// Extra advance after every character, in points.
    pub spacing: f32,
}

impl TypeStyle {
    const fn new(size: f32, weight: Weight, spacing: f32) -> Self {
        Self {
            size,
            weight,
            spacing,
        }
    }

    pub fn font_id(&self) -> FontId {
        FontId::new(self.size, self.weight.family())
    }

    /// The same style at another size (glyph labels on keys).
    pub const fn sized(self, size: f32) -> Self {
        Self { size, ..self }
    }

    pub fn rich(&self, text: impl Into<String>) -> RichText {
        RichText::new(text)
            .font(self.font_id())
            .extra_letter_spacing(self.spacing)
    }

    pub fn format(&self, color: Color32) -> egui::TextFormat {
        egui::TextFormat {
            font_id: self.font_id(),
            extra_letter_spacing: self.spacing,
            color,
            ..Default::default()
        }
    }

    /// Lay `text` out on one line.
    pub fn galley(&self, ctx: &egui::Context, text: &str, color: Color32) -> Arc<egui::Galley> {
        let mut job = egui::text::LayoutJob::single_section(text.to_owned(), self.format(color));
        job.wrap.max_rows = 1;
        job.break_on_newline = false;
        ctx.fonts(|f| f.layout_job(job))
    }

    /// Width of `text` without the trailing letter-space egui adds after
    /// the last character, so spaced labels centre truly.
    pub fn width(&self, ctx: &egui::Context, text: &str) -> f32 {
        self.galley(ctx, text, TEXT).size().x - self.spacing
    }
}

pub const NUMERALS_XL: TypeStyle = TypeStyle::new(30.0, Weight::Numerals, -0.6);
pub const NUMERALS_MD: TypeStyle = TypeStyle::new(18.0, Weight::Numerals, -0.2);
pub const NUMERALS_SM: TypeStyle = TypeStyle::new(13.0, Weight::Numerals, 0.0);
pub const KEY_LABEL: TypeStyle = TypeStyle::new(11.0, Weight::Bold, 0.77);
pub const CAPTION: TypeStyle = TypeStyle::new(10.0, Weight::Bold, 1.4);
pub const PAD_NUMBER: TypeStyle = TypeStyle::new(12.0, Weight::Bold, 0.0);
pub const WORDMARK: TypeStyle = TypeStyle::new(18.0, Weight::Bold, 3.6);
pub const TRACK_TITLE: TypeStyle = TypeStyle::new(16.0, Weight::SemiBold, 0.0);
pub const BODY_TEXT: TypeStyle = TypeStyle::new(13.0, Weight::Regular, 0.0);
pub const NOTE_TEXT: TypeStyle = TypeStyle::new(11.0, Weight::Regular, 0.0);
pub const MONO_TEXT: TypeStyle = TypeStyle::new(12.0, Weight::Mono, 0.0);

/// Register Inter (three weights plus the tabular copy) and JetBrains Mono,
/// each ahead of egui's emoji fallbacks so Halo's Unicode icons still draw.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // egui's stock text faces are replaced outright; only the emoji
    // fallbacks stay.
    fonts.font_data.remove("Hack");
    fonts.font_data.remove("Ubuntu-Light");
    for (name, bytes) in [
        (
            INTER_REGULAR,
            &include_bytes!("../assets/fonts/Inter-Regular.ttf")[..],
        ),
        (
            INTER_SEMIBOLD,
            &include_bytes!("../assets/fonts/Inter-SemiBold.ttf")[..],
        ),
        (
            INTER_BOLD,
            &include_bytes!("../assets/fonts/Inter-Bold.ttf")[..],
        ),
        (
            INTER_TABULAR,
            &include_bytes!("../assets/fonts/InterTabular-SemiBold.ttf")[..],
        ),
        (
            JETBRAINS_MONO,
            &include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")[..],
        ),
    ] {
        fonts.font_data.insert(
            name.to_owned(),
            Arc::new(egui::FontData::from_static(bytes)),
        );
    }
    let chain = |first: &[&str]| -> Vec<String> {
        first
            .iter()
            .chain(EMOJI_FALLBACKS.iter())
            .map(|s| (*s).to_owned())
            .collect()
    };
    fonts.families.clear();
    fonts
        .families
        .insert(FontFamily::Proportional, chain(&[INTER_REGULAR]));
    fonts
        .families
        .insert(SEMIBOLD.clone(), chain(&[INTER_SEMIBOLD]));
    fonts.families.insert(BOLD.clone(), chain(&[INTER_BOLD]));
    fonts
        .families
        .insert(NUMERALS.clone(), chain(&[INTER_TABULAR, INTER_SEMIBOLD]));
    fonts.families.insert(
        FontFamily::Monospace,
        chain(&[JETBRAINS_MONO, INTER_REGULAR]),
    );
    ctx.set_fonts(fonts);
}

// ---------------------------------------------------------------- faces

/// The system's vertical `face-*` gradients.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Panel,
    Key,
    KeyHover,
    KeyDown,
    Accent,
    Play,
    Audition,
    Cap,
}

impl Face {
    const ALL: [Face; 8] = [
        Face::Panel,
        Face::Key,
        Face::KeyHover,
        Face::KeyDown,
        Face::Accent,
        Face::Play,
        Face::Audition,
        Face::Cap,
    ];

    /// Gradient stops, top to bottom, as (offset 0..1, colour).
    fn stops(self) -> &'static [(f32, Color32)] {
        // face-key-hover's stops have no role names of their own.
        const HOVER_HI: Color32 = Color32::from_rgb(0x3c, 0x3d, 0x42);
        const HOVER: Color32 = Color32::from_rgb(0x31, 0x32, 0x36);
        const HOVER_LO: Color32 = Color32::from_rgb(0x29, 0x2a, 0x2d);
        match self {
            Face::Panel => &[(0.0, PANEL), (1.0, PANEL_DEEP)],
            Face::Key => &[(0.0, KEY_HI), (0.45, KEY), (1.0, KEY_LO)],
            Face::KeyHover => &[(0.0, HOVER_HI), (0.45, HOVER), (1.0, HOVER_LO)],
            Face::KeyDown => &[(0.0, SEGMENT_OFF), (1.0, KEY_DOWN)],
            Face::Accent => &[(0.0, ACCENT_HI), (0.46, ACCENT), (1.0, ACCENT_LO)],
            Face::Play => &[(0.0, PLAY_HI), (0.46, PLAY), (1.0, PLAY_LO)],
            Face::Audition => &[(0.0, AUDITION_HI), (0.46, AUDITION), (1.0, AUDITION_LO)],
            Face::Cap => &[(0.0, CAP_HI), (0.55, CAP), (1.0, CAP_LO)],
        }
    }
}

/// Texels per gradient texture.
const FACE_TEXELS: usize = 32;

/// Interpolate gradient stops at `t` (0 = top), in gamma space like CSS.
fn sample_stops(stops: &[(f32, Color32)], t: f32) -> Color32 {
    let mut prev = stops[0];
    for &next in stops {
        if t <= next.0 {
            let span = (next.0 - prev.0).max(f32::EPSILON);
            let k = ((t - prev.0) / span).clamp(0.0, 1.0);
            let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * k).round() as u8;
            return Color32::from_rgb(
                lerp(prev.1.r(), next.1.r()),
                lerp(prev.1.g(), next.1.g()),
                lerp(prev.1.b(), next.1.b()),
            );
        }
        prev = next;
    }
    prev.1
}

#[derive(Clone)]
struct FaceTextures(Arc<Vec<TextureHandle>>);

fn faces_id() -> egui::Id {
    egui::Id::new("halo-theme-faces")
}

/// Build each `face-*` gradient once as a 1×32 texture.
fn install_faces(ctx: &egui::Context) {
    let handles = Face::ALL
        .iter()
        .map(|face| {
            let pixels = (0..FACE_TEXELS)
                .map(|i| sample_stops(face.stops(), (i as f32 + 0.5) / FACE_TEXELS as f32))
                .collect();
            let image = egui::ColorImage {
                size: [1, FACE_TEXELS],
                pixels,
            };
            ctx.load_texture(
                format!("halo-face-{face:?}"),
                image,
                egui::TextureOptions::LINEAR,
            )
        })
        .collect();
    ctx.data_mut(|d| d.insert_temp(faces_id(), FaceTextures(Arc::new(handles))));
}

fn face_texture(ctx: &egui::Context, face: Face) -> egui::TextureId {
    let faces = ctx.data(|d| d.get_temp::<FaceTextures>(faces_id()));
    let faces = match faces {
        Some(f) => f,
        None => {
            install_faces(ctx);
            ctx.data(|d| d.get_temp::<FaceTextures>(faces_id()))
                .expect("face textures just installed")
        }
    };
    let idx = Face::ALL.iter().position(|f| *f == face).unwrap_or(0);
    faces.0[idx].id()
}

/// UV span that lands the first and last texel centres on the rect's top
/// and bottom, so the gradient's end stops are exact.
fn face_uv() -> Rect {
    let half = 0.5 / FACE_TEXELS as f32;
    Rect::from_min_max(egui::pos2(0.5, half), egui::pos2(0.5, 1.0 - half))
}

/// Fill `rect` with a gradient face; `tint` multiplies it (white = as is).
pub fn paint_face(painter: &Painter, rect: Rect, radius: u8, face: Face, tint: Color32) {
    let tex = face_texture(painter.ctx(), face);
    painter.add(RectShape::filled(rect, radius, tint).with_texture(tex, face_uv()));
}

/// A 1px line along the top inside edge of a rounded rect.
fn top_line(painter: &Painter, rect: Rect, radius: u8, y_offset: f32, color: Color32) {
    let inset = radius as f32 * 0.6;
    painter.hline(
        (rect.left() + inset)..=(rect.right() - inset),
        rect.top() + y_offset + 0.5,
        Stroke::new(1.0, color),
    );
}

/// Inner shade along the top of a pushed-in or recessed rect: the
/// `inset 0 1px 3px` of `shade`, fading over three rows.
fn top_shade(painter: &Painter, rect: Rect, radius: u8) {
    for (row, k) in [(0.0, 1.0), (1.0, 0.45), (2.0, 0.15)] {
        top_line(painter, rect, radius, row, SHADE.gamma_multiply(k));
    }
}

/// The 1px dark edge outside a shape.
pub fn paint_edge(painter: &Painter, rect: Rect, radius: u8, color: Color32) {
    painter.rect_stroke(
        rect,
        CornerRadius::same(radius),
        Stroke::new(1.0, color),
        StrokeKind::Outside,
    );
}

/// A raised face: gradient, 1px `edge` around it, 1px `sheen` on top
/// (`face-*` + `shadow-key`).
pub fn paint_raised(painter: &Painter, rect: Rect, radius: u8, face: Face) {
    paint_edge(painter, rect, radius, EDGE);
    paint_face(painter, rect, radius, face, Color32::WHITE);
    top_line(painter, rect, radius, 0.0, SHEEN);
}

/// A lit face: a state gradient with `sheen-lit` on top (`shadow-lit`).
/// `tint` dims it while held.
pub fn paint_lit(painter: &Painter, rect: Rect, radius: u8, face: Face, tint: Color32) {
    paint_edge(painter, rect, radius, EDGE);
    paint_face(painter, rect, radius, face, tint);
    top_line(painter, rect, radius, 0.0, SHEEN_LIT);
}

/// A pushed-in face: `face-key-down` (or a dimmed lit face) with shade
/// replacing the sheen (`shadow-key-down`).
pub fn paint_pressed(painter: &Painter, rect: Rect, radius: u8, face: Face, tint: Color32) {
    paint_edge(painter, rect, radius, EDGE);
    paint_face(painter, rect, radius, face, tint);
    top_shade(painter, rect, radius);
}

/// A recessed well: flat `well`, shade inside the top, a 1px `lip` of
/// light under the bottom (`shadow-well`).
pub fn paint_well(painter: &Painter, rect: Rect, radius: u8) {
    painter.rect_filled(rect, radius, WELL);
    top_shade(painter, rect, radius);
    let inset = radius as f32 * 0.6;
    painter.hline(
        (rect.left() + inset)..=(rect.right() - inset),
        rect.bottom() + 0.5,
        Stroke::new(1.0, LIP),
    );
}

/// A well with a 1px `edge` (meters, fader slots, LED bezels).
pub fn paint_well_edged(painter: &Painter, rect: Rect, radius: u8) {
    painter.rect_filled(rect, radius, WELL);
    paint_edge(painter, rect, radius, EDGE);
}

/// A soft glow behind a light source (lit LEDs, the lit beat step, the
/// master-source letter). Never on keys, text or panels.
pub fn paint_glow(painter: &Painter, rect: Rect, radius: f32, blur: f32, color: Color32) {
    painter.add(RectShape::filled(rect, CornerRadius::from(radius), color).with_blur_width(blur));
}

/// A 7px LED: lit in `color` with `shadow-led`, or a dark `led-off` lens.
pub fn paint_led(painter: &Painter, center: Pos2, on: bool, color: Color32) {
    let r = SIZE_LED / 2.0;
    let rect = Rect::from_center_size(center, Vec2::splat(SIZE_LED));
    if on {
        let glow = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 0x8c);
        paint_glow(painter, rect, r, SIZE_LED, glow);
        painter.circle(center, r, color, Stroke::new(1.0, rgba(0, 0, 0, 0x8c)));
    } else {
        painter.circle_filled(center, r, LED_OFF);
        // inset 0 1px 1px black, then a sheen lip under the lens.
        painter.circle_stroke(
            center + egui::vec2(0.0, 0.5),
            r - 0.5,
            Stroke::new(1.0, Color32::BLACK),
        );
        painter.circle_stroke(center + egui::vec2(0.0, 1.0), r, Stroke::new(1.0, SHEEN));
        painter.circle_filled(center, r - 1.0, LED_OFF);
    }
}

// ---------------------------------------------------------------- panels

/// A panel's surface in `rect`: `face-panel` with `shadow-panel`. Its soft
/// drop (`0 22px 44px -30px black`) needs a negative spread, which a frame
/// `Shadow` can't express, so it is a blurred rect instead.
pub fn panel_shapes(ctx: &egui::Context, rect: Rect) -> Vec<egui::Shape> {
    let mut shapes = Vec::new();
    let drop = rect.shrink(30.0).translate(egui::vec2(0.0, 22.0));
    if drop.is_positive() {
        shapes.push(
            RectShape::filled(drop.expand(22.0), RADIUS_PANEL, Color32::BLACK)
                .with_blur_width(44.0)
                .into(),
        );
    }
    shapes.push(
        RectShape::new(
            rect,
            RADIUS_PANEL,
            Color32::TRANSPARENT,
            Stroke::new(1.0, EDGE_PANEL),
            StrokeKind::Outside,
        )
        .into(),
    );
    shapes.push(
        RectShape::filled(rect, RADIUS_PANEL, Color32::WHITE)
            .with_texture(face_texture(ctx, Face::Panel), face_uv())
            .into(),
    );
    let inset = RADIUS_PANEL as f32 * 0.6;
    shapes.push(egui::Shape::hline(
        (rect.left() + inset)..=(rect.right() - inset),
        rect.top() + 0.5,
        Stroke::new(1.0, SHEEN_PANEL),
    ));
    shapes
}

/// Lay `content` out on a panel: `face-panel` with `shadow-panel`, `space-16`
/// inside. The surface is painted behind the content once its size is known.
pub fn panel<R>(
    ui: &mut egui::Ui,
    margin: f32,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let background = ui.painter().add(egui::Shape::Noop);
    let inner = egui::Frame::new().inner_margin(margin).show(ui, content);
    let shapes = panel_shapes(ui.ctx(), inner.response.rect);
    ui.painter().set(background, egui::Shape::Vec(shapes));
    inner
}

/// The window behind everything: `ground`, with room around the panels.
pub fn ground_frame(margin: f32) -> egui::Frame {
    egui::Frame::new().fill(GROUND).inner_margin(margin)
}

/// Install the theme: fonts, gradient textures and egui's stock visuals.
pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    install_faces(ctx);
}
