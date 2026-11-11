//! Halo's design system ("Halo, as designed") as code: the tokens from the
//! system's `tokens.json` under their role names, the type styles, the fonts,
//! and the paint helpers every widget is drawn with.
//!
//! The look is lit from above. A raised thing (key, cap, panel) is a
//! vertical gradient face, a 1px line of sheen along its top inside edge and
//! a 1px near-black edge around it. A recessed thing (readout, meter, slot)
//! is a flat well with shade inside its top and a 1px lip of light below.
//! Never hand-mix a new grey: every colour here is a token.

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
pub const GROUND: Color32 = Color32::from_rgb(0x05, 0x05, 0x06);
pub const PANEL: Color32 = Color32::from_rgb(0x0e, 0x0f, 0x10);
pub const PANEL_DEEP: Color32 = PANEL;
pub const WELL: Color32 = Color32::from_rgb(0x07, 0x07, 0x08);
pub const KEY_HI: Color32 = Color32::from_rgb(0x1c, 0x1d, 0x20);
pub const KEY: Color32 = Color32::from_rgb(0x15, 0x16, 0x18);
pub const KEY_LO: Color32 = Color32::from_rgb(0x0f, 0x10, 0x11);
pub const KEY_DOWN: Color32 = Color32::from_rgb(0x1c, 0x1d, 0x20);
pub const CAP_HI: Color32 = CAP;
pub const CAP: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x1d);
pub const CAP_LO: Color32 = CAP;
pub const SEGMENT_OFF: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x1e);
pub const LED_OFF: Color32 = Color32::from_rgb(0x0c, 0x0c, 0x0d);

// ---------------------------------------------------------------- light
pub const EDGE: Color32 = rgba(0xff, 0xff, 0xff, 0x1f);
pub const EDGE_PANEL: Color32 = rgba(0xff, 0xff, 0xff, 0x10);
pub const SHEEN: Color32 = Color32::TRANSPARENT;
pub const SHEEN_PANEL: Color32 = Color32::TRANSPARENT;
pub const SHEEN_LIT: Color32 = Color32::TRANSPARENT;
pub const LIP: Color32 = Color32::TRANSPARENT;
pub const SHADE: Color32 = rgba(0, 0, 0, 0xbf);

// ---------------------------------------------------------------- text
pub const TEXT_STRONG: Color32 = Color32::from_rgb(0xec, 0xec, 0xec);
pub const TEXT: Color32 = Color32::from_rgb(0xc8, 0xc8, 0xc8);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8a, 0x8a, 0x8a);
pub const TEXT_UNLIT: Color32 = Color32::from_rgb(0xa3, 0xa3, 0xa6);
pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0x5a, 0x5b, 0x5f);

// ---------------------------------------------------------------- states
pub const ACCENT: Color32 = Color32::from_rgb(0x4a, 0xa8, 0xff);
pub const ACCENT_LO: Color32 = ACCENT;
pub const ACCENT_GLOW: Color32 = rgba(0x4a, 0xa8, 0xff, 0x8c);
pub const PLAY: Color32 = Color32::from_rgb(0x5a, 0xdc, 0x78);
pub const PLAY_HI: Color32 = PLAY;
pub const PLAY_LO: Color32 = PLAY;
pub const AUDITION: Color32 = Color32::from_rgb(0xff, 0xd7, 0x46);
pub const AUDITION_HI: Color32 = AUDITION;
pub const AUDITION_LO: Color32 = AUDITION;
/// The latch's red (A.CUE, Q on).
pub const LATCH: Color32 = Color32::from_rgb(0xf0, 0x66, 0x66);
pub const LATCH_LO: Color32 = Color32::from_rgb(0xd2, 0x44, 0x44);

// ---------------------------------------------------------------- signals
pub const MUSICAL_KEY: Color32 = TEXT;
pub const WARN: Color32 = Color32::from_rgb(0xe6, 0x5a, 0x46);
pub const METER_GREEN: Color32 = Color32::from_rgb(0xa8, 0xaa, 0xb0);
pub const METER_RED: Color32 = Color32::from_rgb(0xe6, 0x50, 0x50);
pub const SELECTION: Color32 = rgba(0x4a, 0xa8, 0xff, 0x2e);

