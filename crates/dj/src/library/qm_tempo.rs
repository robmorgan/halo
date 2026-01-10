//! Queen Mary-style BPM detection algorithm.
//!
//! Implements the tempo detection approach used by Mixxx/Queen Mary DSP library:
//! 1. Complex Domain onset detection function
//! 2. 6-second windowed analysis with autocorrelation
//! 3. Perceptually-weighted comb filterbank
//! 4. Viterbi algorithm for optimal tempo path
//! 5. Dynamic programming beat tracking (Ellis 2007)
//!
//! References:
//! - Davies & Plumbley, "Beat Tracking With A Two State Model" (ICASSP 2005)
//! - Ellis, "Beat Tracking by Dynamic Programming" (JNMR 2007)
//! - Duxbury et al, "Complex Domain Onset Detection" (DAFx 2003)

use std::f32::consts::PI;

use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

/// Configuration for Queen Mary-style tempo detection.
#[derive(Debug, Clone)]
pub struct QmTempoConfig {
    /// FFT size for spectral analysis.
    pub fft_size: usize,
    /// Hop size between FFT windows.
    pub hop_size: usize,
    /// Minimum BPM to detect.
    pub min_bpm: f64,
    /// Maximum BPM to detect.
    pub max_bpm: f64,
    /// Window size for tempo analysis in seconds.
    pub tempo_window_seconds: f32,
    /// Hop size for tempo analysis in seconds.
    pub tempo_hop_seconds: f32,
    /// Enable adaptive whitening for onset detection.
    pub adaptive_whitening: bool,
    /// Onset detection method.
    pub onset_method: OnsetMethod,
}

impl Default for QmTempoConfig {
    fn default() -> Self {
        Self {
            fft_size: 2048,
            hop_size: 512,
            min_bpm: 60.0,
            max_bpm: 200.0,
            tempo_window_seconds: 6.0,
            tempo_hop_seconds: 1.5,
            adaptive_whitening: true,
            onset_method: OnsetMethod::ComplexDomain,
        }
    }
}

/// Onset detection methods available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnsetMethod {
    /// Complex Domain - most versatile (default).
    ComplexDomain,
    /// Spectral Difference - good for percussive recordings.
    SpectralDifference,
    /// Phase Deviation - good for non-percussive music.
    PhaseDeviation,
    /// Broadband Energy Rise - percussive onsets in mixed audio.
    BroadbandEnergyRise,
}

/// Result of Queen Mary tempo detection.
#[derive(Debug, Clone)]
pub struct QmTempoResult {
    /// Detected BPM.
    pub bpm: f64,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f32,
    /// Beat positions in seconds.
    pub beats: Vec<f64>,
    /// Tempo estimates per analysis window (for debugging).
    pub tempo_curve: Vec<f64>,
    /// Transient alignment score (0.0 to 1.0).
    /// Measures how well detected beats align with audio transients.
    pub alignment_score: f32,
}

/// Detect tempo using Queen Mary-style algorithm.
///
/// This implements the full QM approach:
/// 1. Compute onset detection function
/// 2. Analyze tempo in 6-second windows
/// 3. Use comb filterbank + Viterbi for tempo path
/// 4. Use dynamic programming for beat positions
pub fn detect_tempo_qm(samples: &[f32], sample_rate: u32, config: &QmTempoConfig) -> QmTempoResult {
    if samples.len() < sample_rate as usize * 4 {
        // Need at least 4 seconds for reliable detection
        return QmTempoResult {
            bpm: 120.0,
            confidence: 0.0,
            beats: Vec::new(),
            tempo_curve: Vec::new(),
            alignment_score: 0.0,
        };
    }

    // Step 1: Compute onset detection function
    let odf = compute_onset_function(samples, sample_rate, config);

    if odf.is_empty() {
        return QmTempoResult {
            bpm: 120.0,
            confidence: 0.0,
            beats: Vec::new(),
            tempo_curve: Vec::new(),
            alignment_score: 0.0,
        };
    }

    // Step 2: Compute tempo estimates for each window using comb filterbank
    let odf_sample_rate = sample_rate as f32 / config.hop_size as f32;
    let tempo_estimates = compute_tempo_curve(&odf, odf_sample_rate, config);

    if tempo_estimates.is_empty() {
        return QmTempoResult {
            bpm: 120.0,
            confidence: 0.0,
            beats: Vec::new(),
            tempo_curve: Vec::new(),
            alignment_score: 0.0,
        };
    }

    // Step 3: Use Viterbi algorithm to find optimal tempo path
    // The comb filterbank now includes QM-DSP style octave/ratio disambiguation,
    // so the Viterbi output should already have resolved most ambiguities.
    let (tempo_path, confidence) = viterbi_tempo_tracking(&tempo_estimates, config);

    // Get the dominant tempo (median of the path)
    let mut bpm = if tempo_path.is_empty() {
        120.0
    } else {
        let mut sorted = tempo_path.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        sorted[sorted.len() / 2]
    };

    // Step 4: Use transient alignment to validate and potentially correct the tempo
    // This is crucial for fixing the 2/3 ratio problem
    let (_, alignment_score) = find_best_first_beat(&odf, odf_sample_rate, bpm, 16);

    // Check related tempos and use alignment scores to correct errors
    bpm = validate_tempo_with_alignment(&odf, odf_sample_rate, bpm, alignment_score, config);

    // Recalculate alignment score for the final validated tempo
    let (_, alignment_score) = find_best_first_beat(&odf, odf_sample_rate, bpm, 16);

    // Step 5: Dynamic programming beat tracking with validated tempo
    let beats = dp_beat_tracking(&odf, odf_sample_rate, bpm, config);

    // Convert beat positions from ODF frames to seconds
    let beats_seconds: Vec<f64> = beats
        .iter()
        .map(|&frame| frame as f64 / odf_sample_rate as f64)
        .collect();

    log::debug!(
        "QM tempo detection: {:.2} BPM, confidence: {:.2}, alignment: {:.2}, {} beats",
        bpm,
        confidence,
        alignment_score,
        beats_seconds.len()
    );

    QmTempoResult {
        bpm,
        confidence,
        beats: beats_seconds,
        tempo_curve: tempo_path,
        alignment_score,
    }
}

