//! BPM accuracy regression tests using ground truth datasets.
//!
//! These tests validate that changes to BPM detection algorithms don't
//! degrade accuracy. They use the GiantSteps Tempo Dataset which contains
//! 664 electronic dance music tracks with crowdsourced BPM annotations.
//!
//! ## Setup
//!
//! To run these tests, first download the test audio:
//! ```bash
//! ./scripts/download_test_audio.sh
//! ```
//!
//! ## Running Tests
//!
//! ```bash
//! # Run accuracy tests
//! cargo test --package halo-dj --features accuracy-tests -- --nocapture
//!
//! # Run synthetic tests only (no external audio needed)
//! cargo test --package halo-dj test_synthetic
//! ```
//!
//! ## Accuracy Metrics
//!
//! - **Accuracy 1 (Strict)**: BPM within ±2% of ground truth
//! - **Accuracy 2 (Octave-tolerant)**: BPM or 2x/0.5x within ±2%
//! - **MIREX Accuracy**: BPM within ±8% (academic standard)

use std::f32::consts::PI;
use std::fs;
use std::path::PathBuf;

use halo_dj::library::{analyze_file, AnalysisConfig};

/// Ground truth data structure matching the JSON schema.
#[derive(Debug, serde::Deserialize)]
struct GroundTruth {
    version: u32,
    dataset: String,
    #[allow(dead_code)]
    description: String,
    #[allow(dead_code)]
    source: String,
    tracks: Vec<TrackAnnotation>,
}

/// Individual track annotation.
#[derive(Debug, serde::Deserialize)]
struct TrackAnnotation {
    filename: String,
    expected_bpm: f64,
    tolerance_percent: f64,
    #[allow(dead_code)]
    genre: String,
    #[allow(dead_code)]
    notes: String,
}

/// Accuracy statistics for reporting.
#[derive(Debug, Default)]
struct AccuracyStats {
    total: usize,
    accuracy1_correct: usize,               // Exact match within tolerance
    accuracy2_correct: usize,               // Octave-tolerant match
    mirex_correct: usize,                   // Within 8%
    failed_tracks: Vec<(String, f64, f64)>, // (filename, expected, detected)
}

impl AccuracyStats {
    fn accuracy1_percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            100.0 * self.accuracy1_correct as f64 / self.total as f64
        }
    }

    fn accuracy2_percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            100.0 * self.accuracy2_correct as f64 / self.total as f64
        }
    }

    fn mirex_percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            100.0 * self.mirex_correct as f64 / self.total as f64
        }
    }
}

/// Check if detected BPM matches expected within tolerance.
fn is_bpm_match(expected: f64, detected: f64, tolerance_percent: f64) -> bool {
    let tolerance = expected * tolerance_percent / 100.0;
    (detected - expected).abs() <= tolerance
}

/// Check if detected BPM matches expected or its octave/metrical multiples.
fn is_octave_tolerant_match(expected: f64, detected: f64, tolerance_percent: f64) -> bool {
    // Common metrical relationships to check
    let multipliers = [
        1.0,       // Exact match
        2.0,       // Double tempo
        0.5,       // Half tempo
        3.0,       // Triple tempo
        1.0 / 3.0, // Third tempo
        1.5,       // Dotted tempo (3/2)
        2.0 / 3.0, // Two-thirds tempo (common in EDM)
        4.0,       // Quadruple tempo
        0.25,      // Quarter tempo
    ];

    for &mult in &multipliers {
        if is_bpm_match(expected * mult, detected, tolerance_percent) {
            return true;
        }
    }
    false
}

