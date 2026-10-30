//! Beat mapping and bounded callback-side transport policy. Time here is a
//! ratio-derived software delivery boundary, not an acoustic timestamp.
use std::sync::atomic::Ordering;

use crate::output_queue::Cursor;
use crate::state::DeckShared;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SyncState {
    Off,
    Preparing,
    Waiting,
    Locked,
    Nudging,
    Unavailable,
}

impl SyncState {
    pub fn load(shared: &DeckShared) -> Self {
        match shared.sync_state.load(Ordering::Relaxed) {
            1 => Self::Preparing,
            2 => Self::Waiting,
            3 => Self::Locked,
            4 => Self::Nudging,
            5 => Self::Unavailable,
            _ => Self::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "",
            Self::Preparing => "PREPARING",
            Self::Waiting => "WAITING",
            Self::Locked => "LOCKED",
            Self::Nudging => "NUDGING",
            Self::Unavailable => "UNAVAILABLE",
        }
    }
}

/// Immutable UI-prepared mapping. Replaced and retired like playback storage.
pub struct BeatMap {
    frames: Box<[f64]>,
    /// Track BPM is the tempo reference; individual detected intervals only map phase.
    bpm: f64,
    pub track: u64,
}

impl BeatMap {
    pub fn new(frames: impl Iterator<Item = f64>, track: u64, bpm: f64) -> Self {
        let mut frames: Box<[f64]> = frames.collect();
        if frames.len() < 2
            || frames.iter().any(|x| !x.is_finite())
            || frames.windows(2).any(|pair| pair[1] <= pair[0])
        {
            frames = Box::new([]);
        }
        Self { frames, track, bpm }
    }

    pub fn matched_rate(&self, master: &Self, master_rate: f64) -> Option<f64> {
        let rate = master.bpm * master_rate / self.bpm;
        (self.bpm.is_finite()
            && self.bpm > 0.0
            && master.bpm.is_finite()
            && master.bpm > 0.0
            && rate.is_finite()
            && rate > 0.0)
            .then_some(rate)
    }

    /// Only used after matched_rate has validated both nominal BPMs.
    pub fn beat_frames(&self, sample_rate: u32) -> f64 {
        60.0 * sample_rate as f64 / self.bpm
    }

    /// Continuous beat coordinate and local source frames per beat.
    pub fn at(&self, position: f64) -> Option<(f64, f64)> {
        if self.frames.len() < 2 || position < self.frames[0] || position > *self.frames.last()? {
            return None;
        }
        let i = self
            .frames
            .partition_point(|&frame| frame <= position)
            .saturating_sub(1)
            .min(self.frames.len() - 2);
        let length = self.frames[i + 1] - self.frames[i];
        Some((i as f64 + (position - self.frames[i]) / length, length))
    }

    pub fn frame(&self, beat: f64) -> Option<f64> {
        if beat < 0.0 || beat > (self.frames.len().checked_sub(1)?) as f64 {
            return None;
        }
        let i = (beat.floor() as usize).min(self.frames.len() - 2);
        Some(self.frames[i] + (beat - i as f64) * (self.frames[i + 1] - self.frames[i]))
    }

    pub fn supports_loop(&self, region: Option<(usize, usize)>) -> bool {
        let Some((start, end)) = region else {
            return true;
        };
        let Some((a, _)) = self.at(start as f64) else {
            return false;
        };
        let Some((b, length)) = self.at(end as f64) else {
            return false;
        };
        let beats = b - a;
        beats >= 1.0 - 1.0 / length && (beats - beats.round()).abs() <= 1.0 / length
    }

    pub fn aligned_target(&self, cursor: Cursor, phase: f64) -> Option<f64> {
        let (beat, _) = self.at(cursor.position)?;
        let target_beat = beat + phase_error(phase, beat.rem_euclid(1.0));
        let mut target = self.frame(target_beat)?;
        if let Some((start, end)) = cursor.region {
            target = start as f64 + (target - start as f64).rem_euclid((end - start) as f64);
        }
        Some(target.min(cursor.total))
    }
}