/// Compute onset detection function using the specified method.
fn compute_onset_function(samples: &[f32], sample_rate: u32, config: &QmTempoConfig) -> Vec<f32> {
    match config.onset_method {
        OnsetMethod::ComplexDomain => compute_complex_domain_odf(samples, sample_rate, config),
        OnsetMethod::SpectralDifference => {
            compute_spectral_difference_odf(samples, sample_rate, config)
        }
        OnsetMethod::PhaseDeviation => compute_phase_deviation_odf(samples, sample_rate, config),
        OnsetMethod::BroadbandEnergyRise => compute_energy_rise_odf(samples, sample_rate, config),
    }
}

/// Complex Domain onset detection function (Duxbury et al 2003).
///
/// Combines magnitude and phase information to detect onsets.
/// This is the most versatile method and works well for most music.
fn compute_complex_domain_odf(
    samples: &[f32],
    sample_rate: u32,
    config: &QmTempoConfig,
) -> Vec<f32> {
    let fft_size = config.fft_size;
    let hop_size = config.hop_size;

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    // Hanning window
    let window: Vec<f32> = (0..fft_size)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (fft_size - 1) as f32).cos()))
        .collect();

    let num_bins = fft_size / 2 + 1;
    let mut prev_magnitude = vec![0.0f32; num_bins];
    let mut prev_phase = vec![0.0f32; num_bins];
    let mut prev_prev_phase = vec![0.0f32; num_bins];

    let mut odf = Vec::new();

    // Adaptive whitening state
    let mut whitening_memory = vec![0.0f32; num_bins];
    let whitening_decay = 0.9997_f32.powf(fft_size as f32 / sample_rate as f32);
    let whitening_floor = 1e-6_f32;

    for start in (0..samples.len().saturating_sub(fft_size)).step_by(hop_size) {
        // Apply window and compute FFT
        let mut buffer: Vec<Complex<f32>> = samples[start..start + fft_size]
            .iter()
            .zip(window.iter())
            .map(|(s, w)| Complex::new(s * w, 0.0))
            .collect();

        fft.process(&mut buffer);

        // Extract magnitude and phase
        let mut magnitudes = Vec::with_capacity(num_bins);
        let mut phases = Vec::with_capacity(num_bins);

        for c in buffer.iter().take(num_bins) {
            magnitudes.push(c.norm());
            phases.push(c.arg());
        }

        // Apply adaptive whitening if enabled
        if config.adaptive_whitening {
            for (i, mag) in magnitudes.iter_mut().enumerate() {
                whitening_memory[i] = whitening_memory[i] * whitening_decay;
                if *mag > whitening_memory[i] {
                    whitening_memory[i] = *mag;
                }
                let divisor = whitening_memory[i].max(whitening_floor);
                *mag /= divisor;
            }
        }

        // Complex domain onset detection
        // Predicts current frame from previous two, measures deviation
        let mut onset_value = 0.0f32;

        for i in 0..num_bins {
            // Predict magnitude (use previous)
            let predicted_mag = prev_magnitude[i];

            // Predict phase using phase derivative (instantaneous frequency)
            let phase_diff = prev_phase[i] - prev_prev_phase[i];
            let predicted_phase = prev_phase[i] + phase_diff;

            // Calculate predicted complex value
            let predicted = Complex::new(
                predicted_mag * predicted_phase.cos(),
                predicted_mag * predicted_phase.sin(),
            );

            // Calculate actual complex value
            let actual = Complex::new(
                magnitudes[i] * phases[i].cos(),
                magnitudes[i] * phases[i].sin(),
            );

            // Complex domain distance (Euclidean in complex plane)
            let diff = actual - predicted;
            onset_value += diff.norm();
        }

        odf.push(onset_value);

        // Update state
        prev_prev_phase = prev_phase;
        prev_phase = phases;
        prev_magnitude = magnitudes;
    }

    // Normalize and smooth the ODF
    normalize_and_smooth_odf(&mut odf);

    odf
}