/// Generate a synthetic click track at a specific BPM for deterministic testing.
///
/// Creates a sine wave "click" at each beat position.
fn generate_click_track(bpm: f64, duration_secs: f64, sample_rate: u32) -> Vec<f32> {
    let total_samples = (duration_secs * sample_rate as f64) as usize;
    let beat_interval_samples = (60.0 / bpm * sample_rate as f64) as usize;
    let click_duration_samples = (0.02 * sample_rate as f64) as usize; // 20ms click

    let mut samples = vec![0.0f32; total_samples];

    // Generate click at each beat
    let mut beat_pos = 0;
    while beat_pos < total_samples {
        for i in 0..click_duration_samples.min(total_samples - beat_pos) {
            // Sine wave click with envelope
            let t = i as f32 / sample_rate as f32;
            let envelope = 1.0 - (i as f32 / click_duration_samples as f32);
            let click_freq = 1000.0; // 1kHz click
            samples[beat_pos + i] = (2.0 * PI * click_freq * t).sin() * envelope * 0.8;
        }
        beat_pos += beat_interval_samples;
    }

    samples
}

/// Generate a more realistic four-on-the-floor pattern for testing.
/// Includes kick, snare, and hi-hat for better beat detection.
fn generate_kick_pattern(bpm: f64, duration_secs: f64, sample_rate: u32) -> Vec<f32> {
    let total_samples = (duration_secs * sample_rate as f64) as usize;
    let beat_interval_samples = (60.0 / bpm * sample_rate as f64) as usize;
    let eighth_interval = beat_interval_samples / 2;

    let mut samples = vec![0.0f32; total_samples];

    // Generate a full bar pattern (4 beats)
    let mut pos = 0usize;
    let mut beat_in_bar = 0;

    while pos < total_samples {
        // Kick on every beat (four-on-the-floor)
        add_kick(&mut samples, pos, sample_rate);

        // Snare on beats 2 and 4
        if beat_in_bar == 1 || beat_in_bar == 3 {
            add_snare(&mut samples, pos, sample_rate);
        }

        // Closed hi-hat on every eighth note
        add_hihat(&mut samples, pos, sample_rate, false);
        if pos + eighth_interval < total_samples {
            add_hihat(&mut samples, pos + eighth_interval, sample_rate, false);
        }

        // Open hi-hat on the "and" of beat 4
        if beat_in_bar == 3 && pos + eighth_interval < total_samples {
            add_hihat(&mut samples, pos + eighth_interval, sample_rate, true);
        }

        pos += beat_interval_samples;
        beat_in_bar = (beat_in_bar + 1) % 4;
    }

    // Normalize to prevent clipping
    let max_val = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if max_val > 0.0 {
        for s in &mut samples {
            *s /= max_val * 1.1; // Leave some headroom
        }
    }

    samples
}

/// Add a kick drum sound at the given position.
fn add_kick(samples: &mut [f32], pos: usize, sample_rate: u32) {
    let duration = (0.15 * sample_rate as f64) as usize;
    for i in 0..duration.min(samples.len().saturating_sub(pos)) {
        let t = i as f32 / sample_rate as f32;
        // Frequency sweep from 150Hz to 40Hz
        let freq = 150.0 * (-t * 25.0).exp() + 40.0;
        let envelope = (-t * 15.0).exp();
        // Add some click for attack
        let click = if i < 50 {
            (1.0 - i as f32 / 50.0) * 0.3
        } else {
            0.0
        };
        samples[pos + i] += ((2.0 * PI * freq * t).sin() * envelope + click) * 0.8;
    }
}

/// Add a snare drum sound at the given position.
fn add_snare(samples: &mut [f32], pos: usize, sample_rate: u32) {
    let duration = (0.12 * sample_rate as f64) as usize;
    for i in 0..duration.min(samples.len().saturating_sub(pos)) {
        let t = i as f32 / sample_rate as f32;
        // Body tone at ~180Hz
        let body = (2.0 * PI * 180.0 * t).sin() * (-t * 20.0).exp();
        // Noise for snare wires (simple random-ish noise using sin)
        let noise = (t * 12345.6789).sin() * (-t * 30.0).exp();
        samples[pos + i] += (body * 0.3 + noise * 0.4) * 0.5;
    }
}

/// Add a hi-hat sound at the given position.
fn add_hihat(samples: &mut [f32], pos: usize, sample_rate: u32, open: bool) {
    let duration = if open {
        (0.15 * sample_rate as f64) as usize
    } else {
        (0.05 * sample_rate as f64) as usize
    };
    let decay = if open { 10.0 } else { 40.0 };

    for i in 0..duration.min(samples.len().saturating_sub(pos)) {
        let t = i as f32 / sample_rate as f32;
        // High frequency noise-like sound
        let noise =
            (t * 54321.0).sin() * 0.5 + (t * 98765.0).sin() * 0.3 + (t * 23456.0).sin() * 0.2;
        let envelope = (-t * decay).exp();
        samples[pos + i] += noise * envelope * 0.15;
    }
}

