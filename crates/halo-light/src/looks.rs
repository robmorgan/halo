//! Looks: what the rig does between two events on a deck's LOOK lane —
//! colour, intensity, position, gobo and effects, in the programmer's own
//! parameter sections. Each LOOK event switches the rig to one of the
//! [`BUILTIN`] looks until the next event; the programmer overrides what
//! it touches (see `programmer::effective`).

use std::sync::LazyLock;

use crate::programmer::{
    BeamParams, COLOR_PRESETS, ColorParams, Distribution, EffectConfig, IntensityParams, Interval,
    PixelFxParams, PositionParams, Waveform,
};

/// What a look sets: the programmer's parameter sections.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LookParams {
    pub intensity: IntensityParams,
    pub color: ColorParams,
    pub position: PositionParams,
    pub beam: BeamParams,
    pub pixel: PixelFxParams,
}

/// A named look and the colour the LOOK lane draws it in.
#[derive(Debug, Clone)]
pub struct Look {
    pub name: &'static str,
    pub swatch: [u8; 3],
    pub params: LookParams,
}

/// The look `id` names; ids past the end wrap.
pub fn builtin(id: LookId) -> &'static Look {
    &BUILTIN[id.0 % BUILTIN.len()]
}

/// What a deck's LOOK lane holds at `playhead`: nothing before its first
/// event, or without a lane.
pub fn look_at(lane: Option<&LookLane>, playhead: f64) -> Option<&'static LookParams> {
    lane?.active_at(playhead).map(|e| &builtin(e.look).params)
}

/// A running effect.
fn fx(
    waveform: Waveform,
    interval: Interval,
    ratio: f32,
    distribution: Distribution,
) -> EffectConfig {
    EffectConfig {
        waveform,
        interval,
        ratio,
        phase_deg: 0.0,
        distribution,
        applied: true,
    }
}

/// Index into [`COLOR_PRESETS`] by name.
fn preset(name: &str) -> usize {
    COLOR_PRESETS
        .iter()
        .position(|(n, _)| *n == name)
        .expect("a colour preset")
}

