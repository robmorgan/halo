//! Per-fixture DMX output: flattens [`resolve`](crate::programmer::resolve)'d
//! lane levels + programmer parameters into per-universe channel frames.
//!
//! [`render`] is pure — (rig, params, lane outputs, beat time) in, frames
//! out — so the DMX engine thread and any future preview visualizer get
//! identical results. Frames explicitly zero dark fixtures: a rendered
//! universe always ships all 512 channels, so "off" is sent, not held.
//!
//! Value semantics:
//! - A fixture's intensity is what the programmer holds for it, or else its lane's resolved level
//!   (which the deck's active look lights). Fixtures without a dimmer channel fold intensity into
//!   their color channels.
//! - Its colour is what the programmer holds for it, or else the look's, or else open white. A
//!   colour wheel turns to the slot nearest that colour.
//! - Position, gobo, strobe and the pixel effect apply rig-wide, each from the look unless the
//!   programmer has taken that section. The programmer's *effects* run across the fixture selection
//!   (or the whole rig when nothing is selected), a look's across the whole rig, with Step/Wave
//!   distribution by ordinal in that cohort.
//! - HIGHLIGHT snaps the selection to open white. PREVIEW (blind) renders as if the programmer
//!   params were untouched defaults.

use std::collections::{HashMap, HashSet};

use crate::fixture::{FixtureKind, Rig};
use crate::fixture_library::{ChannelType, FixtureLibrary, FixtureMode, FixtureProfile};
use crate::programmer::{
    COLOR_PRESETS, Distribution, EffectConfig, Effective, PanTiltTarget, PixelFxParams,
    ProgrammerParams, Resolved, effect_value, effective, resolve_fixture,
};

pub const UNIVERSE_CHANNELS: usize = 512;
pub type UniverseFrame = [u8; UNIVERSE_CHANNELS];

