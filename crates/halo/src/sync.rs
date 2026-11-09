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

    /// The playback rate that plays this track at `bpm`.
    pub fn rate_for(&self, bpm: f64) -> Option<f64> {
        let rate = bpm / self.bpm;
        (self.bpm.is_finite()
            && self.bpm > 0.0
            && bpm.is_finite()
            && bpm > 0.0
            && rate.is_finite()
            && rate > 0.0)
            .then_some(rate)
    }

    /// Only used after rate_for has validated the nominal BPM.
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

/// The master clock over one output block: where it starts, in beats, and
/// how far it moves per output frame at its tempo. The clock never stops,
/// stalls or bends, so a synced deck's reference is exact.
#[derive(Clone, Copy, Debug)]
pub struct ClockBlock {
    pub beats: f64,
    pub beats_per_frame: f64,
    pub bpm: f64,
}

impl ClockBlock {
    pub fn new(beats: f64, bpm: f64, sample_rate: u32) -> Self {
        Self {
            beats,
            beats_per_frame: bpm / 60.0 / sample_rate as f64,
            bpm,
        }
    }

    /// The clock's beat position `frames` into the block.
    pub fn at(&self, frames: usize) -> f64 {
        self.beats + frames as f64 * self.beats_per_frame
    }
}

/// Output frames into `clock`'s block until it reaches beat `phase`, if
/// that's within `frames`. Already on the beat launches at once.
pub fn clock_crossing(clock: &ClockBlock, frames: usize, phase: f64) -> Option<usize> {
    if clock.beats_per_frame.is_nan() || clock.beats_per_frame <= 0.0 {
        return None;
    }
    let distance = (phase - clock.beats.rem_euclid(1.0)).rem_euclid(1.0);
    if distance < 1e-10 || 1.0 - distance < 1e-10 {
        return Some(0);
    }
    let offset = (distance / clock.beats_per_frame).ceil() as usize;
    (offset <= frames).then_some(offset)
}

/// A phase relationship established at the actual launch boundary. Once
/// aligned, compare the clock's beats with the deck's delivered musical time
/// at its nominal BPM: detected onsets can wander or disappear in a
/// breakdown without transport drifting. Track/grid/anchor revisions
/// invalidate this reference before it is reused.
pub struct PhaseLock {
    clock: f64,
    follower: Cursor,
    follower_beat_frames: f64,
}

impl PhaseLock {
    pub fn new(clock: f64, follower: Cursor, follower_beat_frames: f64) -> Self {
        Self {
            clock,
            follower,
            follower_beat_frames,
        }
    }

    /// (clock, deck) phases since the launch.
    pub fn phases(&self, clock: f64, follower: Cursor) -> (f64, f64) {
        let anchor = self.follower;
        let wraps = follower.iterations.saturating_sub(anchor.iterations) as f64;
        let loop_frames = anchor
            .region
            .map_or(0.0, |(start, end)| (end - start) as f64);
        // Cursor positions already include the source/output conversion.
        let beats =
            (follower.position - anchor.position + wraps * loop_frames) / self.follower_beat_frames;
        ((clock - self.clock).rem_euclid(1.0), beats.rem_euclid(1.0))
    }
}

