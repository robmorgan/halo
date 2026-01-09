//! Audio analysis for BPM detection and beat grid generation.
//!
//! Uses aubio for BPM detection and beat tracking, with FFT for waveform coloring.

use std::fs::File;
use std::path::Path;

use aubio_rs::{OnsetMode, Tempo};
use chrono::Utc;
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::types::{BeatGrid, FrequencyBands, TrackId, TrackWaveform};

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
    /// Waveform samples per second (resolution).
    /// Higher values = smoother zoomed waveforms, more storage.
    /// CDJ-3000 style requires ~150-400 samples/second.
    pub waveform_samples_per_second: f32,
    /// Low frequency band upper limit in Hz (bass, kick drums).
    pub low_freq_cutoff: f32,
    /// Mid frequency band upper limit in Hz (vocals, instruments).
    pub mid_freq_cutoff: f32,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            fft_size: 2048,
            hop_size: 512,
            min_bpm: 60.0,
            max_bpm: 200.0,
            // 150 samples/second gives smooth CDJ-style waveforms when zoomed.
            // For a 5-minute track: 300s * 150 = 45,000 samples (~540KB with frequency data).
            waveform_samples_per_second: 150.0,
            low_freq_cutoff: 250.0,  // 20-250 Hz for bass
            mid_freq_cutoff: 4000.0, // 250-4000 Hz for mids
        }
    }
}