// ---------------------------------------------------------------- content
// The waveform's and the show's own colours. They stay in content.
pub const WAVE_GROUND: Color32 = Color32::from_rgb(0x0a, 0x0a, 0x0f);
pub const WAVE_LOW: Color32 = Color32::from_rgb(0x28, 0x5a, 0xdc);
pub const WAVE_MID: Color32 = Color32::from_rgb(0xe6, 0x96, 0x28);
pub const WAVE_HIGH: Color32 = Color32::from_rgb(0xeb, 0xeb, 0xf0);
pub const WAVE_PLAYED: Color32 = Color32::from_rgb(0x6e, 0x6e, 0x76);
pub const WAVE_ANNOTATION: Color32 = Color32::from_rgb(0x8a, 0x8a, 0xa0);
pub const PLAYHEAD: Color32 = Color32::from_rgb(0xf4, 0xf4, 0xf6);
pub const BEAT_TICK: Color32 = rgba(0xff, 0xff, 0xff, 0xc8);
pub const LOOP_FILL: Color32 = rgba(0x4a, 0xa8, 0xff, 0x3c);
pub const LOOP_EDGE: Color32 = Color32::from_rgb(0x4a, 0xa0, 0xeb);
pub const ENERGY: Color32 = Color32::from_rgb(0x5a, 0xb0, 0xf0);
pub const LANE_GROUND: Color32 = Color32::from_rgb(0x10, 0x10, 0x16);
pub const LANE_SEPARATOR: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x24);
pub const LANE_LIGHTING: Color32 = Color32::from_rgb(0x50, 0xa5, 0xff);
pub const LANE_PIXELS: Color32 = Color32::from_rgb(0xf0, 0x5f, 0xaf);
pub const LANE_FX: Color32 = Color32::from_rgb(0x46, 0xd2, 0x82);
pub const LANE_ACCENTS: Color32 = Color32::from_rgb(0xee, 0xee, 0xf4);
// The look palette's values live in `show_preview.rs` (the show model);
// these name them for the system. Not every look is drawn by name here.
#[allow(dead_code)]
pub const LOOK_WARM_OPEN: Color32 = Color32::from_rgb(0xff, 0xaa, 0x3c);
#[allow(dead_code)]
pub const LOOK_DEEP_BLUE: Color32 = Color32::from_rgb(0x3c, 0x6e, 0xeb);
#[allow(dead_code)]
pub const LOOK_RED_DROP: Color32 = Color32::from_rgb(0xe6, 0x2d, 0x2d);
pub const LOOK_STROBE_WHITE: Color32 = Color32::from_rgb(0xee, 0xee, 0xf4);
#[allow(dead_code)]
pub const LOOK_ACID_GREEN: Color32 = Color32::from_rgb(0x78, 0xe1, 0x46);
#[allow(dead_code)]
pub const LOOK_MAGENTA_CHASE: Color32 = Color32::from_rgb(0xf0, 0x3c, 0xbe);
pub const LOOK_AMBER_SWEEP: Color32 = Color32::from_rgb(0xfa, 0xcd, 0x28);
pub const LOOK_UV_VIOLET: Color32 = Color32::from_rgb(0x8c, 0x46, 0xff);
pub const FIXTURE_R: Color32 = Color32::from_rgb(0xeb, 0x46, 0x3c);
pub const FIXTURE_G: Color32 = Color32::from_rgb(0x50, 0xcd, 0x5f);
pub const FIXTURE_B: Color32 = Color32::from_rgb(0x4b, 0x7d, 0xff);
pub const FIXTURE_W: Color32 = Color32::from_rgb(0xeb, 0xeb, 0xf0);
/// Hot cue colours by slot (1–8): a set pad's strip and its markers on the
/// waveforms. Red, orange, yellow, lime, teal, indigo, violet, pink: each
/// at least 4.9:1 on a deck key, no two closer than ΔE 33, the indigo kept
/// clear of the accent and the red of the latch.
pub const CUE: [Color32; 8] = [
    Color32::from_rgb(0xff, 0x4d, 0x5e),
    Color32::from_rgb(0xff, 0x9a, 0x3d),
    Color32::from_rgb(0xf5, 0xd9, 0x4a),
    Color32::from_rgb(0x9b, 0xe1, 0x5d),
    Color32::from_rgb(0x2e, 0xd3, 0xb7),
    Color32::from_rgb(0x5a, 0x7d, 0xff),
    Color32::from_rgb(0xa6, 0x5c, 0xff),
    Color32::from_rgb(0xff, 0x6f, 0xc0),
];