#[derive(Default)]
pub struct Lock {
    pub launched: bool,
    pub enabled: bool,
    pub version: [u64; 4],
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
        let mut follower = Cursor {
            position: 350.0,
            region: Some((0, 400)),
            conversion: 2.0,
            ..Cursor::default()
        };
        let lock = PhaseLock::new(10.5, follower, 200.0);
        follower.advance(75.0);
        assert_eq!(follower.iterations, 1);
        assert_eq!(lock.phases(11.25, follower), (0.75, 0.75));
        follower.advance(1000.0);
        assert_eq!(follower.iterations, 6);
        assert_eq!(lock.phases(16.25, follower), (0.75, 0.75));
        // A stalled follower delivers no source frames and really loses phase.
        let (clock, follower_phase) = lock.phases(16.5, follower);
        assert_eq!(phase_error(clock, follower_phase), 0.25);
    }

    #[test]
    fn a_lock_holds_through_a_master_bpm_change() {
        // A 120 BPM track (24 000 frames a beat at 48 kHz), locked on the
        // clock's beat, then the master BPM moves from 120 to 126 mid-beat.
        let track = BeatMap::new((0..100).map(|i| i as f64 * 24_000.0), 1, 120.0);
        let mut clock = ClockBlock::new(4.0, 120.0, 48_000);
        let mut deck = Cursor {
            position: 48_000.0,
            ..Cursor::default()
        };
        let lock = PhaseLock::new(clock.beats, deck, track.beat_frames(48_000));
        for (bpm, frames) in [(120.0, 10_000), (126.0, 30_000), (126.0, 777)] {
            clock = ClockBlock::new(clock.beats, bpm, 48_000);
            let rate = track.rate_for(bpm).unwrap();
            for _ in 0..frames {
                deck.advance(rate);
            }
            clock.beats = clock.at(frames);
            let (clock_phase, deck_phase) = lock.phases(clock.beats, deck);
            assert!(
                phase_error(clock_phase, deck_phase).abs() < 1e-9,
                "{bpm}: {clock_phase} vs {deck_phase}"
            );
        }
    }

    #[test]
    fn invalid_nominal_bpms_cannot_generate_a_sync_rate() {
        for bpm in [0.0, -120.0, f64::NAN, f64::INFINITY] {
            let track = BeatMap::new([0.0, 100.0].into_iter(), 1, bpm);
            assert_eq!(track.rate_for(128.0), None);
            let track = BeatMap::new([0.0, 100.0].into_iter(), 1, 120.0);
            assert_eq!(track.rate_for(bpm), None);
        }
        let track = BeatMap::new([0.0, 100.0].into_iter(), 1, 120.0);
        assert_eq!(track.rate_for(132.0), Some(1.1));
    }

    #[test]
    fn clock_crossings_land_within_a_frame() {
        for start in 0..23 {
            for target in 0..29 {
                let clock = ClockBlock::new(7.0 + start as f64 / 23.0, 128.0, 48_000);
                let phase = target as f64 / 29.0;
                // A beat at 128 BPM is 22 500 frames: always found in one.
                let at = clock_crossing(&clock, 22_500, phase).unwrap();
                // At the launch frame the clock has just reached the phase…
                let error = phase_error(clock.at(at), phase);
                assert!(
                    (-1e-9..=clock.beats_per_frame + 1e-9).contains(&error),
                    "{start} {target}: {error}"
                );
                // …and a frame earlier it hadn't.
                if at > 0 {
                    let before = phase_error(clock.at(at - 1), phase);
                    assert!(before < 1e-9, "{start} {target}: {before}");
                }
            }
        }
        // On the beat launches at once; too far away waits for a later block.
        let clock = ClockBlock::new(3.25, 120.0, 48_000);
        assert_eq!(clock_crossing(&clock, 64, 0.25), Some(0));
        assert_eq!(clock_crossing(&clock, 64, 0.75), None);
        let stopped = ClockBlock::new(3.25, 0.0, 48_000);
        assert_eq!(clock_crossing(&stopped, 64, 0.5), None);
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
    }

    #[test]
    fn missing_grids_and_small_errors_stay_quiet() {
        let empty = BeatMap::new([100.0, 100.0].into_iter(), 1, 120.0);
        assert_eq!(empty.at(100.0), None);
        assert_eq!(correction(0.005), 0.0);
        assert_eq!(correction(0.5), 0.015);
        assert_eq!(correction(-0.5), -0.015);
        assert!((phase_error(0.1, 0.9) - 0.2).abs() < 1e-12);
    }
}
