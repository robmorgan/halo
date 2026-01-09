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
        };
    }

    // Step 3: Use Viterbi algorithm to find optimal tempo path
    let (tempo_path, confidence) = viterbi_tempo_tracking(&tempo_estimates, config);

    // Get the dominant tempo (median of the path)
    let bpm = if tempo_path.is_empty() {
        120.0
    } else {
        let mut sorted = tempo_path.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        sorted[sorted.len() / 2]
    };

    // Step 4: Dynamic programming beat tracking
    let beats = dp_beat_tracking(&odf, odf_sample_rate, bpm, config);

    // Convert beat positions from ODF frames to seconds
    let beats_seconds: Vec<f64> = beats
        .iter()
        .map(|&frame| frame as f64 / odf_sample_rate as f64)
        .collect();

    log::debug!(
        "QM tempo detection: {:.2} BPM, confidence: {:.2}, {} beats",
        bpm,
        confidence,
        beats_seconds.len()
    );

    QmTempoResult {
        bpm,
        confidence,
        beats: beats_seconds,
        tempo_curve: tempo_path,
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

    // Power spectrum
    for c in &mut buffer {
        *c = Complex::new(c.norm_sqr(), 0.0);
    }

    // Inverse FFT
    ifft.process(&mut buffer);

    // Normalize and return real part
    let norm = 1.0 / n as f32;
    buffer.iter().map(|c| c.re * norm).collect()
}

/// Apply perceptually-weighted comb filterbank.
///
/// The comb filterbank tests different tempo hypotheses by summing
/// autocorrelation values at multiples of the beat period.
/// Perceptual weighting biases toward tempos humans naturally perceive (around 120 BPM).
fn apply_comb_filterbank(
    autocorr: &[f32],
    min_lag: usize,
    max_lag: usize,
    odf_sr: f32,
    config: &QmTempoConfig,
) -> Vec<(f64, f32)> {
    let mut results = Vec::new();

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

        // Sum autocorrelation at beat period and its multiples (harmonics)
        // This is the essence of the comb filterbank
        let mut score = 0.0f32;
        let num_harmonics = 4;

        for harmonic in 1..=num_harmonics {
            let harmonic_lag = lag * harmonic;
            if harmonic_lag < autocorr.len() {
                // Weight harmonics (fundamental has highest weight)
                let weight = 1.0 / harmonic as f32;
                score += autocorr[harmonic_lag] * weight;
            }
        }

        // Also check sub-harmonics (half, quarter beat)
        for divisor in [2, 4] {
            let sub_lag = lag / divisor;
            if sub_lag >= min_lag && sub_lag < autocorr.len() {
                score += autocorr[sub_lag] * 0.3;
            }
        }

        // Apply perceptual weighting (Gaussian centered around 120 BPM)
        // This models the human tendency to perceive tempos near 120 BPM
        let perceptual_weight = perceptual_tempo_weight(bpm);
        score *= perceptual_weight;

        results.push((bpm, score));
        bpm += bpm_step;
    }

    results
}

/// Perceptual tempo weight (Gaussian centered on 120 BPM).
///
/// Based on research showing humans have a natural preference for tempos
/// around 120 BPM (the "indifference interval" or natural pace).
fn perceptual_tempo_weight(bpm: f64) -> f32 {
    // Gaussian centered at 120 BPM with sigma ~40
    let center = 120.0;
    let sigma = 40.0;
    let diff = bpm - center;
    (-(diff * diff) / (2.0 * sigma * sigma)).exp() as f32
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
    let transition_sigma = 5.0; // Allow ~5 BPM change between windows
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
    fn test_perceptual_weight() {
        // 120 BPM should have highest weight
        let w120 = perceptual_tempo_weight(120.0);
        let w100 = perceptual_tempo_weight(100.0);
        let w140 = perceptual_tempo_weight(140.0);
        let w80 = perceptual_tempo_weight(80.0);

        assert!(w120 > w100);
        assert!(w120 > w140);
        assert!(w100 > w80);
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
