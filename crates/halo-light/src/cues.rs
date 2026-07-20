//! Editable lighting/pixels/FX cues for a track.
//!
//! `CueSet` is the runtime model (frames, painter-friendly windowed
//! queries, mutation with per-lane sort + non-overlap invariants);
//! `CueFile` is the persisted JSON form stored in the library, in seconds
//! so cues survive device sample-rate changes.

use std::collections::HashSet;

/// The three trigger lanes drawn under the zoomed waveform, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Lane {
    Lighting = 0,
    Pixels = 1,
    Fx = 2,
}

pub const LANE_COUNT: usize = 3;
pub const ALL_LANES: [Lane; LANE_COUNT] = [Lane::Lighting, Lane::Pixels, Lane::Fx];

/// One effect firing: a bar on a lane from `start_frame` for
/// `duration_frames` source frames. Ids are runtime-only (selection
/// handles for the editor) and are not persisted.
#[derive(Debug, Clone, Copy)]
pub struct Cue {
    pub id: u64,
    pub start_frame: f64,
    pub duration_frames: f64,
    /// 0..=1; drives bar alpha and, later, the rig level.
    pub intensity: f32,
}

impl Cue {
    pub fn end_frame(&self) -> f64 {
        self.start_frame + self.duration_frames
    }
}

/// Smallest representable cue, in frames — guards against degenerate
/// zero-width cues from clamping; musical minimums are enforced by the
/// editor.
const MIN_DUR_FRAMES: f64 = 1.0;

/// All cues for a track. Invariant per lane: sorted by start frame,
/// non-overlapping. Mutators clamp rather than reject, so editor drags
/// slide a cue until it butts its neighbor.
#[derive(Debug, Clone, Default)]
pub struct CueSet {
    lanes: [Vec<Cue>; LANE_COUNT],
    /// Per-lane max duration, to widen the visibility window cheaply.
    max_duration: [f64; LANE_COUNT],
    next_id: u64,
}

impl CueSet {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Cues possibly overlapping `[start, end)`: a binary search on start
    /// frame, widened by the lane's max duration (same idea as
    /// `GridMarks::visible_range`).
    pub fn visible(&self, lane: Lane, start: f64, end: f64) -> &[Cue] {
        let l = lane as usize;
        let v = &self.lanes[l];
        let lo = v.partition_point(|c| c.start_frame < start - self.max_duration[l]);
        let hi = v.partition_point(|c| c.start_frame < end);
        &v[lo..hi]
    }

    /// The cue covering `frame` on `lane`, if any.
    pub fn active_at(&self, lane: Lane, frame: f64) -> Option<&Cue> {
        self.visible(lane, frame, frame + 1.0)
            .iter()
            .find(|c| c.start_frame <= frame && frame < c.end_frame())
    }

    pub fn find(&self, id: u64) -> Option<(Lane, Cue)> {
        for lane in ALL_LANES {
            if let Some(c) = self.lanes[lane as usize].iter().find(|c| c.id == id) {
                return Some((lane, *c));
            }
        }
        None
    }

    fn rescan_lane(&mut self, lane: Lane) {
        let l = lane as usize;
        self.lanes[l].sort_by(|a, b| a.start_frame.total_cmp(&b.start_frame));
        self.max_duration[l] = self.lanes[l]
            .iter()
            .map(|c| c.duration_frames)
            .fold(0.0, f64::max);
    }

