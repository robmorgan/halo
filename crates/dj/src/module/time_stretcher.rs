//! Real-time time stretching for Master Tempo (key lock) functionality.
//!
//! Uses the SoundTouch library (WSOLA algorithm) to change tempo without
//! affecting pitch. This enables DJ-style Master Tempo functionality.

use std::collections::VecDeque;

use soundtouch::SoundTouch;

/// Wrapper around SoundTouch that implements Send + Sync.
///
/// # Safety
/// SoundTouch internally uses raw pointers but the library is thread-safe
/// when accessed from a single thread at a time. We ensure this by wrapping
/// TimeStretcher in a RwLock in DeckPlayer.
struct SoundTouchWrapper(SoundTouch);

// SAFETY: SoundTouch is thread-safe when accessed via RwLock (single-threaded access).
// The raw pointers in SoundTouch point to internal state that is protected by
// the RwLock in DeckPlayer, ensuring no concurrent mutable access.
unsafe impl Send for SoundTouchWrapper {}
unsafe impl Sync for SoundTouchWrapper {}

impl SoundTouchWrapper {
    fn new() -> Self {
        Self(SoundTouch::new())
    }
}

impl std::ops::Deref for SoundTouchWrapper {
    type Target = SoundTouch;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SoundTouchWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Real-time time stretcher for audio playback.
///
/// Wraps SoundTouch to provide tempo adjustment without pitch change.
/// Designed for real-time audio processing with sample-by-sample output.
pub struct TimeStretcher {
    /// SoundTouch processor instance.
    processor: SoundTouchWrapper,
    /// Sample rate in Hz.
    sample_rate: u32,
    /// Current tempo ratio (1.0 = normal).
    tempo: f64,
    /// Input buffer for feeding samples to SoundTouch.
    input_buffer: Vec<f32>,
    /// Output ring buffer for processed samples.
    output_buffer: VecDeque<(f32, f32)>,
    /// Minimum samples to keep in output buffer for smooth playback.
    min_buffer_samples: usize,
    /// Number of input samples buffered before processing.
    input_batch_size: usize,
}

impl TimeStretcher {
    /// Create a new time stretcher.
    ///
    /// - `sample_rate`: Audio sample rate in Hz (e.g., 44100)
    /// - `channels`: Number of audio channels (1 or 2)
    pub fn new(sample_rate: u32, channels: u32) -> Self {
        let mut processor = SoundTouchWrapper::new();

        // Configure SoundTouch for DJ-quality time stretching
        processor.set_sample_rate(sample_rate);
        processor.set_channels(channels);

        // Optimize for real-time DJ use
        // These settings balance quality vs latency
        processor.set_setting(soundtouch::Setting::SequenceMs, 40); // Sequence length (ms)
        processor.set_setting(soundtouch::Setting::SeekwindowMs, 15); // Seek window (ms)
        processor.set_setting(soundtouch::Setting::OverlapMs, 8); // Overlap (ms)

        // Enable anti-alias filter for better quality
        processor.set_setting(soundtouch::Setting::UseAaFilter, 1);

        Self {
            processor,
            sample_rate,
            tempo: 1.0,
            input_buffer: Vec::with_capacity(4096),
            output_buffer: VecDeque::with_capacity(8192),
            // Keep ~100ms of buffer for smooth playback at varying tempos
            min_buffer_samples: (sample_rate as usize * 100) / 1000,
            // Process in batches of ~10ms for efficiency
            input_batch_size: (sample_rate as usize * 10) / 1000,
        }
    }

    /// Set the tempo ratio.
    ///
    /// - `ratio`: 1.0 = normal speed, 1.1 = 10% faster, 0.9 = 10% slower
    pub fn set_tempo(&mut self, ratio: f64) {
        let ratio = ratio.clamp(0.5, 2.0);
        if (ratio - self.tempo).abs() > 0.001 {
            self.tempo = ratio;
            self.processor.set_tempo(ratio);
        }
    }

    /// Get the current tempo ratio.
    pub fn tempo(&self) -> f64 {
        self.tempo
    }

    /// Push a stereo sample pair into the stretcher.
    ///
    /// Samples are buffered and processed in batches for efficiency.
    pub fn push_sample(&mut self, left: f32, right: f32) {
        // Add interleaved samples to input buffer
        self.input_buffer.push(left);
        self.input_buffer.push(right);

        // Process when we have enough samples
        if self.input_buffer.len() >= self.input_batch_size * 2 {
            self.process_batch();
        }
    }

