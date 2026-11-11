//! The lighting programmer: a live manual-override layer that sits above
//! the track-cue layer, console-style. When a lane's override is active
//! (latched ON or a held FLASH), the programmer owns that lane's output;
//! CLEAR releases every latch and the track cues take back over.
//!
//! [`resolve`] is the single source of truth for "what is the rig doing
//! right now" — every indicator (toolbar LEDs, lane-strip tints, hollow
//! cue bars) must derive from its output so provenance stays consistent.
//!
//! The programmer also holds intensity and colour per fixture, console
//! style: setting them takes the fixtures selected at the time (the whole
//! rig when none are) away from their lanes until CLEAR. Picking a pixel
//! effect or colour brings the bars it applies to up the same way.
//!
//! Under the programmer and the track's cues sits the deck's active look
//! (see [`crate::looks`]): it lights the Lighting and Pixels lanes and
//! supplies every parameter section the programmer hasn't touched
//! ([`effective`]).
//!
//! Energy — the track's NRG curve times the live ENERGY fader — then
//! scales everything's intensity, like a grand master: lanes whatever
//! their source, and what the programmer holds. HIGHLIGHT ignores it.

use std::collections::{BTreeMap, HashSet};

use crate::cues::{ALL_LANES, CueSet, LANE_COUNT, Lane};
use crate::energy::EnergyLane;
use crate::fixture::{Fixture, FixtureKind, Rig};
use crate::looks::LookParams;

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
    /// The deck's active look.
    Look,
    Off,
}

#[derive(Debug, Clone, Copy)]
pub struct LaneOutput {
    pub source: LaneSource,
    pub level: f32,
}

/// The rig's state for one instant: each lane's output, already scaled
/// by `energy`, the energy itself for what the programmer holds, and the
/// active look for what the programmer hasn't touched.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub lanes: [LaneOutput; LANE_COUNT],
    /// 0..=1.
    pub energy: f32,
    pub look: Option<LookParams>,
}

/// The energy at `playhead`: the track's curve (flat without one) times
/// the live fader.
pub fn energy_at(curve: Option<&EnergyLane>, playhead: f64, fader: f32) -> f32 {
    let curve = curve.map_or(1.0, |c| c.value_at(playhead));
    (curve * fader).clamp(0.0, 1.0)
}

/// Resolve the lighting output priority stack for one instant:
/// Programmer > track cues at `playhead` > the active `look` > off, each
/// scaled by `energy` (see [`energy_at`]). The look lights the Lighting
/// and Pixels lanes at its dimmer, never FX: a look can't fire smoke or
/// pyro. Scaling keeps the source: a dimmed lane is still its own.
pub fn resolve(
    prog: &Programmer,
    track_cues: Option<&CueSet>,
    look: Option<&LookParams>,
    energy: f32,
    playhead: f64,
) -> Resolved {
    let energy = energy.clamp(0.0, 1.0);
    let lanes = ALL_LANES.map(|lane| {
        let o = &prog[lane as usize];
        let (source, level) = if o.active() {
            (LaneSource::Programmer, o.intensity)
        } else if let Some(cue) = track_cues.and_then(|c| c.active_at(lane, playhead)) {
            (LaneSource::Track, cue.intensity)
        } else if let Some(look) = look.filter(|_| lane != Lane::Fx) {
            (LaneSource::Look, look.intensity.dimmer / 100.0)
        } else {
            (LaneSource::Off, 0.0)
        };
        LaneOutput {
            source,
            level: level.clamp(0.0, 1.0) * energy,
        }
    });
    Resolved {
        lanes,
        energy,
        look: look.cloned(),
    }
}

/// One fixture's intensity (0..=1) and colour (RGBW percent), merged per
/// fixture as [`resolve`] merges per lane: what the programmer holds for
/// it beats its lane's level, and the look's colour or else open white.
/// A held dimmer is scaled by the energy, as the lanes are.
pub fn resolve_fixture(
    params: &ProgrammerParams,
    resolved: &Resolved,
    fixture: &Fixture,
) -> (f32, [f32; 4]) {
    let held = params.held.get(&fixture.id).copied().unwrap_or_default();
    let level = held
        .dimmer
        .map_or(resolved.lanes[fixture.kind.lane() as usize].level, |d| {
            (d / 100.0).clamp(0.0, 1.0) * resolved.energy
        })
        .clamp(0.0, 1.0);
    let rgbw = held.rgbw.unwrap_or_else(|| {
        resolved
            .look
            .as_ref()
            .map_or(ColorParams::default().rgbw, |l| l.color.rgbw)
    });
    (level, rgbw)
}

