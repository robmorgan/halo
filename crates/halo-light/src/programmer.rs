//! The lighting programmer: a live manual-override layer that sits above
//! the track-cue layer, console-style. When a lane's override is active
//! (latched ON or a held FLASH), the programmer owns that lane's output;
//! CLEAR releases every latch and the track cues take back over.
//!
//! [`resolve`] is the single source of truth for "what is the rig doing
//! right now" — every indicator (toolbar LEDs, lane-strip tints, hollow
//! cue bars) must derive from its output so provenance stays consistent.

use crate::cues::{ALL_LANES, CueSet, LANE_COUNT};

/// One lane's manual override state.
#[derive(Clone)]
pub struct LaneOverride {
    /// Latched ON until CLEAR.
    pub latched: bool,
    /// Momentary: active only while the FLASH button/key is held.
    pub flash_held: bool,
    /// Level the programmer drives the lane at while active.
    pub intensity: f32,
}

impl Default for LaneOverride {
    fn default() -> Self {
        Self {
            latched: false,
            flash_held: false,
            intensity: 1.0,
        }
    }
}

impl LaneOverride {
    pub fn active(&self) -> bool {
        self.latched || self.flash_held
    }
}

pub type Programmer = [LaneOverride; LANE_COUNT];

/// Where a lane's current output comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneSource {
    /// Manual override — highest priority.
    Programmer,
    /// A cue from the active lighting deck's track.
    Track,
    Off,
}

#[derive(Debug, Clone, Copy)]
pub struct LaneOutput {
    pub source: LaneSource,
    pub level: f32,
}

/// Resolve the lighting output priority stack for one instant:
/// Programmer > track cues at `playhead` > off.
pub fn resolve(
    prog: &Programmer,
    track_cues: Option<&CueSet>,
    playhead: f64,
) -> [LaneOutput; LANE_COUNT] {
    ALL_LANES.map(|lane| {
        let o = &prog[lane as usize];
        if o.active() {
            return LaneOutput {
                source: LaneSource::Programmer,
                level: o.intensity.clamp(0.0, 1.0),
            };
        }
        if let Some(cue) = track_cues.and_then(|c| c.active_at(lane, playhead)) {
            return LaneOutput {
                source: LaneSource::Track,
                level: cue.intensity.clamp(0.0, 1.0),
            };
        }
        LaneOutput {
            source: LaneSource::Off,
            level: 0.0,
        }
    })
}

/// Release every latch (held FLASH keys release themselves on key-up).
pub fn clear(prog: &mut Programmer) {
    for o in prog.iter_mut() {
        o.latched = false;
    }
}

/// Whether any lane is latched (drives the CLEAR button's armed styling).
pub fn any_latched(prog: &Programmer) -> bool {
    prog.iter().any(|o| o.latched)
}

// ---------------------------------------------------------------------------
// Parameter views + per-parameter effects.
//
// MOCKUP STATE for now: values and effect configs are real, interactive
// state, but nothing drives fixture output until the fixture engine lands.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParamView {
    #[default]
    Intensity,
    Color,
    Position,
    Beam,
    PixelFx,
}

pub const ALL_VIEWS: [(ParamView, &str); 5] = [
    (ParamView::Intensity, "INTENSITY"),
    (ParamView::Color, "COLOR"),
    (ParamView::Position, "POSITION"),
    (ParamView::Beam, "BEAM"),
    (ParamView::PixelFx, "PIXEL FX"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Square,
    Sawtooth,
    Triangle,
}

pub const ALL_WAVEFORMS: [(Waveform, &str); 4] = [
    (Waveform::Sine, "SINE"),
    (Waveform::Square, "SQR"),
    (Waveform::Sawtooth, "SAW"),
    (Waveform::Triangle, "TRI"),
];

/// Musical span one effect cycle rides on (at ratio 1.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interval {
    Beat,
    Bar,
    Phrase,
}

pub const ALL_INTERVALS: [(Interval, &str); 3] = [
    (Interval::Beat, "BEAT"),
    (Interval::Bar, "BAR"),
    (Interval::Phrase, "PHRASE"),
];

impl Interval {
    /// Length in beats (4/4; 16-bar phrases, matching the deck grids).
    pub fn beats(self) -> f64 {
        match self {
            Self::Beat => 1.0,
            Self::Bar => 4.0,
            Self::Phrase => 64.0,
        }
    }
}

/// How the effect spreads across the selected fixtures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Distribution {
    /// Every fixture in phase.
    All,
    /// Fixtures grouped into n steps.
    Step(u32),
    /// Phase offset (degrees) per fixture, a travelling wave.
    Wave(u32),
}