// ---------------------------------------------------------------- quiet
// The decks' quieter skin (F · Quiet): flatter faces, fainter light.
pub const QUIET_PANEL_HI: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0c);
pub const QUIET_PANEL_LO: Color32 = QUIET_PANEL_HI;
pub const QUIET_KEY_HI: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x1e);
pub const QUIET_KEY_LO: Color32 = Color32::from_rgb(0x0d, 0x0e, 0x0f);
pub const QUIET_HOVER_HI: Color32 = Color32::from_rgb(0x23, 0x24, 0x28);
pub const QUIET_HOVER_LO: Color32 = Color32::from_rgb(0x14, 0x15, 0x18);
pub const QUIET_KEY_DISABLED: Color32 = WELL;
pub const QUIET_SHEEN: Color32 = Color32::TRANSPARENT;
pub const QUIET_LIP: Color32 = Color32::TRANSPARENT;
/// The hairline between the cells of the deck's header tray.
pub const QUIET_HAIRLINE: Color32 = rgba(0xff, 0xff, 0xff, 0x0a);
/// Full-height grid lines on the deck's zoomed waveform.
pub const QUIET_BEAT_LINE: Color32 = rgba(0xff, 0xff, 0xff, 0x0d);
pub const QUIET_DOWNBEAT_LINE: Color32 = rgba(0xff, 0xff, 0xff, 0x29);
/// The 1px black seams between the deck's waveform lanes.
pub const QUIET_SEAM: Color32 = Color32::BLACK;
pub const QUIET_SIZE_LED: f32 = 5.0;

// ---------------------------------------------------------------- shape
pub const RADIUS_CONTROL: u8 = 1;
pub const RADIUS_PANEL: u8 = 1;
pub const RADIUS_WINDOW: u8 = 2;

pub const SPACE_2: f32 = 2.0;
pub const SPACE_4: f32 = 4.0;
pub const SPACE_6: f32 = 6.0;
pub const SPACE_8: f32 = 8.0;
pub const SPACE_12: f32 = 12.0;
pub const SPACE_16: f32 = 16.0;
/// Between control groups; part of the scale, unused by the current layout.
#[allow(dead_code)]
pub const SPACE_24: f32 = 24.0;

pub const SIZE_KEY: f32 = 30.0;
pub const SIZE_KEY_SMALL: f32 = 22.0;
pub const SIZE_PAD: f32 = 40.0;
/// The system's transport key width; the quiet decks size theirs wider.
#[allow(dead_code)]
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
/// Barlow Condensed (SIL OFL, `BarlowCondensed-OFL.txt`) for labels and
/// captions, and a copy with its tabular figures frozen in for numbers.
const BARLOW_SEMIBOLD: &str = "BarlowCondensed-SemiBold";
const BARLOW_TABULAR: &str = "BarlowCondensedTabular-SemiBold";

static SEMIBOLD: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("semibold".into()));
static BOLD: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("bold".into()));
static NUMERALS: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("numerals".into()));

/// egui has no font weights, only families of font files: one per weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    /// Inter 400: reading text.
    Regular,
    /// Barlow Condensed 600: titles, labels and captions.
    SemiBold,
    /// Inter 700. The hard-edge look sets every label in 600 instead.
    #[allow(dead_code)]
    Bold,
    /// Barlow Condensed 600 with tabular figures frozen in: every live
    /// number.
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
        ctx.fonts_mut(|f| f.layout_job(job))
    }

    /// Width of `text` without the trailing letter-space egui adds after
    /// the last character, so spaced labels centre truly.
    pub fn width(&self, ctx: &egui::Context, text: &str) -> f32 {
        self.galley(ctx, text, TEXT).size().x - self.spacing
    }
}