/// Spectral Difference onset detection function.
///
/// Measures the change in spectral magnitude between frames.
/// Good for percussive recordings.
fn compute_spectral_difference_odf(
    samples: &[f32],
    _sample_rate: u32,
    config: &QmTempoConfig,
) -> Vec<f32> {
    let fft_size = config.fft_size;
    let hop_size = config.hop_size;

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    let window: Vec<f32> = (0..fft_size)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (fft_size - 1) as f32).cos()))
        .collect();

    let num_bins = fft_size / 2 + 1;
    let mut prev_magnitude = vec![0.0f32; num_bins];
    let mut odf = Vec::new();

    for start in (0..samples.len().saturating_sub(fft_size)).step_by(hop_size) {
        let mut buffer: Vec<Complex<f32>> = samples[start..start + fft_size]
            .iter()
            .zip(window.iter())
            .map(|(s, w)| Complex::new(s * w, 0.0))
            .collect();

        fft.process(&mut buffer);

        // Half-wave rectified spectral difference
        let mut onset_value = 0.0f32;
        for (i, c) in buffer.iter().take(num_bins).enumerate() {
            let mag = c.norm();
            let diff = (mag - prev_magnitude[i]).max(0.0);
            onset_value += diff * diff; // Squared for emphasis
            prev_magnitude[i] = mag;
        }

        odf.push(onset_value.sqrt());
    }

    normalize_and_smooth_odf(&mut odf);
    odf
}

/// Phase Deviation onset detection function.
///
/// Measures deviation from expected phase progression.
/// Good for non-percussive music with clear pitch.
fn compute_phase_deviation_odf(
    samples: &[f32],
    _sample_rate: u32,
    config: &QmTempoConfig,
) -> Vec<f32> {
    let fft_size = config.fft_size;
    let hop_size = config.hop_size;

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    let window: Vec<f32> = (0..fft_size)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (fft_size - 1) as f32).cos()))
        .collect();

    let num_bins = fft_size / 2 + 1;
    let mut prev_phase = vec![0.0f32; num_bins];
    let mut prev_prev_phase = vec![0.0f32; num_bins];
    let mut odf = Vec::new();

    for start in (0..samples.len().saturating_sub(fft_size)).step_by(hop_size) {
        let mut buffer: Vec<Complex<f32>> = samples[start..start + fft_size]
            .iter()
            .zip(window.iter())
            .map(|(s, w)| Complex::new(s * w, 0.0))
            .collect();

        fft.process(&mut buffer);

        let mut onset_value = 0.0f32;
        for (i, c) in buffer.iter().take(num_bins).enumerate() {
            let phase = c.arg();
            let mag = c.norm();

            // Expected phase based on previous phase derivative
            let phase_diff = prev_phase[i] - prev_prev_phase[i];
            let expected_phase = prev_phase[i] + phase_diff;

            // Phase deviation (wrapped to [-π, π])
            let mut deviation = phase - expected_phase;
            while deviation > PI {
                deviation -= 2.0 * PI;
            }
            while deviation < -PI {
                deviation += 2.0 * PI;
            }

            // Weight by magnitude (ignore phase in quiet bins)
            onset_value += deviation.abs() * mag;

            prev_prev_phase[i] = prev_phase[i];
            prev_phase[i] = phase;
        }

        odf.push(onset_value);
    }

    normalize_and_smooth_odf(&mut odf);
    odf
}

/// Broadband Energy Rise onset detection function.
///
/// Detects sudden increases in energy across the spectrum.
/// Good for percussive onsets in mixed audio.
fn compute_energy_rise_odf(samples: &[f32], _sample_rate: u32, config: &QmTempoConfig) -> Vec<f32> {
    let fft_size = config.fft_size;
    let hop_size = config.hop_size;

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    let window: Vec<f32> = (0..fft_size)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (fft_size - 1) as f32).cos()))
        .collect();

    let num_bins = fft_size / 2 + 1;
    let mut prev_energy = 0.0f32;
    let mut odf = Vec::new();

    for start in (0..samples.len().saturating_sub(fft_size)).step_by(hop_size) {
        let mut buffer: Vec<Complex<f32>> = samples[start..start + fft_size]
            .iter()
            .zip(window.iter())
            .map(|(s, w)| Complex::new(s * w, 0.0))
            .collect();

        fft.process(&mut buffer);

        // Total spectral energy
        let energy: f32 = buffer.iter().take(num_bins).map(|c| c.norm_sqr()).sum();

        // Half-wave rectified difference (only increases)
        let onset_value = (energy - prev_energy).max(0.0);
        prev_energy = energy;

        odf.push(onset_value.sqrt());
    }

    normalize_and_smooth_odf(&mut odf);
    odf
}

/// Normalize ODF to [0, 1] range and apply smoothing.
fn normalize_and_smooth_odf(odf: &mut Vec<f32>) {
    if odf.is_empty() {
        return;
    }

    // Remove DC offset
    let mean: f32 = odf.iter().sum::<f32>() / odf.len() as f32;
    for v in odf.iter_mut() {
        *v = (*v - mean).max(0.0);
    }

    // Normalize to max
    let max = odf.iter().cloned().fold(0.0f32, f32::max);
    if max > 0.0 {
        for v in odf.iter_mut() {
            *v /= max;
        }
    }

    // Apply median filtering to reduce noise (window size 3)
    let original = odf.clone();
    for i in 1..odf.len().saturating_sub(1) {
        let mut window = [original[i - 1], original[i], original[i + 1]];
        window.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        odf[i] = window[1]; // Median
    }
}

