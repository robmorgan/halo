//! Real-time time stretching for Master Tempo (key lock) functionality.
//!
//! Uses the Signalsmith Stretch library for high-quality time-stretching
//! without affecting pitch. This enables DJ-style Master Tempo functionality.

use std::collections::VecDeque;

use ssstretch::Stretch;

/// Real-time time stretcher for audio playback.
///
/// Wraps Signalsmith Stretch to provide tempo adjustment without pitch change.
/// Designed for real-time audio processing with sample-by-sample I/O,
/// internally batching for efficient processing.
pub struct TimeStretcher {
    /// Signalsmith Stretch processor instance.
    processor: Stretch,
    /// Sample rate in Hz.
    sample_rate: u32,
    /// Current tempo ratio (1.0 = normal).
    tempo: f64,
    /// Input buffers for left and right channels.
    input_left: Vec<f32>,
    input_right: Vec<f32>,
    /// Output ring buffer for processed stereo samples.
    output_buffer: VecDeque<(f32, f32)>,
    /// Minimum samples to keep in output buffer for smooth playback.
    min_buffer_samples: usize,
    /// Number of input samples to collect before processing.
    input_batch_size: usize,
}

impl TimeStretcher {
    /// Create a new time stretcher.
    ///
    /// - `sample_rate`: Audio sample rate in Hz (e.g., 44100)
    /// - `_channels`: Number of audio channels (ignored, always stereo)
    pub fn new(sample_rate: u32, _channels: u32) -> Self {
        let mut processor = Stretch::new();

        // Configure with larger block size for better quality on complex harmonic content.
        // Phase vocoders need larger blocks for better frequency resolution.
        // - block_samples: 4096 (good balance of quality vs latency, ~93ms at 44.1kHz)
        // - interval_samples: 512 (block/8 for smooth output with good overlap)
        let block_samples = 4096;
        let interval_samples = 512;
        processor.configure(2, block_samples, interval_samples);

        // Calculate input batch size based on block size
        // Process when we have at least one block worth of input
        let input_batch_size = block_samples as usize;

        Self {
            processor,
            sample_rate,
            tempo: 1.0,
            input_left: Vec::with_capacity(input_batch_size * 2),
            input_right: Vec::with_capacity(input_batch_size * 2),
            output_buffer: VecDeque::with_capacity(input_batch_size * 4),
            // Keep ~150ms of buffer for smooth playback at varying tempos
            min_buffer_samples: (sample_rate as usize * 150) / 1000,
            input_batch_size,
        }
    }

