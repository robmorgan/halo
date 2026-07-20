//! Per-deck channel-strip DSP: 3-band isolator EQ and a resonant LP/HP
//! filter, both running inside the audio callback (allocation-free,
//! per-sample gain smoothing against zipper noise).

use timestretch::core::crossover::LR4Crossover;

/// Low/mid crossover of the isolator EQ.
const EQ_LOW_HZ: f64 = 250.0;
/// Mid/high crossover of the isolator EQ.
const EQ_HIGH_HZ: f64 = 2_600.0;
/// Gain smoothing time constant in seconds.
const SMOOTH_SECS: f32 = 0.005;
/// Frames per filter-coefficient update during cutoff sweeps.
const FILTER_SUBBLOCK: usize = 64;
/// DJ filter resonance (slightly above Butterworth for a gentle sweep bump).
const FILTER_Q: f64 = 1.05;
/// Cutoff sweep range, mapped log₂ from the normalized 0..1 knob.
const FILTER_MIN_HZ: f64 = 20.0;
const FILTER_MAX_HZ: f64 = 20_000.0;

/// Map a normalized cutoff (0..1) to Hz, log-scaled 20 Hz → 20 kHz. Shared
/// by the filter DSP and the UI (knob hover readout).
pub fn filter_cutoff_hz(normalized: f32) -> f64 {
    FILTER_MIN_HZ * (FILTER_MAX_HZ / FILTER_MIN_HZ).powf(normalized.clamp(0.0, 1.0) as f64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    Off,
    LowPass,
    HighPass,
}

impl FilterMode {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => FilterMode::LowPass,
            2 => FilterMode::HighPass,
            _ => FilterMode::Off,
        }
    }
}

/// Target parameters for one block, read from the deck's atomics.
#[derive(Debug, Clone, Copy)]
pub struct StripParams {
    /// Band gains, linear: 0 = kill, 1 = unity, 2 = +6 dB.
    pub eq: [f32; 3],
    pub filter_mode: FilterMode,
    /// Normalized cutoff 0..1 (log-mapped 20 Hz → 20 kHz).
    pub cutoff: f32,
}

/// 3-band isolator: two LR4 crossovers per channel (bands re-sum to a true
/// allpass at unity), per-sample smoothed band gains.
struct IsolatorEq {
    /// Per channel: (low/mid split, mid/high split).
    xovers: [(LR4Crossover, LR4Crossover); 2],
    gains: [f32; 3],
    alpha: f32,
}

impl IsolatorEq {
    fn new(sample_rate: u32) -> Self {
        let make = || {
            (
                LR4Crossover::new(EQ_LOW_HZ, sample_rate),
                LR4Crossover::new(EQ_HIGH_HZ, sample_rate),
            )
        };
        Self {
            xovers: [make(), make()],
            gains: [1.0; 3],
            alpha: 1.0 - (-1.0 / (SMOOTH_SECS * sample_rate as f32)).exp(),
        }
    }

    fn process(&mut self, buf: &mut [f32], targets: [f32; 3]) {
        for frame in buf.chunks_exact_mut(2) {
            for (g, t) in self.gains.iter_mut().zip(targets) {
                *g += (t - *g) * self.alpha;
            }
            for (ch, sample) in frame.iter_mut().enumerate() {
                let (low_mid, mid_high) = &mut self.xovers[ch];
                let (low, upper) = low_mid.process_sample(*sample);
                let (mid, high) = mid_high.process_sample(upper);
                *sample = low * self.gains[0] + mid * self.gains[1] + high * self.gains[2];
            }
        }
    }

    fn reset(&mut self) {
        for (a, b) in &mut self.xovers {
            a.reset();
            b.reset();
        }
    }
}

/// RBJ biquad, Direct Form I, per-channel state with shared coefficients.
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    /// (x1, x2, y1, y2) per channel.
    state: [[f64; 4]; 2],
}