/// The parameter sections the rig renders: each the look's, when a look is
/// active and the programmer hasn't touched that section, or else the
/// programmer's. `*_from_look` says which, for the effect's cohort.
pub struct Effective<'a> {
    pub intensity: &'a IntensityParams,
    pub color: &'a ColorParams,
    pub position: &'a PositionParams,
    pub beam: &'a BeamParams,
    pub pixel: &'a PixelFxParams,
    pub intensity_from_look: bool,
    pub color_from_look: bool,
    pub position_from_look: bool,
    pub beam_from_look: bool,
}

pub fn effective<'a>(params: &'a ProgrammerParams, resolved: &'a Resolved) -> Effective<'a> {
    let look = resolved.look.as_ref();
    let owned = params.owned;
    let from = |owns: bool| look.filter(|_| !owns);
    let pick = |owns: bool| from(owns).is_some();
    Effective {
        intensity: from(owned.intensity).map_or(&params.intensity, |l| &l.intensity),
        color: from(owned.color).map_or(&params.color, |l| &l.color),
        position: from(owned.position).map_or(&params.position, |l| &l.position),
        beam: from(owned.beam).map_or(&params.beam, |l| &l.beam),
        pixel: from(owned.pixel).map_or(&params.pixel, |l| &l.pixel),
        intensity_from_look: pick(owned.intensity),
        color_from_look: pick(owned.color),
        position_from_look: pick(owned.position),
        beam_from_look: pick(owned.beam),
    }
}

/// The parameter sections the programmer has taken from the look, until
/// CLEAR. A fixture's dimmer and colour are held per fixture instead
/// (see [`Held`]), so they don't count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Owned {
    pub intensity: bool,
    pub color: bool,
    pub position: bool,
    pub beam: bool,
    pub pixel: bool,
}

/// The sections [`Owned::mark`] compares.
#[derive(Clone, PartialEq)]
pub struct Sections {
    strobe: f32,
    intensity_fx: EffectConfig,
    color_fx: EffectConfig,
    position: PositionParams,
    beam: BeamParams,
    pixel: PixelFxParams,
}

impl Sections {
    pub fn of(p: &ProgrammerParams) -> Self {
        Self {
            strobe: p.intensity.strobe,
            intensity_fx: p.intensity.effect.clone(),
            color_fx: p.color.effect.clone(),
            position: p.position.clone(),
            beam: p.beam.clone(),
            pixel: p.pixel.clone(),
        }
    }
}

impl Owned {
    /// Take every section that changed between two snapshots.
    pub fn mark(&mut self, before: &Sections, after: &Sections) {
        self.intensity |=
            before.strobe != after.strobe || before.intensity_fx != after.intensity_fx;
        self.color |= before.color_fx != after.color_fx;
        self.position |= before.position != after.position;
        self.beam |= before.beam != after.beam;
        self.pixel |= before.pixel != after.pixel;
    }

    pub fn any(&self) -> bool {
        *self != Self::default()
    }
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
// What `render()` reads, from the programmer or from the deck's look
// (see `effective`).
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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BeamParams {
    /// Selected gobo, 1..=8; 0 is open.
    pub gobo: u8,
    pub effect: EffectConfig,
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PixelFxParams {
    /// Index into [`PIXEL_EFFECTS`].
    pub effect: usize,
    /// Index into [`COLOR_PRESETS`].
    pub color: usize,
}

/// All parameter-view state for the programmer surface.
#[derive(Clone)]
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
    /// What the programmer holds per fixture id, until CLEAR.
    pub held: BTreeMap<u32, Held>,
    /// The live ENERGY fader, 0..=1, on top of the track's NRG curve.
    /// CLEAR leaves it.
    pub energy: f32,
    /// The sections taken from the look, until CLEAR.
    pub owned: Owned,
}

impl Default for ProgrammerParams {
    fn default() -> Self {
        Self {
            view: ParamView::default(),
            intensity: IntensityParams::default(),
            color: ColorParams::default(),
            position: PositionParams::default(),
            beam: BeamParams::default(),
            pixel: PixelFxParams::default(),
            preview: false,
            highlight: false,
            held: BTreeMap::new(),
            energy: 1.0,
            owned: Owned::default(),
        }
    }
}

/// What the programmer holds for one fixture. `None` leaves it to the
/// fixture's lane (intensity) or the rig default (colour).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Held {
    /// Percent, 0..=100.
    pub dimmer: Option<f32>,
    /// R/G/B/W percent, 0..=100.
    pub rgbw: Option<[f32; 4]>,
}