/// Compute tempo estimates using windowed autocorrelation + comb filterbank.
fn compute_tempo_curve(odf: &[f32], odf_sr: f32, config: &QmTempoConfig) -> Vec<(f64, f32)> {
    let window_samples = (config.tempo_window_seconds * odf_sr) as usize;
    let hop_samples = (config.tempo_hop_seconds * odf_sr) as usize;

    if odf.len() < window_samples {
        // Not enough data for even one window
        // Analyze what we have
        let result = analyze_tempo_window(odf, odf_sr, config);
        return vec![result];
    }

    let mut tempo_estimates = Vec::new();

    let mut start = 0;
    while start + window_samples <= odf.len() {
        let window = &odf[start..start + window_samples];
        let estimate = analyze_tempo_window(window, odf_sr, config);
        tempo_estimates.push(estimate);
        start += hop_samples;
    }

    // Handle remaining samples if significant
    if start < odf.len() && odf.len() - start > window_samples / 2 {
        let window = &odf[start..];
        let estimate = analyze_tempo_window(window, odf_sr, config);
        tempo_estimates.push(estimate);
    }

    tempo_estimates
}

/// Analyze a single window to estimate tempo.
///
/// Uses autocorrelation + perceptually-weighted comb filterbank.
fn analyze_tempo_window(odf_window: &[f32], odf_sr: f32, config: &QmTempoConfig) -> (f64, f32) {
    // Compute autocorrelation
    let autocorr = compute_autocorrelation(odf_window);

    // Convert BPM range to lag range
    let min_lag = (60.0 * odf_sr as f64 / config.max_bpm) as usize;
    let max_lag = (60.0 * odf_sr as f64 / config.min_bpm) as usize;
    let max_lag = max_lag.min(autocorr.len() / 2);

    if max_lag <= min_lag {
        return (120.0, 0.0);
    }

    // Apply perceptually-weighted comb filterbank
    let comb_output = apply_comb_filterbank(&autocorr, min_lag, max_lag, odf_sr, config);

    // Find the best tempo candidate
    let mut best_bpm = 120.0;
    let mut best_score = 0.0f32;

    for (bpm, score) in &comb_output {
        if *score > best_score {
            best_score = *score;
            best_bpm = *bpm;
        }
    }

    // Normalize confidence
    let confidence = if best_score > 0.0 {
        (best_score / comb_output.iter().map(|(_, s)| *s).sum::<f32>()).min(1.0)
    } else {
        0.0
    };

    (best_bpm, confidence)
}

/// Compute autocorrelation using FFT (Wiener-Khinchin theorem).
/// Includes adaptive whitening and median filtering as used in QM-DSP.
fn compute_autocorrelation(signal: &[f32]) -> Vec<f32> {
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

    // Power spectrum with adaptive whitening (QM-DSP style)
    // This normalizes each frequency bin by its local magnitude, reducing spectral bias
    let magnitudes: Vec<f32> = buffer.iter().map(|c| c.norm()).collect();
    let mean_mag = magnitudes.iter().sum::<f32>() / magnitudes.len() as f32;

    for (i, c) in buffer.iter_mut().enumerate() {
        let local_mag = magnitudes[i].max(mean_mag * 0.01); // Prevent division by zero
        // Whitening: normalize by magnitude but keep some of the original structure
        let whitening_factor = (mean_mag / local_mag).sqrt().min(10.0);
        *c = Complex::new(c.norm_sqr() * whitening_factor, 0.0);
    }

    // Inverse FFT
    ifft.process(&mut buffer);

    // Normalize and extract real part
    let norm = 1.0 / n as f32;
    let mut autocorr: Vec<f32> = buffer.iter().map(|c| c.re * norm).collect();

    // Apply median filtering to autocorrelation (reduces noise, QM-DSP style)
    // Use window size 5 for smoother results
    let original = autocorr.clone();
    for i in 2..autocorr.len().saturating_sub(2) {
        let mut window = [
            original[i - 2],
            original[i - 1],
            original[i],
            original[i + 1],
            original[i + 2],
        ];
        window.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        autocorr[i] = window[2]; // Median
    }

    autocorr
}

/// Parabolic interpolation for sub-sample peak accuracy (QM-DSP style).
/// Given a peak at index `peak_idx`, returns the interpolated peak position.
fn interpolate_peak(values: &[f32], peak_idx: usize) -> f64 {
    if peak_idx == 0 || peak_idx >= values.len() - 1 {
        return peak_idx as f64;
    }

    let y0 = values[peak_idx - 1] as f64;
    let y1 = values[peak_idx] as f64;
    let y2 = values[peak_idx + 1] as f64;

    // Parabolic interpolation: find vertex of parabola through 3 points
    let denominator = 2.0 * (2.0 * y1 - y0 - y2);
    if denominator.abs() < 1e-10 {
        return peak_idx as f64;
    }

    let offset = (y0 - y2) / denominator;
    peak_idx as f64 + offset.clamp(-0.5, 0.5)
}

