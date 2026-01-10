# BPM Accuracy Test Fixtures

This directory contains ground truth data for BPM detection accuracy testing.

## Setup

To run the BPM accuracy tests, you need to download the GiantSteps Tempo Dataset:

```bash
# From the repository root
./scripts/download_test_audio.sh
```

This will:
1. Clone the GiantSteps dataset into `giantsteps-tempo-dataset/`
2. Download ~664 electronic dance music track previews (~1GB) into `audio/`
3. Generate `ground_truth.json` from the dataset annotations

All downloaded files are ignored by `.gitignore` and won't be committed to the repository.

## Running Tests

```bash
# Run accuracy tests (requires downloaded audio)
cargo test --package halo-dj --features accuracy-tests

# Run with verbose output
cargo test --package halo-dj --features accuracy-tests -- --nocapture
```

## Files

- `ground_truth.json` - Expected BPM values for each track from crowdsourced annotations
- `audio/` - Downloaded audio files (not committed to git)

## Dataset Source

The test audio comes from the [GiantSteps Tempo Dataset](https://github.com/GiantSteps/giantsteps-tempo-dataset):

> P. Knees et al.: "Two data sets for tempo estimation and key detection in
> electronic dance music annotated from user corrections" (ISMIR 2015)

Ground truth BPM values were manually corrected by the DJ community on Beatport forums.

## Accuracy Metrics

The tests report three accuracy levels:

1. **Accuracy 1 (Strict)**: Detected BPM within ±2% of ground truth
2. **Accuracy 2 (Octave-tolerant)**: Detected BPM or octave multiple (2x, 0.5x) within ±2%
3. **MIREX Accuracy**: Detected BPM within ±8% (academic standard)

For DJ applications, Accuracy 2 ≥90% is the target.
