//! A track's energy curve: the NRG lane under each deck. It scales the
//! whole rig's intensity as the track plays, with the programmer's live
//! ENERGY fader on top (see `programmer::resolve`).

/// One saved breakpoint, in seconds.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EnergyPoint {
    pub at: f64,
    pub value: f32,
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

    /// Persisted form, in seconds.
    pub fn to_points(&self, sample_rate: u32) -> Vec<EnergyPoint> {
        let sr = f64::from(sample_rate.max(1));
        self.points
            .iter()
            .map(|p| EnergyPoint {
                at: p.frame / sr,
                value: p.value,
            })
            .collect()
    }

    /// Rebuild from the persisted form at the current device rate, with
    /// fresh ids.
    pub fn from_points(points: &[EnergyPoint], sample_rate: u32) -> Self {
        let sr = f64::from(sample_rate.max(1));
        let mut lane = Self::default();
        for p in points {
            lane.insert(p.at * sr, p.value);
        }
        lane
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn points_round_trip_through_seconds() {
        let mut e = EnergyLane::default();
        e.insert(44_100.0, 0.25);
        e.insert(88_200.0, 0.75);
        let saved = e.to_points(44_100);
        assert_eq!(
            saved[0],
            EnergyPoint {
                at: 1.0,
                value: 0.25
            }
        );
        // At another device rate the points land at the same seconds.
        let back = EnergyLane::from_points(&saved, 48_000);
        let frames: Vec<f64> = back.points().iter().map(|p| p.frame).collect();
        assert_eq!(frames, vec![48_000.0, 96_000.0]);
        assert!((back.value_at(72_000.0) - 0.5).abs() < 1e-6);
    }
}