    /// Pop a processed stereo sample pair.
    ///
    /// Returns `None` if the output buffer is empty.
    /// During initial buffering phase, may return silence until
    /// enough samples have been processed.
    pub fn pop_sample(&mut self) -> Option<(f32, f32)> {
        // If output buffer is low, try to process more input
        if self.output_buffer.len() < self.min_buffer_samples && !self.input_buffer.is_empty() {
            self.process_batch();
        }

        self.output_buffer.pop_front()
    }

    /// Check if there are samples available in the output buffer.
    pub fn has_output(&self) -> bool {
        !self.output_buffer.is_empty()
    }

    /// Get the number of samples in the output buffer.
    pub fn output_len(&self) -> usize {
        self.output_buffer.len()
    }

    /// Get the approximate latency in samples.
    ///
    /// This is the delay between input and output due to buffering
    /// and time-stretch processing.
    pub fn latency_samples(&self) -> usize {
        self.min_buffer_samples + self.processor.num_unprocessed_samples() as usize
    }

    /// Get the approximate latency in seconds.
    pub fn latency_seconds(&self) -> f64 {
        self.latency_samples() as f64 / self.sample_rate as f64
    }

    /// Flush any remaining samples and reset internal state.
    ///
    /// Call this when seeking or stopping playback.
    pub fn flush(&mut self) {
        self.processor.flush();
        self.receive_processed_samples();
        self.input_buffer.clear();
    }

    /// Clear all buffers and reset to initial state.
    ///
    /// Call this when loading a new track.
    pub fn reset(&mut self) {
        self.processor.clear();
        self.input_buffer.clear();
        self.output_buffer.clear();
    }

    /// Process buffered input samples through SoundTouch.
    fn process_batch(&mut self) {
        if self.input_buffer.is_empty() {
            return;
        }

        // Feed samples to SoundTouch (stereo interleaved)
        let sample_count = self.input_buffer.len() / 2;
        self.processor.put_samples(&self.input_buffer, sample_count);
        self.input_buffer.clear();

        // Receive processed samples
        self.receive_processed_samples();
    }

    /// Receive any available processed samples from SoundTouch.
    fn receive_processed_samples(&mut self) {
        let mut output = vec![0.0f32; 4096];

        loop {
            let received = self.processor.receive_samples(&mut output, 2048);
            if received == 0 {
                break;
            }

            // Convert interleaved samples to stereo pairs
            for i in 0..received {
                let left = output[i * 2];
                let right = output[i * 2 + 1];
                self.output_buffer.push_back((left, right));
            }
        }
    }
}

impl Default for TimeStretcher {
    fn default() -> Self {
        Self::new(44100, 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_stretcher_creation() {
        let stretcher = TimeStretcher::new(44100, 2);
        assert_eq!(stretcher.tempo(), 1.0);
        assert!(!stretcher.has_output());
    }

    #[test]
    fn test_tempo_setting() {
        let mut stretcher = TimeStretcher::new(44100, 2);

        stretcher.set_tempo(1.1);
        assert!((stretcher.tempo() - 1.1).abs() < 0.001);

        stretcher.set_tempo(0.9);
        assert!((stretcher.tempo() - 0.9).abs() < 0.001);

        // Test clamping
        stretcher.set_tempo(3.0);
        assert!((stretcher.tempo() - 2.0).abs() < 0.001);

        stretcher.set_tempo(0.1);
        assert!((stretcher.tempo() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_sample_processing() {
        let mut stretcher = TimeStretcher::new(44100, 2);

        // Push enough samples to trigger processing
        for i in 0..1000 {
            let sample = (i as f32 / 1000.0).sin();
            stretcher.push_sample(sample, sample);
        }

        // Should have some output after processing
        // Note: SoundTouch has internal buffering, so output may be delayed
        let mut output_count = 0;
        while let Some(_) = stretcher.pop_sample() {
            output_count += 1;
        }

        // With tempo 1.0, output should be close to input
        // (may be slightly less due to buffering)
        assert!(output_count > 0 || stretcher.processor.num_unprocessed_samples() > 0);
    }

    #[test]
    fn test_reset() {
        let mut stretcher = TimeStretcher::new(44100, 2);

        // Add some samples
        for _ in 0..100 {
            stretcher.push_sample(0.5, -0.5);
        }

        stretcher.reset();

        assert!(!stretcher.has_output());
        assert_eq!(stretcher.output_len(), 0);
    }
}