impl Biquad {
    fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            state: [[0.0; 4]; 2],
        }
    }

    fn set_lowpass(&mut self, freq: f64, sample_rate: f64, q: f64) {
        let w0 = std::f64::consts::TAU * (freq / sample_rate).min(0.49);
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);
        let a0 = 1.0 + alpha;
        self.b0 = (1.0 - cos_w0) / 2.0 / a0;
        self.b1 = (1.0 - cos_w0) / a0;
        self.b2 = self.b0;
        self.a1 = -2.0 * cos_w0 / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn set_highpass(&mut self, freq: f64, sample_rate: f64, q: f64) {
        let w0 = std::f64::consts::TAU * (freq / sample_rate).min(0.49);
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);
        let a0 = 1.0 + alpha;
        self.b0 = (1.0 + cos_w0) / 2.0 / a0;
        self.b1 = -(1.0 + cos_w0) / a0;
        self.b2 = self.b0;
        self.a1 = -2.0 * cos_w0 / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    #[inline]
    fn process_sample(&mut self, ch: usize, x: f64) -> f64 {
        let s = &mut self.state[ch];
        let y = self.b0 * x + self.b1 * s[0] + self.b2 * s[1] - self.a1 * s[2] - self.a2 * s[3];
        s[1] = s[0];
        s[0] = x;
        s[3] = s[2];
        s[2] = y;
        y
    }

    fn reset(&mut self) {
        self.state = [[0.0; 4]; 2];
    }
}

/// Sweepable LP/HP DJ filter: one resonant biquad per channel pair, with the
/// cutoff smoothed and coefficients refreshed every [`FILTER_SUBBLOCK`]
/// frames so sweeps stay zipper-free.
struct DjFilter {
    biquad: Biquad,
    mode: FilterMode,
    /// Smoothed normalized cutoff.
    cutoff: f32,
    alpha: f32,
    sample_rate: f64,
}

impl DjFilter {
    fn new(sample_rate: u32) -> Self {
        Self {
            biquad: Biquad::identity(),
            mode: FilterMode::Off,
            cutoff: 1.0,
            // Smoothing steps happen once per sub-block, not per sample.
            alpha: 1.0 - (-(FILTER_SUBBLOCK as f32) / (SMOOTH_SECS * sample_rate as f32)).exp(),
            sample_rate: sample_rate as f64,
        }
    }

    fn process(&mut self, buf: &mut [f32], mode: FilterMode, target_cutoff: f32) {
        if mode != self.mode {
            // Mode flips restart the filter cleanly at the new response.
            self.mode = mode;
            self.cutoff = target_cutoff;
            self.biquad.reset();
        }
        if self.mode == FilterMode::Off {
            return;
        }

        for block in buf.chunks_mut(FILTER_SUBBLOCK * 2) {
            self.cutoff += (target_cutoff - self.cutoff) * self.alpha;
            let hz = filter_cutoff_hz(self.cutoff);
            match self.mode {
                FilterMode::LowPass => self.biquad.set_lowpass(hz, self.sample_rate, FILTER_Q),
                FilterMode::HighPass => self.biquad.set_highpass(hz, self.sample_rate, FILTER_Q),
                FilterMode::Off => unreachable!(),
            }
            for frame in block.chunks_exact_mut(2) {
                for (ch, sample) in frame.iter_mut().enumerate() {
                    *sample = self.biquad.process_sample(ch, *sample as f64) as f32;
                }
            }
        }
    }

    fn reset(&mut self) {
        self.biquad.reset();
    }
}

/// One deck's post-engine DSP chain: isolator EQ then filter. Gains
/// (trim/fader/crossfader) stay in the mixer where they always were — for a
/// linear chain the order doesn't change the result.
pub struct ChannelStrip {
    eq: IsolatorEq,
    filter: DjFilter,
}