/// The fixtures a programmer edit applies to: the selection, or the whole
/// rig when nothing is selected.
pub fn targets(rig: &Rig, selection: &HashSet<u32>) -> Vec<u32> {
    rig.iter()
        .map(|f| f.id)
        .filter(|id| selection.is_empty() || selection.contains(id))
        .collect()
}

/// The pixel bars a PIXELS edit applies to: the selected ones, or every
/// bar when none is selected.
pub fn pixel_targets(rig: &Rig, selection: &HashSet<u32>) -> Vec<u32> {
    let bars = || {
        rig.iter()
            .filter(|f| f.kind == FixtureKind::PixelBar)
            .map(|f| f.id)
    };
    let selected: Vec<u32> = bars().filter(|id| selection.contains(id)).collect();
    if selected.is_empty() {
        bars().collect()
    } else {
        selected
    }
}

impl ProgrammerParams {
    /// Bring `targets` up to full, leaving any held dimmer as it is. The
    /// DIMMER fader's last value doesn't change.
    pub fn bring_up(&mut self, targets: &[u32]) {
        for &id in targets {
            self.held.entry(id).or_default().dimmer.get_or_insert(100.0);
        }
    }

    /// Hold `dimmer` on `targets`; the fader keeps it as its last value.
    pub fn hold_dimmer(&mut self, targets: &[u32], dimmer: f32) {
        self.intensity.dimmer = dimmer;
        for &id in targets {
            self.held.entry(id).or_default().dimmer = Some(dimmer);
        }
    }

    /// Hold `rgbw` on `targets`; the faders keep it as their last value.
    pub fn hold_rgbw(&mut self, targets: &[u32], rgbw: [f32; 4]) {
        self.color.rgbw = rgbw;
        for &id in targets {
            self.held.entry(id).or_default().rgbw = Some(rgbw);
        }
    }

    /// The first held value in `selection` (lowest id), or the fader's
    /// last value.
    fn shown<T>(&self, selection: &HashSet<u32>, value: impl Fn(&Held) -> Option<T>) -> Option<T> {
        self.held
            .iter()
            .filter(|(id, _)| selection.contains(id))
            .find_map(|(_, held)| value(held))
    }

    /// What DIMMER shows for `selection`.
    pub fn shown_dimmer(&self, selection: &HashSet<u32>) -> f32 {
        self.shown(selection, |h| h.dimmer)
            .unwrap_or(self.intensity.dimmer)
    }

    /// What the colour faders show for `selection`.
    pub fn shown_rgbw(&self, selection: &HashSet<u32>) -> [f32; 4] {
        self.shown(selection, |h| h.rgbw).unwrap_or(self.color.rgbw)
    }

    /// Let go of every fixture and section: CLEAR.
    pub fn release(&mut self) {
        self.held.clear();
        self.owned = Owned::default();
    }