    /// Insert a cue, truncated into the free gap around `start`; `None`
    /// when there is no usable gap.
    pub fn insert(&mut self, lane: Lane, start: f64, dur: f64, intensity: f32) -> Option<u64> {
        let l = lane as usize;
        let mut start = start.max(0.0);
        let idx = self.lanes[l].partition_point(|c| c.start_frame < start);
        if let Some(prev) = idx.checked_sub(1).map(|i| &self.lanes[l][i]) {
            start = start.max(prev.end_frame());
        }
        let mut end = start + dur.max(MIN_DUR_FRAMES);
        if let Some(next) = self.lanes[l].get(idx) {
            end = end.min(next.start_frame);
        }
        if end - start < MIN_DUR_FRAMES {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.lanes[l].push(Cue {
            id,
            start_frame: start,
            duration_frames: end - start,
            intensity: intensity.clamp(0.0, 1.0),
        });
        self.rescan_lane(lane);
        Some(id)
    }

    /// Neighbor bounds of cue `pos` in `lane`: (min start, max end).
    fn gap_around(&self, lane: Lane, pos: usize) -> (f64, f64) {
        let v = &self.lanes[lane as usize];
        let lo = pos.checked_sub(1).map_or(0.0, |i| v[i].end_frame());
        let hi = v.get(pos + 1).map_or(f64::INFINITY, |c| c.start_frame);
        (lo, hi)
    }

    fn position_of(&self, id: u64) -> Option<(Lane, usize)> {
        for lane in ALL_LANES {
            if let Some(i) = self.lanes[lane as usize].iter().position(|c| c.id == id) {
                return Some((lane, i));
            }
        }
        None
    }

    /// Move a cue, keeping its duration; clamped between its neighbors.
    pub fn move_cue(&mut self, id: u64, new_start: f64) {
        let Some((lane, i)) = self.position_of(id) else {
            return;
        };
        // Moving can pass over neighbors only by removing + reinserting;
        // v1 clamps within the current gap, which reads as the cue
        // butting its neighbor mid-drag.
        let (lo, hi) = self.gap_around(lane, i);
        let cue = &mut self.lanes[lane as usize][i];
        let max_start = (hi - cue.duration_frames).max(lo);
        cue.start_frame = new_start.clamp(lo.max(0.0), max_start.max(0.0));
        self.rescan_lane(lane);
    }

    /// Resize a cue to `[new_start, new_end)`, clamped to its neighbors.
    pub fn resize(&mut self, id: u64, new_start: f64, new_end: f64) {
        let Some((lane, i)) = self.position_of(id) else {
            return;
        };
        let (lo, hi) = self.gap_around(lane, i);
        let cue = &mut self.lanes[lane as usize][i];
        let start = new_start.clamp(lo.max(0.0), hi - MIN_DUR_FRAMES);
        let end = new_end.clamp(start + MIN_DUR_FRAMES, hi);
        cue.start_frame = start;
        cue.duration_frames = end - start;
        self.rescan_lane(lane);
    }

    pub fn set_intensity(&mut self, id: u64, v: f32) {
        if let Some((lane, i)) = self.position_of(id) {
            self.lanes[lane as usize][i].intensity = v.clamp(0.0, 1.0);
        }
    }

    pub fn remove(&mut self, ids: &HashSet<u64>) {
        for lane in ALL_LANES {
            self.lanes[lane as usize].retain(|c| !ids.contains(&c.id));
            self.rescan_lane(lane);
        }
    }

    pub fn clear_lane(&mut self, lane: Lane) {
        self.lanes[lane as usize].clear();
        self.max_duration[lane as usize] = 0.0;
    }

    /// Persisted form, in seconds.
    pub fn to_file(&self, sample_rate: u32) -> CueFile {
        let sr = sample_rate.max(1) as f64;
        CueFile {
            version: 1,
            lanes: ALL_LANES.map(|lane| {
                self.lanes[lane as usize]
                    .iter()
                    .map(|c| CueJson {
                        start: c.start_frame / sr,
                        dur: c.duration_frames / sr,
                        intensity: c.intensity,
                    })
                    .collect()
            }),
        }
    }

    /// Rebuild from the persisted form at the current device rate. Fresh
    /// ids; defensively re-sorts and drops any overlapping cues.
    pub fn from_file(file: &CueFile, sample_rate: u32) -> Self {
        let sr = sample_rate.max(1) as f64;
        let mut set = Self::empty();
        for lane in ALL_LANES {
            let mut sorted: Vec<&CueJson> = file.lanes[lane as usize].iter().collect();
            sorted.sort_by(|a, b| a.start.total_cmp(&b.start));
            for c in sorted {
                set.insert(lane, c.start * sr, c.dur * sr, c.intensity);
            }
        }
        set
    }
}

/// JSON blob stored per track in the library's `lighting_cues` table.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CueFile {
    pub version: u32,
    pub lanes: [Vec<CueJson>; LANE_COUNT],
}