pub const NUMERALS_XL: TypeStyle = TypeStyle::new(28.0, Weight::Numerals, 0.0);
pub const NUMERALS_MD: TypeStyle = TypeStyle::new(17.0, Weight::Numerals, 0.2);
pub const NUMERALS_SM: TypeStyle = TypeStyle::new(13.0, Weight::Numerals, 0.2);
pub const KEY_LABEL: TypeStyle = TypeStyle::new(12.0, Weight::SemiBold, 1.2);
pub const CAPTION: TypeStyle = TypeStyle::new(10.5, Weight::SemiBold, 1.6);
pub const PAD_NUMBER: TypeStyle = TypeStyle::new(11.0, Weight::SemiBold, 0.4);
pub const WORDMARK: TypeStyle = TypeStyle::new(17.0, Weight::SemiBold, 6.0);
pub const TRACK_TITLE: TypeStyle = TypeStyle::new(18.0, Weight::SemiBold, 0.3);
pub const BODY_TEXT: TypeStyle = TypeStyle::new(13.0, Weight::Regular, 0.0);
pub const NOTE_TEXT: TypeStyle = TypeStyle::new(11.0, Weight::Regular, 0.0);
pub const MONO_TEXT: TypeStyle = TypeStyle::new(12.0, Weight::Mono, 0.0);
/// Numbers drawn on the waveforms (bar numbers, hot cue numbers):
/// `numerals-sm` at the performance surface's 10px floor.
pub const WAVE_NUMBER: TypeStyle = NUMERALS_SM.sized(10.0);
/// The quiet skin's key labels and captions: 600 weight, tighter spacing.
pub const QUIET_KEY_LABEL: TypeStyle = TypeStyle::new(12.0, Weight::SemiBold, 1.1);
pub const QUIET_CAPTION: TypeStyle = TypeStyle::new(10.5, Weight::SemiBold, 1.4);
/// The deck header's time and BPM.
pub const QUIET_NUMERALS: TypeStyle = TypeStyle::new(26.0, Weight::Numerals, 0.2);
/// A text toggle's label (A.CUE, Q).
pub const TEXT_TOGGLE: TypeStyle = TypeStyle::new(13.0, Weight::SemiBold, 1.2);
/// The footer's task message and help text: size and line height, kept
/// here to tune.
pub const STATUS_MESSAGE_SIZE: f32 = 14.5;
pub const STATUS_MESSAGE_LINE: f32 = 17.5;
pub const STATUS_HELP_SIZE: f32 = 11.5;
pub const STATUS_HELP_LINE: f32 = 16.0;
/// `status-message`: the footer's background task, Barlow Condensed 600.
pub const STATUS_MESSAGE: TypeStyle = TypeStyle::new(STATUS_MESSAGE_SIZE, Weight::SemiBold, 0.2);
/// `status-help`: the footer's help for the control under the pointer.
pub const STATUS_HELP: TypeStyle = TypeStyle::new(STATUS_HELP_SIZE, Weight::Regular, 0.0);

/// Paint one line of `text` in `style` anchored like `Painter::text`,
/// keeping the style's letter-spacing (which `Painter::text` drops).
pub fn paint_text(
    painter: &Painter,
    pos: Pos2,
    anchor: egui::Align2,
    text: impl ToString,
    style: TypeStyle,
    color: Color32,
) -> Rect {
    let galley = style.galley(painter.ctx(), &text.to_string(), color);
    let size = egui::vec2(galley.size().x - style.spacing, galley.size().y);
    let rect = anchor.anchor_size(pos, size);
    painter.galley(rect.min, galley, color);
    rect
}

/// One line of `text` in `style`, cut short with "…" at `width`.
pub fn truncated(
    ctx: &egui::Context,
    text: &str,
    style: TypeStyle,
    color: Color32,
    width: f32,
) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.to_owned(), style.format(color));
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(0.0));
    job.break_on_newline = false;
    ctx.fonts_mut(|f| f.layout_job(job))
}

