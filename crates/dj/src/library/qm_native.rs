//! Native QM-DSP TempoTrackV2 bindings via FFI.
//!
//! This module provides a safe Rust wrapper around the vendored QM-DSP C++ library
//! for accurate BPM detection.

use std::ffi::c_int;

/// Opaque handle to QmTempoTracker C++ object
#[repr(C)]
pub struct QmTempoTrackerHandle {
    _private: [u8; 0],
}

extern "C" {
    fn qm_tempo_new(sample_rate: f32, df_increment: c_int) -> *mut QmTempoTrackerHandle;
    fn qm_tempo_free(tracker: *mut QmTempoTrackerHandle);
    fn qm_tempo_calculate_beat_period(
        tracker: *mut QmTempoTrackerHandle,
        df: *const f64,
        df_len: c_int,
        beat_periods: *mut f64,
        tempi: *mut f64,
        out_len: *mut c_int,
    ) -> c_int;
    fn qm_tempo_calculate_beats(
        tracker: *mut QmTempoTrackerHandle,
        df: *const f64,
        df_len: c_int,
        beat_periods: *const f64,
        bp_len: c_int,
        beats: *mut f64,
        beats_len: *mut c_int,
    ) -> c_int;
}

/// Safe Rust wrapper around QM-DSP TempoTrackV2.
///
/// # Example
/// ```ignore
/// let mut tracker = NativeTempoTracker::new(44100.0, 512);
/// let (beat_periods, tempi) = tracker.calculate_beat_period(&detection_function);
/// let median_bpm = tempi.iter().sum::<f64>() / tempi.len() as f64;
/// ```
pub struct NativeTempoTracker {
    handle: *mut QmTempoTrackerHandle,
}

impl NativeTempoTracker {
    /// Create a new tempo tracker.
    ///
    /// # Arguments
    /// * `sample_rate` - Audio sample rate (e.g., 44100.0)
    /// * `df_increment` - Detection function frame increment (e.g., 512)
    ///
    /// # Panics
    /// Panics if the C++ tracker creation fails (out of memory).
    pub fn new(sample_rate: f32, df_increment: i32) -> Self {
        let handle = unsafe { qm_tempo_new(sample_rate, df_increment) };
        assert!(!handle.is_null(), "Failed to create QM-DSP tempo tracker");
        Self { handle }
    }

    /// Calculate beat periods and tempi from a detection function.
    ///
    /// The detection function should be computed from audio samples using
    /// an onset detection algorithm.
    ///
    /// # Returns
    /// A tuple of (beat_periods, tempi) where:
    /// - beat_periods: Beat period in detection function frames
    /// - tempi: Tempo in BPM at each frame
    pub fn calculate_beat_period(&mut self, df: &[f64]) -> (Vec<f64>, Vec<f64>) {
        if df.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut beat_periods = vec![0.0; df.len()];
        let mut tempi = vec![0.0; df.len()];
        let mut out_len: c_int = 0;

        let result = unsafe {
            qm_tempo_calculate_beat_period(
                self.handle,
                df.as_ptr(),
                df.len() as c_int,
                beat_periods.as_mut_ptr(),
                tempi.as_mut_ptr(),
                &mut out_len,
            )
        };

        if result != 0 {
            log::error!("qm_tempo_calculate_beat_period failed");
            return (Vec::new(), Vec::new());
        }

        beat_periods.truncate(out_len as usize);
        tempi.truncate(out_len as usize);
        (beat_periods, tempi)
    }

    /// Calculate beat positions from detection function and beat periods.
    ///
    /// # Returns
    /// Beat positions in detection function frame units.
    pub fn calculate_beats(&mut self, df: &[f64], beat_periods: &[f64]) -> Vec<f64> {
        if df.is_empty() || beat_periods.is_empty() {
            return Vec::new();
        }

        let mut beats = vec![0.0; df.len()];
        let mut beats_len: c_int = 0;

        let result = unsafe {
            qm_tempo_calculate_beats(
                self.handle,
                df.as_ptr(),
                df.len() as c_int,
                beat_periods.as_ptr(),
                beat_periods.len() as c_int,
                beats.as_mut_ptr(),
                &mut beats_len,
            )
        };

        if result != 0 {
            log::error!("qm_tempo_calculate_beats failed");
            return Vec::new();
        }

        beats.truncate(beats_len as usize);
        beats
    }

    /// Calculate both beat periods and beat positions in one call.
    ///
    /// This is a convenience method that calls `calculate_beat_period` and
    /// `calculate_beats` in sequence.
    ///
    /// # Returns
    /// A tuple of (beat_periods, tempi, beat_positions)
    pub fn analyze(&mut self, df: &[f64]) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        let (beat_periods, tempi) = self.calculate_beat_period(df);
        let beats = self.calculate_beats(df, &beat_periods);
        (beat_periods, tempi, beats)
    }
}

impl Drop for NativeTempoTracker {
    fn drop(&mut self) {
        unsafe { qm_tempo_free(self.handle) }
    }
}

// SAFETY: The C++ TempoTrackV2 object is not shared between threads
// and our wrapper provides exclusive access through &mut self.
unsafe impl Send for NativeTempoTracker {}

/// Compute median tempo from a tempi array.
///
/// This is the standard way to extract a single BPM value from the
/// per-frame tempo estimates.
pub fn median_tempo(tempi: &[f64]) -> f64 {
    if tempi.is_empty() {
        return 120.0; // Default fallback
    }

    let mut sorted: Vec<f64> = tempi.iter().copied().filter(|&t| t > 0.0).collect();
    if sorted.is_empty() {
        return 120.0;
    }

    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    sorted[sorted.len() / 2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_creation() {
        let tracker = NativeTempoTracker::new(44100.0, 512);
        drop(tracker);
    }

    #[test]
    fn test_empty_input() {
        let mut tracker = NativeTempoTracker::new(44100.0, 512);
        let (bp, tempi) = tracker.calculate_beat_period(&[]);
        assert!(bp.is_empty());
        assert!(tempi.is_empty());
    }

    #[test]
    fn test_median_tempo() {
        assert_eq!(median_tempo(&[]), 120.0);
        assert_eq!(median_tempo(&[100.0]), 100.0);
        assert_eq!(median_tempo(&[100.0, 120.0, 140.0]), 120.0);
    }
}