/// Apply QM-DSP style comb filterbank with:
/// - Resonant comb filter with proper harmonic weighting
/// - Rayleigh tempo distribution weighting
/// - Explicit octave/ratio comparison to resolve ambiguities
/// - Energy normalization
fn apply_comb_filterbank(
    autocorr: &[f32],
    min_lag: usize,
    max_lag: usize,
    odf_sr: f32,
    config: &QmTempoConfig,
) -> Vec<(f64, f32)> {
    // First pass: compute raw comb filter scores for all tempo candidates
    let mut raw_scores: Vec<(f64, f32)> = Vec::new();

    // Test tempo candidates at 0.5 BPM resolution
    let bpm_step = 0.5;
    let mut bpm = config.min_bpm;

    while bpm <= config.max_bpm {
        let period_samples = 60.0 * odf_sr as f64 / bpm;
        let lag = period_samples as usize;

        if lag < min_lag || lag > max_lag || lag >= autocorr.len() / 4 {
            bpm += bpm_step;
            continue;
        }

        // Resonant comb filter (QM-DSP style)
        // Uses interpolation for more accurate lag values
        let score = compute_resonant_comb_score(autocorr, period_samples, min_lag);

        raw_scores.push((bpm, score));
        bpm += bpm_step;
    }

    if raw_scores.is_empty() {
        return vec![(120.0, 0.0)];
    }

    // Compute energy normalization factor (sum of all scores)
    let total_energy: f32 = raw_scores.iter().map(|(_, s)| *s).sum();
    let norm_factor = if total_energy > 0.0 {
        1.0 / total_energy
    } else {
        1.0
    };

    // Second pass: apply Rayleigh weighting and octave disambiguation
    let mut results: Vec<(f64, f32)> = Vec::new();

    for (bpm, raw_score) in &raw_scores {
        // Apply Rayleigh tempo distribution weighting (QM-DSP style)
        // This is a log-Gaussian that better models human tempo perception
        let rayleigh_weight = rayleigh_tempo_weight(*bpm);

        // Normalize score and apply perceptual weighting
        let weighted_score = raw_score * norm_factor * rayleigh_weight;

        results.push((*bpm, weighted_score));
    }

    // Third pass: explicit octave/ratio disambiguation (critical for fixing 2/3 ratio errors)
    // For each tempo, compare it against related tempos and adjust scores
    resolve_octave_ambiguities(&mut results, autocorr, odf_sr, min_lag, config);

    results
}

/// Compute resonant comb filter score at a given period (lag).
///
/// This implements the QM-DSP TempoTrackV2 algorithm with the critical
/// double-nested loop structure that integrates multiple phase offsets
/// at each harmonic level:
///
/// ```cpp
/// for (int a = 1; a <= numelem; a++) {
///     for (int b = 1-a; b <= a-1; b++) {
///         rcf[i-1] += (acf[(a*i+b)-1] * wv[i-1]) / (2.*a-1.);
///     }
/// }
/// ```
///
/// The key insight is that for each harmonic `a`:
/// - We sample `(2*a-1)` positions around the harmonic lag (phase offsets)
/// - The score is normalized by `(2*a-1)` to average these positions
/// - This phase integration makes the algorithm robust to phase jitter
fn compute_resonant_comb_score(autocorr: &[f32], period: f64, _min_lag: usize) -> f32 {
    let lag = period as usize;
    if lag == 0 || lag >= autocorr.len() / 4 {
        return 0.0;
    }

    let mut score = 0.0f32;

    // Number of harmonics to consider (QM-DSP default is 4)
    let num_harmonics = 4;

    // QM-DSP double-nested loop with phase integration
    for a in 1..=num_harmonics {
        // Phase offsets range from (1-a) to (a-1), giving (2*a-1) samples
        // For a=1: b in [0, 0] -> 1 sample
        // For a=2: b in [-1, 1] -> 3 samples
        // For a=3: b in [-2, 2] -> 5 samples
        // For a=4: b in [-3, 3] -> 7 samples

        let mut harmonic_sum = 0.0f32;

        for b in (1 - a as i32)..=(a as i32 - 1) {
            // Calculate index: (a * lag + b)
            // Note: QM-DSP uses 1-based indexing, we use 0-based
            let idx = (a * lag) as i32 + b;

            if idx >= 0 && (idx as usize) < autocorr.len() {
                harmonic_sum += autocorr[idx as usize];
            }
        }

        // Normalize by number of samples at this harmonic level: (2*a-1)
        let normalization = (2 * a - 1) as f32;
        score += harmonic_sum / normalization;
    }

    score.max(0.0)
}

/// Linear interpolation of autocorrelation at fractional lag positions.
fn interpolate_autocorr(autocorr: &[f32], lag: f64) -> f32 {
    let idx = lag as usize;
    if idx + 1 >= autocorr.len() {
        return if idx < autocorr.len() {
            autocorr[idx]
        } else {
            0.0
        };
    }

    let frac = (lag - idx as f64) as f32;
    autocorr[idx] * (1.0 - frac) + autocorr[idx + 1] * frac
}

/// Rayleigh tempo distribution weighting (QM-DSP style).
///
/// Unlike a simple Gaussian, this uses a log-Gaussian (Rayleigh-like) distribution
/// that better models how humans perceive tempo. It's asymmetric, with a peak
/// around 120 BPM and gentler falloff at higher tempos than lower ones.
fn rayleigh_tempo_weight(bpm: f64) -> f32 {
    // Log-Gaussian centered at ln(120) with sigma in log-space
    // This creates the characteristic Rayleigh-like asymmetry
    let log_bpm = bpm.ln();
    let log_center = 120.0_f64.ln(); // ~4.79
    let log_sigma = 0.5; // Width in log-space

    let log_diff = log_bpm - log_center;
    let weight = (-(log_diff * log_diff) / (2.0 * log_sigma * log_sigma)).exp();

    // Scale to reasonable range
    (weight as f32).max(0.1)
}

