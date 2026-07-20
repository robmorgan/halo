//! Per-fixture DMX output: flattens [`resolve`](crate::programmer::resolve)'d
//! lane levels + programmer parameters into per-universe channel frames.
//!
//! [`render`] is pure — (rig, params, lane outputs, beat time) in, frames
//! out — so the DMX engine thread and any future preview visualizer get
//! identical results. Frames explicitly zero dark fixtures: a rendered
//! universe always ships all 512 channels, so "off" is sent, not held.
//!
//! Value semantics (v1, until Phase L2's per-fixture programming):
//! - A fixture's intensity is its lane's resolved level × the programmer dimmer. Fixtures without a
//!   dimmer channel fold intensity into their color channels.
//! - Palette (RGBW), position, gobo, and strobe apply rig-wide; *effects* run across the fixture
//!   selection (or the whole rig when nothing is selected), with Step/Wave distribution by ordinal
//!   in that cohort.
//! - HIGHLIGHT snaps the selection to open white. PREVIEW (blind) renders as if the programmer
//!   params were untouched defaults.

use std::collections::{HashMap, HashSet};

use crate::cues::LANE_COUNT;
use crate::fixture::{FixtureKind, Rig};
use crate::fixture_library::{ChannelType, FixtureLibrary, FixtureProfile};
use crate::programmer::{
    COLOR_PRESETS, Distribution, EffectConfig, LaneOutput, PanTiltTarget, ProgrammerParams,
    effect_value,
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
    lanes: &[LaneOutput; LANE_COUNT],
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

    // Effect cohort: the selection, or everything when nothing is
    // selected. Ordinals follow rig order.
    let cohort: Vec<u32> = rig
        .iter()
        .filter(|f| selection.is_empty() || selection.contains(&f.id))
        .map(|f| f.id)
        .collect();

    let mut universes: HashMap<u8, UniverseFrame> = HashMap::new();
    for f in rig.iter() {
        let Some(profile) = library.get(&f.profile_id) else {
            continue;
        };
        let footprint = profile.footprint();
        let base = (f.start_address.max(1) - 1) as usize;
        if base + footprint > UNIVERSE_CHANNELS {
            continue;
        }
        let frame = universes
            .entry(f.universe)
            .or_insert([0u8; UNIVERSE_CHANNELS]);
        let slot = &mut frame[base..base + footprint];

        if params.highlight && selection.contains(&f.id) {
            render_highlight(profile, slot);
            continue;
        }

        let level = lanes[f.kind.lane() as usize].level.clamp(0.0, 1.0);
        let ord = cohort.iter().position(|&id| id == f.id);
        if f.kind == FixtureKind::PixelBar {
            render_pixel_bar(profile, slot, p, level, beat_t);
        } else {
            render_conventional(profile, slot, p, level, beat_t, ord);
        }
    }
    universes
}

/// Open white at full for fixture identification.
fn render_highlight(profile: &FixtureProfile, slot: &mut [u8]) {
    for (i, ch) in profile.channel_layout.iter().enumerate() {
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

fn render_conventional(
    profile: &FixtureProfile,
    slot: &mut [u8],
    p: &ProgrammerParams,
    level: f32,
    beat_t: f64,
    ord: Option<usize>,
) {
    let mut dim = level * (p.intensity.dimmer / 100.0);
    if let Some(fx) = effect_at(&p.intensity.effect, beat_t, ord) {
        dim *= fx;
    }

    let mut rgbw = p.color.rgbw.map(|v| v / 100.0);
    if let Some(fx) = effect_at(&p.color.effect, beat_t, ord) {
        for c in rgbw.iter_mut() {
            *c *= fx;
        }
    }

    let pos_fx = effect_at(&p.position.effect, beat_t, ord);
    let axis = |deg: f32, driven: bool| -> u8 {
        let base = deg / 360.0;
        match pos_fx {
            // ±1/8 of travel swing around the base position.
            Some(fx) if driven => u8_of(base + (fx - 0.5) * 0.25),
            _ => u8_of(base),
        }
    };
    let pan_driven = matches!(p.position.target, PanTiltTarget::Both | PanTiltTarget::Pan);
    let tilt_driven = matches!(p.position.target, PanTiltTarget::Both | PanTiltTarget::Tilt);

    // Fixtures without a dimmer channel carry intensity in their color
    // channels instead.
    let color_scale = if profile.channel_offset(&ChannelType::Dimmer).is_some() {
        1.0
    } else {
        dim
    };

    for (i, ch) in profile.channel_layout.iter().enumerate() {
        slot[i] = match &ch.channel_type {
            ChannelType::Dimmer => u8_of(dim),
            ChannelType::Red => u8_of(rgbw[0] * color_scale),
            ChannelType::Green => u8_of(rgbw[1] * color_scale),
            ChannelType::Blue => u8_of(rgbw[2] * color_scale),
            ChannelType::White => u8_of(rgbw[3] * color_scale),
            ChannelType::Strobe => u8_of(p.intensity.strobe / 100.0),
            ChannelType::Pan => axis(p.position.pan, pan_driven),
            ChannelType::Tilt => axis(p.position.tilt, tilt_driven),
            // 8 gobo slots spread across the wheel's DMX range.
            ChannelType::Gobo => p.beam.gobo.saturating_sub(1).min(7) * 32,
            ChannelType::Other(name) => match name.as_str() {
                // The FX lane level drives smoke output directly.
                "Smoke" => u8_of(dim),
                // Igniter convention: Safety arms with any level, Fire
                // needs the lane driven hard (a deliberate cue at ≥ 0.9).
                "Safety" => {
                    if level > 0.0 {
                        255
                    } else {
                        0
                    }
                }
                "Fire" => {
                    if level >= 0.9 {
                        255
                    } else {
                        0
                    }
                }
                _ => 0,
            },
            _ => 0,
        };
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
    profile: &FixtureProfile,
    slot: &mut [u8],
    p: &ProgrammerParams,
    level: f32,
    beat_t: f64,
) {
    let n = profile.footprint() / 3;
    if n == 0 || level <= 0.0 {
        return; // slot is already zeroed
    }
    let base_rgb = preset_rgb(COLOR_PRESETS[p.pixel.color.min(COLOR_PRESETS.len() - 1)].1);
    let frac = |x: f64| x.rem_euclid(1.0);

    for i in 0..n {
        let fi = i as f32;
        let nf = n as f32;
        // (intensity, optional color override) per pixel.
        let (v, over): (f32, Option<[f32; 3]>) = match p.pixel.effect {
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

    fn lanes(lighting: f32, pixels: f32, fx: f32) -> [LaneOutput; LANE_COUNT] {
        [lighting, pixels, fx].map(|level| LaneOutput {
            source: if level > 0.0 {
                LaneSource::Track
            } else {
                LaneSource::Off
            },
            level,
        })
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
        let profile = library.get(&f.profile_id).unwrap();
        let off = profile.channel_offset(ch).expect("channel");
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
            let profile = library.get(&f.profile_id).unwrap();
            let base = (f.start_address - 1) as usize;
            let slot = &frames[&f.universe][base..base + profile.footprint()];
            match profile.channel_offset(&ChannelType::Dimmer) {
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
        let mut params = ProgrammerParams::default();
        params.highlight = true;
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
        let profile = library.get(&pb1.profile_id).unwrap();
        let base = (pb1.start_address - 1) as usize;
        let slot = &lit[&pb1.universe][base..base + profile.footprint()];
        let lit_pixels = slot
            .chunks(3)
            .filter(|px| px.iter().any(|&v| v > 0))
            .count();
        let n = profile.footprint() / 3;
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
}