/// One persisted cue, in seconds.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CueJson {
    pub start: f64,
    pub dur: f64,
    pub intensity: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_with(cues: &[(f64, f64)]) -> CueSet {
        let mut s = CueSet::empty();
        for &(start, dur) in cues {
            s.insert(Lane::Lighting, start, dur, 0.8).unwrap();
        }
        s
    }

    fn spans(s: &CueSet, lane: Lane) -> Vec<(f64, f64)> {
        s.visible(lane, f64::MIN, f64::MAX)
            .iter()
            .map(|c| (c.start_frame, c.duration_frames))
            .collect()
    }

    #[test]
    fn insert_truncates_into_gap() {
        let mut s = set_with(&[(0.0, 100.0), (300.0, 100.0)]);
        // Requested span overlaps both neighbors: clamped to [100, 300).
        let id = s.insert(Lane::Lighting, 50.0, 500.0, 1.0).unwrap();
        let (_, cue) = s.find(id).unwrap();
        assert_eq!((cue.start_frame, cue.end_frame()), (100.0, 300.0));
        // A fully-occupied gap rejects the insert.
        assert!(s.insert(Lane::Lighting, 150.0, 10.0, 1.0).is_none());
    }

    #[test]
    fn move_clamps_between_neighbors() {
        let mut s = set_with(&[(0.0, 100.0), (200.0, 100.0), (500.0, 100.0)]);
        let mid = s.active_at(Lane::Lighting, 250.0).unwrap().id;
        s.move_cue(mid, 0.0); // butts the left neighbor
        assert_eq!(spans(&s, Lane::Lighting)[1].0, 100.0);
        s.move_cue(mid, 1_000.0); // butts the right neighbor
        assert_eq!(spans(&s, Lane::Lighting)[1].0, 400.0);
    }

    #[test]
    fn resize_clamps_and_keeps_min_duration() {
        let mut s = set_with(&[(0.0, 100.0), (200.0, 100.0), (500.0, 100.0)]);
        let mid = s.active_at(Lane::Lighting, 250.0).unwrap().id;
        s.resize(mid, 50.0, 600.0); // both edges hit neighbors
        let (_, cue) = s.find(mid).unwrap();
        assert_eq!((cue.start_frame, cue.end_frame()), (100.0, 500.0));
        s.resize(mid, 300.0, 300.0); // collapses to the minimum, not zero
        let (_, cue) = s.find(mid).unwrap();
        assert!(cue.duration_frames >= 1.0);
    }

    #[test]
    fn remove_and_active_at() {
        let mut s = set_with(&[(0.0, 100.0), (200.0, 100.0)]);
        assert!(s.active_at(Lane::Lighting, 50.0).is_some());
        assert!(s.active_at(Lane::Lighting, 150.0).is_none());
        let first = s.active_at(Lane::Lighting, 50.0).unwrap().id;
        s.remove(&HashSet::from([first]));
        assert!(s.active_at(Lane::Lighting, 50.0).is_none());
        assert_eq!(spans(&s, Lane::Lighting).len(), 1);
    }

    #[test]
    fn file_round_trip_across_sample_rates() {
        let mut s = CueSet::empty();
        s.insert(Lane::Lighting, 44_100.0, 88_200.0, 0.9);
        s.insert(Lane::Pixels, 22_050.0, 11_025.0, 0.5);
        s.insert(Lane::Fx, 0.0, 44_100.0, 1.0);
        let file = s.to_file(44_100);
        // Reload at a different device rate: times in seconds are stable.
        let reloaded = CueSet::from_file(&file, 48_000);
        let cue = reloaded.active_at(Lane::Lighting, 1.5 * 48_000.0).unwrap();
        assert!((cue.start_frame - 48_000.0).abs() < 1e-6);
        assert!((cue.duration_frames - 96_000.0).abs() < 1e-6);
        assert!((cue.intensity - 0.9).abs() < 1e-6);
        // JSON round-trip too (what the library stores).
        let json = serde_json::to_string(&file).unwrap();
        let parsed: CueFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.lanes[0].len(), 1);
        assert_eq!(parsed.version, 1);
    }

    #[test]
    fn from_file_drops_overlaps_defensively() {
        let file = CueFile {
            version: 1,
            lanes: [
                vec![
                    CueJson {
                        start: 0.0,
                        dur: 2.0,
                        intensity: 1.0,
                    },
                    CueJson {
                        start: 1.0, // overlaps the first
                        dur: 2.0,
                        intensity: 1.0,
                    },
                ],
                Vec::new(),
                Vec::new(),
            ],
        };
        let s = CueSet::from_file(&file, 100);
        let got = spans(&s, Lane::Lighting);
        // Second cue is truncated into the remaining gap, not overlapping.
        assert_eq!(got.len(), 2);
        assert!(got[0].0 + got[0].1 <= got[1].0 + 1e-9);
    }
}