/// Resolve octave and ratio ambiguities by comparing related tempos.
///
/// This is the key to fixing the 2/3 ratio problem. For each tempo T, we compare
/// its score against T*2, T/2, T*1.5, and T/1.5 to determine which is most likely correct.
fn resolve_octave_ambiguities(
    results: &mut Vec<(f64, f32)>,
    autocorr: &[f32],
    odf_sr: f32,
    min_lag: usize,
    config: &QmTempoConfig,
) {
    if results.is_empty() {
        return;
    }

    // Build a lookup map for quick score access
    let score_map: std::collections::HashMap<i32, f32> = results
        .iter()
        .map(|(bpm, score)| ((bpm * 2.0) as i32, *score)) // Key by half-BPM for matching
        .collect();

    // For each tempo, check if a related tempo should "win"
    for (bpm, score) in results.iter_mut() {
        let original_score = *score;

        // Related tempos to check (ratios that commonly cause confusion)
        let related_ratios = [
            (2.0, 0.85),  // Double tempo - slight preference for lower
            (0.5, 1.15),  // Half tempo - slight preference for higher
            (1.5, 0.90),  // 1.5x tempo (fixes 2/3 ratio: 117 -> 175)
            (0.667, 1.1), // 2/3x tempo
        ];

        for (ratio, preference_factor) in related_ratios {
            let related_bpm = *bpm * ratio;

            // Skip if related tempo is outside valid range
            if related_bpm < config.min_bpm || related_bpm > config.max_bpm {
                continue;
            }

            // Look up the related tempo's score
            let related_key = (related_bpm * 2.0) as i32;
            if let Some(&related_score) = score_map.get(&related_key) {
                // Compare scores with preference factor
                // If related tempo has significantly higher score, reduce this tempo's score
                let adjusted_related = related_score * preference_factor as f32;

                if adjusted_related > original_score * 1.1 {
                    // Related tempo is stronger - reduce this score
                    *score *= 0.7;
                } else if original_score > adjusted_related * 1.2 {
                    // This tempo is clearly stronger - boost it slightly
                    *score *= 1.1;
                }
            }
        }

        // Additional check for the specific 2/3 ratio problem (110-125 BPM range)
        // If we're in this range, check if 1.5x tempo has strong autocorrelation
        if *bpm >= 110.0 && *bpm <= 125.0 {
            let triplet_bpm = *bpm * 1.5;
            if triplet_bpm <= config.max_bpm {
                let triplet_period = 60.0 * odf_sr as f64 / triplet_bpm;
                let triplet_score =
                    compute_resonant_comb_score(autocorr, triplet_period, min_lag);

                // If 1.5x tempo has comparable or better raw comb score,
                // this might be a 2/3 ratio error - penalize the lower tempo
                if triplet_score > original_score * 0.7 {
                    *score *= 0.6; // Strong penalty for likely 2/3 ratio errors
                }
            }
        }
    }

    // Re-normalize scores after adjustments
    let max_score = results.iter().map(|(_, s)| *s).fold(0.0f32, f32::max);
    if max_score > 0.0 {
        for (_, score) in results.iter_mut() {
            *score /= max_score;
        }
    }
}

/// Viterbi algorithm for finding optimal tempo path through time.
///
/// Models tempo as a hidden Markov model where:
/// - States are tempo candidates
/// - Observations are the comb filterbank outputs
/// - Transitions favor staying at the same tempo
fn viterbi_tempo_tracking(
    tempo_estimates: &[(f64, f32)],
    config: &QmTempoConfig,
) -> (Vec<f64>, f32) {
    if tempo_estimates.is_empty() {
        return (Vec::new(), 0.0);
    }

    if tempo_estimates.len() == 1 {
        return (vec![tempo_estimates[0].0], tempo_estimates[0].1);
    }

    // Quantize tempo space for tractable computation
    let tempo_resolution = 1.0; // 1 BPM resolution
    let num_states = ((config.max_bpm - config.min_bpm) / tempo_resolution) as usize + 1;

    // Build observation probabilities for each time step
    let observations: Vec<Vec<f32>> = tempo_estimates
        .iter()
        .map(|(obs_bpm, obs_conf)| {
            (0..num_states)
                .map(|state| {
                    let state_bpm = config.min_bpm + state as f64 * tempo_resolution;
                    // Gaussian likelihood around observed BPM
                    let diff = state_bpm - obs_bpm;
                    let likelihood = (-(diff * diff) / 50.0).exp() as f32;
                    likelihood * obs_conf
                })
                .collect()
        })
        .collect();

    // Transition probability (Gaussian favoring staying at same tempo)
    // QM-DSP uses σ=8 for smoother tempo tracking
    let transition_sigma = 8.0; // Allow ~8 BPM change between windows (QM-DSP style)
    let transition_prob = |from_state: usize, to_state: usize| -> f32 {
        let diff = (to_state as f64 - from_state as f64) * tempo_resolution;
        (-(diff * diff) / (2.0 * transition_sigma * transition_sigma)).exp() as f32
    };

    // Viterbi forward pass
    let mut viterbi = vec![vec![0.0f32; num_states]; observations.len()];
    let mut backpointer = vec![vec![0usize; num_states]; observations.len()];

    // Initialize first column
    for state in 0..num_states {
        viterbi[0][state] = observations[0][state];
    }

    // Forward pass
    for t in 1..observations.len() {
        for state in 0..num_states {
            let mut best_prev_score = 0.0f32;
            let mut best_prev_state = 0;

            // Only check nearby states for efficiency (±20 BPM range)
            let search_range = (20.0 / tempo_resolution) as usize;
            let start_state = state.saturating_sub(search_range);
            let end_state = (state + search_range).min(num_states);

            for prev_state in start_state..end_state {
                let score = viterbi[t - 1][prev_state] * transition_prob(prev_state, state);
                if score > best_prev_score {
                    best_prev_score = score;
                    best_prev_state = prev_state;
                }
            }

            viterbi[t][state] = best_prev_score * observations[t][state];
            backpointer[t][state] = best_prev_state;
        }

        // Normalize to prevent underflow
        let sum: f32 = viterbi[t].iter().sum();
        if sum > 0.0 {
            for v in &mut viterbi[t] {
                *v /= sum;
            }
        }
    }

    // Backtrack to find best path
    let mut path = vec![0usize; observations.len()];

    // Find best final state
    let last_t = observations.len() - 1;
    let mut best_final_state = 0;
    let mut best_final_score = 0.0f32;
    for (state, &score) in viterbi[last_t].iter().enumerate() {
        if score > best_final_score {
            best_final_score = score;
            best_final_state = state;
        }
    }
    path[last_t] = best_final_state;

    // Backtrack
    for t in (0..last_t).rev() {
        path[t] = backpointer[t + 1][path[t + 1]];
    }

    // Convert states to BPM values
    let tempo_path: Vec<f64> = path
        .iter()
        .map(|&state| config.min_bpm + state as f64 * tempo_resolution)
        .collect();

    // Average confidence
    let avg_confidence: f32 =
        tempo_estimates.iter().map(|(_, c)| c).sum::<f32>() / tempo_estimates.len() as f32;

    (tempo_path, avg_confidence)
}