/// Register Inter (three weights plus the tabular copy) and JetBrains Mono,
/// each ahead of egui's icon and monochrome emoji fallbacks so Halo's
/// Unicode icons still draw in one colour. Anything none of these cover
/// (other scripts, say) comes from the system's fonts.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // egui's stock text faces are replaced outright; only the icon and
    // emoji fallbacks at the end of its chains stay.
    const STOCK_TEXT: [&str; 2] = ["Hack", "Ubuntu-Light"];
    for name in STOCK_TEXT {
        fonts.font_data.remove(name);
    }
    let fallbacks: Vec<String> = fonts
        .families
        .get(&FontFamily::Proportional)
        .into_iter()
        .flatten()
        .filter(|name| !STOCK_TEXT.contains(&name.as_str()))
        .cloned()
        .collect();
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
        (
            BARLOW_SEMIBOLD,
            &include_bytes!("../assets/fonts/BarlowCondensed-SemiBold.ttf")[..],
        ),
        (
            BARLOW_TABULAR,
            &include_bytes!("../assets/fonts/BarlowCondensedTabular-SemiBold.ttf")[..],
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
            .map(|s| (*s).to_owned())
            .chain(fallbacks.iter().cloned())
            .collect()
    };
    fonts.families.clear();
    fonts
        .families
        .insert(FontFamily::Proportional, chain(&[INTER_REGULAR]));
    fonts
        .families
        .insert(SEMIBOLD.clone(), chain(&[BARLOW_SEMIBOLD, INTER_SEMIBOLD]));
    fonts.families.insert(BOLD.clone(), chain(&[INTER_BOLD]));
    fonts
        .families
        .insert(NUMERALS.clone(), chain(&[BARLOW_TABULAR, INTER_TABULAR]));
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
    QuietPanel,
    QuietKey,
    QuietKeyHover,
    /// A [`crate::key::latch`] that is on.
    Latch,
}

impl Face {
    const ALL: [Face; 12] = [
        Face::Panel,
        Face::Key,
        Face::KeyHover,
        Face::KeyDown,
        Face::Accent,
        Face::Play,
        Face::Audition,
        Face::Cap,
        Face::QuietPanel,
        Face::QuietKey,
        Face::QuietKeyHover,
        Face::Latch,
    ];