/// Correct octave errors by preferring common dance music tempos.
///
/// Uses a two-tier preference system:
/// - Primary: 115-135 BPM (house/tech house sweet spot)
/// - Secondary: 80-160 BPM (full dance music range)
///
/// This prevents 120 BPM tracks from being detected as 60 or 240 BPM.
fn correct_octave_errors_dance(raw_bpm: f64, min_bpm: f64, max_bpm: f64) -> f64 {
    let candidates = [raw_bpm, raw_bpm * 2.0, raw_bpm / 2.0];

    candidates
        .into_iter()
        .filter(|&bpm| bpm >= min_bpm && bpm <= max_bpm)
        .min_by(|&a, &b| {
            let score_a = tempo_preference_score(a);
            let score_b = tempo_preference_score(b);
            score_a
                .partial_cmp(&score_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(raw_bpm.clamp(min_bpm, max_bpm))
}

/// Score a tempo by how close it is to common dance music ranges.
/// Lower score = better (0 = within primary target range).
fn tempo_preference_score(bpm: f64) -> f64 {
    // Primary sweet spot: 115-135 BPM (house/techno center)
    const PRIMARY_LOW: f64 = 115.0;
    const PRIMARY_HIGH: f64 = 135.0;
    // Secondary range: 80-160 BPM (hip-hop through techno)
    const SECONDARY_LOW: f64 = 80.0;
    const SECONDARY_HIGH: f64 = 160.0;
    // Center point for scoring
    const CENTER: f64 = 125.0;

    if bpm >= PRIMARY_LOW && bpm <= PRIMARY_HIGH {
        // Primary range - best score, slight preference toward center
        (bpm - CENTER).abs() * 0.01
    } else if bpm >= SECONDARY_LOW && bpm <= SECONDARY_HIGH {
        // Secondary range - good but penalized slightly
        10.0 + (bpm - CENTER).abs() * 0.1
    } else {
        // Outside range - heavily penalized
        100.0 + (bpm - CENTER).abs()
    }
}

/// Result of audio analysis.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// Detected BPM (single source of truth for tempo).
    pub bpm: f64,
    /// Detected beat grid (first_beat_offset and beat_positions).
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

    // Generate colored waveform with 3-band frequency analysis
    let waveform = generate_colored_waveform(&samples, sample_rate, track_id, config);

    // Detect BPM and beat positions using multi-method consensus
    let (detected_bpm, detected_confidence, detected_beats) =
        detect_beats_aubio(&samples, sample_rate, config);
    log::info!(
        "Detected BPM: {:.2} (confidence: {:.2}, {} beats)",
        detected_bpm,
        detected_confidence,
        detected_beats.len()
    );

    // Validate and correct beat grid if necessary
    let track_duration = samples.len() as f64 / sample_rate as f64;
    let (bpm, confidence, beat_positions) = validate_and_correct_beat_grid(
        &detected_beats,
        detected_bpm,
        detected_confidence,
        track_duration,
        config.min_bpm,
        config.max_bpm,
    );

    // Calculate first beat offset from validated beats
    let first_beat_offset_ms = beat_positions.first().copied().unwrap_or(0.0) * 1000.0;

    let beat_grid = BeatGrid {
        track_id,
        first_beat_offset_ms,
        beat_positions,
        confidence,
        analyzed_at: Utc::now(),
    };

    Ok(AnalysisResult {
        bpm,
        beat_grid,
        waveform,
    })
}

/// Analyze an audio file with streaming waveform progress.
///
/// Calls `on_waveform_progress` with partial waveform samples as they're generated.
/// This allows the UI to progressively display the waveform during analysis.
pub fn analyze_file_streaming<P, F>(
    path: P,
    track_id: TrackId,
    config: &AnalysisConfig,
    chunk_size: usize,
    mut on_waveform_progress: F,
) -> Result<AnalysisResult, anyhow::Error>
where
    P: AsRef<Path>,
    F: FnMut(Vec<f32>, f32),
{
    let path = path.as_ref();
    log::info!("Analyzing file (streaming): {:?}", path);

    // Load audio samples
    let (samples, sample_rate) = load_audio_samples(path)?;
    log::debug!("Loaded {} samples at {} Hz", samples.len(), sample_rate);

    // Stream amplitude-only progress updates for UI responsiveness
    stream_waveform_progress(
        &samples,
        sample_rate,
        config,
        chunk_size,
        &mut on_waveform_progress,
    );

    // Generate full colored waveform with 3-band FFT analysis
    let waveform = generate_colored_waveform(&samples, sample_rate, track_id, config);

    // Detect BPM and beat positions using multi-method consensus
    let (detected_bpm, detected_confidence, detected_beats) =
        detect_beats_aubio(&samples, sample_rate, config);
    log::info!(
        "Detected BPM: {:.2} (confidence: {:.2}, {} beats)",
        detected_bpm,
        detected_confidence,
        detected_beats.len()
    );

    // Validate and correct beat grid if necessary
    let track_duration = samples.len() as f64 / sample_rate as f64;
    let (bpm, confidence, beat_positions) = validate_and_correct_beat_grid(
        &detected_beats,
        detected_bpm,
        detected_confidence,
        track_duration,
        config.min_bpm,
        config.max_bpm,
    );

    // Calculate first beat offset from validated beats
    let first_beat_offset_ms = beat_positions.first().copied().unwrap_or(0.0) * 1000.0;

    let beat_grid = BeatGrid {
        track_id,
        first_beat_offset_ms,
        beat_positions,
        confidence,
        analyzed_at: Utc::now(),
    };

    Ok(AnalysisResult {
        bpm,
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

/// Detect BPM using FFT-based autocorrelation on the onset envelope.
///
/// This algorithm is more accurate than SoundTouch for precise BPM detection:
/// 1. Calculates onset envelope using spectral flux in bass frequencies
/// 2. Computes autocorrelation using FFT (Wiener-Khinchin theorem)
/// 3. Finds peaks in the autocorrelation corresponding to beat intervals
/// 4. Selects the highest peak within the configured BPM range
fn detect_bpm(samples: &[f32], sample_rate: u32, config: &AnalysisConfig) -> (f64, f32) {
    if samples.len() < sample_rate as usize * 4 {
        // Need at least 4 seconds for reliable detection
        return (120.0, 0.0);
    }

    // Calculate onset envelope from bass frequencies (better for kick detection)
    let onset_env = calculate_bass_onset_envelope(samples, sample_rate, config);

    if onset_env.len() < 256 {
        return (120.0, 0.0);
    }

    // Time resolution of onset envelope
    let hop_time = config.hop_size as f64 / sample_rate as f64;

    // Compute autocorrelation using FFT (Wiener-Khinchin theorem)
    // This is O(n log n) vs O(n^2) for direct computation
    let autocorr = compute_fft_autocorrelation(&onset_env);

    // Convert BPM range to lag range (in onset envelope samples)
    let min_lag = (60.0 / config.max_bpm / hop_time) as usize;
    let max_lag = (60.0 / config.min_bpm / hop_time) as usize;

    // Ensure we have enough autocorrelation data
    let max_lag = max_lag.min(autocorr.len() / 2);
    if max_lag <= min_lag {
        return (120.0, 0.0);
    }

    // Find peaks in autocorrelation within the BPM range
    let mut peaks: Vec<(usize, f32)> = Vec::new();
    for lag in min_lag..max_lag {
        let val = autocorr[lag];
        let prev = if lag > 0 { autocorr[lag - 1] } else { 0.0 };
        let next = if lag + 1 < autocorr.len() {
            autocorr[lag + 1]
        } else {
            0.0
        };

        // Local maximum detection
        if val > prev && val > next && val > 0.0 {
            peaks.push((lag, val));
        }
    }

    if peaks.is_empty() {
        return (120.0, 0.0);
    }

    // Sort peaks by strength (descending)
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Find the best peak, preferring stronger peaks but also considering
    // if a peak at half the lag has similar strength (octave detection)
    let best_lag = peaks[0].0;
    let best_strength = peaks[0].1;

    // Check for octave ambiguity: if there's a peak at 2x the frequency (half lag)
    // with similar strength, prefer the higher frequency (shorter lag)
    let mut final_lag = best_lag;
    let half_lag = best_lag / 2;
    if half_lag >= min_lag {
        // Look for a peak near half the lag
        for &(lag, strength) in &peaks {
            if lag >= half_lag.saturating_sub(2)
                && lag <= half_lag + 2
                && strength > best_strength * 0.7
            {
                // Found a strong peak at half the lag, prefer it
                final_lag = lag;
                break;
            }
        }
    }

    // Convert lag to BPM with parabolic interpolation for sub-sample accuracy
    let refined_lag = if final_lag > 0 && final_lag + 1 < autocorr.len() {
        let y0 = autocorr[final_lag - 1];
        let y1 = autocorr[final_lag];
        let y2 = autocorr[final_lag + 1];
        let offset = (y0 - y2) / (2.0 * (y0 - 2.0 * y1 + y2));
        if offset.is_finite() && offset.abs() < 1.0 {
            final_lag as f64 + offset as f64
        } else {
            final_lag as f64
        }
    } else {
        final_lag as f64
    };

    let beat_interval = refined_lag * hop_time;
    let bpm = 60.0 / beat_interval;

    // Calculate confidence based on peak prominence
    let max_autocorr = autocorr[1..].iter().cloned().fold(0.0f32, f32::max);
    let peak_val = autocorr[final_lag];
    let confidence = if max_autocorr > 0.0 {
        (peak_val / max_autocorr).min(1.0)
    } else {
        0.0
    };

    // Clamp to valid range
    let bpm = bpm.clamp(config.min_bpm, config.max_bpm);

    log::debug!(
        "FFT autocorr BPM: {:.2}, lag: {:.2}, confidence: {:.2}",
        bpm,
        refined_lag,
        confidence
    );

    (bpm, confidence)
}

/// Detect BPM and beat positions using multi-method consensus.
///
/// Runs multiple detection methods (Energy, SpecFlux, FFT autocorrelation)
/// and selects the best result based on confidence and dance tempo proximity.
///
/// Returns (bpm, confidence, beat_positions_in_seconds).
fn detect_beats_aubio(
    samples: &[f32],
    sample_rate: u32,
    config: &AnalysisConfig,
) -> (f64, f32, Vec<f64>) {
    // Method 1: Energy mode (good for kick drums in dance music)
    let (bpm_energy, conf_energy, beats_energy) =
        detect_beats_aubio_with_mode(samples, sample_rate, config, OnsetMode::Energy);

    // Method 2: SpecFlux mode (aubio's recommended for general tempo detection)
    let (bpm_specflux, conf_specflux, beats_specflux) =
        detect_beats_aubio_with_mode(samples, sample_rate, config, OnsetMode::SpecFlux);

    // Method 3: FFT autocorrelation on bass envelope (fallback)
    let (bpm_fft, conf_fft) = detect_bpm(samples, sample_rate, config);

    log::debug!(
        "Multi-method BPM: Energy={:.2} (conf={:.2}), SpecFlux={:.2} (conf={:.2}), FFT={:.2} (conf={:.2})",
        bpm_energy, conf_energy, bpm_specflux, conf_specflux, bpm_fft, conf_fft
    );

    // Select best result using consensus
    select_best_bpm_consensus(
        bpm_energy,
        conf_energy,
        &beats_energy,
        bpm_specflux,
        conf_specflux,
        &beats_specflux,
        bpm_fft,
        conf_fft,
    )
}

/// Select the best BPM from multiple detection methods using consensus.
///
/// Scoring considers:
/// - Detection confidence
/// - Agreement between methods
/// - Proximity to common dance tempos
fn select_best_bpm_consensus(
    bpm1: f64,
    conf1: f32,
    beats1: &[f64],
    bpm2: f64,
    conf2: f32,
    beats2: &[f64],
    bpm3: f64,
    conf3: f32,
) -> (f64, f32, Vec<f64>) {
    // Check if methods agree (within 2% or octave relationship)
    let agree_1_2 = bpms_agree(bpm1, bpm2);
    let agree_1_3 = bpms_agree(bpm1, bpm3);
    let agree_2_3 = bpms_agree(bpm2, bpm3);

    // If two or more methods agree, use that BPM
    if agree_1_2 && agree_1_3 {
        // All three agree - use method with highest confidence
        if conf1 >= conf2 {
            log::debug!("Consensus: all methods agree, using Energy ({:.2} BPM)", bpm1);
            return (bpm1, (conf1 + conf2 + conf3) / 3.0, beats1.to_vec());
        } else {
            log::debug!(
                "Consensus: all methods agree, using SpecFlux ({:.2} BPM)",
                bpm2
            );
            return (bpm2, (conf1 + conf2 + conf3) / 3.0, beats2.to_vec());
        }
    } else if agree_1_2 {
        // Energy and SpecFlux agree
        let combined_conf = (conf1 + conf2) / 2.0;
        if conf1 >= conf2 {
            log::debug!(
                "Consensus: Energy and SpecFlux agree, using Energy ({:.2} BPM)",
                bpm1
            );
            return (bpm1, combined_conf, beats1.to_vec());
        } else {
            log::debug!(
                "Consensus: Energy and SpecFlux agree, using SpecFlux ({:.2} BPM)",
                bpm2
            );
            return (bpm2, combined_conf, beats2.to_vec());
        }
    } else if agree_1_3 {
        // Energy and FFT agree
        log::debug!(
            "Consensus: Energy and FFT agree, using Energy ({:.2} BPM)",
            bpm1
        );
        return (bpm1, (conf1 + conf3) / 2.0, beats1.to_vec());
    } else if agree_2_3 {
        // SpecFlux and FFT agree
        log::debug!(
            "Consensus: SpecFlux and FFT agree, using SpecFlux ({:.2} BPM)",
            bpm2
        );
        return (bpm2, (conf2 + conf3) / 2.0, beats2.to_vec());
    }

    // No agreement - score each by confidence and tempo preference
    let score1 = conf1 as f64 * 10.0 - tempo_preference_score(bpm1) * 0.5;
    let score2 = conf2 as f64 * 10.0 - tempo_preference_score(bpm2) * 0.5;
    let score3 = conf3 as f64 * 10.0 - tempo_preference_score(bpm3) * 0.5;

    if score1 >= score2 && score1 >= score3 {
        log::debug!(
            "Consensus: no agreement, using Energy ({:.2} BPM, score={:.2})",
            bpm1,
            score1
        );
        (bpm1, conf1, beats1.to_vec())
    } else if score2 >= score3 {
        log::debug!(
            "Consensus: no agreement, using SpecFlux ({:.2} BPM, score={:.2})",
            bpm2,
            score2
        );
        (bpm2, conf2, beats2.to_vec())
    } else {
        // FFT method doesn't provide beats, use Energy's beats with FFT's BPM
        log::debug!(
            "Consensus: no agreement, using FFT BPM ({:.2}) with Energy beats",
            bpm3
        );
        (bpm3, conf3, beats1.to_vec())
    }
}

/// Validate and correct beat grid to ensure intervals match the detected BPM.
///
/// If the median beat interval differs significantly from the expected interval,
/// recalculates the BPM from the actual intervals and regenerates a consistent grid.
///
/// Returns (corrected_bpm, corrected_confidence, corrected_beats).
fn validate_and_correct_beat_grid(
    beats: &[f64],
    detected_bpm: f64,
    detected_confidence: f32,
    track_duration: f64,
    min_bpm: f64,
    max_bpm: f64,
) -> (f64, f32, Vec<f64>) {
    if beats.len() < 8 {
        // Not enough beats to validate - return original
        return (detected_bpm, detected_confidence, beats.to_vec());
    }

    // Calculate actual intervals between detected beats
    let intervals: Vec<f64> = beats.windows(2).map(|w| w[1] - w[0]).collect();

    // Calculate median interval (robust against outliers)
    let mut sorted_intervals = intervals.clone();
    sorted_intervals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_interval = sorted_intervals[sorted_intervals.len() / 2];

    // Calculate BPM from median interval
    let actual_bpm = 60.0 / median_interval;

    // Check if detected BPM matches actual beat intervals
    let expected_interval = 60.0 / detected_bpm;
    let interval_error = (median_interval - expected_interval).abs() / expected_interval;

    if interval_error <= 0.02 {
        // Within 2% tolerance - beat grid is consistent
        log::debug!(
            "Beat grid validation passed: detected={:.2} BPM, actual intervals suggest {:.2} BPM",
            detected_bpm,
            actual_bpm
        );
        return (detected_bpm, detected_confidence, beats.to_vec());
    }

    // BPM mismatch - recalculate from beat intervals
    log::info!(
        "Beat grid validation: BPM mismatch! Detected {:.2}, intervals suggest {:.2} (error={:.1}%)",
        detected_bpm,
        actual_bpm,
        interval_error * 100.0
    );

    // Apply octave correction to the actual BPM
    let corrected_bpm = correct_octave_errors_dance(actual_bpm, min_bpm, max_bpm);
    let corrected_interval = 60.0 / corrected_bpm;

    // Regenerate consistent beat grid from first beat
    let first_beat = beats[0];
    let num_beats = ((track_duration - first_beat) / corrected_interval).ceil() as usize;

    let corrected_beats: Vec<f64> = (0..num_beats)
        .map(|i| first_beat + i as f64 * corrected_interval)
        .filter(|&t| t < track_duration)
        .collect();

    // Reduce confidence since we had to correct
    let corrected_confidence = (detected_confidence * 0.8).max(0.3);

    log::info!(
        "Beat grid corrected: {:.2} BPM -> {:.2} BPM, {} beats",
        detected_bpm,
        corrected_bpm,
        corrected_beats.len()
    );

    (corrected_bpm, corrected_confidence, corrected_beats)
}

/// Check if two BPM values agree (within 2% or octave relationship).
fn bpms_agree(bpm1: f64, bpm2: f64) -> bool {
    let ratio = bpm1 / bpm2;
    let tolerance = 0.02;

    // Check direct agreement
    if (ratio - 1.0).abs() < tolerance {
        return true;
    }
    // Check octave relationships (2x or 0.5x)
    if (ratio - 2.0).abs() < tolerance || (ratio - 0.5).abs() < tolerance {
        return true;
    }
    false
}

/// Detect BPM and beat positions using aubio's Tempo tracker with a specific onset mode.
///
/// Returns (bpm, confidence, beat_positions_in_seconds).
fn detect_beats_aubio_with_mode(
    samples: &[f32],
    sample_rate: u32,
    config: &AnalysisConfig,
    onset_mode: OnsetMode,
) -> (f64, f32, Vec<f64>) {
    let buf_size = 1024;
    let hop_size = 512;

    // Create aubio Tempo detector with specified onset mode
    let mut tempo = match Tempo::new(onset_mode, buf_size, hop_size, sample_rate) {
        Ok(t) => t,
        Err(e) => {
            log::warn!("Failed to create aubio Tempo: {}", e);
            return (120.0, 0.0, Vec::new());
        }
    };

    // Process audio in chunks and collect beat positions
    let mut beat_positions: Vec<f64> = Vec::new();
    let mut output = vec![0.0f32; 1];

    for (chunk_idx, chunk) in samples.chunks(hop_size).enumerate() {
        // Pad last chunk if needed
        let input: Vec<f32> = if chunk.len() < hop_size {
            let mut padded = chunk.to_vec();
            padded.resize(hop_size, 0.0);
            padded
        } else {
            chunk.to_vec()
        };

        // Process chunk through tempo detector
        if tempo.do_(&input, &mut output).is_ok() {
            // If output[0] > 0, a beat was detected at this position
            if output[0] > 0.0 {
                let beat_time = chunk_idx as f64 * hop_size as f64 / sample_rate as f64;
                beat_positions.push(beat_time);
            }
        }
    }

    // Get final BPM and confidence from aubio
    let raw_bpm = tempo.get_bpm() as f64;
    let confidence = tempo.get_confidence();

    // Apply dance-tempo-aware octave correction
    let bpm = if raw_bpm > 0.0 {
        correct_octave_errors_dance(raw_bpm, config.min_bpm, config.max_bpm)
    } else {
        120.0 // Fallback for invalid detection
    };

    log::debug!(
        "Aubio BPM: {:.2}, confidence: {:.2}, beats detected: {}",
        bpm,
        confidence,
        beat_positions.len()
    );

    // Refine beat positions to align with actual transients
    let refined_positions = refine_beats_to_transients(samples, sample_rate, &beat_positions);

    log::debug!(
        "Refined {} beats to transient positions",
        refined_positions.len()
    );

    (bpm, confidence, refined_positions)
}

/// Refine all beat positions to align with actual transients.
///
/// Takes coarse beat positions (from aubio) and refines each one to the
/// precise transient onset within a search window.
fn refine_beats_to_transients(samples: &[f32], sample_rate: u32, coarse_beats: &[f64]) -> Vec<f64> {
    coarse_beats
        .iter()
        .map(|&beat_time| refine_beat_to_transient(samples, sample_rate, beat_time))
        .collect()
}

/// Refine a single beat position to the nearest transient onset.
///
/// Searches within ±30ms of the coarse beat position to find the exact
/// sample where the transient attack begins. Uses onset detection based
/// on energy increase rate, focusing on low frequencies (kick drums).
///
/// The algorithm:
/// 1. Extract a window of samples around the coarse beat position
/// 2. Apply a simple low-pass filter to focus on kick drum frequencies
/// 3. Calculate the onset function (rate of energy increase)
/// 4. Find the maximum onset within the window
/// 5. Return the refined time position
fn refine_beat_to_transient(samples: &[f32], sample_rate: u32, coarse_beat_time: f64) -> f64 {
    // Search window: ±50ms around the coarse beat (wider for better kick detection)
    let window_ms = 50.0;
    let window_samples = ((window_ms / 1000.0) * sample_rate as f64) as usize;

    let beat_sample = (coarse_beat_time * sample_rate as f64) as usize;
    let start = beat_sample.saturating_sub(window_samples);
    let end = (beat_sample + window_samples).min(samples.len());

    if end <= start + 10 {
        return coarse_beat_time;
    }

    // Simple low-pass filter for kick drum emphasis (moving average)
    // This smooths high frequencies while preserving the kick transient
    let filter_size = (sample_rate / 2000) as usize; // ~500Hz cutoff
    let filter_size = filter_size.max(4).min(32);

    // Calculate filtered energy in small windows
    let hop = filter_size / 2;
    let mut energies: Vec<(usize, f32)> = Vec::new();

    let mut i = start;
    while i + filter_size <= end {
        // Calculate RMS energy in this small window
        let energy: f32 = samples[i..i + filter_size]
            .iter()
            .map(|&s| s * s)
            .sum::<f32>()
            / filter_size as f32;
        energies.push((i + filter_size / 2, energy.sqrt()));
        i += hop;
    }

    if energies.len() < 3 {
        return coarse_beat_time;
    }

    // Calculate onset function (positive derivative of energy)
    // The transient is where energy increases most rapidly
    let mut max_onset = 0.0f32;
    let mut max_onset_idx = beat_sample;

    for i in 1..energies.len() {
        let onset = (energies[i].1 - energies[i - 1].1).max(0.0);

        // Weight by proximity to original beat (prefer refinements close to the coarse position)
        let distance_from_beat =
            (energies[i].0 as f64 - beat_sample as f64).abs() / window_samples as f64;
        let proximity_weight = 1.0 - (distance_from_beat * 0.3) as f32; // Mild preference for center

        let weighted_onset = onset * proximity_weight;

        if weighted_onset > max_onset {
            max_onset = weighted_onset;
            max_onset_idx = energies[i].0;
        }
    }

    // If no significant onset found, return original
    if max_onset < 0.001 {
        return coarse_beat_time;
    }

    // Further refine: find the exact sample where the attack starts
    // Look backwards from the onset peak to find where energy starts rising
    let attack_search_start = max_onset_idx.saturating_sub(filter_size * 2);
    let attack_search_end = max_onset_idx.min(samples.len());

    if attack_search_end > attack_search_start {
        // Find the sample with steepest positive slope (attack start)
        let mut max_slope = 0.0f32;
        let mut attack_sample = max_onset_idx;

        for i in (attack_search_start + 1)..attack_search_end {
            let slope = samples[i].abs() - samples[i - 1].abs();
            if slope > max_slope {
                max_slope = slope;
                attack_sample = i;
            }
        }

        if max_slope > 0.01 {
            return attack_sample as f64 / sample_rate as f64;
        }
    }

    max_onset_idx as f64 / sample_rate as f64
}

/// Compute autocorrelation using FFT (Wiener-Khinchin theorem).
///
/// The autocorrelation of a signal equals the inverse FFT of its power spectrum.
/// This is O(n log n) compared to O(n^2) for direct computation.
fn compute_fft_autocorrelation(signal: &[f32]) -> Vec<f32> {
    // Pad to power of 2 for efficient FFT
    let n = signal.len().next_power_of_two() * 2;

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);

    // Zero-pad signal
    let mut buffer: Vec<Complex<f32>> = signal
        .iter()
        .map(|&x| Complex::new(x, 0.0))
        .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
        .take(n)
        .collect();

    // Forward FFT
    fft.process(&mut buffer);

    // Compute power spectrum (|X(f)|^2)
    for c in &mut buffer {
        *c = Complex::new(c.norm_sqr(), 0.0);
    }

    // Inverse FFT to get autocorrelation
    ifft.process(&mut buffer);

    // Normalize and return real part
    let norm = 1.0 / n as f32;
    buffer.iter().map(|c| c.re * norm).collect()
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

/// Find the offset to the first downbeat using low-frequency onset detection.
///
/// This function detects kick drum hits by analyzing low-frequency energy,
/// then finds the phase offset that best aligns with the detected BPM.
fn find_first_beat(samples: &[f32], sample_rate: u32, bpm: f64) -> f64 {
    let config = AnalysisConfig::default();

    // Calculate low-frequency onset envelope (kick drums are typically 40-120 Hz)
    let bass_onset_env = calculate_bass_onset_envelope(samples, sample_rate, &config);

    if bass_onset_env.is_empty() {
        return 0.0;
    }

    let beat_interval_seconds = 60.0 / bpm;
    let hop_time = config.hop_size as f64 / sample_rate as f64;

    // Find onset threshold (mean + 2 * std deviation for strong kicks)
    let mean: f32 = bass_onset_env.iter().sum::<f32>() / bass_onset_env.len() as f32;
    let variance: f32 = bass_onset_env
        .iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f32>()
        / bass_onset_env.len() as f32;
    let std_dev = variance.sqrt();
    let threshold = mean + 2.0 * std_dev;

    // Collect strong onset times (potential kick drums) in the first 30 seconds
    let max_search_frames = (30.0 / hop_time) as usize;
    let search_frames = bass_onset_env.len().min(max_search_frames);

    let mut onset_times: Vec<f64> = Vec::new();
    for (i, &value) in bass_onset_env[..search_frames].iter().enumerate() {
        if value > threshold {
            let time = i as f64 * hop_time;
            // Avoid onsets too close together (minimum 100ms apart)
            if onset_times.last().map_or(true, |&last| time - last > 0.1) {
                onset_times.push(time);
            }
        }
    }

    if onset_times.is_empty() {
        // Fallback: find the single strongest onset in first 10 seconds
        let search_limit = (10.0 / hop_time) as usize;
        let limit = bass_onset_env.len().min(search_limit);
        if let Some((idx, _)) = bass_onset_env[..limit]
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        {
            return idx as f64 * hop_time * 1000.0;
        }
        return 0.0;
    }

    // Find the phase offset that maximizes alignment with detected onsets
    // Test 100 different phase offsets within one beat interval
    let num_phases = 100;
    let mut best_phase = 0.0;
    let mut best_score = 0.0;

    for phase_idx in 0..num_phases {
        let phase_offset = (phase_idx as f64 / num_phases as f64) * beat_interval_seconds;
        let mut score = 0.0;

        for &onset_time in &onset_times {
            // Calculate distance to nearest beat at this phase
            let beats_from_start = (onset_time - phase_offset) / beat_interval_seconds;
            let nearest_beat_offset =
                beats_from_start.round() * beat_interval_seconds + phase_offset;
            let distance = (onset_time - nearest_beat_offset).abs();

            // Score based on proximity (closer = higher score)
            // Use Gaussian weighting: exp(-(distance/sigma)^2)
            let sigma = beat_interval_seconds * 0.1; // 10% of beat interval tolerance
            score += (-((distance / sigma).powi(2))).exp();
        }

        if score > best_score {
            best_score = score;
            best_phase = phase_offset;
        }
    }

    // Return phase offset in milliseconds
    best_phase * 1000.0
}

/// Calculate low-frequency (bass) onset envelope for kick drum detection.
///
/// Focuses on 40-200 Hz range where kick drums have most energy.
fn calculate_bass_onset_envelope(
    samples: &[f32],
    sample_rate: u32,
    config: &AnalysisConfig,
) -> Vec<f32> {
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(config.fft_size);

    let mut onset_env = Vec::new();
    let mut prev_bass_energy = 0.0f32;

    // Frequency bins for bass range (40-200 Hz)
    let bin_width = sample_rate as f32 / config.fft_size as f32;
    let bass_low_bin = (40.0 / bin_width) as usize;
    let bass_high_bin = (200.0 / bin_width) as usize;

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

        // Calculate bass energy (sum of magnitudes in bass range)
        let bass_energy: f32 = buffer[bass_low_bin..=bass_high_bin.min(buffer.len() - 1)]
            .iter()
            .map(|c| c.norm())
            .sum();

        // Half-wave rectified difference (onset = increase in bass energy)
        let onset = (bass_energy - prev_bass_energy).max(0.0);
        onset_env.push(onset);

        prev_bass_energy = bass_energy;
    }

    onset_env
}

/// Generate colored waveform with 3-band frequency analysis for visualization.
///
/// Uses FFT to extract low, mid, and high frequency energy for each waveform sample.
/// - Low: 20-250 Hz (bass, kick drums) -> Red
/// - Mid: 250-4000 Hz (vocals, instruments) -> Green
/// - High: 4000+ Hz (hi-hats, cymbals) -> Blue
fn generate_colored_waveform(
    audio_samples: &[f32],
    sample_rate: u32,
    track_id: TrackId,
    config: &AnalysisConfig,
) -> TrackWaveform {
    let duration_seconds = audio_samples.len() as f64 / sample_rate as f64;

    // Calculate target samples based on duration and samples-per-second config
    // This gives consistent resolution regardless of track length
    let target_samples =
        ((duration_seconds as f32 * config.waveform_samples_per_second).ceil() as usize).max(100);

    if audio_samples.is_empty() {
        return TrackWaveform {
            track_id,
            samples: vec![0.0; target_samples],
            frequency_bands: Some(vec![FrequencyBands::default(); target_samples]),
            sample_count: target_samples,
            duration_seconds: 0.0,
        };
    }

    let samples_per_bucket = audio_samples.len() / target_samples.max(1);

    // FFT setup
    let fft_size = config.fft_size;
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    // Hanning window for smoother FFT
    let window: Vec<f32> = (0..fft_size)
        .map(|i| {
            0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (fft_size - 1) as f32).cos())
        })
        .collect();

    // Frequency bin calculations
    let freq_resolution = sample_rate as f32 / fft_size as f32;
    let low_bin_end = (config.low_freq_cutoff / freq_resolution).round() as usize;
    let mid_bin_end = (config.mid_freq_cutoff / freq_resolution).round() as usize;
    let nyquist_bin = fft_size / 2;

    let mut waveform_samples = Vec::with_capacity(target_samples);
    let mut frequency_bands = Vec::with_capacity(target_samples);

    for i in 0..target_samples {
        let bucket_start = i * samples_per_bucket;
        let bucket_end = ((i + 1) * samples_per_bucket).min(audio_samples.len());

        if bucket_start >= audio_samples.len() {
            waveform_samples.push(0.0);
            frequency_bands.push(FrequencyBands::default());
            continue;
        }

        // Calculate peak amplitude for this bucket
        let peak = audio_samples[bucket_start..bucket_end]
            .iter()
            .map(|s| s.abs())
            .fold(0.0f32, f32::max);
        waveform_samples.push(peak);

        // Find center of bucket for FFT analysis
        let center = (bucket_start + bucket_end) / 2;
        let fft_start = center.saturating_sub(fft_size / 2);
        let fft_end = (fft_start + fft_size).min(audio_samples.len());
        let available = fft_end - fft_start;

        // Prepare FFT buffer with zero-padding if needed
        let mut buffer: Vec<Complex<f32>> = (0..fft_size)
            .map(|j| {
                if j < available {
                    let sample = audio_samples[fft_start + j];
                    Complex::new(sample * window[j], 0.0)
                } else {
                    Complex::new(0.0, 0.0)
                }
            })
            .collect();

        // Compute FFT
        fft.process(&mut buffer);

        // Calculate energy in each frequency band (magnitude squared)
        let mut low_energy = 0.0f32;
        let mut mid_energy = 0.0f32;
        let mut high_energy = 0.0f32;

        for (bin, c) in buffer.iter().enumerate().take(nyquist_bin) {
            let mag_sq = c.norm_sqr();
            if bin < low_bin_end {
                low_energy += mag_sq;
            } else if bin < mid_bin_end {
                mid_energy += mag_sq;
            } else {
                high_energy += mag_sq;
            }
        }

        // Normalize by band size to get average energy per bin
        let low_bins = low_bin_end.max(1) as f32;
        let mid_bins = (mid_bin_end - low_bin_end).max(1) as f32;
        let high_bins = (nyquist_bin - mid_bin_end).max(1) as f32;

        low_energy = (low_energy / low_bins).sqrt();
        mid_energy = (mid_energy / mid_bins).sqrt();
        high_energy = (high_energy / high_bins).sqrt();

        // Normalize to relative energy (CDJ/rekordbox style)
        // This shows which frequency band dominates, not absolute energy
        let total_energy = low_energy + mid_energy + high_energy;
        if total_energy > 0.001 {
            low_energy /= total_energy;
            mid_energy /= total_energy;
            high_energy /= total_energy;
        } else {
            // Silent section - show as dark gray
            low_energy = 0.33;
            mid_energy = 0.33;
            high_energy = 0.33;
        }

        frequency_bands.push(FrequencyBands::new(low_energy, mid_energy, high_energy));
    }

    TrackWaveform {
        track_id,
        samples: waveform_samples,
        frequency_bands: Some(frequency_bands),
        sample_count: target_samples,
        duration_seconds,
    }
}

/// Stream waveform progress updates for UI responsiveness.
///
/// Generates amplitude-only samples progressively and calls `on_progress`
/// after each chunk with the accumulated samples and progress (0.0 to 1.0).
/// This is used for UI updates during analysis; the final colored waveform
/// is generated separately by `generate_colored_waveform`.
fn stream_waveform_progress<F>(
    audio_samples: &[f32],
    sample_rate: u32,
    config: &AnalysisConfig,
    chunk_size: usize,
    mut on_progress: F,
) where
    F: FnMut(Vec<f32>, f32),
{
    let duration_seconds = audio_samples.len() as f64 / sample_rate as f64;

    // Calculate target samples using same formula as generate_colored_waveform
    let target_samples =
        ((duration_seconds as f32 * config.waveform_samples_per_second).ceil() as usize).max(100);

    if audio_samples.is_empty() {
        on_progress(vec![0.0; target_samples], 1.0);
        return;
    }

    let samples_per_bucket = audio_samples.len() / target_samples.max(1);

    let mut waveform_samples = Vec::with_capacity(target_samples);

    for i in 0..target_samples {
        let start = i * samples_per_bucket;
        let end = ((i + 1) * samples_per_bucket).min(audio_samples.len());

        let peak = if start >= audio_samples.len() {
            0.0
        } else {
            audio_samples[start..end]
                .iter()
                .map(|s| s.abs())
                .fold(0.0f32, f32::max)
        };

        waveform_samples.push(peak);

        // Send progress after each chunk
        if (i + 1) % chunk_size == 0 || i == target_samples - 1 {
            let progress = (i + 1) as f32 / target_samples as f32;
            on_progress(waveform_samples.clone(), progress);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_colored_waveform() {
        let samples: Vec<f32> = (0..44100).map(|i| (i as f32 * 0.01).sin()).collect();

        let config = AnalysisConfig::default();
        let waveform = generate_colored_waveform(&samples, 44100, TrackId(1), &config);

        // 1 second of audio at 150 samples/second = 150 samples
        let expected_samples = 150;
        assert_eq!(waveform.sample_count, expected_samples);
        assert_eq!(waveform.samples.len(), expected_samples);
        assert!((waveform.duration_seconds - 1.0).abs() < 0.01);
        // Verify frequency bands are generated
        assert!(waveform.frequency_bands.is_some());
        assert_eq!(
            waveform.frequency_bands.as_ref().unwrap().len(),
            expected_samples
        );
    }
}