fn u8_of(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Phase offset (in cycles) for the fixture at ordinal `i` of a cohort.
fn dist_offset(d: Distribution, i: usize) -> f64 {
    match d {
        Distribution::All => 0.0,
        Distribution::Step(s) if s > 0 => f64::from(i as u32 % s) / f64::from(s),
        Distribution::Step(_) => 0.0,
        Distribution::Wave(deg) => i as f64 * f64::from(deg) / 360.0,
    }
}

/// Evaluate an applied effect for cohort ordinal `i` at musical time
/// `beat_t` (in beats). Returns None when the effect isn't applied or the
/// fixture isn't in the cohort.
fn effect_at(cfg: &EffectConfig, beat_t: f64, ord: Option<usize>) -> Option<f32> {
    if !cfg.applied {
        return None;
    }
    let i = ord?;
    let t = beat_t / cfg.interval.beats() - dist_offset(cfg.distribution, i);
    Some(effect_value(cfg, t))
}

/// Render one instant of the rig into per-universe DMX frames.
///
/// `beat_t` is musical time in beats (fractional), the same clock the
/// programmer's effect previews run on.
pub fn render(
    rig: &Rig,
    library: &FixtureLibrary,
    resolved: &Resolved,
    params: &ProgrammerParams,
    selection: &HashSet<u32>,
    beat_t: f64,
) -> HashMap<u8, UniverseFrame> {
    // PREVIEW (blind): programmer values stay in the editor. Highlight
    // remains live — it's an identification tool, not a look.
    let defaults;
    let p = if params.preview {
        defaults = ProgrammerParams::default();
        &defaults
    } else {
        params
    };

    // Effect cohorts: the programmer's effects run across the selection,
    // or everything when nothing is selected; a look's across the whole
    // rig, so selecting fixtures to edit them doesn't shrink the show.
    // Ordinals follow rig order.
    let cohort: Vec<u32> = rig
        .iter()
        .filter(|f| selection.is_empty() || selection.contains(&f.id))
        .map(|f| f.id)
        .collect();
    let sections = effective(p, resolved);

    let mut universes: HashMap<u8, UniverseFrame> = HashMap::new();
    for (i, f) in rig.iter().enumerate() {
        let Some((profile, mode)) = library.patched(f) else {
            continue;
        };
        let footprint = mode.footprint();
        let base = (f.start_address.max(1) - 1) as usize;
        if base + footprint > UNIVERSE_CHANNELS {
            continue;
        }
        let frame = universes
            .entry(f.universe)
            .or_insert([0u8; UNIVERSE_CHANNELS]);
        let slot = &mut frame[base..base + footprint];

        if params.highlight && selection.contains(&f.id) {
            render_highlight(mode, slot);
            continue;
        }

        // PREVIEW's defaults hold nothing.
        let (level, rgbw) = resolve_fixture(p, resolved, f);
        let ords = Ords {
            selection: cohort.iter().position(|&id| id == f.id),
            rig: Some(i),
        };
        if f.kind == FixtureKind::PixelBar {
            render_pixel_bar(mode, slot, sections.pixel, level, beat_t);
        } else {
            let state = State { level, rgbw };
            render_conventional(profile, mode, slot, &sections, state, beat_t, ords);
        }
    }
    universes
}

/// A fixture's ordinal in each effect cohort; `None` when it's outside it.
#[derive(Clone, Copy)]
struct Ords {
    selection: Option<usize>,
    rig: Option<usize>,
}

impl Ords {
    /// The ordinal for an effect from the look or from the programmer.
    fn of(self, from_look: bool) -> Option<usize> {
        if from_look { self.rig } else { self.selection }
    }
}

/// Open white at full for fixture identification.
fn render_highlight(mode: &FixtureMode, slot: &mut [u8]) {
    for (i, ch) in mode.channel_layout.iter().enumerate() {
        slot[i] = match ch.channel_type {
            ChannelType::Dimmer
            | ChannelType::Red
            | ChannelType::Green
            | ChannelType::Blue
            | ChannelType::White
            | ChannelType::PixelRed(_)
            | ChannelType::PixelGreen(_)
            | ChannelType::PixelBlue(_) => 255,
            _ => 0,
        };
    }
}

/// One fixture's intensity (0..=1) and colour (RGBW percent).
struct State {
    level: f32,
    rgbw: [f32; 4],
}

fn render_conventional(
    profile: &FixtureProfile,
    mode: &FixtureMode,
    slot: &mut [u8],
    p: &Effective<'_>,
    state: State,
    beat_t: f64,
    ords: Ords,
) {
    let level = state.level;
    let mut dim = level;
    if let Some(fx) = effect_at(&p.intensity.effect, beat_t, ords.of(p.intensity_from_look)) {
        dim *= fx;
    }

    let mut rgbw = state.rgbw.map(|v| v / 100.0);
    if let Some(fx) = effect_at(&p.color.effect, beat_t, ords.of(p.color_from_look)) {
        for c in rgbw.iter_mut() {
            *c *= fx;
        }
    }

    let pos_fx = effect_at(&p.position.effect, beat_t, ords.of(p.position_from_look));
    // 16-bit: the coarse channel takes the high byte, a fine channel
    // the low one.
    let axis = |deg: f32, driven: bool| -> u16 {
        let base = deg / 360.0;
        let at = match pos_fx {
            // ±1/8 of travel swing around the base position.
            Some(fx) if driven => base + (fx - 0.5) * 0.25,
            _ => base,
        };
        (at.clamp(0.0, 1.0) * f32::from(u16::MAX)).round() as u16
    };
    let pan_driven = matches!(p.position.target, PanTiltTarget::Both | PanTiltTarget::Pan);
    let tilt_driven = matches!(p.position.target, PanTiltTarget::Both | PanTiltTarget::Tilt);

    // Fixtures without a dimmer channel carry intensity in their color
    // channels instead.
    let color_scale = if mode.channel_offset(&ChannelType::Dimmer).is_some() {
        1.0
    } else {
        dim
    };

    let [r, g, b, w] = rgbw;
    let shade = [(r + w).min(1.0), (g + w).min(1.0), (b + w).min(1.0)];
    for (i, ch) in mode.channel_layout.iter().enumerate() {
        slot[i] = match &ch.channel_type {
            ChannelType::Dimmer => u8_of(dim),
            ChannelType::Red => u8_of(rgbw[0] * color_scale),
            ChannelType::Green => u8_of(rgbw[1] * color_scale),
            ChannelType::Blue => u8_of(rgbw[2] * color_scale),
            ChannelType::White => u8_of(rgbw[3] * color_scale),
            ChannelType::Strobe => u8_of(p.intensity.strobe / 100.0),
            ChannelType::Pan => (axis(p.position.pan, pan_driven) >> 8) as u8,
            ChannelType::PanFine => axis(p.position.pan, pan_driven) as u8,
            ChannelType::Tilt => (axis(p.position.tilt, tilt_driven) >> 8) as u8,
            ChannelType::TiltFine => axis(p.position.tilt, tilt_driven) as u8,
            ChannelType::Color => wheel_value(profile, shade),
            ChannelType::Gobo => gobo_value(profile, p.beam.gobo),
            ChannelType::Other(name) => match name.as_str() {
                // The FX lane level drives smoke output directly.
                "Smoke" => u8_of(dim),
                // Igniter convention: Safety arms with any level, Fire
                // needs the lane driven hard (a deliberate cue at ≥ 0.9).
                // The level is after energy, so low energy can't fire.
                "Safety" if level > 0.0 => 255,
                "Fire" if level >= 0.9 => 255,
                _ => 0,
            },
            _ => 0,
        };
    }
}

/// The colour wheel slot nearest `rgb`, or 0 (no colour) without a wheel.
fn wheel_value(profile: &FixtureProfile, rgb: [f32; 3]) -> u8 {
    let distance =
        |slot: [f32; 3]| -> f32 { slot.iter().zip(rgb).map(|(a, b)| (a - b) * (a - b)).sum() };
    profile
        .color_wheel
        .iter()
        .min_by(|a, b| distance(a.rgb).total_cmp(&distance(b.rgb)))
        .map_or(0, |slot| slot.dmx)
}

/// The programmer's GOBO `n` (1-based; 0 = open): the profile's value for
/// it, or 8 slots spread across the channel.
fn gobo_value(profile: &FixtureProfile, gobo: u8) -> u8 {
    match gobo.checked_sub(1) {
        Some(i) if !profile.gobo_wheel.is_empty() => profile
            .gobo_wheel
            .get(usize::from(i))
            .or(profile.gobo_wheel.last())
            .copied()
            .unwrap_or(0),
        _ => gobo.saturating_sub(1).min(7) * 32,
    }
}

/// Fold a `[r, g, b, w]` percent preset into RGB in 0..=1.
fn preset_rgb(preset: [f32; 4]) -> [f32; 3] {
    let w = preset[3] / 100.0;
    [
        (preset[0] / 100.0 + w).min(1.0),
        (preset[1] / 100.0 + w).min(1.0),
        (preset[2] / 100.0 + w).min(1.0),
    ]
}

/// h in 0..=1 → RGB at full saturation/value (rainbow effect).
fn hue_rgb(h: f32) -> [f32; 3] {
    let h6 = h.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h6.rem_euclid(2.0) - 1.0).abs();
    match h6 as u32 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    }
}