impl ChannelStrip {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            eq: IsolatorEq::new(sample_rate),
            filter: DjFilter::new(sample_rate),
        }
    }

    pub fn process(&mut self, buf: &mut [f32], params: StripParams) {
        self.eq.process(buf, params.eq);
        self.filter.process(buf, params.filter_mode, params.cutoff);
    }

    /// Clear filter state (e.g. when a deck stops rendering) so stale
    /// history can't transient on the next start.
    pub fn reset(&mut self) {
        self.eq.reset();
        self.filter.reset();
    }
}

/// Master-bus peak limiter: instantaneous attack, exponential release.
/// Keeps two full-gain decks from hard-clipping into the DAC; a final
/// clamp stays as the safety net for intersample overs.
pub struct Limiter {
    envelope: f32,
    release_alpha: f32,
}

/// Limiter ceiling (linear).
const LIMIT_THRESHOLD: f32 = 0.98;
/// Release time constant in seconds.
const LIMIT_RELEASE_SECS: f32 = 0.05;

impl Limiter {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            envelope: 0.0,
            release_alpha: 1.0 - (-1.0 / (LIMIT_RELEASE_SECS * sample_rate as f32)).exp(),
        }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for frame in buf.chunks_exact_mut(2) {
            let peak = frame[0].abs().max(frame[1].abs());
            if peak > self.envelope {
                self.envelope = peak;
            } else {
                self.envelope += (peak - self.envelope) * self.release_alpha;
            }
            let gain = if self.envelope > LIMIT_THRESHOLD {
                LIMIT_THRESHOLD / self.envelope
            } else {
                1.0
            };
            for s in frame.iter_mut() {
                *s = (*s * gain).clamp(-1.0, 1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stereo_sine(freq: f64, secs: f64, sample_rate: u32) -> Vec<f32> {
        let n = (secs * sample_rate as f64) as usize;
        let mut out = Vec::with_capacity(n * 2);
        for i in 0..n {
            let s = (std::f64::consts::TAU * freq * i as f64 / sample_rate as f64).sin() as f32;
            out.push(s);
            out.push(s);
        }
        out
    }

    fn energy(buf: &[f32], skip_frames: usize) -> f64 {
        buf[skip_frames * 2..]
            .iter()
            .map(|s| (*s as f64).powi(2))
            .sum()
    }

    fn run_strip(input: &[f32], params: StripParams, sample_rate: u32) -> Vec<f32> {
        let mut strip = ChannelStrip::new(sample_rate);
        let mut buf = input.to_vec();
        // Feed in callback-sized blocks like the real audio path.
        for block in buf.chunks_mut(1024) {
            strip.process(block, params);
        }
        buf
    }

    const SR: u32 = 48_000;
    /// Frames to skip for filter settling + gain smoothing ramp-in.
    const SETTLE: usize = 8_192;

    fn unity() -> StripParams {
        StripParams {
            eq: [1.0; 3],
            filter_mode: FilterMode::Off,
            cutoff: 1.0,
        }
    }

    #[test]
    fn unity_strip_is_transparent() {
        // The isolator re-sums to an allpass: energy preserved within a dB.
        for freq in [60.0, 250.0, 1_000.0, 2_600.0, 8_000.0] {
            let input = stereo_sine(freq, 1.0, SR);
            let out = run_strip(&input, unity(), SR);
            let ratio = energy(&out, SETTLE) / energy(&input, SETTLE);
            let db = 10.0 * ratio.log10();
            assert!(
                db.abs() < 1.0,
                "unity strip changed level at {freq} Hz: {db:+.2} dB"
            );
        }
    }

    #[test]
    fn low_kill_removes_bass_keeps_highs() {
        let params = StripParams {
            eq: [0.0, 1.0, 1.0],
            ..unity()
        };
        let bass = run_strip(&stereo_sine(60.0, 1.0, SR), params, SR);
        let bass_db =
            10.0 * (energy(&bass, SETTLE) / energy(&stereo_sine(60.0, 1.0, SR), SETTLE)).log10();
        assert!(bass_db < -30.0, "low kill left {bass_db:+.1} dB of 60 Hz");

        let highs = run_strip(&stereo_sine(8_000.0, 1.0, SR), params, SR);
        let highs_db = 10.0
            * (energy(&highs, SETTLE) / energy(&stereo_sine(8_000.0, 1.0, SR), SETTLE)).log10();
        assert!(
            highs_db.abs() < 1.0,
            "low kill touched 8 kHz: {highs_db:+.2} dB"
        );
    }

    #[test]
    fn mid_kill_notches_mids() {
        let params = StripParams {
            eq: [1.0, 0.0, 1.0],
            ..unity()
        };
        let mids = run_strip(&stereo_sine(1_000.0, 1.0, SR), params, SR);
        let db =
            10.0 * (energy(&mids, SETTLE) / energy(&stereo_sine(1_000.0, 1.0, SR), SETTLE)).log10();
        assert!(db < -30.0, "mid kill left {db:+.1} dB of 1 kHz");
    }

    #[test]
    fn highpass_removes_bass() {
        let params = StripParams {
            eq: [1.0; 3],
            filter_mode: FilterMode::HighPass,
            cutoff: 0.5, // ~630 Hz
        };
        let out = run_strip(&stereo_sine(60.0, 1.0, SR), params, SR);
        let db =
            10.0 * (energy(&out, SETTLE) / energy(&stereo_sine(60.0, 1.0, SR), SETTLE)).log10();
        assert!(db < -20.0, "highpass left {db:+.1} dB of 60 Hz");
    }

    #[test]
    fn lowpass_removes_highs() {
        let params = StripParams {
            eq: [1.0; 3],
            filter_mode: FilterMode::LowPass,
            cutoff: 0.5, // ~630 Hz
        };
        let out = run_strip(&stereo_sine(8_000.0, 1.0, SR), params, SR);
        let db =
            10.0 * (energy(&out, SETTLE) / energy(&stereo_sine(8_000.0, 1.0, SR), SETTLE)).log10();
        assert!(db < -20.0, "lowpass left {db:+.1} dB of 8 kHz");
    }

    #[test]
    fn limiter_caps_hot_signal_and_passes_quiet_one() {
        let mut limiter = Limiter::new(SR);
        // Two full-scale decks summed: 2.0 peak.
        let mut hot: Vec<f32> = stereo_sine(1_000.0, 0.2, SR)
            .iter()
            .map(|s| s * 2.0)
            .collect();
        limiter.process(&mut hot);
        assert!(hot.iter().all(|s| s.abs() <= 1.0));
        // Steady-state output should sit at the ceiling, not squashed below.
        let peak_tail = hot[hot.len() / 2..]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak_tail > 0.9, "over-limited: peak {peak_tail}");

        let mut quiet: Vec<f32> = stereo_sine(1_000.0, 0.2, SR)
            .iter()
            .map(|s| s * 0.5)
            .collect();
        let reference = quiet.clone();
        let mut limiter = Limiter::new(SR);
        limiter.process(&mut quiet);
        for (a, b) in quiet.iter().zip(&reference) {
            assert!((a - b).abs() < 1e-6, "limiter touched sub-threshold audio");
        }
    }

    #[test]
    fn output_stays_finite_through_sweeps_and_mode_flips() {
        let input = stereo_sine(440.0, 0.5, SR);
        let mut strip = ChannelStrip::new(SR);
        let mut buf = input.clone();
        let modes = [
            FilterMode::Off,
            FilterMode::LowPass,
            FilterMode::HighPass,
            FilterMode::LowPass,
        ];
        for (i, block) in buf.chunks_mut(512).enumerate() {
            let params = StripParams {
                eq: [(i % 3) as f32, 1.0, ((i + 1) % 2) as f32],
                filter_mode: modes[i % modes.len()],
                cutoff: (i as f32 * 0.13) % 1.0,
            };
            strip.process(block, params);
        }
        assert!(buf.iter().all(|s| s.is_finite()), "non-finite DSP output");
    }
}
