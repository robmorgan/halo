//use crate::effects::{apply_effect, Effect, RhythmState};
use approx::assert_relative_eq;
//use apply_effect, Effect, RhythmState;

#[test]
fn test_apply_effect_min_value() {
    let effect = Effect {
        apply: |_| 0.0,
        params: vec![],
        min: 0,
        max: 255,
    };
    let rhythm = RhythmState::default();
    let result = apply_effect(&effect, &rhythm, 128);
    assert_eq!(result, 0);
}

#[test]
fn test_apply_effect_max_value() {
    let effect = Effect {
        apply: |_| 1.0,
        params: vec![],
        min: 0,
        max: 255,
    };
    let rhythm = RhythmState::default();
    let result = apply_effect(&effect, &rhythm, 128);
    assert_eq!(result, 255);
}

#[test]
fn test_apply_effect_midpoint() {
    let effect = Effect {
        apply: |_| 0.5,
        params: vec![],
        min: 0,
        max: 200,
    };
    let rhythm = RhythmState::default();
    let result = apply_effect(&effect, &rhythm, 100);
    assert_eq!(result, 100);
}

#[test]
fn test_apply_effect_smoothing() {
    let effect = Effect {
        apply: |_| 1.0,
        params: vec![],
        min: 0,
        max: 255,
    };
    let rhythm = RhythmState::default();
    let result = apply_effect(&effect, &rhythm, 0);
    assert!(
        result > 0 && result < 255,
        "Smoothing should prevent instant jump to max"
    );
}

#[test]
fn test_apply_effect_custom_range() {
    let effect = Effect {
        apply: |_| 0.75,
        params: vec![],
        min: 100,
        max: 200,
    };
    let rhythm = RhythmState::default();
    let result = apply_effect(&effect, &rhythm, 150);
    assert_relative_eq!(result as f64, 175.0, epsilon = 1.0);
}
