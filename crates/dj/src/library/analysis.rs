//! Audio analysis for BPM detection and beat grid generation.
//!
//! Uses FFT-based onset detection to identify beats and calculate BPM.

use std::fs::File;
use std::path::Path;

use chrono::Utc;
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::types::{BeatGrid, TrackId, TrackWaveform};

/// Analysis configuration.
#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    /// FFT window size for spectral analysis.
    pub fft_size: usize,
    /// Hop size between FFT windows.
    pub hop_size: usize,
    /// Minimum BPM to detect.
    pub min_bpm: f64,
    /// Maximum BPM to detect.
    pub max_bpm: f64,
    /// Number of waveform samples to generate.
    pub waveform_samples: usize,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            fft_size: 2048,
            hop_size: 512,
            min_bpm: 60.0,
            max_bpm: 200.0,
            waveform_samples: 1000,
        }
    }
}

/// Result of audio analysis.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// Detected beat grid.
    pub beat_grid: BeatGrid,
    /// Generated waveform.
    pub waveform: TrackWaveform,
}

/// Analyze an audio file for BPM and beat grid.
pub fn analyze_file<P: AsRef<Path>>(
    path: P,
    track_id: TrackId,
    config: &AnalysisConfig,
) -> Result<AnalysisResult, anyhow::Error> {
    let path = path.as_ref();
    log::info!("Analyzing file: {:?}", path);

    // Load audio samples
    let (samples, sample_rate) = load_audio_samples(path)?;
    log::debug!("Loaded {} samples at {} Hz", samples.len(), sample_rate);

    // Generate waveform for visualization
    let waveform = generate_waveform(&samples, sample_rate, track_id, config.waveform_samples);

    // Detect BPM using autocorrelation
    let (bpm, confidence) = detect_bpm(&samples, sample_rate, config);
    log::info!("Detected BPM: {:.2} (confidence: {:.2})", bpm, confidence);

    // Find first beat offset
    let first_beat_offset_ms = find_first_beat(&samples, sample_rate, bpm);
    log::debug!("First beat offset: {:.2} ms", first_beat_offset_ms);

    // Generate beat positions
    let duration_seconds = samples.len() as f64 / sample_rate as f64;
    let beat_interval = 60.0 / bpm;
    let first_beat_seconds = first_beat_offset_ms / 1000.0;

    let mut beat_positions = Vec::new();
    let mut pos = first_beat_seconds;
    while pos < duration_seconds {
        beat_positions.push(pos);
        pos += beat_interval;
    }

    let beat_grid = BeatGrid {
        track_id,
        bpm,
        first_beat_offset_ms,
        beat_positions,
        confidence,
        analyzed_at: Utc::now(),
        algorithm_version: "1.0".to_string(),
    };

    Ok(AnalysisResult {
        beat_grid,
        waveform,
    })
}

/// Load audio samples from a file (mono, normalized to -1.0 to 1.0).
fn load_audio_samples<P: AsRef<Path>>(path: P) -> Result<(Vec<f32>, u32), anyhow::Error> {
    let path = path.as_ref();
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow::anyhow!("No audio track found"))?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);

    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let mut samples = Vec::new();

    // Decode all packets
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => {
                log::warn!("Error reading packet: {}", e);
                break;
            }
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                // Convert to mono f32
                append_mono_samples(&mut samples, &decoded);
            }
            Err(e) => {
                log::warn!("Error decoding: {}", e);
            }
        }
    }

    Ok((samples, sample_rate))
}

/// Append decoded audio to the sample buffer (converting to mono).
fn append_mono_samples(samples: &mut Vec<f32>, decoded: &AudioBufferRef) {
    match decoded {
        AudioBufferRef::F32(buf) => {
            let channels = buf.spec().channels.count();
            for frame in 0..buf.frames() {
                let mut sum = 0.0;
                for ch in 0..channels {
                    sum += buf.chan(ch)[frame];
                }
                samples.push(sum / channels as f32);
            }
        }
        AudioBufferRef::S16(buf) => {
            let channels = buf.spec().channels.count();
            for frame in 0..buf.frames() {
                let mut sum = 0.0;
                for ch in 0..channels {
                    sum += buf.chan(ch)[frame] as f32 / 32768.0;
                }
                samples.push(sum / channels as f32);
            }
        }
        AudioBufferRef::S32(buf) => {
            let channels = buf.spec().channels.count();
            for frame in 0..buf.frames() {
                let mut sum = 0.0;
                for ch in 0..channels {
                    sum += buf.chan(ch)[frame] as f32 / 2147483648.0;
                }
                samples.push(sum / channels as f32);
            }
        }
        _ => {}
    }
}

/// Detect BPM using autocorrelation.
fn detect_bpm(samples: &[f32], sample_rate: u32, config: &AnalysisConfig) -> (f64, f32) {
    if samples.len() < config.fft_size * 2 {
        return (120.0, 0.0); // Default to 120 BPM if not enough samples
    }

    // Calculate onset strength function using spectral flux
    let onset_env = calculate_onset_envelope(samples, config);

    if onset_env.is_empty() {
        return (120.0, 0.0);
    }

    // Calculate autocorrelation of onset envelope
    let onset_rate = sample_rate as f64 / config.hop_size as f64;
    let min_lag = (60.0 * onset_rate / config.max_bpm) as usize;
    let max_lag = (60.0 * onset_rate / config.min_bpm) as usize;

    let autocorr = autocorrelation(&onset_env, max_lag);

    // Find peak in autocorrelation within BPM range
    let mut best_lag = min_lag;
    let mut best_value = 0.0;

    for lag in min_lag..max_lag.min(autocorr.len()) {
        if autocorr[lag] > best_value {
            best_value = autocorr[lag];
            best_lag = lag;
        }
    }

    // Convert lag to BPM
    let bpm = 60.0 * onset_rate / best_lag as f64;

    // Calculate confidence based on autocorrelation strength
    let max_autocorr = autocorr.iter().cloned().fold(0.0_f32, f32::max);
    let confidence = if max_autocorr > 0.0 {
        (best_value / max_autocorr).min(1.0)
    } else {
        0.0
    };

    (bpm, confidence)
}