    /// Gradient stops, top to bottom, as (offset 0..1, colour).
    fn stops(self) -> &'static [(f32, Color32)] {
        // face-key-hover and the pushed-in face (darker at the top) have no
        // role names of their own.
        const HOVER_HI: Color32 = Color32::from_rgb(0x24, 0x25, 0x28);
        const HOVER_LO: Color32 = Color32::from_rgb(0x17, 0x18, 0x1a);
        const DOWN_HI: Color32 = Color32::from_rgb(0x12, 0x13, 0x15);
        match self {
            Face::Panel => &[(0.0, PANEL), (1.0, PANEL_DEEP)],
            Face::Key => &[(0.0, KEY_HI), (0.45, KEY), (1.0, KEY_LO)],
            Face::KeyHover => &[(0.0, HOVER_HI), (1.0, HOVER_LO)],
            Face::KeyDown => &[(0.0, DOWN_HI), (1.0, KEY_DOWN)],
            Face::Accent => &[(0.0, ACCENT), (1.0, ACCENT_LO)],
            Face::Play => &[(0.0, PLAY_HI), (0.46, PLAY), (1.0, PLAY_LO)],
            Face::Audition => &[(0.0, AUDITION_HI), (0.46, AUDITION), (1.0, AUDITION_LO)],
            Face::Cap => &[(0.0, CAP_HI), (0.55, CAP), (1.0, CAP_LO)],
            Face::QuietPanel => &[(0.0, QUIET_PANEL_HI), (1.0, QUIET_PANEL_LO)],
            Face::QuietKey => &[(0.0, QUIET_KEY_HI), (1.0, QUIET_KEY_LO)],
            Face::QuietKeyHover => &[(0.0, QUIET_HOVER_HI), (1.0, QUIET_HOVER_LO)],
            Face::Latch => &[(0.0, LATCH), (1.0, LATCH_LO)],
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
    egui::Id::unique("halo-theme-faces")
}

/// Build each `face-*` gradient once as a 1×32 texture.
fn install_faces(ctx: &egui::Context) {
    let handles = Face::ALL
        .iter()
        .map(|face| {
            let pixels = (0..FACE_TEXELS)
                .map(|i| sample_stops(face.stops(), (i as f32 + 0.5) / FACE_TEXELS as f32))
                .collect();
            let image = egui::ColorImage::new([1, FACE_TEXELS], pixels);
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

/// The surfaces a key or panel is drawn with: the system's own, or the
/// decks' quieter skin.
#[derive(Clone, Copy, Debug)]
pub struct Skin {
    pub panel: Face,
    pub key: Face,
    pub key_hover: Face,
    /// A disabled key's flat face.
    pub key_disabled: Color32,
    pub sheen: Color32,
    pub lip: Color32,
    pub led: f32,
    pub key_label: TypeStyle,
}

pub const SKIN_SYSTEM: Skin = Skin {
    panel: Face::Panel,
    key: Face::Key,
    key_hover: Face::KeyHover,
    key_disabled: KEY_LO,
    sheen: SHEEN,
    lip: LIP,
    led: SIZE_LED,
    key_label: KEY_LABEL,
};

pub const SKIN_QUIET: Skin = Skin {
    panel: Face::QuietPanel,
    key: Face::QuietKey,
    key_hover: Face::QuietKeyHover,
    key_disabled: QUIET_KEY_DISABLED,
    sheen: QUIET_SHEEN,
    lip: QUIET_LIP,
    led: QUIET_SIZE_LED,
    key_label: QUIET_KEY_LABEL,
};

/// Fill `rect` with a gradient face; `tint` multiplies it (white = as is).
pub fn paint_face(
    painter: &Painter,
    rect: Rect,
    corners: impl Into<CornerRadius>,
    face: Face,
    tint: Color32,
) {
    let tex = face_texture(painter.ctx(), face);
    painter.add(RectShape::filled(rect, corners, tint).with_texture(tex, face_uv()));
}

/// A 1px line along the top inside edge of a rounded rect, kept clear of
/// its top corners.
fn top_line(painter: &Painter, rect: Rect, corners: CornerRadius, y_offset: f32, color: Color32) {
    painter.hline(
        (rect.left() + corners.nw as f32 * 0.6)..=(rect.right() - corners.ne as f32 * 0.6),
        rect.top() + y_offset + 0.5,
        Stroke::new(1.0, color),
    );
}

/// Inner shade along the top of a pushed-in or recessed rect: the
/// `inset 0 1px 3px` of `shade`, fading over three rows.
fn top_shade(painter: &Painter, rect: Rect, corners: CornerRadius) {
    for (row, k) in [(0.0, 1.0), (1.0, 0.45), (2.0, 0.15)] {
        top_line(painter, rect, corners, row, SHADE.gamma_multiply(k));
    }
}

/// The 1px dark edge outside a shape.
pub fn paint_edge(painter: &Painter, rect: Rect, corners: impl Into<CornerRadius>, color: Color32) {
    painter.rect_stroke(rect, corners, Stroke::new(1.0, color), StrokeKind::Outside);
}

/// What lines the top of a key's face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Top {
    /// A raised key, lined in its skin's `sheen`.
    Sheen(Color32),
    /// The pushed-in `shade`: a held or chosen key.
    Shade,
}

/// A key's face and its top line, without the edge: the part each key of
/// a joined strip (SegmentedButton) keeps, the strip drawing one edge
/// round them all.
pub fn paint_key_face(
    painter: &Painter,
    rect: Rect,
    corners: impl Into<CornerRadius>,
    face: Face,
    tint: Color32,
    top: Top,
) {
    let corners = corners.into();
    paint_face(painter, rect, corners, face, tint);
    match top {
        Top::Sheen(sheen) => top_line(painter, rect, corners, 0.0, sheen),
        Top::Shade => top_shade(painter, rect, corners),
    }
}

/// A recessed well: flat `well`, shade inside the top, a 1px `lip` of
/// light under the bottom (`shadow-well`).
pub fn paint_well(painter: &Painter, rect: Rect, radius: u8) {
    paint_content_well_radius(painter, rect, radius, WELL);
}

/// A flat fill with a [`WELL_LINE`] hairline inside its edge.
fn paint_content_well_radius(painter: &Painter, rect: Rect, radius: u8, ground: Color32) {
    painter.add(RectShape::new(
        rect,
        radius,
        ground,
        Stroke::new(1.0, WELL_LINE),
        StrokeKind::Inside,
    ));
}

/// The hairline round a well.
const WELL_LINE: Color32 = rgba(0xff, 0xff, 0xff, 0x0c);

/// A content well (waveform, show lanes): the content's own ground with
/// the well's shade and lip.
pub fn paint_content_well(painter: &Painter, rect: Rect, ground: Color32) {
    paint_content_well_radius(painter, rect, RADIUS_CONTROL, ground);
}

/// A well with a 1px `edge` (meters, fader slots, LED bezels).
pub fn paint_well_edged(painter: &Painter, rect: Rect, radius: u8) {
    painter.rect_filled(rect, radius, WELL);
    paint_edge(painter, rect, radius, EDGE);
}

/// Keyboard focus: a 1px `accent` ring just outside a control, clear of a
/// lit key's outline inside it.
pub fn paint_focus(painter: &Painter, rect: Rect) {
    painter.rect_stroke(
        rect.expand(FOCUS_GAP),
        RADIUS_CONTROL,
        Stroke::new(1.0, ACCENT),
        StrokeKind::Outside,
    );
}

/// Between a control and its focus ring.
const FOCUS_GAP: f32 = 2.0;

/// A soft glow behind a light source (lit LEDs, the lit beat step, the
/// master-source letter). Never on keys, text or panels.
pub fn paint_glow(painter: &Painter, rect: Rect, radius: f32, blur: f32, color: Color32) {
    painter.add(RectShape::filled(rect, CornerRadius::from(radius), color).with_blur_width(blur));
}

/// A 7px LED: lit in `color` with `shadow-led`, or a dark `led-off` lens.
pub fn paint_led(painter: &Painter, center: Pos2, on: bool, color: Color32) {
    paint_led_sized(painter, center, on, color, SIZE_LED);
}

/// [`paint_led`] at another lens size (the quiet skin's 5px LEDs).
pub fn paint_led_sized(painter: &Painter, center: Pos2, on: bool, color: Color32, size: f32) {
    let r = size / 2.0;
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    if on {
        let glow = if color == ACCENT {
            ACCENT_GLOW
        } else {
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 0x8c)
        };
        paint_glow(painter, rect, r, size, glow);
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

// ---------------------------------------------------------------- meters

/// Size of an LED step bar (the master output meter).
pub const STEPS_SIZE: Vec2 = Vec2::new(64.0, 10.0);

/// Fifteen 3×6 LED steps in an edged well, lit up to `level` (0..1) and
/// zoned by position: `meter-green` to 70%, `accent` to 95%, `meter-red`
/// above.
pub fn paint_steps(painter: &Painter, rect: Rect, level: f32) {
    const STEPS: usize = 15;
    let level = level.clamp(0.0, 1.0);
    paint_well_edged(painter, rect.shrink(0.5), 2);
    for i in 0..STEPS {
        let t = (i as f32 + 0.5) / STEPS as f32;
        let color = if t > level {
            SEGMENT_OFF
        } else if t < 0.7 {
            METER_GREEN
        } else if t < 0.95 {
            ACCENT
        } else {
            METER_RED
        };
        let step = Rect::from_min_size(
            rect.min + egui::vec2(2.0 + i as f32 * 4.0, 2.0),
            egui::vec2(3.0, 6.0),
        );
        painter.rect_filled(step, 0.0, color);
    }
}

/// A deck readout: a well with a spaced caption over `numerals-xl`, 9 and
/// 12 inside. `cells` sizes the value column in digit widths so nothing
/// moves while it plays.
pub fn paint_readout(ui: &mut egui::Ui, caption: &str, value: &str, color: Color32, cells: usize) {
    let ctx = ui.ctx().clone();
    let digit = NUMERALS_XL.width(&ctx, "0");
    let size = egui::vec2(cells as f32 * digit + 2.0 * SPACE_12, READOUT_H);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    paint_well(painter, rect, RADIUS_CONTROL);
    let cap = CAPTION.galley(&ctx, caption, TEXT_MUTED);
    let cap_h = cap.size().y;
    painter.galley(rect.min + egui::vec2(SPACE_12, 9.0), cap, TEXT_MUTED);
    let val = NUMERALS_XL.galley(&ctx, value, color);
    // Numerals sit on a 30px line under the caption, 7 below it.
    let line_top = rect.top() + 9.0 + cap_h + 7.0;
    let y = line_top + (SIZE_NUMERALS - val.size().y) / 2.0;
    painter.galley(egui::pos2(rect.left() + SPACE_12, y), val, color);
}

/// Height of a deck readout well.
pub const READOUT_H: f32 = 67.0;

// ---------------------------------------------------------------- panels

/// A panel's surface in `rect`: `face-panel` with `shadow-panel`. Its soft
/// drop (`0 22px 44px -30px black`) needs a negative spread, which a frame
/// `Shadow` can't express, so it is a blurred rect instead.
pub fn panel_shapes(ctx: &egui::Context, rect: Rect) -> Vec<egui::Shape> {
    panel_shapes_with(ctx, rect, Face::Panel)
}

/// [`panel_shapes`] with another panel face (the quiet skin's).
pub fn panel_shapes_with(ctx: &egui::Context, rect: Rect, face: Face) -> Vec<egui::Shape> {
    let mut shapes = Vec::new();
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
            .with_texture(face_texture(ctx, face), face_uv())
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

/// Lay `content` out on a panel (`face-panel` with `shadow-panel`), `margin`
/// inside. The surface is painted behind the content once its size is known.
pub fn panel<R>(
    ui: &mut egui::Ui,
    margin: impl Into<egui::Margin>,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    panel_with(ui, margin, Face::Panel, content)
}

/// [`panel`] on another panel face (the quiet skin's).
pub fn panel_with<R>(
    ui: &mut egui::Ui,
    margin: impl Into<egui::Margin>,
    face: Face,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let background = ui.painter().add(egui::Shape::Noop);
    let inner = egui::Frame::new().inner_margin(margin).show(ui, content);
    let shapes = panel_shapes_with(ui.ctx(), inner.response.rect, face);
    ui.painter().set(background, egui::Shape::Vec(shapes));
    inner
}

/// Lay `content` out in a well (`well` + `shadow-well`), `margin` inside:
/// the library table, the playlist tree, programmer trays.
pub fn well<R>(
    ui: &mut egui::Ui,
    margin: impl Into<egui::Margin>,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let background = ui.painter().add(egui::Shape::Noop);
    let inner = egui::Frame::new().inner_margin(margin).show(ui, content);
    let rect = inner.response.rect;
    ui.painter().set(
        background,
        RectShape::new(
            rect,
            RADIUS_CONTROL,
            WELL,
            Stroke::new(1.0, WELL_LINE),
            StrokeKind::Inside,
        ),
    );
    inner
}

/// Padding inside a [`tray`].
pub const TRAY_PAD: f32 = 3.0;
/// A [`tray_divider`]'s line, and the room before and after it.
pub const TRAY_DIVIDER: Color32 = rgba(0xff, 0xff, 0xff, 0x1f);
const TRAY_DIVIDER_BEFORE: f32 = 3.0;
const TRAY_DIVIDER_AFTER: f32 = 4.0;
/// The room a [`tray_divider`] takes across.
pub const TRAY_DIVIDER_W: f32 = TRAY_DIVIDER_BEFORE + 1.0 + TRAY_DIVIDER_AFTER;

/// A row of controls sunk in a [`well`], [`TRAY_PAD`] inside (the decks'
/// hot cue and key rows). Laid out left to right with no spacing of its
/// own: callers add their gaps, and [`tray_divider`] between unrelated
/// groups. The tray is as wide as its row, so a right-to-left group at the
/// end stretches it across the available width.
pub fn tray<R>(
    ui: &mut egui::Ui,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    well(ui, TRAY_PAD as i8, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            content(ui)
        })
        .inner
    })
}

/// A 1px light line between groups in a [`tray`], 3 before it and 4
/// after (mirrored in a right-to-left group). It runs the height of the
/// row laid out so far, so it goes after the group it follows. (The dark
/// `groove` would vanish against the well.)
pub fn tray_divider(ui: &mut egui::Ui) {
    let height = ui.min_rect().height();
    let (before, after) = if ui.layout().prefer_right_to_left() {
        (TRAY_DIVIDER_AFTER, TRAY_DIVIDER_BEFORE)
    } else {
        (TRAY_DIVIDER_BEFORE, TRAY_DIVIDER_AFTER)
    };
    ui.add_space(before);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, height), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().vline(
            rect.left() + 0.5,
            rect.y_range(),
            Stroke::new(1.0, TRAY_DIVIDER),
        );
    }
    ui.add_space(after);
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