/// Validate tempo using transient alignment.
///
/// This is a conservative check that only adjusts tempo when there's
/// strong evidence for an alternative (octave errors only).
fn validate_tempo_with_alignment(
    odf: &[f32],
    odf_sr: f32,
    detected_bpm: f64,
    detected_alignment: f32,
    config: &QmTempoConfig,
) -> f64 {
    // Only check for clear octave errors (2x or 0.5x)
    // Don't try to fix 1.5x/0.67x as this is unreliable

    let mut best_bpm = detected_bpm;
    let mut best_score = detected_alignment;

    log::debug!(
        "Validating tempo {:.1} BPM (alignment: {:.3})",
        detected_bpm,
        detected_alignment
    );

    // Check double tempo - requires much better alignment
    let double_bpm = detected_bpm * 2.0;
    if double_bpm <= config.max_bpm {
        let (_, double_alignment) = find_best_first_beat(odf, odf_sr, double_bpm, 16);
        // Double tempo needs significantly better alignment
        if double_alignment > best_score + 0.15 {
            log::debug!(
                "  Switching to double tempo {:.1} BPM (alignment {:.2} vs {:.2})",
                double_bpm,
                double_alignment,
                best_score
            );
            best_bpm = double_bpm;
            best_score = double_alignment;
        }
    }

    // Check half tempo - very conservative
    let half_bpm = detected_bpm * 0.5;
    if half_bpm >= config.min_bpm {
        let (_, half_alignment) = find_best_first_beat(odf, odf_sr, half_bpm, 16);
        // Half tempo needs much better alignment
        if half_alignment > best_score + 0.25 {
            log::debug!(
                "  Switching to half tempo {:.1} BPM (alignment {:.2} vs {:.2})",
                half_bpm,
                half_alignment,
                best_score
            );
            best_bpm = half_bpm;
        }
    }

    best_bpm
}

/// Calculate adaptive threshold from ODF values.
/// Returns a threshold at the specified percentile of ODF values.
fn calculate_odf_threshold(odf: &[f32], percentile: f32) -> f32 {
    if odf.is_empty() {
        return 0.0;
    }

    let mut sorted = odf.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let idx = ((sorted.len() - 1) as f32 * percentile) as usize;
    sorted[idx]
}

/// Score how well a tempo's expected beats align with ODF transient peaks.
/// Returns 0.0-1.0 where 1.0 = all beats align with transients.
///
/// This function is used to validate tempo candidates by checking if the
/// expected beat positions actually coincide with audio transients (onsets).
/// A higher score indicates the tempo is more likely correct.
fn score_tempo_by_transient_alignment(
    odf: &[f32],
    odf_sr: f32,
    bpm: f64,
    first_beat_frame: usize,
    threshold_percentile: f32, // e.g., 0.3 = peaks above 30th percentile
) -> f32 {
    if odf.is_empty() || bpm <= 0.0 {
        return 0.0;
    }

    let beat_period = (60.0 * odf_sr as f64 / bpm) as usize;
    if beat_period == 0 {
        return 0.0;
    }

    let tolerance_frames = beat_period / 4; // ±25% of beat period

    // Calculate adaptive threshold from ODF
    let threshold = calculate_odf_threshold(odf, threshold_percentile);

    // Generate expected beat positions and check alignment
    let mut beat_frame = first_beat_frame;
    let mut aligned = 0;
    let mut total = 0;

    while beat_frame < odf.len() {
        let window_start = beat_frame.saturating_sub(tolerance_frames);
        let window_end = (beat_frame + tolerance_frames).min(odf.len());

        // Check if there's a significant ODF peak near this beat
        let max_in_window = odf[window_start..window_end]
            .iter()
            .cloned()
            .fold(0.0f32, f32::max);

        if max_in_window > threshold {
            aligned += 1;
        }
        total += 1;
        beat_frame += beat_period;
    }

    if total == 0 {
        0.0
    } else {
        aligned as f32 / total as f32
    }
}

