//! Simulated show generator for the deck lane strips.
//!
//! Real cues are authored in Prepare mode and persisted in the library;
//! this generator exists to seed a track with plausible, beat-aligned
//! demo cues (and to exercise the lane painters before a rig exists).

use halo_light::cues::{CueSet, Lane};

use crate::show_preview::{ACCENT_LANE, LookId, ShowPreview};
use crate::waveform::GridMarks;

/// splitmix64: a tiny deterministic PRNG so the simulation needs no `rand`
/// dependency and is stable for a given seed.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Uniform in [0, 1).
fn rand_f32(state: &mut u64) -> f32 {
    (splitmix64(state) >> 40) as f32 / (1u64 << 24) as f32
}

/// Beat sequence the simulation rules walk: either the track's real grid
/// or a synthesized fallback for un-analyzed tracks.
struct BeatSeq {
    frames: Vec<f64>,
    downbeat: Vec<bool>,
    phrase_start: Vec<bool>,
    beat_frames: f64,
}

impl BeatSeq {
    fn from_marks(marks: &GridMarks) -> Self {
        let n = marks.len();
        Self {
            frames: (0..n).map(|i| marks.frame(i)).collect(),
            downbeat: (0..n).map(|i| marks.is_downbeat(i)).collect(),
            phrase_start: (0..n).map(|i| marks.is_phrase_start(i)).collect(),
            beat_frames: marks.median_beat_frames(),
        }
    }

    /// 120 BPM 4/4 grid with 16-bar phrases, so lanes still render on
    /// tracks without a usable beat grid.
    fn synthetic(total_frames: usize, sample_rate: u32) -> Self {
        let beat_frames = 0.5 * sample_rate.max(1) as f64;
        let n = (total_frames as f64 / beat_frames) as usize;
        Self {
            frames: (0..n).map(|i| i as f64 * beat_frames).collect(),
            downbeat: (0..n).map(|i| i % 4 == 0).collect(),
            phrase_start: (0..n).map(|i| i % (4 * 16) == 0).collect(),
            beat_frames,
        }
    }
}

/// SIMULATED show generator: deterministic per `(grid, seed)`, aligned to
/// phrases and bars so the bars land musically.
// Legacy three-lane generator, kept (unit-tested) for the full L3
// landing; the preview seeds `simulate_show_l3` instead.
#[allow(dead_code)]
pub fn simulate_show(
    marks: &GridMarks,
    total_frames: usize,
    sample_rate: u32,
    seed: u64,
) -> CueSet {
    if total_frames == 0 {
        return CueSet::empty();
    }
    let seq = if marks.is_usable() && marks.median_beat_frames() > 0.0 {
        BeatSeq::from_marks(marks)
    } else {
        BeatSeq::synthetic(total_frames, sample_rate)
    };
    if seq.frames.len() < 2 {
        return CueSet::empty();
    }

    let beat = seq.beat_frames;
    let bar = 4.0 * beat;
    let mut show = CueSet::empty();
    let mut rng = seed;

    // Lighting: a cue at every phrase start lasting 2 bars, plus 1-bar
    // accents on ~40% of mid-phrase bar-group starts (every 4th bar), which
    // never collide with the 2-bar phrase cue.
    // Pixels: ~half the phrases run a chase — one half-bar hit per bar.
    // FX (smoke/pyro): every 4th phrase start, plus ~20% of the others.
    let mut phrase_idx: usize = 0;
    let mut pixels_active = false;
    let mut bars_since_phrase: usize = 0;
    let mut seen_phrase = false;
    for i in 0..seq.frames.len() {
        if !seq.downbeat[i] {
            continue;
        }
        let frame = seq.frames[i];
        if seq.phrase_start[i] {
            phrase_idx += if seen_phrase { 1 } else { 0 };
            seen_phrase = true;
            bars_since_phrase = 0;
            pixels_active = rand_f32(&mut rng) < 0.5;

            show.insert(Lane::Lighting, frame, 2.0 * bar, 0.9);
            let fx_hit = phrase_idx.is_multiple_of(4) || rand_f32(&mut rng) < 0.2;
            if fx_hit {
                show.insert(Lane::Fx, frame, beat, 1.0);
            }
        } else {
            bars_since_phrase += 1;
            if bars_since_phrase.is_multiple_of(4) && rand_f32(&mut rng) < 0.4 {
                let intensity = 0.4 + 0.4 * rand_f32(&mut rng);
                show.insert(Lane::Lighting, frame, bar, intensity);
            }
        }
        if pixels_active && seen_phrase {
            let intensity = 0.6 + 0.4 * rand_f32(&mut rng);
            show.insert(Lane::Pixels, frame, 0.5 * bar, intensity);
        }
    }
    show
}

/// Linear interpolation for the energy arc segments.
fn lerp(a: f32, b: f32, t: f64) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0) as f32
}

