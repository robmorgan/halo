//! Session-only Phase L3 preview model: the role-based show lanes
//! (look / energy / accent) with a hardcoded mock look palette.
//!
//! This is a UX preview, deliberately kept out of `halo-light`: nothing
//! here is persisted, and nothing here touches the DMX path — the legacy
//! `CueSet` keeps driving the rig. When L3 lands for real, looks become
//! library entities and `resolve()` learns this shape; these types don't
//! pretend to be that domain model.

use eframe::egui::Color32;
use halo_light::cues::{CueSet, Lane};

/// A mock look: name + signature color for the lane blocks and palette.
pub struct LookDef {
    pub name: &'static str,
    pub color: Color32,
}

/// Hardcoded palette the preview picks looks from. [`LookId`] indexes it.
pub const LOOK_PALETTE: [LookDef; 8] = [
    LookDef {
        name: "Warm Open",
        color: Color32::from_rgb(255, 170, 60),
    },
    LookDef {
        name: "Deep Blue",
        color: Color32::from_rgb(60, 110, 235),
    },
    LookDef {
        name: "Red Drop",
        color: Color32::from_rgb(230, 45, 45),
    },
    LookDef {
        name: "Strobe White",
        color: Color32::from_rgb(238, 238, 244),
    },
    LookDef {
        name: "Acid Green",
        color: Color32::from_rgb(120, 225, 70),
    },
    LookDef {
        name: "Magenta Chase",
        color: Color32::from_rgb(240, 60, 190),
    },
    LookDef {
        name: "Amber Sweep",
        color: Color32::from_rgb(250, 205, 40),
    },
    LookDef {
        name: "UV Violet",
        color: Color32::from_rgb(140, 70, 255),
    },
];

/// Index into [`LOOK_PALETTE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookId(pub usize);

impl LookId {
    pub fn def(self) -> &'static LookDef {
        &LOOK_PALETTE[self.0 % LOOK_PALETTE.len()]
    }
}

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

/// One energy breakpoint. Ids are runtime-only selection handles.
#[derive(Debug, Clone, Copy)]
pub struct Breakpoint {
    pub id: u64,
    pub frame: f64,
    pub value: f32,
}

/// Minimum frame separation between breakpoints, so the envelope stays a
/// function of time.
const POINT_SEP: f64 = 1.0;

/// Piecewise-linear energy envelope. Invariant: sorted by frame, strictly
/// increasing. Empty means flat 1.0.
#[derive(Debug, Clone, Default)]
pub struct EnergyLane {
    points: Vec<Breakpoint>,
    next_id: u64,
}

impl EnergyLane {
    pub fn points(&self) -> &[Breakpoint] {
        &self.points
    }

    fn pos_of(&self, id: u64) -> Option<usize> {
        self.points.iter().position(|p| p.id == id)
    }

    /// Insert a breakpoint; a frame collision nudges past the occupant.
    pub fn insert(&mut self, frame: f64, value: f32) -> u64 {
        let mut frame = frame.max(0.0);
        while self
            .points
            .iter()
            .any(|p| (p.frame - frame).abs() < POINT_SEP)
        {
            frame += POINT_SEP;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.points.push(Breakpoint {
            id,
            frame,
            value: value.clamp(0.0, 1.0),
        });
        self.points.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        id
    }

    /// Move a breakpoint: x clamped between its neighbors, y to 0..=1.
    pub fn move_point(&mut self, id: u64, frame: f64, value: f32) {
        let Some(i) = self.pos_of(id) else {
            return;
        };
        let lo = i
            .checked_sub(1)
            .map_or(0.0, |p| self.points[p].frame + POINT_SEP);
        let hi = self
            .points
            .get(i + 1)
            .map_or(f64::INFINITY, |n| n.frame - POINT_SEP);
        self.points[i].frame = frame.clamp(lo.max(0.0), hi.max(0.0));
        self.points[i].value = value.clamp(0.0, 1.0);
    }

    pub fn remove(&mut self, id: u64) {
        self.points.retain(|p| p.id != id);
    }

    pub fn find(&self, id: u64) -> Option<Breakpoint> {
        self.pos_of(id).map(|i| self.points[i])
    }

    /// Envelope value at `frame`: 1.0 when empty, flat extension before
    /// the first and after the last point, linear in between.
    pub fn value_at(&self, frame: f64) -> f32 {
        let (Some(first), Some(last)) = (self.points.first(), self.points.last()) else {
            return 1.0;
        };
        if frame <= first.frame {
            return first.value;
        }
        if frame >= last.frame {
            return last.value;
        }
        let i = self.points.partition_point(|p| p.frame <= frame);
        let (a, b) = (&self.points[i - 1], &self.points[i]);
        let t = ((frame - a.frame) / (b.frame - a.frame)) as f32;
        a.value + (b.value - a.value) * t
    }

    pub fn clear(&mut self) {
        self.points.clear();
    }
}

/// The accent lane borrows `CueSet`'s sorted/non-overlap machinery on one
/// designated legacy lane.
pub const ACCENT_LANE: Lane = Lane::Fx;

/// The full session-only show preview for one track.
#[derive(Debug, Clone, Default)]
pub struct ShowPreview {
    pub looks: LookLane,
    pub energy: EnergyLane,
    /// Only [`ACCENT_LANE`] is populated.
    pub accents: CueSet,
}

/// Typed selection handle — the three lanes have independent id spaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShowSel {
    Look(u64),
    Energy(u64),
    Accent(u64),
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
    fn energy_empty_is_flat_one() {
        let e = EnergyLane::default();
        assert_eq!(e.value_at(0.0), 1.0);
        assert_eq!(e.value_at(1e9), 1.0);
    }

    #[test]
    fn energy_interpolates_and_extends_flat() {
        let mut e = EnergyLane::default();
        e.insert(100.0, 0.2);
        e.insert(300.0, 0.8);
        assert!((e.value_at(0.0) - 0.2).abs() < 1e-6); // flat before
        assert!((e.value_at(200.0) - 0.5).abs() < 1e-6); // midpoint lerp
        assert!((e.value_at(500.0) - 0.8).abs() < 1e-6); // flat after
    }

    #[test]
    fn energy_move_clamps_x_between_neighbors_and_y_to_unit() {
        let mut e = EnergyLane::default();
        e.insert(0.0, 0.5);
        let mid = e.insert(200.0, 0.5);
        e.insert(400.0, 0.5);
        e.move_point(mid, -100.0, 2.0);
        let p = e.find(mid).unwrap();
        assert_eq!(p.frame, 1.0); // clamped 1 frame past the left neighbor
        assert_eq!(p.value, 1.0);
        e.move_point(mid, 1e9, -1.0);
        let p = e.find(mid).unwrap();
        assert_eq!(p.frame, 399.0);
        assert_eq!(p.value, 0.0);
    }

    #[test]
    fn energy_insert_nudges_off_duplicate_frames() {
        let mut e = EnergyLane::default();
        e.insert(100.0, 0.1);
        e.insert(100.0, 0.9);
        let f: Vec<f64> = e.points().iter().map(|p| p.frame).collect();
        assert!(f.windows(2).all(|w| w[1] - w[0] >= POINT_SEP));
    }

    #[test]
    fn show_preview_default_is_empty() {
        let s = ShowPreview::default();
        assert!(s.looks.events().is_empty());
        assert!(s.energy.points().is_empty());
        assert!(
            s.accents
                .visible(ACCENT_LANE, f64::MIN, f64::MAX)
                .is_empty()
        );
    }
}