fn render_pixel_bar(
    mode: &FixtureMode,
    slot: &mut [u8],
    pixel: &PixelFxParams,
    level: f32,
    beat_t: f64,
) {
    let n = mode.footprint() / 3;
    if n == 0 || level <= 0.0 {
        return; // slot is already zeroed
    }
    let base_rgb = preset_rgb(COLOR_PRESETS[pixel.color.min(COLOR_PRESETS.len() - 1)].1);
    let frac = |x: f64| x.rem_euclid(1.0);

    for i in 0..n {
        let fi = i as f32;
        let nf = n as f32;
        // (intensity, optional color override) per pixel.
        let (v, over): (f32, Option<[f32; 3]>) = match pixel.effect {
            // Chase L-R: a window sweeping once per beat.
            0 => {
                let pos = frac(beat_t) as f32 * nf;
                let w = (nf / 8.0).max(1.0);
                ((1.0 - (fi - pos).abs() / w).max(0.0), None)
            }
            // Bounce: the window ping-pongs over a 2-beat cycle.
            1 => {
                let ph = frac(beat_t / 2.0) as f32;
                let tri = if ph < 0.5 { ph * 2.0 } else { 2.0 - ph * 2.0 };
                let pos = tri * (nf - 1.0);
                let w = (nf / 8.0).max(1.0);
                ((1.0 - (fi - pos).abs() / w).max(0.0), None)
            }
            // Rainbow: hue wheel across the bar, drifting one revolution
            // every 4 beats.
            2 => (1.0, Some(hue_rgb(fi / nf - frac(beat_t / 4.0) as f32))),
            // Sparkle: ~1 in 4 pixels re-rolled every quarter beat.
            3 => {
                let tick = (beat_t * 4.0).floor() as u64;
                let h = (i as u64)
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add(tick.wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
                (if (h >> 33) & 3 == 0 { 1.0 } else { 0.0 }, None)
            }
            // VU meter: lane level fills the bar from the left.
            4 => (if fi < level * nf { 1.0 } else { 0.0 }, None),
            // Breathe: everything swells over 2 beats.
            5 => {
                let ph = frac(beat_t / 2.0) as f32;
                (0.5 - 0.5 * (ph * std::f32::consts::TAU).cos(), None)
            }
            // Strobe all: a hard flash on each beat.
            6 => (if frac(beat_t) < 0.15 { 1.0 } else { 0.0 }, None),
            // Theater: odd/even pixels alternate every half beat.
            _ => {
                let step = (beat_t * 2.0).floor() as usize;
                (if i % 2 == step % 2 { 1.0 } else { 0.0 }, None)
            }
        };

        let rgb = over.unwrap_or(base_rgb);
        let scale = v * level;
        slot[i * 3] = u8_of(rgb[0] * scale);
        slot[i * 3 + 1] = u8_of(rgb[1] * scale);
        slot[i * 3 + 2] = u8_of(rgb[2] * scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::default_rig;
    use crate::programmer::{LaneOutput, LaneSource, Waveform};

    fn lanes(lighting: f32, pixels: f32, fx: f32) -> Resolved {
        Resolved {
            lanes: [lighting, pixels, fx].map(|level| LaneOutput {
                source: if level > 0.0 {
                    LaneSource::Track
                } else {
                    LaneSource::Off
                },
                level,
            }),
            energy: 1.0,
            look: None,
        }
    }

    fn setup() -> (Rig, FixtureLibrary) {
        let library = FixtureLibrary::new();
        (default_rig(&library), library)
    }

    /// Channel value for a fixture found by label, by channel type.
    fn value_of(
        rig: &Rig,
        library: &FixtureLibrary,
        frames: &HashMap<u8, UniverseFrame>,
        label: &str,
        ch: &ChannelType,
    ) -> u8 {
        let f = rig.iter().find(|f| f.label == label).expect("fixture");
        let off = library
            .layout(f)
            .unwrap()
            .channel_offset(ch)
            .expect("channel");
        frames[&f.universe][(f.start_address - 1) as usize + off]
    }

    #[test]
    fn dark_rig_closes_every_dimmer() {
        let (rig, library) = setup();
        let frames = render(
            &rig,
            &library,
            &lanes(0.0, 0.0, 0.0),
            &ProgrammerParams::default(),
            &HashSet::new(),
            0.0,
        );
        // All patched universes ship frames (1 conventional + 4 pixel).
        assert_eq!(frames.len(), 5);
        // Console-style dark: dimmer channels closed (palette may sit on
        // color channels behind them); fixtures without a dimmer — smoke,
        // pyro, pixels — are all-zero.
        for f in rig.iter() {
            let mode = library.layout(f).unwrap();
            let base = (f.start_address - 1) as usize;
            let slot = &frames[&f.universe][base..base + mode.footprint()];
            match mode.channel_offset(&ChannelType::Dimmer) {
                Some(off) => assert_eq!(slot[off], 0, "{}: dimmer open", f.label),
                None => assert!(
                    slot.iter().all(|&v| v == 0),
                    "{}: dimmerless fixture not dark",
                    f.label
                ),
            }
        }
    }

    #[test]
    fn lighting_lane_drives_par_dimmer_and_default_white() {
        let (rig, library) = setup();
        let frames = render(
            &rig,
            &library,
            &lanes(0.8, 0.0, 0.0),
            &ProgrammerParams::default(),
            &HashSet::new(),
            0.0,
        );
        assert_eq!(
            value_of(&rig, &library, &frames, "P1", &ChannelType::Dimmer),
            u8_of(0.8)
        );
        // Default palette is RGB full, W off.
        assert_eq!(
            value_of(&rig, &library, &frames, "P1", &ChannelType::Red),
            255
        );
        assert_eq!(
            value_of(&rig, &library, &frames, "P1", &ChannelType::White),
            0
        );
    }

    #[test]
    fn fx_lane_drives_smoke_and_gates_pyro() {
        let (rig, library) = setup();
        let smoke_ch = ChannelType::Other("Smoke".to_string());
        let fire_ch = ChannelType::Other("Fire".to_string());
        let safety_ch = ChannelType::Other("Safety".to_string());

        let half = render(
            &rig,
            &library,
            &lanes(0.0, 0.0, 0.5),
            &ProgrammerParams::default(),
            &HashSet::new(),
            0.0,
        );
        assert_eq!(
            value_of(&rig, &library, &half, "SM1", &smoke_ch),
            u8_of(0.5)
        );
        assert_eq!(value_of(&rig, &library, &half, "PY1", &safety_ch), 255);
        assert_eq!(
            value_of(&rig, &library, &half, "PY1", &fire_ch),
            0,
            "half level must not fire pyro"
        );

        let full = render(
            &rig,
            &library,
            &lanes(0.0, 0.0, 1.0),
            &ProgrammerParams::default(),
            &HashSet::new(),
            0.0,
        );
        assert_eq!(value_of(&rig, &library, &full, "PY1", &fire_ch), 255);
    }

    #[test]
    fn highlight_snaps_selection_to_open_white() {
        let (rig, library) = setup();
        let params = ProgrammerParams {
            highlight: true,
            ..Default::default()
        };
        let p1 = rig.iter().find(|f| f.label == "P1").unwrap().id;
        let selection = HashSet::from([p1]);
        let frames = render(
            &rig,
            &library,
            &lanes(0.0, 0.0, 0.0),
            &params,
            &selection,
            0.0,
        );
        assert_eq!(
            value_of(&rig, &library, &frames, "P1", &ChannelType::Dimmer),
            255
        );
        assert_eq!(
            value_of(&rig, &library, &frames, "P1", &ChannelType::White),
            255
        );
        // Unselected neighbor stays dark.
        assert_eq!(
            value_of(&rig, &library, &frames, "P2", &ChannelType::Dimmer),
            0
        );
    }

    #[test]
    fn preview_blinds_programmer_values() {
        let (rig, library) = setup();
        let mut params = ProgrammerParams::default();
        params.intensity.dimmer = 0.0; // would black out the rig...
        params.preview = true; // ...but blind keeps it in the editor
        let frames = render(
            &rig,
            &library,
            &lanes(1.0, 0.0, 0.0),
            &params,
            &HashSet::new(),
            0.0,
        );
        assert_eq!(
            value_of(&rig, &library, &frames, "P1", &ChannelType::Dimmer),
            255
        );
    }

    #[test]
    fn step_distribution_splits_the_cohort() {
        let (rig, library) = setup();
        let mut params = ProgrammerParams::default();
        params.intensity.effect.applied = true;
        params.intensity.effect.waveform = Waveform::Square;
        params.intensity.effect.distribution = Distribution::Step(2);
        let ids: Vec<u32> = rig
            .iter()
            .filter(|f| f.label == "P1" || f.label == "P2")
            .map(|f| f.id)
            .collect();
        let selection: HashSet<u32> = ids.into_iter().collect();
        let frames = render(
            &rig,
            &library,
            &lanes(1.0, 0.0, 0.0),
            &params,
            &selection,
            0.25,
        );
        let a = value_of(&rig, &library, &frames, "P1", &ChannelType::Dimmer);
        let b = value_of(&rig, &library, &frames, "P2", &ChannelType::Dimmer);
        assert_eq!(
            (a, b),
            (255, 0),
            "square wave at Step(2) puts the halves in antiphase"
        );
        // Fixtures outside the selection are untouched by the effect.
        assert_eq!(
            value_of(&rig, &library, &frames, "P3", &ChannelType::Dimmer),
            255
        );
    }

    #[test]
    fn pixel_bar_scales_with_lane_and_chases() {
        let (rig, library) = setup();
        let params = ProgrammerParams::default();
        let dark = render(
            &rig,
            &library,
            &lanes(0.0, 0.0, 0.0),
            &params,
            &HashSet::new(),
            0.0,
        );
        let pb1 = rig.iter().find(|f| f.label == "PB1").unwrap();
        let frame = &dark[&pb1.universe];
        assert!(frame.iter().all(|&v| v == 0), "pixels dark with lane off");

        let lit = render(
            &rig,
            &library,
            &lanes(0.0, 1.0, 0.0),
            &params,
            &HashSet::new(),
            0.25,
        );
        let mode = library.layout(pb1).unwrap();
        let base = (pb1.start_address - 1) as usize;
        let slot = &lit[&pb1.universe][base..base + mode.footprint()];
        let lit_pixels = slot
            .chunks(3)
            .filter(|px| px.iter().any(|&v| v > 0))
            .count();
        let n = mode.footprint() / 3;
        assert!(lit_pixels > 0, "chase lights a window");
        assert!(lit_pixels < n, "chase is a window, not the whole bar");
    }

    #[test]
    fn position_maps_degrees_and_swings_with_effect() {
        let (rig, library) = setup();
        let mut params = ProgrammerParams::default();
        params.position.pan = 0.0;
        params.position.tilt = 360.0;
        let frames = render(
            &rig,
            &library,
            &lanes(1.0, 0.0, 0.0),
            &params,
            &HashSet::new(),
            0.0,
        );
        assert_eq!(
            value_of(&rig, &library, &frames, "S1", &ChannelType::Pan),
            0
        );
        assert_eq!(
            value_of(&rig, &library, &frames, "S1", &ChannelType::Tilt),
            255
        );
    }

    use crate::fixture_library::WheelSlot;

    fn id_of(rig: &Rig, label: &str) -> u32 {
        rig.iter().find(|f| f.label == label).expect("fixture").id
    }

    fn dark(
        rig: &Rig,
        library: &FixtureLibrary,
        p: &ProgrammerParams,
    ) -> HashMap<u8, UniverseFrame> {
        render(rig, library, &lanes(0.0, 0.0, 0.0), p, &HashSet::new(), 0.0)
    }

    /// The spot profile with a small test wheel.
    fn with_wheels(mut library: FixtureLibrary) -> FixtureLibrary {
        let spot = library.profiles.get_mut("shehds-led-spot-60w").unwrap();
        let slot = |name: &str, rgb, dmx| WheelSlot {
            name: name.into(),
            rgb,
            dmx,
        };
        spot.color_wheel = vec![
            slot("White", [1.0, 1.0, 1.0], 0),
            slot("Red", [1.0, 0.0, 0.0], 20),
            slot("Blue", [0.0, 0.0, 1.0], 60),
        ];
        spot.gobo_wheel = vec![8, 16, 24];
        library
    }

    #[test]
    fn a_held_dimmer_lights_a_fixture_without_its_lane() {
        let (mut rig, library) = setup();
        let s1 = id_of(&rig, "S1");
        let mut p = ProgrammerParams::default();
        p.hold_dimmer(&[s1], 60.0);
        let frames = dark(&rig, &library, &p);
        assert_eq!(
            value_of(&rig, &library, &frames, "S1", &ChannelType::Dimmer),
            u8_of(0.6)
        );
        // The unheld spot follows its dark lane.
        assert_eq!(
            value_of(&rig, &library, &frames, "S2", &ChannelType::Dimmer),
            0
        );
        // In 11CH the dimmer is channel 8.
        for f in rig.fixtures_mut().iter_mut().filter(|f| f.id == s1) {
            f.mode = "11CH".into();
        }
        let frames = dark(&rig, &library, &p);
        let f = rig.iter().find(|f| f.id == s1).unwrap();
        assert_eq!(
            frames[&f.universe][(f.start_address - 1) as usize + 7],
            u8_of(0.6)
        );
        // PREVIEW keeps it in the editor; CLEAR lets it go.
        p.preview = true;
        let frames = dark(&rig, &library, &p);
        assert_eq!(
            value_of(&rig, &library, &frames, "S1", &ChannelType::Dimmer),
            0
        );
        p.preview = false;
        p.release();
        let frames = dark(&rig, &library, &p);
        assert_eq!(
            value_of(&rig, &library, &frames, "S1", &ChannelType::Dimmer),
            0
        );
    }

    #[test]
    fn a_held_colour_turns_the_wheel_to_its_nearest_slot() {
        let (rig, library) = setup();
        let library = with_wheels(library);
        let (s1, p1) = (id_of(&rig, "S1"), id_of(&rig, "P1"));
        let mut p = ProgrammerParams::default();
        p.hold_rgbw(&[s1], [100.0, 0.0, 0.0, 0.0]);
        let frames = dark(&rig, &library, &p);
        assert_eq!(
            value_of(&rig, &library, &frames, "S1", &ChannelType::Color),
            20
        );
        // The unheld spot sits on white; the unheld PAR stays white.
        assert_eq!(
            value_of(&rig, &library, &frames, "S2", &ChannelType::Color),
            0
        );
        assert_eq!(
            value_of(&rig, &library, &frames, "P1", &ChannelType::Green),
            255
        );
        // Blue, and a PAR held red, on the same frame.
        p.hold_rgbw(&[s1], [0.0, 0.0, 100.0, 0.0]);
        p.hold_rgbw(&[p1], [100.0, 0.0, 0.0, 0.0]);
        let frames = dark(&rig, &library, &p);
        assert_eq!(
            value_of(&rig, &library, &frames, "S1", &ChannelType::Color),
            60
        );
        assert_eq!(
            value_of(&rig, &library, &frames, "P1", &ChannelType::Green),
            0
        );
        // Without a wheel table the channel stays at 0.
        let mut library = library;
        let spot = library.profiles.get_mut("shehds-led-spot-60w").unwrap();
        spot.color_wheel.clear();
        let frames = dark(&rig, &library, &p);
        assert_eq!(
            value_of(&rig, &library, &frames, "S1", &ChannelType::Color),
            0
        );
    }

    #[test]
    fn gobos_use_the_profiles_values() {
        let (rig, library) = setup();
        let library = with_wheels(library);
        let mut p = ProgrammerParams::default();
        p.beam.gobo = 3;
        let frames = dark(&rig, &library, &p);
        assert_eq!(
            value_of(&rig, &library, &frames, "S1", &ChannelType::Gobo),
            24
        );
        // Past the wheel: its last gobo. 0 is open.
        p.beam.gobo = 8;
        let frames = dark(&rig, &library, &p);
        assert_eq!(
            value_of(&rig, &library, &frames, "S1", &ChannelType::Gobo),
            24
        );
        p.beam.gobo = 0;
        let frames = dark(&rig, &library, &p);
        assert_eq!(
            value_of(&rig, &library, &frames, "S1", &ChannelType::Gobo),
            0
        );
    }

    #[test]
    fn picking_a_pixel_effect_brings_the_bars_up_without_a_lane() {
        let (rig, library) = setup();
        let (pb1, pb2, s1) = (id_of(&rig, "PB1"), id_of(&rig, "PB2"), id_of(&rig, "S1"));
        // PB1's first pixel, on beat 1 with every lane dark.
        let red = |p: &ProgrammerParams| {
            let frames = render(
                &rig,
                &library,
                &lanes(0.0, 0.0, 0.0),
                p,
                &HashSet::new(),
                1.0,
            );
            value_of(&rig, &library, &frames, "PB1", &ChannelType::PixelRed(0))
        };
        // Breathe, at its peak on beat 1.
        let mut p = ProgrammerParams::default();
        p.pixel.effect = 5;
        assert_eq!(red(&p), 0);
        // Spots selected: no bar is, so every bar comes up.
        let bars = crate::programmer::pixel_targets(&rig, &HashSet::from([s1]));
        assert_eq!(bars.len(), rig.ids_of_kind(FixtureKind::PixelBar).count());
        p.bring_up(&bars);
        assert_eq!(red(&p), 255);
        p.release();
        assert_eq!(red(&p), 0);

        // One bar selected: only it. A held dimmer stays as it was.
        let mut p = ProgrammerParams::default();
        p.hold_dimmer(&[pb1], 40.0);
        p.bring_up(&crate::programmer::pixel_targets(
            &rig,
            &HashSet::from([pb1]),
        ));
        assert_eq!(p.held[&pb1].dimmer, Some(40.0));
        assert!(!p.held.contains_key(&pb2));
        assert_eq!(p.intensity.dimmer, 40.0);
    }

    #[test]
    fn energy_dims_everything_but_highlight() {
        use crate::programmer::{Programmer, resolve};
        let (rig, library) = setup();
        let (s1, p1) = (id_of(&rig, "S1"), id_of(&rig, "P1"));
        let mut prog = Programmer::default();
        prog[2].latched = true;
        let half = resolve(&prog, None, None, 0.5, 0.0);
        let mut p = ProgrammerParams::default();
        p.hold_dimmer(&[s1], 80.0);
        p.bring_up(&crate::programmer::pixel_targets(&rig, &HashSet::new()));
        // Breathe, at its peak on beat 1.
        p.pixel.effect = 5;
        let frames = render(&rig, &library, &half, &p, &HashSet::new(), 1.0);
        let at = |label, ch: &ChannelType| value_of(&rig, &library, &frames, label, ch);
        assert_eq!(at("S1", &ChannelType::Dimmer), u8_of(0.4));
        assert_eq!(at("PB1", &ChannelType::PixelRed(0)), u8_of(0.5));
        // The FX lane at full, halved: smoke at half, and pyro can't fire.
        let smoke = ChannelType::Other("Smoke".to_string());
        assert_eq!(at("SM1", &smoke), u8_of(0.5));
        assert_eq!(at("PY1", &ChannelType::Other("Fire".to_string())), 0);
        // HIGHLIGHT still goes out at full.
        p.highlight = true;
        let frames = render(&rig, &library, &half, &p, &HashSet::from([p1]), 1.0);
        assert_eq!(
            value_of(&rig, &library, &frames, "P1", &ChannelType::Dimmer),
            255
        );
    }

    /// The rig with only `look` running: no programmer, no cues.
    fn with_look(id: usize) -> Resolved {
        use crate::looks::{LookId, builtin};
        use crate::programmer::{Programmer, resolve};
        resolve(
            &Programmer::default(),
            None,
            Some(&builtin(LookId(id)).params),
            1.0,
            0.0,
        )
    }

    #[test]
    fn a_look_runs_the_rig_without_the_programmer() {
        let (rig, library) = setup();
        let red_drop = with_look(2);
        let p = ProgrammerParams::default();
        let at = |beat: f64, selection: &HashSet<u32>, label, ch: &ChannelType| {
            let frames = render(&rig, &library, &red_drop, &p, selection, beat);
            value_of(&rig, &library, &frames, label, ch)
        };
        let none = HashSet::new();
        // Red on a PAR and on the spots' wheel, and the look's gobo.
        assert_eq!(at(0.0, &none, "P1", &ChannelType::Red), 255);
        assert_eq!(at(0.0, &none, "P1", &ChannelType::Green), 0);
        assert_eq!(at(0.0, &none, "S1", &ChannelType::Color), 15);
        assert_eq!(at(0.0, &none, "S1", &ChannelType::Gobo), 28);
        // The heads move between beats…
        let pan = |beat, selection| at(beat, selection, "S1", &ChannelType::Pan);
        assert_ne!(pan(0.0, &none), pan(1.0, &none));
        // …even when the selection leaves them out.
        let p1 = HashSet::from([id_of(&rig, "P1")]);
        assert_ne!(pan(0.0, &p1), pan(1.0, &p1));
        // The pixel bars run the look's chase in its red.
        assert!(at(0.0, &none, "PB1", &ChannelType::PixelRed(0)) > 0);
        assert_eq!(at(0.0, &none, "PB1", &ChannelType::PixelGreen(0)), 0);
        // Smoke and pyro stay off.
        assert_eq!(
            at(0.0, &none, "SM1", &ChannelType::Other("Smoke".into())),
            0
        );
        assert_eq!(
            at(0.0, &none, "PY1", &ChannelType::Other("Safety".into())),
            0
        );
    }

    #[test]
    fn the_programmer_beats_the_look_where_it_touches() {
        use crate::programmer::Sections;
        let (rig, library) = setup();
        let deep_blue = with_look(1);
        let p1 = id_of(&rig, "P1");
        let mut p = ProgrammerParams::default();
        p.hold_rgbw(&[p1], [100.0, 0.0, 0.0, 0.0]);
        let before = Sections::of(&p);
        p.beam.gobo = 7;
        p.owned.mark(&before, &Sections::of(&p));
        let frames = render(&rig, &library, &deep_blue, &p, &HashSet::new(), 0.0);
        let at = |label, ch: &ChannelType| value_of(&rig, &library, &frames, label, ch);
        // P1 is held red; P2 keeps the look's blue.
        assert_eq!(at("P1", &ChannelType::Red), 255);
        assert_eq!(at("P1", &ChannelType::Blue), 0);
        assert_eq!(at("P2", &ChannelType::Blue), 255);
        // The gobo is the programmer's.
        assert_eq!(at("S1", &ChannelType::Gobo), 60);
        // PREVIEW (blind) shows the look alone.
        p.preview = true;
        let frames = render(&rig, &library, &deep_blue, &p, &HashSet::new(), 0.0);
        assert_eq!(
            value_of(&rig, &library, &frames, "P1", &ChannelType::Blue),
            255
        );
        assert_eq!(
            value_of(&rig, &library, &frames, "S1", &ChannelType::Gobo),
            0
        );
    }

    #[test]
    fn each_colour_preset_lands_on_the_shehds_wheel() {
        let (rig, library) = setup();
        let s1 = id_of(&rig, "S1");
        let expected = [
            ("White", 5),
            ("Red", 15),
            ("Orange", 25),
            ("Yellow", 35),
            ("Green", 45),
            ("Cyan", 75),
            ("Blue", 55),
            ("Magenta", 65),
            ("Pink", 65),
            ("UV", 55),
        ];
        for ((name, preset), (want_name, want)) in COLOR_PRESETS.iter().zip(expected) {
            assert_eq!(*name, want_name);
            let mut p = ProgrammerParams::default();
            p.hold_rgbw(&[s1], *preset);
            let frames = dark(&rig, &library, &p);
            assert_eq!(
                value_of(&rig, &library, &frames, "S1", &ChannelType::Color),
                want,
                "{name}"
            );
        }
    }

    #[test]
    fn eleven_channel_spots_get_16_bit_position() {
        let (mut rig, library) = setup();
        for f in rig.fixtures_mut().iter_mut().filter(|f| f.label == "S1") {
            f.mode = "11CH".into();
        }
        let mut p = ProgrammerParams::default();
        p.position.pan = 1.0;
        p.position.tilt = 180.0;
        let frames = dark(&rig, &library, &p);
        let value = |ch| value_of(&rig, &library, &frames, "S1", ch);
        // 1° of 360 is 182 of 65535: all in the fine byte.
        assert_eq!(
            (value(&ChannelType::Pan), value(&ChannelType::PanFine)),
            (0, 182)
        );
        assert_eq!(
            (value(&ChannelType::Tilt), value(&ChannelType::TiltFine)),
            (128, 0)
        );
    }
}
