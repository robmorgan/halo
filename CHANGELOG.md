# Changelog

All notable changes to Halo are documented in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## 0.1.0 - 2026-08-03

### Added

- Live multi-color LUFS meters per deck: segmented LED ladders on a −40..0 LUFS
  scale (green/amber/red), with a marker at the track's integrated loudness for
  gain-matching decks, a peak-clip strip, and numeric values on hover. Track
  analysis now measures BS.1770-4 loudness; previously analyzed tracks show a
  marker after re-analysis. (#88)
- Library context menu on the whole track row with two new actions: **Reanalyze**
  (re-runs analysis and refreshes the beat grid, even on a loaded deck) and
  **Remove from library…** (confirmation dialog; deletes the track, its playlist
  membership, and authored lighting cues — the audio file on disk is untouched).
  (#89)

### Fixed

- The DSP percentage readout in the toolbar no longer flickers at frame rate: the
  digits and threshold color latch at ~2 Hz while the bar still tracks the live
  per-block load. (#90)

### Internal

- Added a `/release` skill automating the release process end to end.