/// One look: `rgbw` and `dimmer` in percent, `gobo` 0 for open, and the
/// pixel bars' effect (index into `PIXEL_EFFECTS`) in a colour preset.
struct Spec {
    name: &'static str,
    swatch: [u8; 3],
    rgbw: [f32; 4],
    dimmer: f32,
    strobe: f32,
    intensity_fx: Option<EffectConfig>,
    position_fx: Option<EffectConfig>,
    gobo: u8,
    pixel: (usize, &'static str),
}

impl Spec {
    fn look(self) -> Look {
        let off = EffectConfig::default();
        Look {
            name: self.name,
            swatch: self.swatch,
            params: LookParams {
                intensity: IntensityParams {
                    dimmer: self.dimmer,
                    strobe: self.strobe,
                    effect: self.intensity_fx.unwrap_or_else(|| off.clone()),
                },
                color: ColorParams {
                    rgbw: self.rgbw,
                    effect: off.clone(),
                },
                position: PositionParams {
                    effect: self.position_fx.unwrap_or(off),
                    ..PositionParams::default()
                },
                beam: BeamParams {
                    gobo: self.gobo,
                    ..BeamParams::default()
                },
                pixel: PixelFxParams {
                    effect: self.pixel.0,
                    color: preset(self.pixel.1),
                },
            },
        }
    }
}

/// The eight looks the LOOK lane picks from, in its palette's order.
pub static BUILTIN: LazyLock<[Look; 8]> = LazyLock::new(|| {
    use Distribution::{Step, Wave};
    use Interval::{Bar, Beat, Phrase};
    use Waveform::{Sawtooth, Sine, Square, Triangle};
    let calm = |interval, ratio, wave| Some(fx(Sine, interval, ratio, Wave(wave)));
    [
        Spec {
            name: "Warm Open",
            swatch: [255, 170, 60],
            rgbw: [100.0, 70.0, 30.0, 40.0],
            dimmer: 80.0,
            strobe: 0.0,
            intensity_fx: None,
            position_fx: calm(Bar, 0.5, 45),
            gobo: 0,
            pixel: (5, "Orange"),
        },
        Spec {
            name: "Deep Blue",
            swatch: [60, 110, 235],
            rgbw: [0.0, 0.0, 100.0, 0.0],
            dimmer: 70.0,
            strobe: 0.0,
            intensity_fx: None,
            position_fx: calm(Bar, 0.25, 90),
            gobo: 0,
            pixel: (5, "Blue"),
        },
        Spec {
            name: "Red Drop",
            swatch: [230, 45, 45],
            rgbw: [100.0, 0.0, 0.0, 0.0],
            dimmer: 100.0,
            strobe: 0.0,
            intensity_fx: Some(fx(Square, Beat, 1.0, Step(2))),
            position_fx: Some(fx(Triangle, Beat, 0.5, Wave(45))),
            gobo: 3,
            pixel: (0, "Red"),
        },
        Spec {
            name: "Strobe White",
            swatch: [238, 238, 244],
            rgbw: [0.0, 0.0, 0.0, 100.0],
            dimmer: 100.0,
            strobe: 70.0,
            intensity_fx: None,
            position_fx: None,
            gobo: 0,
            pixel: (6, "White"),
        },
        Spec {
            name: "Acid Green",
            swatch: [120, 225, 70],
            rgbw: [0.0, 100.0, 0.0, 0.0],
            dimmer: 100.0,
            strobe: 0.0,
            intensity_fx: Some(fx(Sawtooth, Beat, 1.0, Wave(60))),
            position_fx: calm(Bar, 1.0, 30),
            gobo: 5,
            pixel: (3, "Green"),
        },
        Spec {
            name: "Magenta Chase",
            swatch: [240, 60, 190],
            rgbw: [100.0, 0.0, 100.0, 0.0],
            dimmer: 100.0,
            strobe: 0.0,
            intensity_fx: Some(fx(Square, Beat, 1.0, Step(4))),
            position_fx: Some(fx(Sine, Beat, 0.5, Step(2))),
            gobo: 2,
            pixel: (7, "Magenta"),
        },
        Spec {
            name: "Amber Sweep",
            swatch: [250, 205, 40],
            rgbw: [100.0, 70.0, 0.0, 0.0],
            dimmer: 90.0,
            strobe: 0.0,
            intensity_fx: None,
            position_fx: calm(Bar, 0.5, 90),
            gobo: 4,
            pixel: (1, "Yellow"),
        },
        Spec {
            name: "UV Violet",
            swatch: [140, 70, 255],
            rgbw: [45.0, 0.0, 100.0, 0.0],
            dimmer: 60.0,
            strobe: 0.0,
            intensity_fx: calm(Bar, 1.0, 45),
            position_fx: calm(Phrase, 1.0, 0),
            gobo: 6,
            pixel: (2, "UV"),
        },
    ]
    .map(Spec::look)
});

/// One of the [`BUILTIN`] looks, by index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookId(pub usize);

/// One look change: the rig switches to `look` at `frame` and holds until
/// the next event. Ids are runtime-only selection handles.
#[derive(Debug, Clone, Copy)]
pub struct LookEvent {
    pub id: u64,
    pub frame: f64,
    pub look: LookId,
}

/// Sparse look events. Invariant: sorted by frame; mutators enforce the
/// `min_sep` they're given (the editor passes ~1 beat), clamping rather
/// than rejecting on moves so drags butt their neighbors.
#[derive(Debug, Clone, Default)]
pub struct LookLane {
    events: Vec<LookEvent>,
    next_id: u64,
}

impl LookLane {
    pub fn events(&self) -> &[LookEvent] {
        &self.events
    }

    fn pos_of(&self, id: u64) -> Option<usize> {
        self.events.iter().position(|e| e.id == id)
    }