    /// Set the tempo ratio.
    ///
    /// - `ratio`: 1.0 = normal speed, 1.1 = 10% faster, 0.9 = 10% slower Supports down to 0.01
    ///   (near-stopped) and up to 2.0 (double speed).
    pub fn set_tempo(&mut self, ratio: f64) {
        // Clamp to valid range: 0.01 (near-stopped) to 2.0 (double speed)
        // This supports ±100% pitch fader range
        let ratio = ratio.clamp(0.01, 2.0);
        if (ratio - self.tempo).abs() > 0.001 {
            self.tempo = ratio;
            // Note: Signalsmith Stretch doesn't have a set_tempo() method.
            // Tempo is controlled by the ratio of output_samples to input_samples
            // in the process_vec() call. We store the ratio and apply it during processing.
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
        self.input_left.push(left);
        self.input_right.push(right);

        // Process when we have enough samples
        if self.input_left.len() >= self.input_batch_size {
            self.process_batch();
        }
    }

    /// Pop a processed stereo sample pair.
    ///
    /// Returns `None` if the output buffer is empty.
    pub fn pop_sample(&mut self) -> Option<(f32, f32)> {
        // If output buffer is low, try to process more input
        if self.output_buffer.len() < self.min_buffer_samples && !self.input_left.is_empty() {
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
    pub fn latency_samples(&self) -> usize {
        self.min_buffer_samples
            + self.processor.input_latency() as usize
            + self.processor.output_latency() as usize
    }

    /// Get the approximate latency in seconds.
    pub fn latency_seconds(&self) -> f64 {
        self.latency_samples() as f64 / self.sample_rate as f64
    }

    /// Flush any remaining samples and reset internal state.
    ///
    /// Call this when seeking or stopping playback.
    pub fn flush(&mut self) {
        // Process any remaining input
        if !self.input_left.is_empty() {
            self.process_batch();
        }

        // Flush the processor
        let flush_samples = 1024;
        let mut output_left = vec![0.0f32; flush_samples];
        let mut output_right = vec![0.0f32; flush_samples];
        let mut output = vec![output_left, output_right];

        self.processor.flush_vec(&mut output, flush_samples as i32);

        // Add flushed samples to output buffer
        for i in 0..flush_samples {
            if output[0][i].abs() > 1e-10 || output[1][i].abs() > 1e-10 {
                self.output_buffer.push_back((output[0][i], output[1][i]));
            }
        }

        self.input_left.clear();
        self.input_right.clear();
    }

    /// Clear all buffers and reset to initial state.
    ///
    /// Call this when loading a new track.
    pub fn reset(&mut self) {
        self.processor.reset();
        self.input_left.clear();
        self.input_right.clear();
        self.output_buffer.clear();
    }

    /// Process buffered input samples through Signalsmith Stretch.
    fn process_batch(&mut self) {
        if self.input_left.is_empty() {
            return;
        }

        let input_len = self.input_left.len();

        // Calculate output length based on tempo
        // tempo > 1.0 means faster playback, so fewer output samples
        // tempo < 1.0 means slower playback, so more output samples
        let output_len = ((input_len as f64) / self.tempo).ceil() as usize;

        // Prepare input as Vec of Vecs (ssstretch API requirement)
        let input = vec![
            std::mem::take(&mut self.input_left),
            std::mem::take(&mut self.input_right),
        ];

        // Prepare output buffers
        let mut output = vec![vec![0.0f32; output_len], vec![0.0f32; output_len]];

        // Process through Signalsmith Stretch
        self.processor
            .process_vec(&input, input_len as i32, &mut output, output_len as i32);

        // Add processed samples to output buffer
        for i in 0..output_len {
            self.output_buffer.push_back((output[0][i], output[1][i]));
        }
    }
}

impl Default for TimeStretcher {
    fn default() -> Self {
        Self::new(44100, 2)
    }
}

// SAFETY: TimeStretcher is only accessed from a single thread at a time.
// The underlying Stretch object contains raw pointers but doesn't share
// state across threads. All operations use &mut self, ensuring exclusive access.
unsafe impl Send for TimeStretcher {}
unsafe impl Sync for TimeStretcher {}

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

        // Lower bound is now 0.01 to support ±100% range
        stretcher.set_tempo(0.005);
        assert!((stretcher.tempo() - 0.01).abs() < 0.001);

        // 0.1 should now be allowed (within range)
        stretcher.set_tempo(0.1);
        assert!((stretcher.tempo() - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_sample_processing() {
        let mut stretcher = TimeStretcher::new(44100, 2);

        // Push enough samples to trigger processing
        for i in 0..2000 {
            let sample = (i as f32 / 1000.0).sin();
            stretcher.push_sample(sample, sample);
        }

        // Should have some output after processing
        let mut output_count = 0;
        while stretcher.pop_sample().is_some() {
            output_count += 1;
        }

        // With tempo 1.0, output should be close to input
        assert!(output_count > 0);
    }

    #[test]
    fn test_tempo_affects_output_length() {
        // Test faster tempo (should produce fewer samples)
        let mut stretcher_fast = TimeStretcher::new(44100, 2);
        stretcher_fast.set_tempo(1.5);

        // Test slower tempo (should produce more samples)
        let mut stretcher_slow = TimeStretcher::new(44100, 2);
        stretcher_slow.set_tempo(0.75);

        let input_samples = 2000;

        // Push same input to both
        for i in 0..input_samples {
            let sample = (i as f32 / 1000.0).sin();
            stretcher_fast.push_sample(sample, sample);
            stretcher_slow.push_sample(sample, sample);
        }

        // Force processing of remaining samples
        stretcher_fast.flush();
        stretcher_slow.flush();

        // Count outputs
        let mut fast_count = 0;
        while stretcher_fast.pop_sample().is_some() {
            fast_count += 1;
        }

        let mut slow_count = 0;
        while stretcher_slow.pop_sample().is_some() {
            slow_count += 1;
        }

        // Faster tempo should produce fewer samples
        // Slower tempo should produce more samples
        assert!(
            fast_count < slow_count,
            "Fast ({}) should be less than slow ({})",
            fast_count,
            slow_count
        );
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