#[derive(Debug, Clone)]
pub struct EffectConfig {
    pub waveform: Waveform,
    pub interval: Interval,
    /// Cycles per interval, 0.00..=2.00.
    pub ratio: f32,
    /// Phase offset in degrees, 0..=360.
    pub phase_deg: f32,
    pub distribution: Distribution,
    /// Latched by the APPLY button.
    pub applied: bool,
}

impl Default for EffectConfig {
    fn default() -> Self {
        Self {
            waveform: Waveform::Sine,
            interval: Interval::Beat,
            ratio: 1.0,
            phase_deg: 0.0,
            distribution: Distribution::All,
            applied: false,
        }
    }
}

/// Normalized effect value in 0..=1 at musical time `t`, measured in
/// intervals (so `t` advances by 1.0 per beat/bar/phrase as configured).
pub fn effect_value(cfg: &EffectConfig, t: f64) -> f32 {
    let cycles = t * cfg.ratio as f64 + cfg.phase_deg as f64 / 360.0;
    let phase = (cycles.rem_euclid(1.0)) as f32;
    match cfg.waveform {
        Waveform::Sine => 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos(),
        Waveform::Square => {
            if phase < 0.5 {
                1.0
            } else {
                0.0
            }
        }
        Waveform::Sawtooth => phase,
        Waveform::Triangle => {
            if phase < 0.5 {
                phase * 2.0
            } else {
                2.0 - phase * 2.0
            }
        }
    }
}

#[derive(Clone)]
pub struct IntensityParams {
    /// Percent, 0..=100.
    pub dimmer: f32,
    pub strobe: f32,
    pub effect: EffectConfig,
}

impl Default for IntensityParams {
    fn default() -> Self {
        Self {
            dimmer: 100.0,
            strobe: 0.0,
            effect: EffectConfig::default(),
        }
    }
}

#[derive(Clone)]
pub struct ColorParams {
    /// R/G/B/W percent, 0..=100.
    pub rgbw: [f32; 4],
    pub effect: EffectConfig,
}

impl Default for ColorParams {
    fn default() -> Self {
        Self {
            rgbw: [100.0, 100.0, 100.0, 0.0],
            effect: EffectConfig::default(),
        }
    }
}

/// Which axes a position effect drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanTiltTarget {
    #[default]
    Both,
    Pan,
    Tilt,
}

#[derive(Clone)]
pub struct PositionParams {
    /// Degrees, 0..=360.
    pub pan: f32,
    pub tilt: f32,
    pub target: PanTiltTarget,
    pub effect: EffectConfig,
}

impl Default for PositionParams {
    fn default() -> Self {
        Self {
            pan: 180.0,
            tilt: 180.0,
            target: PanTiltTarget::default(),
            effect: EffectConfig::default(),
        }
    }
}

#[derive(Clone)]
pub struct BeamParams {
    /// Selected gobo, 1..=8.
    pub gobo: u8,
    pub effect: EffectConfig,
}

impl Default for BeamParams {
    fn default() -> Self {
        Self {
            gobo: 1,
            effect: EffectConfig::default(),
        }
    }
}

/// Placeholder pixel-bar effects until the fixture engine defines real ones.
pub const PIXEL_EFFECTS: [&str; 8] = [
    "Chase L-R",
    "Bounce",
    "Rainbow",
    "Sparkle",
    "VU Meter",
    "Breathe",
    "Strobe All",
    "Theater",
];

/// Placeholder color presets `(name, [r, g, b, w])` in percent, shared by
/// the Color view's preset buttons and the Pixel FX color picker.
pub const COLOR_PRESETS: [(&str, [f32; 4]); 10] = [
    ("White", [0.0, 0.0, 0.0, 100.0]),
    ("Red", [100.0, 0.0, 0.0, 0.0]),
    ("Orange", [100.0, 40.0, 0.0, 0.0]),
    ("Yellow", [100.0, 85.0, 0.0, 0.0]),
    ("Green", [0.0, 100.0, 0.0, 0.0]),
    ("Cyan", [0.0, 90.0, 100.0, 0.0]),
    ("Blue", [0.0, 0.0, 100.0, 0.0]),
    ("Magenta", [100.0, 0.0, 100.0, 0.0]),
    ("Pink", [100.0, 25.0, 55.0, 10.0]),
    ("UV", [45.0, 0.0, 100.0, 0.0]),
];