pub fn phase_error(master: f64, follower: f64) -> f64 {
    let error = master - follower;
    error - error.round()
}

pub const DRIFT_TOLERANCE_BEATS: f64 = 1.0 / 64.0;

pub fn correction(error: f64) -> f64 {
    // Correct rounded DSP ratios before they reach the persistent-drift
    // timeout. A deadband wider than that tolerance would force a seek for
    // small errors that the rate controller was deliberately ignoring.
    let deadband = DRIFT_TOLERANCE_BEATS / 2.0;
    (error.signum() * (error.abs() - deadband).max(0.0) * 0.3).clamp(-0.015, 0.015)
}

/// Prefixes contain ONLY delivered source advances. Underrun silence contributes
/// zero, so a master stall can never manufacture a launch crossing.
pub fn crossing(map: &BeatMap, cursor: Cursor, advances: &[f64], phase: f64) -> Option<usize> {
    let mut cursor = cursor;
    let (beat, _) = map.at(cursor.position)?;
    let mut previous = beat;
    let mut iteration = cursor.iterations;
    let loop_beats = if let Some((start, end)) = cursor.region {
        map.at(end as f64)?.0 - map.at(start as f64)?.0
    } else {
        0.0
    };
    let mut distance = (phase - previous.rem_euclid(1.0)).rem_euclid(1.0);
    if distance < 1e-10 || 1.0 - distance < 1e-10 {
        return advances.iter().position(|&advance| advance > 0.0);
    }
    for (i, &advance) in advances.iter().enumerate() {
        if advance == 0.0 {
            continue;
        }
        cursor.advance(advance);
        let (beat, _) = map.at(cursor.position)?;
        let next = beat;
        let delta = next - previous + (cursor.iterations - iteration) as f64 * loop_beats;
        iteration = cursor.iterations;
        if delta >= distance {
            return Some(i + 1);
        }
        distance -= delta;
        previous = next;
    }
    None
}

/// A phase relationship established at the actual launch boundary. Once
/// aligned, compare delivered musical time at the nominal BPMs: detected
/// onsets can wander or disappear in a breakdown without transport drifting.
/// Track/grid/anchor revisions invalidate this reference before it is reused.
pub struct PhaseLock {
    master: Cursor,
    follower: Cursor,
    master_beat_frames: f64,
    follower_beat_frames: f64,
}

impl PhaseLock {
    pub fn new(
        master: Cursor,
        follower: Cursor,
        master_beat_frames: f64,
        follower_beat_frames: f64,
    ) -> Self {
        Self {
            master,
            follower,
            master_beat_frames,
            follower_beat_frames,
        }
    }

    pub fn phases(&self, master: Cursor, follower: Cursor) -> (f64, f64) {
        let beats = |anchor: Cursor, cursor: Cursor, length: f64| {
            let wraps = cursor.iterations.saturating_sub(anchor.iterations) as f64;
            let loop_frames = anchor
                .region
                .map_or(0.0, |(start, end)| (end - start) as f64);
            // Cursor positions already include the source/output conversion.
            (cursor.position - anchor.position + wraps * loop_frames) / length
        };
        (
            beats(self.master, master, self.master_beat_frames).rem_euclid(1.0),
            beats(self.follower, follower, self.follower_beat_frames).rem_euclid(1.0),
        )
    }
}

#[derive(Default)]
pub struct Lock {
    pub launched: bool,
    pub enabled: bool,
    pub version: [u64; 9],
    pub phase: Option<PhaseLock>,
    pub fading_out: usize,
    pub fading_in: usize,
    pub drift_beats: f64,
    pub nudging: bool,
    pub reprime: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivered_phase_reference_counts_wraps_and_converts_only_once() {
        let mut master = Cursor {
            position: 1234.0,
            ..Cursor::default()
        };
        let mut follower = Cursor {
            position: 350.0,
            region: Some((0, 400)),
            conversion: 2.0,
            ..Cursor::default()
        };
        let lock = PhaseLock::new(master, follower, 100.0, 200.0);
        master.advance(75.0);
        follower.advance(75.0);
        assert_eq!(follower.iterations, 1);
        assert_eq!(lock.phases(master, follower), (0.75, 0.75));
        master.advance(1000.0);
        follower.advance(1000.0);
        assert_eq!(follower.iterations, 6);
        assert_eq!(lock.phases(master, follower), (0.75, 0.75));
        // A stalled follower delivers no source frames and really loses phase.
        master.advance(25.0);
        let (master_phase, follower_phase) = lock.phases(master, follower);
        assert_eq!(phase_error(master_phase, follower_phase), 0.25);
    }