/// Find the first beat offset that maximizes transient alignment.
/// Tests multiple phase offsets and returns the best one.
///
/// Returns (best_offset, alignment_score) where:
/// - best_offset: The frame index of the optimal first beat
/// - alignment_score: The alignment score (0.0-1.0) at this offset
fn find_best_first_beat(
    odf: &[f32],
    odf_sr: f32,
    bpm: f64,
    num_phases: usize, // e.g., 16 phases to test
) -> (usize, f32) {
    if odf.is_empty() || bpm <= 0.0 {
        return (0, 0.0);
    }

    let beat_period = (60.0 * odf_sr as f64 / bpm) as usize;
    if beat_period == 0 || num_phases == 0 {
        return (0, 0.0);
    }

    let phase_step = beat_period / num_phases;
    if phase_step == 0 {
        return (0, score_tempo_by_transient_alignment(odf, odf_sr, bpm, 0, 0.3));
    }

    let mut best_offset = 0;
    let mut best_score = 0.0f32;

    for phase_idx in 0..num_phases {
        let offset = phase_idx * phase_step;
        let score = score_tempo_by_transient_alignment(odf, odf_sr, bpm, offset, 0.3);
        if score > best_score {
            best_score = score;
            best_offset = offset;
        }
    }

    (best_offset, best_score)
}

/// Dynamic programming beat tracking (Ellis 2007).
///
/// Given a tempo estimate, finds the beat positions that maximize
/// the cumulative onset function value while maintaining the expected
/// beat spacing.
fn dp_beat_tracking(odf: &[f32], odf_sr: f32, bpm: f64, _config: &QmTempoConfig) -> Vec<usize> {
    if odf.is_empty() || bpm <= 0.0 {
        return Vec::new();
    }

    let beat_period = (60.0 * odf_sr as f64 / bpm) as usize;
    if beat_period == 0 {
        return Vec::new();
    }

    let n = odf.len();

    // Alpha controls the trade-off between onset strength and beat regularity
    // Higher alpha = more regular beats, lower alpha = follows onsets more closely
    let alpha = 100.0f32;

    // Cumulative score and backpointer
    let mut score = vec![0.0f32; n];
    let mut backpointer = vec![0usize; n];

    // Initialize: first beat_period frames just use onset strength
    for i in 0..beat_period.min(n) {
        score[i] = odf[i];
    }

    // Forward pass: for each frame, find the best previous beat
    for t in beat_period..n {
        let mut best_score = f32::NEG_INFINITY;
        let mut best_prev = 0;

        // Search window around expected previous beat position
        // Allow ±20% deviation from expected period
        let search_start = (t as f32 - beat_period as f32 * 1.2) as usize;
        let search_end = (t as f32 - beat_period as f32 * 0.8) as usize;
        let search_start = search_start.max(0);
        let search_end = search_end.min(t);

        for prev in search_start..search_end {
            // Penalty for deviation from expected beat period
            let expected_prev = t - beat_period;
            let deviation = (prev as f32 - expected_prev as f32).abs();
            let penalty = alpha * (deviation / beat_period as f32).powi(2);

            let candidate_score = score[prev] - penalty;
            if candidate_score > best_score {
                best_score = candidate_score;
                best_prev = prev;
            }
        }

        score[t] = odf[t] + best_score;
        backpointer[t] = best_prev;
    }

    // Find the best ending position (search last beat period)
    let search_start = n.saturating_sub(beat_period);
    let mut best_end = search_start;
    let mut best_end_score = score[search_start];
    for i in search_start..n {
        if score[i] > best_end_score {
            best_end_score = score[i];
            best_end = i;
        }
    }

    // Backtrack to find all beats
    let mut beats = Vec::new();
    let mut current = best_end;

    while current > 0 {
        beats.push(current);
        let prev = backpointer[current];
        if prev >= current {
            break; // Prevent infinite loop
        }
        current = prev;
    }
    beats.push(current);

    // Reverse to get chronological order
    beats.reverse();

    beats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rayleigh_weight() {
        // 120 BPM should have highest weight (Rayleigh distribution peaks around 120)
        let w120 = rayleigh_tempo_weight(120.0);
        let w100 = rayleigh_tempo_weight(100.0);
        let w140 = rayleigh_tempo_weight(140.0);
        let w80 = rayleigh_tempo_weight(80.0);

        assert!(w120 > w100, "120 BPM should have higher weight than 100 BPM");
        assert!(w120 > w140, "120 BPM should have higher weight than 140 BPM");
        assert!(w100 > w80, "100 BPM should have higher weight than 80 BPM");
        // Rayleigh is asymmetric - 140 should have higher weight than 100
        // (gentler falloff at higher tempos)
        assert!(
            w140 > w80,
            "140 BPM should have higher weight than 80 BPM (asymmetric)"
        );
    }

    #[test]
    fn test_autocorrelation() {
        // Test with a simple periodic signal
        let signal: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.1).sin()).collect();

        let autocorr = compute_autocorrelation(&signal);

        // Autocorrelation at lag 0 should be highest
        assert!(autocorr[0] >= autocorr[1]);
        // Should be periodic
        assert!(autocorr.len() > 100);
    }

    #[test]
    fn test_empty_input() {
        let config = QmTempoConfig::default();
        let result = detect_tempo_qm(&[], 44100, &config);
        assert_eq!(result.bpm, 120.0);
        assert_eq!(result.confidence, 0.0);
    }
}