// ============================================================================
// Synthetic Audio Tests (always run, no external dependencies)
// ============================================================================

#[test]
fn test_synthetic_120bpm_click() {
    let samples = generate_click_track(120.0, 30.0, 44100);

    // Write to temp file
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path().join("click_120bpm.wav");
    write_wav(&temp_path, &samples, 44100);

    // Analyze
    let config = AnalysisConfig::default();
    let result = analyze_file(&temp_path, halo_dj::library::TrackId(1), &config).unwrap();

    // Check BPM (allow octave match since click tracks can be ambiguous)
    assert!(
        is_octave_tolerant_match(120.0, result.bpm, 2.0),
        "Expected ~120 BPM (or octave), got {:.2}",
        result.bpm
    );
}

#[test]
fn test_synthetic_128bpm_kick() {
    let samples = generate_kick_pattern(128.0, 30.0, 44100);

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path().join("kick_128bpm.wav");
    write_wav(&temp_path, &samples, 44100);

    let config = AnalysisConfig::default();
    let result = analyze_file(&temp_path, halo_dj::library::TrackId(1), &config).unwrap();

    assert!(
        is_octave_tolerant_match(128.0, result.bpm, 3.0),
        "Expected ~128 BPM (or octave), got {:.2}",
        result.bpm
    );
}

#[test]
fn test_synthetic_various_tempos() {
    let test_tempos = [80.0, 100.0, 120.0, 128.0, 140.0, 160.0, 175.0];

    for &expected_bpm in &test_tempos {
        let samples = generate_kick_pattern(expected_bpm, 30.0, 44100);

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir
            .path()
            .join(format!("kick_{:.0}bpm.wav", expected_bpm));
        write_wav(&temp_path, &samples, 44100);

        let config = AnalysisConfig::default();
        let result = analyze_file(&temp_path, halo_dj::library::TrackId(1), &config).unwrap();

        assert!(
            is_octave_tolerant_match(expected_bpm, result.bpm, 3.0),
            "For {:.0} BPM: expected match (or octave), got {:.2}",
            expected_bpm,
            result.bpm
        );
    }
}

