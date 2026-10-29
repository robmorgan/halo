// Adapted from timestretch-rs v0.15.0 (MIT, see LICENSES/timestretch.txt).
//! LR4 crossover retained for Halo's isolator EQ.

use std::f64::consts::PI;

/// Butterworth Q factor (1/sqrt(2)) for maximally-flat magnitude response.
const BUTTERWORTH_Q: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// A single biquad (second-order IIR) filter section.
///
/// Implements the Direct Form I difference equation:
///   y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]
///
/// Coefficients are pre-normalized by a0.
#[derive(Debug, Clone)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    // Input delay line
    x1: f64,
    x2: f64,
    // Output delay line
    y1: f64,
    y2: f64,
}

impl Biquad {
    /// Creates a 2nd-order Butterworth low-pass biquad.
    fn lowpass(freq: f64, sample_rate: u32) -> Self {
        Self::lowpass_q(freq, sample_rate, BUTTERWORTH_Q)
    }

    /// Creates a 2nd-order low-pass biquad with an explicit Q.
    fn lowpass_q(freq: f64, sample_rate: u32, q: f64) -> Self {
        let w0 = 2.0 * PI * freq / sample_rate as f64;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let a0 = 1.0 + alpha;
        let b0 = (1.0 - cos_w0) / 2.0 / a0;
        let b1 = (1.0 - cos_w0) / a0;
        let b2 = (1.0 - cos_w0) / 2.0 / a0;
        let a1 = -2.0 * cos_w0 / a0;
        let a2 = (1.0 - alpha) / a0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Creates a 2nd-order Butterworth high-pass biquad.
    fn highpass(freq: f64, sample_rate: u32) -> Self {
        Self::highpass_q(freq, sample_rate, BUTTERWORTH_Q)
    }

    /// Creates a 2nd-order high-pass biquad with an explicit Q.
    fn highpass_q(freq: f64, sample_rate: u32, q: f64) -> Self {
        let w0 = 2.0 * PI * freq / sample_rate as f64;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let a0 = 1.0 + alpha;
        let b0 = (1.0 + cos_w0) / 2.0 / a0;
        let b1 = -(1.0 + cos_w0) / a0;
        let b2 = (1.0 + cos_w0) / 2.0 / a0;
        let a1 = -2.0 * cos_w0 / a0;
        let a2 = (1.0 - alpha) / a0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Processes a single sample through the biquad filter.
    #[inline]
    fn process_sample(&mut self, input: f64) -> f64 {
        let output = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;
        output
    }

    /// Resets all delay line state to zero.
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// 4th-order Linkwitz-Riley (LR4) crossover filter (24 dB/oct slope).
///
/// Splits an input signal into low-pass and high-pass bands at a specified
/// crossover frequency. The LR4 topology cascades two 2nd-order Butterworth
/// filters, producing a flat magnitude sum at the crossover point.
pub struct LR4Crossover {
    /// Two cascaded 2nd-order Butterworth low-pass filters.
    low_pass: [Biquad; 2],
    /// Two cascaded 2nd-order Butterworth high-pass filters.
    high_pass: [Biquad; 2],
}

impl LR4Crossover {
    /// Creates a new LR4 crossover at the specified frequency.
    ///
    /// # Arguments
    ///
    /// * `crossover_freq` - Crossover frequency in Hz
    /// * `sample_rate` - Audio sample rate in Hz
    pub fn new(crossover_freq: f64, sample_rate: u32) -> Self {
        Self {
            low_pass: [
                Biquad::lowpass(crossover_freq, sample_rate),
                Biquad::lowpass(crossover_freq, sample_rate),
            ],
            high_pass: [
                Biquad::highpass(crossover_freq, sample_rate),
                Biquad::highpass(crossover_freq, sample_rate),
            ],
        }
    }

    /// Processes a single sample, returning (low, high) band outputs.
    ///
    /// The input is split into two complementary frequency bands at the
    /// crossover frequency set during construction.
    #[inline]
    pub fn process_sample(&mut self, input: f32) -> (f32, f32) {
        let x = input as f64;

        // Cascade two Butterworth LP stages for 4th-order LR low-pass
        let lp_stage1 = self.low_pass[0].process_sample(x);
        let low = self.low_pass[1].process_sample(lp_stage1);

        // Cascade two Butterworth HP stages for 4th-order LR high-pass
        let hp_stage1 = self.high_pass[0].process_sample(x);
        let high = self.high_pass[1].process_sample(hp_stage1);

        (low as f32, high as f32)
    }

    /// Resets all filter state to zero.
    ///
    /// Call this when processing a new, discontinuous signal to prevent
    /// transient artifacts from stale state.
    pub fn reset(&mut self) {
        for bq in &mut self.low_pass {
            bq.reset();
        }
        for bq in &mut self.high_pass {
            bq.reset();
        }
    }
}
