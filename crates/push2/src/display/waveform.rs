//! Waveform visualization for Push 2 display.

/// Waveform renderer for DJ decks.
///
/// Displays an overview waveform with position indicator,
/// cue points, and hot cue markers.
pub struct WaveformRenderer {
    /// Cached waveform data (amplitude samples)
    waveform_data: Vec<f32>,

    /// Track duration in seconds
    duration_seconds: f64,
}

impl WaveformRenderer {
    /// Create a new waveform renderer.
    pub fn new() -> Self {
        Self {
            waveform_data: Vec::new(),
            duration_seconds: 0.0,
        }
    }

    /// Set waveform data for a track.
    pub fn set_waveform(&mut self, data: Vec<f32>, duration: f64) {
        self.waveform_data = data;
        self.duration_seconds = duration;
    }

    /// Clear waveform data.
    pub fn clear(&mut self) {
        self.waveform_data.clear();
        self.duration_seconds = 0.0;
    }

    /// Check if waveform data is loaded.
    pub fn has_data(&self) -> bool {
        !self.waveform_data.is_empty()
    }

    /// Get amplitude at a given position (0.0-1.0 of track duration).
    pub fn amplitude_at(&self, position: f64) -> f32 {
        if self.waveform_data.is_empty() || position < 0.0 || position > 1.0 {
            return 0.0;
        }

        let index = (position * (self.waveform_data.len() - 1) as f64) as usize;
        self.waveform_data.get(index).copied().unwrap_or(0.0)
    }
}

impl Default for WaveformRenderer {
    fn default() -> Self {
        Self::new()
    }
}