    /// Whether CLEAR has anything to release: a held fixture or a section
    /// taken from the look.
    pub fn holds_any(&self) -> bool {
        !self.held.is_empty() || self.owned.any()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cues::Lane;

    #[test]
    fn energy_scales_every_lane_and_keeps_its_source() {
        let mut cues = CueSet::empty();
        cues.insert(Lane::Lighting, 0.0, 100.0, 0.8);
        let mut prog = Programmer::default();
        prog[1].latched = true;
        let out = resolve(&prog, Some(&cues), None, 0.5, 50.0);
        assert_eq!(out.energy, 0.5);
        assert_eq!(out.lanes[0].source, LaneSource::Track);
        assert!((out.lanes[0].level - 0.4).abs() < 1e-6);
        assert_eq!(out.lanes[1].source, LaneSource::Programmer);
        assert!((out.lanes[1].level - 0.5).abs() < 1e-6);
        assert_eq!(out.lanes[2].level, 0.0);
    }

    #[test]
    fn the_look_lights_lanes_under_the_programmer_and_the_track() {
        use crate::looks::{LookId, builtin};
        let look = &builtin(LookId(0)).params; // Warm Open, dimmer 80
        let mut cues = CueSet::empty();
        cues.insert(Lane::Lighting, 0.0, 100.0, 0.3);
        let mut prog = Programmer::default();
        prog[1].latched = true;
        let out = resolve(&prog, Some(&cues), Some(look), 0.5, 50.0);
        // The track's cue and the programmer's latch beat the look.
        assert_eq!(out.lanes[0].source, LaneSource::Track);
        assert_eq!(out.lanes[1].source, LaneSource::Programmer);
        // A look never drives FX.
        assert_eq!(out.lanes[2].source, LaneSource::Off);
        // Past the cue, the look lights Lighting at its dimmer × energy.
        let out = resolve(&Programmer::default(), Some(&cues), Some(look), 0.5, 200.0);
        assert_eq!(out.lanes[0].source, LaneSource::Look);
        assert!((out.lanes[0].level - 0.4).abs() < 1e-6);
        assert_eq!(out.look.as_ref(), Some(look));
    }

    #[test]
    fn untouched_sections_come_from_the_look() {
        use crate::looks::{LookId, builtin};
        let look = builtin(LookId(2)).params.clone(); // Red Drop
        let mut p = ProgrammerParams::default();
        // No look: the programmer's sections, as before looks.
        let none = resolve(&Programmer::default(), None, None, 1.0, 0.0);
        let e = effective(&p, &none);
        assert_eq!(e.beam, &p.beam);
        assert!(!e.position_from_look);

        let with = resolve(&Programmer::default(), None, Some(&look), 1.0, 0.0);
        let e = effective(&p, &with);
        assert_eq!(e.beam.gobo, 3);
        assert_eq!(e.position, &look.position);
        assert!(e.position_from_look);

        // Touch position and DIMMER: position becomes the programmer's,
        // intensity stays the look's.
        let before = Sections::of(&p);
        p.position.pan = 90.0;
        p.hold_dimmer(&[1], 50.0);
        p.owned.mark(&before, &Sections::of(&p));
        assert_eq!(
            p.owned,
            Owned {
                position: true,
                ..Owned::default()
            }
        );
        assert!(p.holds_any());
        let e = effective(&p, &with);
        assert_eq!(e.position.pan, 90.0);
        assert!(!e.position_from_look);
        assert_eq!(e.intensity, &look.intensity);
        // CLEAR hands everything back.
        p.release();
        assert!(!p.holds_any());
        assert!(effective(&p, &with).position_from_look);
    }

    #[test]
    fn energy_is_the_curve_at_the_playhead_times_the_fader() {
        // No track, or a track with no curve: flat, so only the fader.
        assert_eq!(energy_at(None, 0.0, 1.0), 1.0);
        assert_eq!(energy_at(Some(&EnergyLane::default()), 9.0, 0.25), 0.25);
        let mut curve = EnergyLane::default();
        curve.insert(100.0, 0.2);
        curve.insert(300.0, 0.8);
        assert!((energy_at(Some(&curve), 200.0, 1.0) - 0.5).abs() < 1e-6);
        assert!((energy_at(Some(&curve), 200.0, 0.5) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn programmer_beats_track_beats_off() {
        let mut cues = CueSet::empty();
        cues.insert(Lane::Lighting, 0.0, 100.0, 0.7);

        let mut prog = Programmer::default();
        // Track cue wins while the programmer is idle.
        let out = resolve(&prog, Some(&cues), None, 1.0, 50.0).lanes;
        assert_eq!(out[0].source, LaneSource::Track);
        assert!((out[0].level - 0.7).abs() < 1e-6);
        // Off past the cue.
        assert_eq!(
            resolve(&prog, Some(&cues), None, 1.0, 200.0).lanes[0].source,
            LaneSource::Off
        );
        // Latching takes the lane over at the programmer's intensity.
        prog[0].latched = true;
        prog[0].intensity = 0.4;
        let out = resolve(&prog, Some(&cues), None, 1.0, 50.0).lanes;
        assert_eq!(out[0].source, LaneSource::Programmer);
        assert!((out[0].level - 0.4).abs() < 1e-6);
        // Other lanes are untouched.
        assert_eq!(out[1].source, LaneSource::Off);
        // CLEAR hands back to the track.
        clear(&mut prog);
        assert_eq!(
            resolve(&prog, Some(&cues), None, 1.0, 50.0).lanes[0].source,
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
        assert_eq!(
            resolve(&prog, None, None, 1.0, 0.0).lanes[2].source,
            LaneSource::Programmer
        );
        // CLEAR only drops latches; a held flash stays until key-up.
        clear(&mut prog);
        assert_eq!(
            resolve(&prog, None, None, 1.0, 0.0).lanes[2].source,
            LaneSource::Programmer
        );
        prog[2].flash_held = false;
        assert_eq!(
            resolve(&prog, None, None, 1.0, 0.0).lanes[2].source,
            LaneSource::Off
        );
    }
}