/// Target energy for a section at normalized set position `t` (0..1):
/// intro → build → drop → breakdown → second build → outro.
fn arc_target(t: f64) -> f32 {
    if t < 0.15 {
        0.35
    } else if t < 0.35 {
        lerp(0.4, 0.8, (t - 0.15) / 0.20)
    } else if t < 0.55 {
        1.0
    } else if t < 0.70 {
        0.45
    } else if t < 0.85 {
        lerp(0.6, 0.95, (t - 0.70) / 0.15)
    } else {
        0.30
    }
}

/// SIMULATED L3 show: deterministic per `(grid, seed)`. Look events at
/// phrase boundaries following a cool/hot palette split, an energy arc
/// (intro → build → drop → breakdown → outro), and sparse accents.
/// Session-only seed content for the role-lane preview.
pub fn simulate_show_l3(
    marks: &GridMarks,
    total_frames: usize,
    sample_rate: u32,
    seed: u64,
) -> ShowPreview {
    if total_frames == 0 {
        return ShowPreview::default();
    }
    let seq = if marks.is_usable() && marks.median_beat_frames() > 0.0 {
        BeatSeq::from_marks(marks)
    } else {
        BeatSeq::synthetic(total_frames, sample_rate)
    };
    if seq.frames.len() < 2 {
        return ShowPreview::default();
    }

    let beat = seq.beat_frames;
    let bar = 4.0 * beat;
    let total = total_frames as f64;

    // Section boundaries: phrase starts, falling back to every 8th
    // downbeat, then every 32nd beat, so short/gridless tracks still
    // get an arc.
    let mut sections: Vec<f64> = (0..seq.frames.len())
        .filter(|&i| seq.phrase_start[i])
        .map(|i| seq.frames[i])
        .collect();
    if sections.len() < 2 {
        sections = (0..seq.frames.len())
            .filter(|&i| seq.downbeat[i])
            .step_by(8)
            .map(|i| seq.frames[i])
            .collect();
    }
    if sections.len() < 2 {
        sections = seq.frames.iter().copied().step_by(32).collect();
    }
    if sections.is_empty() {
        sections = vec![seq.frames[0]];
    }

    // Palette pools by temperature; indices into LOOK_PALETTE.
    const COOL: [usize; 4] = [1, 7, 0, 5]; // Deep Blue, UV Violet, Warm Open, Magenta Chase
    const HOT: [usize; 4] = [2, 3, 4, 6]; // Red Drop, Strobe White, Acid Green, Amber Sweep

    let mut show = ShowPreview::default();
    let mut rng = seed;
    let mut prev_energy: f32 = 0.0;
    let mut prev_look: Option<usize> = None;

    for (i, &frame) in sections.iter().enumerate() {
        let t = i as f64 / sections.len() as f64;
        let target = arc_target(t);
        let energy = (target + (rand_f32(&mut rng) - 0.5) * 0.1).clamp(0.0, 1.0);
        show.energy.insert(frame, energy);

        let pool = if target < 0.6 { &COOL } else { &HOT };
        let mut look = pool[(rand_f32(&mut rng) * pool.len() as f32) as usize % pool.len()];
        if prev_look == Some(look) {
            look = pool[(rand_f32(&mut rng) * pool.len() as f32) as usize % pool.len()];
        }
        show.looks.insert(frame, LookId(look), bar);
        prev_look = Some(look);

        // Accents: a full hit on the drop (energy stepping up past 0.9);
        // otherwise ~25% of sections hit the downbeat of their last bar.
        if prev_energy < 0.9 && energy >= 0.9 {
            show.accents.insert(ACCENT_LANE, frame, beat, 1.0);
        } else if rand_f32(&mut rng) < 0.25 {
            let next = sections.get(i + 1).copied().unwrap_or(total);
            let hit = (next - bar).max(frame);
            show.accents
                .insert(ACCENT_LANE, hit, beat, 0.6 + 0.4 * rand_f32(&mut rng));
        }
        prev_energy = energy;
    }
    // Closing breakpoint so the outro level holds visibly to the end.
    show.energy.insert(total, arc_target(1.0));
    show
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_marks() -> GridMarks {
        // 256 beats at 120 BPM / 44.1 kHz, downbeat every 4th.
        let mut grid = timestretch::BeatGrid::empty(44100);
        grid.beats = (0..256).map(|i| i as f64 * 22050.0).collect();
        grid.downbeats = (0..64).map(|b| b * 4).collect();
        grid.bpm = 120.0;
        GridMarks::from_grid(&grid)
    }

    fn collect(show: &CueSet, lane: Lane) -> Vec<(f64, f64)> {
        show.visible(lane, f64::MIN, f64::MAX)
            .iter()
            .map(|c| (c.start_frame, c.duration_frames))
            .collect()
    }

    #[test]
    fn deterministic_per_seed() {
        let marks = test_marks();
        let a = simulate_show(&marks, 256 * 22050, 44100, 7);
        let b = simulate_show(&marks, 256 * 22050, 44100, 7);
        for lane in [Lane::Lighting, Lane::Pixels, Lane::Fx] {
            assert_eq!(collect(&a, lane), collect(&b, lane));
        }
    }

    #[test]
    fn sorted_and_non_overlapping() {
        let marks = test_marks();
        let show = simulate_show(&marks, 256 * 22050, 44100, 42);
        for lane in [Lane::Lighting, Lane::Pixels, Lane::Fx] {
            let v = collect(&show, lane);
            for w in v.windows(2) {
                assert!(w[0].0 + w[0].1 <= w[1].0 + 1e-6, "{lane:?}: {w:?}");
            }
        }
    }

    #[test]
    fn lighting_has_phrase_cues() {
        let marks = test_marks();
        let show = simulate_show(&marks, 256 * 22050, 44100, 42);
        // 256 beats = 64 bars = 4 phrases of 16 bars.
        assert!(collect(&show, Lane::Lighting).len() >= 4);
    }

    #[test]
    fn empty_track_is_empty() {
        let show = simulate_show(&GridMarks::empty(), 0, 44100, 1);
        for lane in [Lane::Lighting, Lane::Pixels, Lane::Fx] {
            assert!(collect(&show, lane).is_empty());
        }
    }

    #[test]
    fn no_grid_falls_back_to_synthetic() {
        // 60 s at 44.1 kHz, no grid: still produces lighting cues.
        let show = simulate_show(&GridMarks::empty(), 60 * 44100, 44100, 1);
        assert!(!collect(&show, Lane::Lighting).is_empty());
    }

    #[test]
    fn l3_deterministic_per_seed() {
        let marks = test_marks();
        let a = simulate_show_l3(&marks, 256 * 22050, 44100, 7);
        let b = simulate_show_l3(&marks, 256 * 22050, 44100, 7);
        let looks = |s: &ShowPreview| -> Vec<(f64, usize)> {
            s.looks
                .events()
                .iter()
                .map(|e| (e.frame, e.look.0))
                .collect()
        };
        let energy = |s: &ShowPreview| -> Vec<(f64, f32)> {
            s.energy
                .points()
                .iter()
                .map(|p| (p.frame, p.value))
                .collect()
        };
        assert_eq!(looks(&a), looks(&b));
        assert_eq!(energy(&a), energy(&b));
        assert_eq!(
            collect(&a.accents, ACCENT_LANE),
            collect(&b.accents, ACCENT_LANE)
        );
    }

    #[test]
    fn l3_looks_sorted_with_bar_separation() {
        let marks = test_marks();
        let show = simulate_show_l3(&marks, 256 * 22050, 44100, 42);
        let bar = 4.0 * 22050.0;
        let ev = show.looks.events();
        assert!(!ev.is_empty());
        for w in ev.windows(2) {
            assert!(w[1].frame - w[0].frame >= bar - 1e-6);
        }
    }

    #[test]
    fn l3_energy_in_unit_range_and_strictly_increasing() {
        let marks = test_marks();
        let show = simulate_show_l3(&marks, 256 * 22050, 44100, 42);
        let pts = show.energy.points();
        assert!(pts.len() >= 2);
        for p in pts {
            assert!((0.0..=1.0).contains(&p.value));
        }
        for w in pts.windows(2) {
            assert!(w[1].frame > w[0].frame);
        }
    }

    #[test]
    fn l3_accents_sorted_non_overlapping() {
        let marks = test_marks();
        let show = simulate_show_l3(&marks, 256 * 22050, 44100, 42);
        let v = collect(&show.accents, ACCENT_LANE);
        for w in v.windows(2) {
            assert!(w[0].0 + w[0].1 <= w[1].0 + 1e-6, "{w:?}");
        }
    }

    #[test]
    fn l3_empty_track_is_empty_and_no_grid_falls_back() {
        let empty = simulate_show_l3(&GridMarks::empty(), 0, 44100, 1);
        assert!(empty.looks.events().is_empty());
        assert!(empty.energy.points().is_empty());
        // 60 s without a grid: synthetic fallback still seeds a show.
        let show = simulate_show_l3(&GridMarks::empty(), 60 * 44100, 44100, 1);
        assert!(!show.looks.events().is_empty());
        assert!(show.energy.points().len() >= 2);
    }

    #[test]
    fn visible_windows_by_start_and_duration() {
        let marks = test_marks();
        let show = simulate_show(&marks, 256 * 22050, 44100, 42);
        let all = collect(&show, Lane::Lighting);
        let (start, dur) = all[0];
        // A window starting mid-cue still returns it.
        let vis = show.visible(Lane::Lighting, start + dur * 0.5, start + dur);
        assert!(
            vis.iter()
                .any(|c| (c.start_frame - start).abs() < f64::EPSILON)
        );
    }
}