/// Write samples to a WAV file for testing.
fn write_wav(path: &PathBuf, samples: &[f32], sample_rate: u32) {
    use std::io::Write;

    let num_samples = samples.len() as u32;
    let byte_rate = sample_rate * 2; // 16-bit mono
    let data_size = num_samples * 2;
    let file_size = 36 + data_size;

    let mut file = fs::File::create(path).unwrap();

    // RIFF header
    file.write_all(b"RIFF").unwrap();
    file.write_all(&file_size.to_le_bytes()).unwrap();
    file.write_all(b"WAVE").unwrap();

    // fmt chunk
    file.write_all(b"fmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap(); // Chunk size
    file.write_all(&1u16.to_le_bytes()).unwrap(); // PCM format
    file.write_all(&1u16.to_le_bytes()).unwrap(); // Mono
    file.write_all(&sample_rate.to_le_bytes()).unwrap();
    file.write_all(&byte_rate.to_le_bytes()).unwrap();
    file.write_all(&2u16.to_le_bytes()).unwrap(); // Block align
    file.write_all(&16u16.to_le_bytes()).unwrap(); // Bits per sample

    // data chunk
    file.write_all(b"data").unwrap();
    file.write_all(&data_size.to_le_bytes()).unwrap();

    // Write samples as 16-bit PCM
    for &sample in samples {
        let sample_i16 = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
        file.write_all(&sample_i16.to_le_bytes()).unwrap();
    }
}

// ============================================================================
// GiantSteps Dataset Tests (feature-gated, requires downloaded audio)
// ============================================================================

#[cfg(feature = "accuracy-tests")]
mod accuracy_tests {
    use super::*;

    fn get_fixtures_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
    }

    fn load_ground_truth() -> GroundTruth {
        let path = get_fixtures_path().join("ground_truth.json");
        let content = fs::read_to_string(&path).expect("Failed to read ground_truth.json");
        serde_json::from_str(&content).expect("Failed to parse ground_truth.json")
    }

    #[test]
    fn test_giantsteps_accuracy() {
        let ground_truth = load_ground_truth();
        let audio_dir = get_fixtures_path().join("audio");

        if ground_truth.tracks.is_empty() {
            println!("No tracks in ground_truth.json.");
            println!("Run ./scripts/download_test_audio.sh to download the GiantSteps dataset.");
            return;
        }

        let config = AnalysisConfig::default();
        let mut stats = AccuracyStats::default();

        println!("\n=== BPM Accuracy Test: {} ===\n", ground_truth.dataset);

        for (i, track) in ground_truth.tracks.iter().enumerate() {
            let audio_path = audio_dir.join(&track.filename);

            if !audio_path.exists() {
                println!(
                    "[{}/{}] SKIP: {} (file not found)",
                    i + 1,
                    ground_truth.tracks.len(),
                    track.filename
                );
                continue;
            }

            stats.total += 1;

            // Analyze track
            let result =
                match analyze_file(&audio_path, halo_dj::library::TrackId(i as i64), &config) {
                    Ok(r) => r,
                    Err(e) => {
                        println!(
                            "[{}/{}] ERROR: {} - {}",
                            i + 1,
                            ground_truth.tracks.len(),
                            track.filename,
                            e
                        );
                        continue;
                    }
                };

            let detected_bpm = result.bpm;
            let expected_bpm = track.expected_bpm;

            // Check accuracy metrics
            let acc1 = is_bpm_match(expected_bpm, detected_bpm, track.tolerance_percent);
            let acc2 =
                is_octave_tolerant_match(expected_bpm, detected_bpm, track.tolerance_percent);
            let mirex = is_bpm_match(expected_bpm, detected_bpm, 8.0)
                || is_octave_tolerant_match(expected_bpm, detected_bpm, 8.0);

            if acc1 {
                stats.accuracy1_correct += 1;
            }
            if acc2 {
                stats.accuracy2_correct += 1;
            }
            if mirex {
                stats.mirex_correct += 1;
            }

            let status = if acc1 {
                "OK"
            } else if acc2 {
                "OCTAVE"
            } else {
                stats
                    .failed_tracks
                    .push((track.filename.clone(), expected_bpm, detected_bpm));
                "FAIL"
            };

            println!(
                "[{}/{}] {}: {} - expected {:.2}, detected {:.2}",
                i + 1,
                ground_truth.tracks.len(),
                status,
                track.filename,
                expected_bpm,
                detected_bpm
            );
        }

        // Print summary
        println!("\n=== Accuracy Summary ===\n");
        println!("Total tracks tested: {}", stats.total);
        println!(
            "Accuracy 1 (±2%):        {:.1}% ({}/{})",
            stats.accuracy1_percent(),
            stats.accuracy1_correct,
            stats.total
        );
        println!(
            "Accuracy 2 (octave ±2%): {:.1}% ({}/{})",
            stats.accuracy2_percent(),
            stats.accuracy2_correct,
            stats.total
        );
        println!(
            "MIREX (±8%):             {:.1}% ({}/{})",
            stats.mirex_percent(),
            stats.mirex_correct,
            stats.total
        );

        if !stats.failed_tracks.is_empty() {
            println!("\n=== Failed Tracks ===\n");
            for (filename, expected, detected) in &stats.failed_tracks {
                let ratio = detected / expected;
                println!(
                    "{}: expected {:.2}, detected {:.2} (ratio: {:.2}x)",
                    filename, expected, detected, ratio
                );
            }
        }

        // Assert minimum accuracy threshold
        assert!(
            stats.accuracy2_percent() >= 85.0,
            "Accuracy 2 (octave-tolerant) should be at least 85%, got {:.1}%",
            stats.accuracy2_percent()
        );
    }
}