/// Calculate onset envelope using spectral flux.
fn calculate_onset_envelope(samples: &[f32], config: &AnalysisConfig) -> Vec<f32> {
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(config.fft_size);

    let mut onset_env = Vec::new();
    let mut prev_spectrum = vec![0.0f32; config.fft_size / 2 + 1];

    let window: Vec<f32> = (0..config.fft_size)
        .map(|i| {
            0.5 * (1.0
                - (2.0 * std::f32::consts::PI * i as f32 / (config.fft_size - 1) as f32).cos())
        })
        .collect();

    for start in (0..samples.len().saturating_sub(config.fft_size)).step_by(config.hop_size) {
        // Apply window and compute FFT
        let mut buffer: Vec<Complex<f32>> = samples[start..start + config.fft_size]
            .iter()
            .zip(window.iter())
            .map(|(s, w)| Complex::new(s * w, 0.0))
            .collect();

        fft.process(&mut buffer);

        // Calculate magnitude spectrum
        let spectrum: Vec<f32> = buffer[..config.fft_size / 2 + 1]
            .iter()
            .map(|c| c.norm())
            .collect();

        // Calculate spectral flux (half-wave rectified difference)
        let flux: f32 = spectrum
            .iter()
            .zip(prev_spectrum.iter())
            .map(|(curr, prev)| (curr - prev).max(0.0))
            .sum();

        onset_env.push(flux);
        prev_spectrum = spectrum;
    }

    onset_env
}

/// Calculate autocorrelation of a signal.
fn autocorrelation(signal: &[f32], max_lag: usize) -> Vec<f32> {
    let n = signal.len();
    let mut result = vec![0.0; max_lag];

    for lag in 0..max_lag {
        let mut sum = 0.0;
        for i in 0..n - lag {
            sum += signal[i] * signal[i + lag];
        }
        result[lag] = sum / (n - lag) as f32;
    }

    result
}

/// Find the offset to the first beat.
fn find_first_beat(samples: &[f32], sample_rate: u32, bpm: f64) -> f64 {
    // Simple approach: find first significant onset
    let config = AnalysisConfig::default();
    let onset_env = calculate_onset_envelope(samples, &config);

    if onset_env.is_empty() {
        return 0.0;
    }

    // Find threshold (mean + 1.5 * std deviation)
    let mean: f32 = onset_env.iter().sum::<f32>() / onset_env.len() as f32;
    let variance: f32 =
        onset_env.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / onset_env.len() as f32;
    let std_dev = variance.sqrt();
    let threshold = mean + 1.5 * std_dev;

    // Find first onset above threshold
    for (i, &value) in onset_env.iter().enumerate() {
        if value > threshold {
            let time_seconds = (i * config.hop_size) as f64 / sample_rate as f64;
            return time_seconds * 1000.0; // Convert to ms
        }
    }

    0.0
}

/// Generate waveform for visualization.
fn generate_waveform(
    samples: &[f32],
    sample_rate: u32,
    track_id: TrackId,
    target_samples: usize,
) -> TrackWaveform {
    if samples.is_empty() {
        return TrackWaveform {
            track_id,
            samples: vec![0.0; target_samples],
            sample_count: target_samples,
            duration_seconds: 0.0,
        };
    }

    let duration_seconds = samples.len() as f64 / sample_rate as f64;
    let samples_per_bucket = samples.len() / target_samples.max(1);

    let waveform_samples: Vec<f32> = (0..target_samples)
        .map(|i| {
            let start = i * samples_per_bucket;
            let end = ((i + 1) * samples_per_bucket).min(samples.len());

            if start >= samples.len() {
                return 0.0;
            }

            // Find peak in this bucket
            samples[start..end]
                .iter()
                .map(|s| s.abs())
                .fold(0.0f32, f32::max)
        })
        .collect();

    TrackWaveform {
        track_id,
        samples: waveform_samples,
        sample_count: target_samples,
        duration_seconds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autocorrelation() {
        // Simple signal with known periodicity
        let signal: Vec<f32> = (0..200)
            .map(|i| if i % 20 < 10 { 1.0 } else { -1.0 })
            .collect();

        let autocorr = autocorrelation(&signal, 50);

        // Autocorrelation should be computed without panic
        assert!(!autocorr.is_empty());
        // At lag 0, we should have maximum correlation
        let max_corr = autocorr.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        assert!((autocorr[0] - max_corr).abs() < 0.01);
    }

    #[test]
    fn test_generate_waveform() {
        let samples: Vec<f32> = (0..44100).map(|i| (i as f32 * 0.01).sin()).collect();

        let waveform = generate_waveform(&samples, 44100, TrackId(1), 100);

        assert_eq!(waveform.sample_count, 100);
        assert_eq!(waveform.samples.len(), 100);
        assert!((waveform.duration_seconds - 1.0).abs() < 0.01);
    }
}
