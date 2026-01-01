//! Real-time time stretching for Master Tempo (key lock) functionality.
//!
//! Uses the SoundTouch library (WSOLA algorithm) to change tempo without
//! affecting pitch. This enables DJ-style Master Tempo functionality.

use std::collections::VecDeque;

use soundtouch::SoundTouch;

/// Wrapper around SoundTouch that implements Send + Sync.
struct SoundTouchWrapper(SoundTouch);

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
pub struct TimeStretcher {
    processor: SoundTouchWrapper,
    sample_rate: u32,
    tempo: f64,
    /// Output buffer for processed stereo samples.
    output_buffer: VecDeque<(f32, f32)>,
    /// Temp buffer for receiving from SoundTouch.
    receive_buffer: Vec<f32>,
}

impl TimeStretcher {
    /// Create a new time stretcher.
    pub fn new(sample_rate: u32, channels: u32) -> Self {
        let mut processor = SoundTouchWrapper::new();

        processor.set_sample_rate(sample_rate);
        processor.set_channels(channels);

        log::info!(
            "TimeStretcher initialized: {} Hz, {} channels",
            sample_rate,
            channels
        );

        Self {
            processor,
            sample_rate,
            tempo: 1.0,
            output_buffer: VecDeque::with_capacity(8192),
            receive_buffer: vec![0.0f32; 4096],
        }
    }

    /// Set the tempo ratio (1.0 = normal speed).
    pub fn set_tempo(&mut self, ratio: f64) {
        let ratio = ratio.clamp(0.01, 2.0);
        if (ratio - self.tempo).abs() > 0.001 {
            self.tempo = ratio;
            self.processor.set_tempo(ratio);
            log::debug!("TimeStretcher: tempo set to {:.4}", ratio);
        }
    }

    /// Get the current tempo ratio.
    pub fn tempo(&self) -> f64 {
        self.tempo
    }

    /// Push a stereo sample pair and immediately try to get output.
    pub fn push_sample(&mut self, left: f32, right: f32) {
        // Feed one stereo frame to SoundTouch
        let input = [left, right];
        self.processor.put_samples(&input, 1);

        // Try to receive any available output
        self.receive_samples();
    }

    /// Pop a processed stereo sample pair.
    pub fn pop_sample(&mut self) -> Option<(f32, f32)> {
        // Try to get more samples if buffer is low
        if self.output_buffer.len() < 100 {
            self.receive_samples();
        }
        self.output_buffer.pop_front()
    }

    /// Receive available samples from SoundTouch.
    fn receive_samples(&mut self) {
        loop {
            let received = self
                .processor
                .receive_samples(&mut self.receive_buffer, 1024);
            if received == 0 {
                break;
            }
            for i in 0..received {
                let left = self.receive_buffer[i * 2];
                let right = self.receive_buffer[i * 2 + 1];
                self.output_buffer.push_back((left, right));
            }
        }
    }

    /// Check if there are samples available.
    pub fn has_output(&self) -> bool {
        !self.output_buffer.is_empty()
    }

    /// Get the number of buffered output samples.
    pub fn output_len(&self) -> usize {
        self.output_buffer.len()
    }

    /// Flush remaining samples.
    pub fn flush(&mut self) {
        self.processor.flush();
        self.receive_samples();
    }

    /// Reset to initial state.
    pub fn reset(&mut self) {
        self.processor.clear();
        self.output_buffer.clear();
    }

    /// Get latency in samples.
    pub fn latency_samples(&self) -> usize {
        self.processor.num_unprocessed_samples() as usize
    }

    /// Get latency in seconds.
    pub fn latency_seconds(&self) -> f64 {
        self.latency_samples() as f64 / self.sample_rate as f64
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
    }

    #[test]
    fn test_tempo_setting() {
        let mut stretcher = TimeStretcher::new(44100, 2);

        stretcher.set_tempo(1.1);
        assert!((stretcher.tempo() - 1.1).abs() < 0.001);

        stretcher.set_tempo(0.9);
        assert!((stretcher.tempo() - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_sample_processing() {
        let mut stretcher = TimeStretcher::new(44100, 2);

        // Push samples
        for i in 0..2000 {
            let sample = (i as f32 / 1000.0).sin();
            stretcher.push_sample(sample, sample);
        }

        stretcher.flush();

        // Should have output
        let mut count = 0;
        while stretcher.pop_sample().is_some() {
            count += 1;
        }
        assert!(count > 0);
    }
}