    #[test]
    fn invalid_nominal_bpms_cannot_generate_a_sync_rate() {
        let master = BeatMap::new([0.0, 100.0].into_iter(), 1, 130.0);
        for bpm in [0.0, -120.0, f64::NAN, f64::INFINITY] {
            let follower = BeatMap::new([0.0, 100.0].into_iter(), 1, bpm);
            assert_eq!(follower.matched_rate(&master, 1.0), None);
        }
    }

    #[test]
    fn crossings_follow_delivered_prefixes_within_one_frame() {
        let map = BeatMap::new((0..100).map(|i| i as f64 * 997.0), 1, 120.0);
        for master in 0..23 {
            for follower in 0..29 {
                let start = master as f64 / 23.0 * 997.0;
                let phase = follower as f64 / 29.0;
                let advances: Vec<_> = (0..3000)
                    .map(|i| match i % 3 {
                        0 => 0.0,
                        1 => 0.5,
                        _ => 1.25,
                    })
                    .collect();
                let cursor = Cursor {
                    position: start,
                    ..Cursor::default()
                };
                let at = crossing(&map, cursor, &advances, phase).unwrap();
                let position = start + advances[..at].iter().sum::<f64>();
                let error = phase_error(position / 997.0, phase);
                assert!(
                    error.abs() <= 1.25 / 997.0 + 1e-12,
                    "{master} {follower}: {error}"
                );
                if at > 0 {
                    let before = start + advances[..at - 1].iter().sum::<f64>();
                    let distance = (phase - (start / 997.0)).rem_euclid(1.0) * 997.0;
                    assert!(before - start < distance + 1e-9);
                }
            }
        }
    }

    #[test]
    fn loops_require_whole_beats_and_targets_stay_inside() {
        let map = BeatMap::new((0..10).map(|i| i as f64 * 100.0), 1, 120.0);
        assert!(map.supports_loop(Some((150, 350))));
        assert!(!map.supports_loop(Some((150, 175))));
        assert!(!map.supports_loop(Some((150, 375))));
        let cursor = Cursor {
            position: 155.0,
            region: Some((150, 350)),
            ..Cursor::default()
        };
        assert_eq!(map.aligned_target(cursor, 0.2), Some(320.0));
        let cursor = Cursor {
            position: 349.0,
            region: Some((150, 350)),
            ..Cursor::default()
        };
        assert_eq!(crossing(&map, cursor, &[1.0; 10], 0.52), Some(3));
    }

    #[test]
    fn missing_grid_and_underruns_cannot_invent_crossings() {
        let empty = BeatMap::new([100.0, 100.0].into_iter(), 1, 120.0);
        assert_eq!(empty.at(100.0), None);
        let map = BeatMap::new([0.0, 100.0].into_iter(), 1, 120.0);
        assert_eq!(crossing(&map, Cursor::default(), &[0.0; 128], 0.5), None);
        assert_eq!(crossing(&map, Cursor::default(), &[0.0; 128], 0.0), None);
        assert_eq!(
            crossing(&map, Cursor::default(), &[0.0, 0.0, 1.0], 0.0),
            Some(2)
        );
        assert_eq!(correction(0.005), 0.0);
        assert_eq!(correction(0.5), 0.015);
        assert_eq!(correction(-0.5), -0.015);
        assert!((phase_error(0.1, 0.9) - 0.2).abs() < 1e-12);
    }
}