    /// Insert an event; `None` when another event sits within `min_sep`.
    pub fn insert(&mut self, frame: f64, look: LookId, min_sep: f64) -> Option<u64> {
        let frame = frame.max(0.0);
        if self
            .events
            .iter()
            .any(|e| (e.frame - frame).abs() < min_sep)
        {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.events.push(LookEvent { id, frame, look });
        self.events.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        Some(id)
    }

    /// Move an event, clamped `min_sep` clear of both neighbors (and ≥ 0).
    pub fn move_event(&mut self, id: u64, new_frame: f64, min_sep: f64) {
        let Some(i) = self.pos_of(id) else {
            return;
        };
        let lo = i
            .checked_sub(1)
            .map_or(0.0, |p| self.events[p].frame + min_sep);
        let hi = self
            .events
            .get(i + 1)
            .map_or(f64::INFINITY, |n| n.frame - min_sep);
        if lo > hi {
            return; // neighbors closer than 2×min_sep: hold position
        }
        self.events[i].frame = new_frame.clamp(lo.max(0.0), hi.max(0.0));
    }

    pub fn set_look(&mut self, id: u64, look: LookId) {
        if let Some(i) = self.pos_of(id) {
            self.events[i].look = look;
        }
    }

    pub fn remove(&mut self, id: u64) {
        self.events.retain(|e| e.id != id);
    }

    pub fn find(&self, id: u64) -> Option<LookEvent> {
        self.pos_of(id).map(|i| self.events[i])
    }

    /// Hold semantics: the last event at or before `frame`; `None` before
    /// the first event.
    pub fn active_at(&self, frame: f64) -> Option<&LookEvent> {
        let i = self.events.partition_point(|e| e.frame <= frame);
        i.checked_sub(1).map(|i| &self.events[i])
    }

    /// Events whose *block* overlaps `[start, end)` — including the
    /// carry-in event that starts before `start` but holds into the
    /// window. (Differs from `CueSet::visible`, which windows by explicit
    /// durations.)
    pub fn visible(&self, start: f64, end: f64) -> &[LookEvent] {
        let hi = self.events.partition_point(|e| e.frame < end);
        let lo = self.events[..hi]
            .partition_point(|e| e.frame <= start)
            .saturating_sub(1);
        &self.events[lo..hi]
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(lane: &LookLane) -> Vec<f64> {
        lane.events().iter().map(|e| e.frame).collect()
    }

    #[test]
    fn look_insert_sorts_and_enforces_min_sep() {
        let mut l = LookLane::default();
        assert!(l.insert(300.0, LookId(0), 100.0).is_some());
        assert!(l.insert(0.0, LookId(1), 100.0).is_some());
        // Within min_sep of the event at 300: rejected.
        assert!(l.insert(250.0, LookId(2), 100.0).is_none());
        assert_eq!(frames(&l), vec![0.0, 300.0]);
    }

    #[test]
    fn look_move_clamps_between_neighbors() {
        let mut l = LookLane::default();
        let a = l.insert(0.0, LookId(0), 10.0).unwrap();
        let b = l.insert(500.0, LookId(1), 10.0).unwrap();
        let mid = l.insert(200.0, LookId(2), 10.0).unwrap();
        l.move_event(mid, -1000.0, 10.0);
        assert_eq!(l.find(mid).unwrap().frame, 10.0);
        l.move_event(mid, 1000.0, 10.0);
        assert_eq!(l.find(mid).unwrap().frame, 490.0);
        // The first event clamps at 0 with no left neighbor.
        l.move_event(a, -50.0, 10.0);
        assert_eq!(l.find(a).unwrap().frame, 0.0);
        // Order still sorted after all the shoving.
        let _ = b;
        assert!(frames(&l).windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn look_active_at_holds_until_next() {
        let mut l = LookLane::default();
        l.insert(100.0, LookId(3), 1.0);
        l.insert(400.0, LookId(5), 1.0);
        assert!(l.active_at(50.0).is_none());
        assert_eq!(l.active_at(100.0).unwrap().look, LookId(3));
        assert_eq!(l.active_at(399.0).unwrap().look, LookId(3));
        assert_eq!(l.active_at(400.0).unwrap().look, LookId(5));
        assert_eq!(l.active_at(9_999.0).unwrap().look, LookId(5));
    }

    #[test]
    fn look_visible_includes_carry_in() {
        let mut l = LookLane::default();
        l.insert(0.0, LookId(0), 1.0);
        l.insert(100.0, LookId(1), 1.0);
        l.insert(500.0, LookId(2), 1.0);
        // Window opens mid-hold of the event at 100: it must be included.
        let vis = frames_of(l.visible(200.0, 600.0));
        assert_eq!(vis, vec![100.0, 500.0]);
        // Window entirely inside one hold returns just that event.
        assert_eq!(frames_of(l.visible(150.0, 160.0)), vec![100.0]);
        // Window before everything returns the first event only.
        assert_eq!(frames_of(l.visible(-10.0, 50.0)), vec![0.0]);
    }

    fn frames_of(events: &[LookEvent]) -> Vec<f64> {
        events.iter().map(|e| e.frame).collect()
    }

    #[test]
    fn look_set_look_and_remove() {
        let mut l = LookLane::default();
        let a = l.insert(0.0, LookId(0), 1.0).unwrap();
        l.insert(100.0, LookId(1), 1.0);
        l.set_look(a, LookId(7));
        assert_eq!(l.find(a).unwrap().look, LookId(7));
        l.remove(a);
        assert!(l.find(a).is_none());
        assert_eq!(l.events().len(), 1);
    }

    #[test]
    fn the_builtin_looks_are_well_formed() {
        use crate::programmer::PIXEL_EFFECTS;
        assert_eq!(BUILTIN.len(), 8);
        for look in BUILTIN.iter() {
            let p = &look.params;
            assert!(p.beam.gobo <= 8, "{}", look.name);
            assert!(p.pixel.effect < PIXEL_EFFECTS.len(), "{}", look.name);
            assert!((0.0..=100.0).contains(&p.intensity.dimmer), "{}", look.name);
            for fx in [&p.intensity.effect, &p.position.effect] {
                assert!((0.0..=2.0).contains(&fx.ratio), "{}", look.name);
            }
        }
        // Every look but Strobe White moves the heads.
        let still: Vec<_> = BUILTIN
            .iter()
            .filter(|l| !l.params.position.effect.applied)
            .map(|l| l.name)
            .collect();
        assert_eq!(still, ["Strobe White"]);
        // Ids wrap.
        assert_eq!(builtin(LookId(9)).name, "Deep Blue");
    }
}