#[derive(Clone, Default)]
pub struct PixelFxParams {
    /// Index into [`PIXEL_EFFECTS`].
    pub effect: usize,
    /// Index into [`COLOR_PRESETS`].
    pub color: usize,
}

/// All parameter-view state for the programmer surface.
#[derive(Clone, Default)]
pub struct ProgrammerParams {
    pub view: ParamView,
    pub intensity: IntensityParams,
    pub color: ColorParams,
    pub position: PositionParams,
    pub beam: BeamParams,
    pub pixel: PixelFxParams,
    /// Blind mode: view programmer values without sending them to the rig.
    pub preview: bool,
    /// Snap the selected fixtures to full white for identification.
    pub highlight: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cues::Lane;

    #[test]
    fn programmer_beats_track_beats_off() {
        let mut cues = CueSet::empty();
        cues.insert(Lane::Lighting, 0.0, 100.0, 0.7);

        let mut prog = Programmer::default();
        // Track cue wins while the programmer is idle.
        let out = resolve(&prog, Some(&cues), 50.0);
        assert_eq!(out[0].source, LaneSource::Track);
        assert!((out[0].level - 0.7).abs() < 1e-6);
        // Off past the cue.
        assert_eq!(
            resolve(&prog, Some(&cues), 200.0)[0].source,
            LaneSource::Off
        );
        // Latching takes the lane over at the programmer's intensity.
        prog[0].latched = true;
        prog[0].intensity = 0.4;
        let out = resolve(&prog, Some(&cues), 50.0);
        assert_eq!(out[0].source, LaneSource::Programmer);
        assert!((out[0].level - 0.4).abs() < 1e-6);
        // Other lanes are untouched.
        assert_eq!(out[1].source, LaneSource::Off);
        // CLEAR hands back to the track.
        clear(&mut prog);
        assert_eq!(
            resolve(&prog, Some(&cues), 50.0)[0].source,
            LaneSource::Track
        );
    }

    #[test]
    fn effect_value_waveform_shapes() {
        let mut cfg = EffectConfig::default(); // sine, ratio 1, phase 0
        assert!((effect_value(&cfg, 0.0) - 0.0).abs() < 1e-6);
        assert!((effect_value(&cfg, 0.25) - 0.5).abs() < 1e-6);
        assert!((effect_value(&cfg, 0.5) - 1.0).abs() < 1e-6);

        cfg.waveform = Waveform::Square;
        assert_eq!(effect_value(&cfg, 0.1), 1.0);
        assert_eq!(effect_value(&cfg, 0.6), 0.0);

        cfg.waveform = Waveform::Sawtooth;
        assert!((effect_value(&cfg, 0.75) - 0.75).abs() < 1e-6);

        cfg.waveform = Waveform::Triangle;
        assert!((effect_value(&cfg, 0.25) - 0.5).abs() < 1e-6);
        assert!((effect_value(&cfg, 0.5) - 1.0).abs() < 1e-6);
        assert!((effect_value(&cfg, 0.75) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn effect_value_ratio_and_phase() {
        let mut cfg = EffectConfig {
            waveform: Waveform::Sawtooth,
            ..Default::default()
        };
        // Ratio 0: frozen at the phase offset.
        cfg.ratio = 0.0;
        cfg.phase_deg = 90.0;
        assert!((effect_value(&cfg, 0.0) - 0.25).abs() < 1e-6);
        assert!((effect_value(&cfg, 5.3) - 0.25).abs() < 1e-6);
        // Ratio 2: two cycles per interval.
        cfg.ratio = 2.0;
        cfg.phase_deg = 0.0;
        assert!((effect_value(&cfg, 0.25) - 0.5).abs() < 1e-6);
        // Always normalized.
        for wf in ALL_WAVEFORMS.map(|(w, _)| w) {
            cfg.waveform = wf;
            for i in 0..40 {
                let v = effect_value(&cfg, i as f64 * 0.173 - 3.0);
                assert!((0.0..=1.0).contains(&v), "{wf:?} out of range: {v}");
            }
        }
    }

    #[test]
    fn flash_is_momentary_and_survives_clear() {
        let mut prog = Programmer::default();
        prog[2].flash_held = true;
        assert_eq!(resolve(&prog, None, 0.0)[2].source, LaneSource::Programmer);
        // CLEAR only drops latches; a held flash stays until key-up.
        clear(&mut prog);
        assert_eq!(resolve(&prog, None, 0.0)[2].source, LaneSource::Programmer);
        prog[2].flash_held = false;
        assert_eq!(resolve(&prog, None, 0.0)[2].source, LaneSource::Off);
    }
}
