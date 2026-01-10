//! Deck audio player for sample-by-sample playback.
//!
//! Handles decoding and playback of audio files with tempo/pitch adjustment.
//! Uses varispeed for tempo changes (tempo change = pitch change).
//! Includes beat tracking for lighting synchronization.

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

use super::time_stretcher::TimeStretcher;
use crate::deck::DeckId;
use crate::library::{BeatGrid, MasterTempoMode, TempoRange};

/// State of the deck player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    /// No file loaded.
    Empty,
    /// File loaded, ready to play.
    Ready,
    /// Currently playing.
    Playing,
    /// Paused.
    Paused,
}

/// Information about a beat event.
#[derive(Debug, Clone, Copy)]
pub struct BeatEvent {
    /// The beat number (0-indexed from start of track).
    pub beat_number: u64,
    /// Position in seconds where the beat occurred.
    pub position_seconds: f64,
    /// Whether this is a downbeat (first beat of a bar, every 4 beats).
    pub is_downbeat: bool,
    /// Whether this is the first beat of a phrase (every 16 beats).
    pub is_phrase_start: bool,
    /// BPM at this beat.
    pub bpm: f64,
}

/// Audio deck player for sample-accurate playback.
pub struct DeckPlayer {
    /// Deck identifier.
    deck_id: DeckId,
    /// Current player state.
    state: PlayerState,
    /// Format reader.
    format: Option<Box<dyn FormatReader>>,
    /// Audio decoder.
    decoder: Option<Box<dyn symphonia::core::codecs::Decoder>>,
    /// Track ID for the audio stream.
    track_id: Option<u32>,
    /// Sample rate of the loaded file.
    sample_rate: u32,
    /// Number of channels in the loaded file.
    channels: usize,
    /// Current sample position (in source samples).
    sample_position: u64,
    /// Total samples in the file.
    total_samples: u64,
    /// Playback rate multiplier (1.0 = normal, affects pitch).
    playback_rate: f64,
    /// Current audio buffer (interleaved samples).
    buffer: Vec<f32>,
    /// Position in the current buffer (in samples, not frames).
    buffer_position: usize,
    /// Fractional position for varispeed interpolation.
    fractional_position: f64,
    /// Previous stereo sample for interpolation.
    prev_sample: (f32, f32),
    /// Current stereo sample for interpolation.
    curr_sample: (f32, f32),
    /// Cue point position in seconds (None if not set).
    cue_point: Option<f64>,
    /// Whether we need to seek on next decode.
    pending_seek: Option<f64>,
    /// Path to the loaded file (for reloading on seek).
    loaded_path: Option<std::path::PathBuf>,

    // Beat tracking fields
    /// Beat grid for the loaded track.
    beat_grid: Option<BeatGrid>,
    /// Base BPM from track.bpm (single source of truth for tempo).
    base_bpm: f64,
    /// Current beat index in the beat grid.
    current_beat_index: usize,
    /// Beat event that occurred during the last sample (if any).
    last_beat_event: Option<BeatEvent>,
    /// Previous position for beat crossing detection.
    prev_position_seconds: f64,

    // Hot cue fields
    /// 4 hot cue positions in seconds (None if not set).
    hot_cues: [Option<f64>; 4],

    // Loop fields
    /// Loop IN point in samples (None if not set).
    loop_in_sample: Option<u64>,
    /// Loop OUT point in samples (None if not set).
    loop_out_sample: Option<u64>,
    /// Whether the loop is currently active.
    loop_active: bool,

    // Master Tempo fields
    /// Master Tempo mode (key lock).
    master_tempo: MasterTempoMode,
    /// Time stretcher for Master Tempo mode.
    time_stretcher: TimeStretcher,
    /// Current tempo range setting.
    tempo_range: TempoRange,

    // Sync fields
    /// Whether sync is enabled for this deck.
    sync_enabled: bool,
    /// Sync phase correction factor (-1.0 to 1.0, applied to playback rate).
    /// Positive = speed up slightly, negative = slow down slightly.
    sync_correction: f64,
    /// Base playback rate (before sync correction is applied).
    base_playback_rate: f64,

    // Quantized play fields
    /// Scheduled play delay in seconds (for quantized sync start).
    pending_play_delay: Option<f64>,
    /// When the quantized play was scheduled.
    play_scheduled_at: Option<std::time::Instant>,
    /// Virtual position offset for display during countdown (can be negative).
    virtual_position_offset: f64,
}

impl DeckPlayer {
    /// Create a new deck player.
    pub fn new(deck_id: DeckId) -> Self {
        Self {
            deck_id,
            state: PlayerState::Empty,
            format: None,
            decoder: None,
            track_id: None,
            sample_rate: 44100,
            channels: 2,
            sample_position: 0,
            total_samples: 0,
            playback_rate: 1.0,
            buffer: Vec::new(),
            buffer_position: 0,
            fractional_position: 0.0,
            prev_sample: (0.0, 0.0),
            curr_sample: (0.0, 0.0),
            cue_point: None,
            pending_seek: None,
            loaded_path: None,
            beat_grid: None,
            base_bpm: 120.0,
            current_beat_index: 0,
            last_beat_event: None,
            prev_position_seconds: 0.0,
            hot_cues: [None; 4],
            loop_in_sample: None,
            loop_out_sample: None,
            loop_active: false,
            master_tempo: MasterTempoMode::Off,
            time_stretcher: TimeStretcher::new(44100, 2),
            tempo_range: TempoRange::default(),
            sync_enabled: false,
            sync_correction: 0.0,
            base_playback_rate: 1.0,
            pending_play_delay: None,
            play_scheduled_at: None,
            virtual_position_offset: 0.0,
        }
    }

    /// Load an audio file.
    pub fn load<P: AsRef<Path>>(&mut self, path: P) -> Result<(), anyhow::Error> {
        let path = path.as_ref();
        log::info!("Deck {}: Loading file {:?}", self.deck_id, path);

        // Store the path for potential reloading
        self.loaded_path = Some(path.to_path_buf());

        // Open the file
        let file = File::open(path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        // Create a hint for the format
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        // Probe the format
        let probed = symphonia::default::get_probe().format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?;

        let format = probed.format;

        // Find the first audio track
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| anyhow::anyhow!("No audio track found"))?;

        let track_id = track.id;
        let codec_params = &track.codec_params;

        // Get audio parameters
        self.sample_rate = codec_params.sample_rate.unwrap_or(44100);
        self.channels = codec_params.channels.map(|c| c.count()).unwrap_or(2);
        self.total_samples = codec_params.n_frames.unwrap_or(0);

        // Create the decoder
        let decoder =
            symphonia::default::get_codecs().make(codec_params, &DecoderOptions::default())?;

        self.format = Some(format);
        self.decoder = Some(decoder);
        self.track_id = Some(track_id);
        self.sample_position = 0;
        self.buffer.clear();
        self.buffer_position = 0;
        self.fractional_position = 0.0;
        self.prev_sample = (0.0, 0.0);
        self.curr_sample = (0.0, 0.0);
        self.cue_point = None;
        self.pending_seek = None;
        self.beat_grid = None;
        self.current_beat_index = 0;
        self.last_beat_event = None;
        self.prev_position_seconds = 0.0;
        self.hot_cues = [None; 4];
        self.loop_in_sample = None;
        self.loop_out_sample = None;
        self.loop_active = false;
        // Reset time stretcher with new sample rate
        self.time_stretcher = TimeStretcher::new(self.sample_rate, self.channels as u32);
        // Reset playback rate to 1.0 (no pitch adjustment)
        self.playback_rate = 1.0;
        self.base_playback_rate = 1.0;
        self.state = PlayerState::Ready;

        log::info!(
            "Deck {}: Loaded {} Hz, {} channels, {} samples ({:.2}s)",
            self.deck_id,
            self.sample_rate,
            self.channels,
            self.total_samples,
            self.duration_seconds()
        );

        Ok(())
    }

    /// Start playback.
    pub fn play(&mut self) {
        if matches!(self.state, PlayerState::Ready | PlayerState::Paused) {
            self.state = PlayerState::Playing;
            log::debug!("Deck {}: Playing", self.deck_id);
        }
    }

    /// Pause playback.
    pub fn pause(&mut self) {
        if self.state == PlayerState::Playing {
            self.state = PlayerState::Paused;
            log::debug!("Deck {}: Paused", self.deck_id);
        }
    }

    /// Stop playback and return to start.
    pub fn stop(&mut self) {
        if self.state != PlayerState::Empty {
            self.state = PlayerState::Ready;
            self.pending_seek = Some(0.0);
            log::debug!("Deck {}: Stopped", self.deck_id);
        }
    }

    /// Eject the loaded file.
    pub fn eject(&mut self) {
        self.format = None;
        self.decoder = None;
        self.track_id = None;
        self.sample_position = 0;
        self.total_samples = 0;
        self.buffer.clear();
        self.buffer_position = 0;
        self.fractional_position = 0.0;
        self.prev_sample = (0.0, 0.0);
        self.curr_sample = (0.0, 0.0);
        self.cue_point = None;
        self.pending_seek = None;
        self.loaded_path = None;
        self.beat_grid = None;
        self.current_beat_index = 0;
        self.last_beat_event = None;
        self.prev_position_seconds = 0.0;
        self.hot_cues = [None; 4];
        self.loop_in_sample = None;
        self.loop_out_sample = None;
        self.loop_active = false;
        self.time_stretcher.reset();
        self.state = PlayerState::Empty;
        log::debug!("Deck {}: Ejected", self.deck_id);
    }

    /// Set the playback rate (1.0 = normal speed).
    pub fn set_playback_rate(&mut self, rate: f64) {
        self.playback_rate = rate.clamp(0.5, 2.0);
        // Update time stretcher tempo when in Master Tempo mode
        if self.master_tempo == MasterTempoMode::On {
            self.time_stretcher.set_tempo(self.playback_rate);
        }
    }

    /// Set the playback rate using a pitch fader value and tempo range.
    ///
    /// - `pitch`: Pitch fader position from -1.0 to 1.0 (0.0 = center)
    /// - `tempo_range`: The tempo range setting (±6%, ±10%, etc.)
    ///
    /// Returns the resulting playback rate.
    pub fn set_pitch(&mut self, pitch: f64, tempo_range: TempoRange) -> f64 {
        let pitch = pitch.clamp(-1.0, 1.0);
        let rate = tempo_range.pitch_to_multiplier(pitch);
        self.base_playback_rate = rate;
        self.playback_rate = self.effective_rate();
        self.tempo_range = tempo_range;
        // Update time stretcher tempo when in Master Tempo mode
        if self.master_tempo == MasterTempoMode::On {
            self.time_stretcher.set_tempo(self.playback_rate);
        }
        rate
    }

    /// Get the effective playback rate including sync correction.
    fn effective_rate(&self) -> f64 {
        if self.sync_enabled {
            // Apply sync correction (typically very small, ±0.5%)
            (self.base_playback_rate * (1.0 + self.sync_correction)).clamp(0.5, 2.0)
        } else {
            self.base_playback_rate
        }
    }

    /// Enable or disable sync mode.
    pub fn set_sync_enabled(&mut self, enabled: bool) {
        self.sync_enabled = enabled;
        if !enabled {
            // Reset correction when sync is disabled
            self.sync_correction = 0.0;
            self.playback_rate = self.base_playback_rate;
        }
    }

    /// Check if sync is enabled.
    pub fn is_sync_enabled(&self) -> bool {
        self.sync_enabled
    }

    /// Set the sync phase correction.
    ///
    /// - `correction`: Small adjustment factor (e.g., 0.005 = speed up 0.5%)
    pub fn set_sync_correction(&mut self, correction: f64) {
        // Limit correction to ±2% to avoid audible pitch change
        self.sync_correction = correction.clamp(-0.02, 0.02);
        self.playback_rate = self.effective_rate();
        // Update time stretcher if in Master Tempo mode
        if self.master_tempo == MasterTempoMode::On {
            self.time_stretcher.set_tempo(self.playback_rate);
        }
    }

    /// Nudge the playback rate temporarily (for beatmatching).
    ///
    /// - `amount`: Nudge amount (-1.0 to 1.0, typically ±0.04 for 4% nudge)
    ///
    /// Call with 0.0 to return to the current pitch setting.
    pub fn nudge(&mut self, amount: f64) {
        // Nudge adds to the current rate (for live beatmatching)
        // Typically used with small values like ±0.04
        let nudge = amount.clamp(-0.5, 0.5);
        let base_rate = self.playback_rate;
        // Apply nudge temporarily - caller should set back to base rate when released
        self.playback_rate = (base_rate + nudge).clamp(0.5, 2.0);
    }

    /// Calculate the pitch adjustment needed to match a target BPM.
    ///
    /// Returns the pitch fader value (-1.0 to 1.0) needed to match the target BPM
    /// within the given tempo range. Returns None if the target BPM cannot be
    /// reached within the tempo range.
    ///
    /// - `target_bpm`: The BPM to sync to
    /// - `tempo_range`: The current tempo range setting
    pub fn calculate_sync_pitch(&self, target_bpm: f64, tempo_range: TempoRange) -> Option<f64> {
        let original_bpm = self.original_bpm()?;
        if original_bpm <= 0.0 || target_bpm <= 0.0 {
            return None;
        }

        // Calculate required playback rate
        let required_rate = target_bpm / original_bpm;

        // Convert rate to pitch fader value
        // rate = 1.0 + (pitch * range_fraction)
        // pitch = (rate - 1.0) / range_fraction
        let range_fraction = tempo_range.as_fraction();
        let pitch = (required_rate - 1.0) / range_fraction;

        // Check if within range
        if pitch >= -1.0 && pitch <= 1.0 {
            Some(pitch)
        } else {
            None
        }
    }

    /// Sync this deck's playback rate to a target BPM.
    ///
    /// Returns true if sync was successful, false if target BPM is out of range.
    ///
    /// - `target_bpm`: The BPM to sync to
    /// - `tempo_range`: The current tempo range setting
    pub fn sync_to_bpm(&mut self, target_bpm: f64, tempo_range: TempoRange) -> bool {
        if let Some(pitch) = self.calculate_sync_pitch(target_bpm, tempo_range) {
            self.set_pitch(pitch, tempo_range);
            log::debug!(
                "Deck {}: Synced to {:.2} BPM (pitch: {:.3})",
                self.deck_id,
                target_bpm,
                pitch
            );
            true
        } else {
            log::warn!(
                "Deck {}: Cannot sync to {:.2} BPM (out of range)",
                self.deck_id,
                target_bpm
            );
            false
        }
    }

    /// Get the time in seconds of the first beat in this track.
    pub fn first_beat_seconds(&self) -> Option<f64> {
        self.beat_grid
            .as_ref()
            .and_then(|bg| bg.beat_positions.first().copied())
    }

    /// Schedule playback to start after a delay (for quantized sync start).
    ///
    /// - `delay_seconds`: How long to wait before starting playback
    /// - `first_beat_time`: Time of first beat in track (for virtual position calculation)
    pub fn schedule_play_after(&mut self, delay_seconds: f64, first_beat_time: f64) {
        self.pending_play_delay = Some(delay_seconds);
        self.play_scheduled_at = Some(std::time::Instant::now());
        // Virtual position starts negative (time before first beat fires)
        self.virtual_position_offset = -(delay_seconds + first_beat_time);
        self.state = PlayerState::Paused; // Show as "ready to play"
        log::info!(
            "Deck {}: Scheduled quantized play in {:.3}s (virtual pos: {:.3})",
            self.deck_id,
            delay_seconds,
            self.virtual_position_offset
        );
    }

    /// Check if waiting for quantized play start.
    pub fn is_waiting_for_quantized_start(&self) -> bool {
        self.pending_play_delay.is_some()
    }

    /// Get the virtual position (including offset for quantized start countdown).
    /// This can be negative when waiting for quantized play.
    pub fn virtual_position(&self) -> f64 {
        if let (Some(delay), Some(scheduled_at)) = (self.pending_play_delay, self.play_scheduled_at)
        {
            // During countdown, return negative position that counts up to 0
            let elapsed = scheduled_at.elapsed().as_secs_f64();
            let remaining = delay - elapsed;
            if let Some(first_beat) = self.first_beat_seconds() {
                // Position relative to first beat: negative means before first beat fires
                -remaining - first_beat + self.position_seconds()
            } else {
                -remaining + self.position_seconds()
            }
        } else {
            self.position_seconds() + self.virtual_position_offset
        }
    }

    /// Cancel any pending quantized play.
    pub fn cancel_quantized_play(&mut self) {
        self.pending_play_delay = None;
        self.play_scheduled_at = None;
        self.virtual_position_offset = 0.0;
    }

    /// Check and trigger scheduled play if delay has elapsed.
    /// Returns true if playback was just started.
    pub fn check_quantized_play(&mut self) -> bool {
        if let (Some(delay), Some(scheduled_at)) = (self.pending_play_delay, self.play_scheduled_at)
        {
            if scheduled_at.elapsed().as_secs_f64() >= delay {
                // Time to start playback
                self.pending_play_delay = None;
                self.play_scheduled_at = None;
                self.virtual_position_offset = 0.0;
                self.state = PlayerState::Playing;
                log::info!("Deck {}: Quantized play started", self.deck_id);
                return true;
            }
        }
        false
    }

    /// Get the current effective BPM (adjusted for playback rate).
    pub fn effective_bpm(&self) -> Option<f64> {
        self.original_bpm().map(|bpm| bpm * self.playback_rate)
    }

    /// Get the current position in seconds.
    pub fn position_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.sample_position as f64 / self.sample_rate as f64
    }

    /// Get the total duration in seconds.
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.total_samples as f64 / self.sample_rate as f64
    }

    /// Seek to a position in seconds.
    pub fn seek(&mut self, position_seconds: f64) {
        let position = position_seconds.clamp(0.0, self.duration_seconds());
        self.pending_seek = Some(position);
        log::debug!("Deck {}: Seek requested to {:.2}s", self.deck_id, position);
    }

    /// Perform the actual seek operation.
    fn perform_seek(&mut self, position_seconds: f64) -> bool {
        let Some(format) = &mut self.format else {
            return false;
        };

        // Use symphonia's seek functionality
        let seek_to = SeekTo::Time {
            time: Time::from(position_seconds),
            track_id: self.track_id,
        };

        match format.seek(SeekMode::Accurate, seek_to) {
            Ok(seeked_to) => {
                // Update our position based on what symphonia actually seeked to
                self.sample_position = seeked_to.actual_ts;
                self.buffer.clear();
                self.buffer_position = 0;
                self.fractional_position = 0.0;
                self.prev_sample = (0.0, 0.0);
                self.curr_sample = (0.0, 0.0);

                // Reset the decoder after seeking
                if let Some(decoder) = &mut self.decoder {
                    decoder.reset();
                }

                // Flush time stretcher buffer on seek
                self.time_stretcher.reset();

                log::debug!(
                    "Deck {}: Seeked to {:.2}s (sample {})",
                    self.deck_id,
                    position_seconds,
                    self.sample_position
                );
                true
            }
            Err(e) => {
                log::warn!("Deck {}: Seek failed: {}", self.deck_id, e);
                false
            }
        }
    }

    /// Set the cue point at the current position.
    pub fn set_cue(&mut self) {
        self.cue_point = Some(self.position_seconds());
        log::debug!(
            "Deck {}: Cue point set at {:.2}s",
            self.deck_id,
            self.position_seconds()
        );
    }

    /// Set the cue point at a specific position.
    pub fn set_cue_at(&mut self, position_seconds: f64) {
        self.cue_point = Some(position_seconds.clamp(0.0, self.duration_seconds()));
    }

    /// Get the cue point position.
    pub fn cue_point(&self) -> Option<f64> {
        self.cue_point
    }

    /// Jump to the cue point.
    pub fn jump_to_cue(&mut self) {
        if let Some(cue) = self.cue_point {
            self.seek(cue);
        }
    }

    /// Get the playback rate.
    pub fn playback_rate(&self) -> f64 {
        self.playback_rate
    }

    /// Get the Master Tempo mode.
    pub fn master_tempo(&self) -> MasterTempoMode {
        self.master_tempo
    }

    /// Set the Master Tempo mode.
    ///
    /// - `Off`: Varispeed - pitch changes with tempo (lower latency)
    /// - `On`: Time-stretch - pitch locked (key lock)
    pub fn set_master_tempo(&mut self, mode: MasterTempoMode) {
        if self.master_tempo != mode {
            self.master_tempo = mode;

            match mode {
                MasterTempoMode::On => {
                    // Reset and initialize time stretcher with current tempo
                    self.time_stretcher.reset();
                    self.time_stretcher.set_tempo(self.playback_rate);
                    // Reset fractional position for clean start
                    self.fractional_position = 0.0;

                    // Pre-fill the time stretcher to reduce initial latency
                    // SoundTouch needs ~100-200ms of audio to start producing output
                    let prefill_samples = (self.sample_rate as usize / 10).min(4410); // ~100ms
                    for _ in 0..prefill_samples {
                        if self.sample_position < self.total_samples {
                            let sample = self.read_next_raw_sample();
                            self.sample_position += 1;
                            self.time_stretcher.push_sample(sample.0, sample.1);
                        }
                    }

                    log::info!(
                        "Deck {}: Master Tempo enabled (tempo: {:.4}x, sample_rate: {} Hz, prefilled: {} samples)",
                        self.deck_id,
                        self.playback_rate,
                        self.sample_rate,
                        prefill_samples
                    );
                }
                MasterTempoMode::Off => {
                    // Reset time stretcher when disabling
                    self.time_stretcher.reset();
                    // Reset fractional position for varispeed
                    self.fractional_position = 0.0;
                    log::info!("Deck {}: Master Tempo disabled", self.deck_id);
                }
            }
        }
    }

    /// Toggle Master Tempo mode.
    pub fn toggle_master_tempo(&mut self) {
        let new_mode = match self.master_tempo {
            MasterTempoMode::Off => MasterTempoMode::On,
            MasterTempoMode::On => MasterTempoMode::Off,
        };
        self.set_master_tempo(new_mode);
    }

    /// Get the tempo range.
    pub fn tempo_range(&self) -> TempoRange {
        self.tempo_range
    }

    /// Set the tempo range.
    pub fn set_tempo_range(&mut self, range: TempoRange) {
        self.tempo_range = range;
        log::debug!("Deck {}: Tempo range set to {:?}", self.deck_id, range);
    }

    /// Get the next stereo sample pair.
    ///
    /// Behavior depends on Master Tempo mode:
    /// - **Off**: Varispeed interpolation (pitch changes with tempo)
    /// - **On**: Time-stretching via SoundTouch (pitch locked, tempo changes independently)
    ///
    /// After calling this method, use `take_beat_event()` to check if a beat
    /// crossing occurred during this sample period.
    pub fn next_stereo_sample(&mut self) -> (f32, f32) {
        // Clear any previous beat event
        self.last_beat_event = None;

        if self.state != PlayerState::Playing {
            return (0.0, 0.0);
        }

        // Handle pending seek
        if let Some(seek_pos) = self.pending_seek.take() {
            self.perform_seek(seek_pos);
            self.update_beat_index_for_position();
        }

        // Store previous position for beat crossing detection
        self.prev_position_seconds = self.position_seconds();

        // Route to appropriate playback method based on Master Tempo mode
        let sample = match self.master_tempo {
            MasterTempoMode::Off => self.next_varispeed_sample(),
            MasterTempoMode::On => self.next_timestretched_sample(),
        };

        // Check for beat crossing
        self.check_beat_crossing();

        sample
    }

    /// Get next sample using varispeed (pitch changes with tempo).
    ///
    /// Uses fractional positioning and linear interpolation:
    /// - Rate 1.0: advance 1 sample per call (normal)
    /// - Rate 2.0: advance 2 samples per call (double speed, octave up)
    /// - Rate 0.5: advance 0.5 samples per call (half speed, octave down)
    fn next_varispeed_sample(&mut self) -> (f32, f32) {
        // Get current interpolated sample
        let t = self.fractional_position.fract() as f32;
        let left = self.prev_sample.0 * (1.0 - t) + self.curr_sample.0 * t;
        let right = self.prev_sample.1 * (1.0 - t) + self.curr_sample.1 * t;

        // Advance position by playback rate
        self.fractional_position += self.playback_rate;

        // Consume whole samples as needed
        while self.fractional_position >= 1.0 {
            self.fractional_position -= 1.0;
            self.prev_sample = self.curr_sample;
            self.curr_sample = self.read_next_raw_sample();
            self.sample_position += 1;

            // Check for loop wrap
            if self.loop_active {
                if let (Some(loop_out), Some(loop_in)) = (self.loop_out_sample, self.loop_in_sample)
                {
                    if self.sample_position >= loop_out {
                        // Wrap back to loop IN point
                        let loop_in_seconds = loop_in as f64 / self.sample_rate as f64;
                        self.perform_seek(loop_in_seconds);
                        self.sample_position = loop_in;
                        log::trace!(
                            "Deck {}: Loop wrap at sample {} -> {}",
                            self.deck_id,
                            loop_out,
                            loop_in
                        );
                    }
                }
            }

            // Check for end of file
            if self.sample_position >= self.total_samples {
                self.state = PlayerState::Ready;
                self.pending_seek = Some(0.0);
                return (0.0, 0.0);
            }
        }

        (left, right)
    }

    /// Get next sample using time-stretching (pitch locked, tempo changes).
    ///
    /// Reads samples based on the tempo ratio and processes through SoundTouch
    /// for WSOLA-based time stretching. This allows tempo changes without
    /// affecting pitch (Master Tempo / key lock).
    fn next_timestretched_sample(&mut self) -> (f32, f32) {
        // Feed samples based on tempo ratio.
        // At tempo 2.0, we need to feed ~2 input samples per output sample.
        // At tempo 0.5, we need to feed ~0.5 input samples per output sample.
        // We use fractional accumulation to handle this smoothly.

        // Accumulate input samples needed based on tempo
        self.fractional_position += self.playback_rate;

        // Feed whole samples to time stretcher
        while self.fractional_position >= 1.0 && self.sample_position < self.total_samples {
            let sample = self.read_next_raw_sample();
            self.sample_position += 1;
            self.time_stretcher.push_sample(sample.0, sample.1);
            self.fractional_position -= 1.0;

            // Check for loop wrap
            if self.loop_active {
                if let (Some(loop_out), Some(loop_in)) = (self.loop_out_sample, self.loop_in_sample)
                {
                    if self.sample_position >= loop_out {
                        // Wrap back to loop IN point
                        let loop_in_seconds = loop_in as f64 / self.sample_rate as f64;
                        self.perform_seek(loop_in_seconds);
                        self.sample_position = loop_in;
                        // Reset time stretcher for clean loop transition
                        self.time_stretcher.reset();
                        log::trace!(
                            "Deck {}: Loop wrap (timestretched) at sample {} -> {}",
                            self.deck_id,
                            loop_out,
                            loop_in
                        );
                    }
                }
            }
        }

        // Check for end of file
        if self.sample_position >= self.total_samples && !self.time_stretcher.has_output() {
            self.state = PlayerState::Ready;
            self.pending_seek = Some(0.0);
            return (0.0, 0.0);
        }

        // Get processed sample from time stretcher
        // If no output available yet (latency), return silence
        self.time_stretcher.pop_sample().unwrap_or((0.0, 0.0))
    }

    /// Read the next raw stereo sample from the decoder buffer.
    fn read_next_raw_sample(&mut self) -> (f32, f32) {
        // Decode more data if needed
        if self.buffer_position >= self.buffer.len() {
            if !self.decode_next_packet() {
                return (0.0, 0.0);
            }
        }

        // Get the next sample pair from buffer
        let left = self
            .buffer
            .get(self.buffer_position)
            .copied()
            .unwrap_or(0.0);
        let right = if self.channels >= 2 {
            self.buffer
                .get(self.buffer_position + 1)
                .copied()
                .unwrap_or(left)
        } else {
            left
        };

        self.buffer_position += self.channels;
        (left, right)
    }

    /// Decode the next packet of audio data.
    fn decode_next_packet(&mut self) -> bool {
        let Some(track_id) = self.track_id else {
            return false;
        };

        // Read the next packet
        let packet = {
            let Some(format) = &mut self.format else {
                return false;
            };
            match format.next_packet() {
                Ok(packet) => packet,
                Err(symphonia::core::errors::Error::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return false;
                }
                Err(e) => {
                    log::warn!("Deck {}: Error reading packet: {}", self.deck_id, e);
                    return false;
                }
            }
        };

        // Skip packets that don't belong to our track
        if packet.track_id() != track_id {
            return self.decode_next_packet();
        }

        // Decode the packet and copy samples to a temporary buffer
        let new_samples = {
            let Some(decoder) = &mut self.decoder else {
                return false;
            };

            match decoder.decode(&packet) {
                Ok(decoded) => {
                    let mut samples = Vec::new();

                    // Copy samples to temporary buffer
                    match &decoded {
                        AudioBufferRef::F32(buf) => {
                            for frame in 0..buf.frames() {
                                for ch in 0..buf.spec().channels.count() {
                                    samples.push(buf.chan(ch)[frame]);
                                }
                            }
                        }
                        AudioBufferRef::S16(buf) => {
                            for frame in 0..buf.frames() {
                                for ch in 0..buf.spec().channels.count() {
                                    samples.push(buf.chan(ch)[frame] as f32 / 32768.0);
                                }
                            }
                        }
                        AudioBufferRef::S32(buf) => {
                            for frame in 0..buf.frames() {
                                for ch in 0..buf.spec().channels.count() {
                                    samples.push(buf.chan(ch)[frame] as f32 / 2147483648.0);
                                }
                            }
                        }
                        AudioBufferRef::U8(buf) => {
                            for frame in 0..buf.frames() {
                                for ch in 0..buf.spec().channels.count() {
                                    samples.push((buf.chan(ch)[frame] as f32 - 128.0) / 128.0);
                                }
                            }
                        }
                        _ => {
                            log::warn!("Unsupported audio buffer format");
                        }
                    }

                    Some(samples)
                }
                Err(e) => {
                    log::warn!("Deck {}: Error decoding: {}", self.deck_id, e);
                    None
                }
            }
        };

        // Now we can safely modify self.buffer
        if let Some(samples) = new_samples {
            self.buffer = samples;
            self.buffer_position = 0;
            true
        } else {
            false
        }
    }

    /// Get the current player state.
    pub fn state(&self) -> PlayerState {
        self.state
    }

    /// Get the sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get the number of channels.
    pub fn channels(&self) -> usize {
        self.channels
    }

    // Beat tracking methods

    /// Set the beat grid for beat tracking.
    ///
    /// The `bpm` parameter should come from `track.bpm` (the single source of truth).
    pub fn set_beat_grid(&mut self, beat_grid: BeatGrid, bpm: f64) {
        log::debug!(
            "Deck {}: Beat grid set - BPM: {:.2}, {} beats",
            self.deck_id,
            bpm,
            beat_grid.beat_positions.len()
        );
        self.beat_grid = Some(beat_grid);
        self.base_bpm = bpm;
        self.update_beat_index_for_position();
    }

    /// Clear the beat grid.
    pub fn clear_beat_grid(&mut self) {
        self.beat_grid = None;
        self.current_beat_index = 0;
        self.last_beat_event = None;
    }

    /// Get the beat grid (if set).
    pub fn beat_grid(&self) -> Option<&BeatGrid> {
        self.beat_grid.as_ref()
    }

    /// Get a mutable reference to the beat grid (if set).
    pub fn beat_grid_mut(&mut self) -> Option<&mut BeatGrid> {
        self.beat_grid.as_mut()
    }

    /// Get the BPM (adjusted for playback rate).
    pub fn bpm(&self) -> Option<f64> {
        if self.beat_grid.is_some() {
            Some(self.base_bpm * self.playback_rate)
        } else {
            None
        }
    }

    /// Get the original BPM (from track.bpm).
    pub fn original_bpm(&self) -> Option<f64> {
        if self.beat_grid.is_some() {
            Some(self.base_bpm)
        } else {
            None
        }
    }

    /// Get the current beat number (0-indexed).
    pub fn current_beat_number(&self) -> Option<u64> {
        if self.beat_grid.is_some() {
            Some(self.current_beat_index as u64)
        } else {
            None
        }
    }

    /// Get the current beat phase (0.0 to 1.0 within the current beat).
    pub fn beat_phase(&self) -> Option<f64> {
        let beat_grid = self.beat_grid.as_ref()?;
        let positions = &beat_grid.beat_positions;

        if positions.is_empty() {
            return None;
        }

        let current_pos = self.position_seconds();

        // If before first beat, calculate "virtual" beat phase
        // This allows proper sync alignment when starting before the first beat
        if current_pos < positions[0] {
            let time_to_first = positions[0] - current_pos;
            let beat_duration = 60.0 / self.base_bpm;
            let beats_before = time_to_first / beat_duration;
            // Phase counts backwards from 1.0 (e.g., 0.5 beats before = phase 0.5)
            let phase = 1.0 - (beats_before - beats_before.floor());
            return Some(if phase >= 1.0 { 0.0 } else { phase });
        }

        // Find current beat interval
        if self.current_beat_index < positions.len() {
            let beat_start = positions[self.current_beat_index];
            let beat_end = if self.current_beat_index + 1 < positions.len() {
                positions[self.current_beat_index + 1]
            } else {
                // Estimate next beat using BPM
                beat_start + 60.0 / self.base_bpm
            };

            let beat_duration = beat_end - beat_start;
            if beat_duration > 0.0 {
                return Some(((current_pos - beat_start) / beat_duration).clamp(0.0, 1.0));
            }
        }

        Some(0.0)
    }

    /// Get the current bar phase (0.0 to 1.0 within the current 4-beat bar).
    pub fn bar_phase(&self) -> Option<f64> {
        let beat_num = self.current_beat_number()?;
        let beat_phase = self.beat_phase()?;
        let beat_in_bar = (beat_num % 4) as f64;
        Some((beat_in_bar + beat_phase) / 4.0)
    }

    /// Get the current phrase phase (0.0 to 1.0 within the current 16-beat phrase).
    pub fn phrase_phase(&self) -> Option<f64> {
        let beat_num = self.current_beat_number()?;
        let beat_phase = self.beat_phase()?;
        let beat_in_phrase = (beat_num % 16) as f64;
        Some((beat_in_phrase + beat_phase) / 16.0)
    }

    /// Take the last beat event (if any), consuming it.
    ///
    /// This should be called after each `next_stereo_sample()` to check
    /// if a beat occurred during that sample period.
    pub fn take_beat_event(&mut self) -> Option<BeatEvent> {
        self.last_beat_event.take()
    }

    /// Peek at the last beat event without consuming it.
    pub fn peek_beat_event(&self) -> Option<&BeatEvent> {
        self.last_beat_event.as_ref()
    }

    /// Update beat index to match current position (used after seeking).
    fn update_beat_index_for_position(&mut self) {
        let Some(beat_grid) = &self.beat_grid else {
            return;
        };

        let current_pos = self.position_seconds();
        let positions = &beat_grid.beat_positions;

        // Binary search for the appropriate beat index
        self.current_beat_index = match positions.binary_search_by(|pos| {
            pos.partial_cmp(&current_pos)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
    }

    /// Check if a beat crossing occurred between prev and current position.
    fn check_beat_crossing(&mut self) {
        let Some(beat_grid) = &self.beat_grid else {
            return;
        };

        let positions = &beat_grid.beat_positions;
        if positions.is_empty() {
            return;
        }

        let current_pos = self.position_seconds();
        let prev_pos = self.prev_position_seconds;

        // Check if we crossed any beat positions
        while self.current_beat_index < positions.len() {
            let beat_pos = positions[self.current_beat_index];

            // Did we cross this beat?
            if prev_pos < beat_pos && current_pos >= beat_pos {
                let beat_number = self.current_beat_index as u64;

                self.last_beat_event = Some(BeatEvent {
                    beat_number,
                    position_seconds: beat_pos,
                    is_downbeat: beat_number % 4 == 0,
                    is_phrase_start: beat_number % 16 == 0,
                    bpm: self.base_bpm * self.playback_rate,
                });

                self.current_beat_index += 1;
                return; // Only emit one beat per sample
            } else if current_pos < beat_pos {
                // Haven't reached this beat yet
                break;
            } else {
                // Already past this beat
                self.current_beat_index += 1;
            }
        }
    }

    // Hot cue methods

    /// Set a hot cue at the given slot (0-3) to the current position.
    pub fn set_hot_cue(&mut self, slot: u8) {
        if slot < 4 {
            let position = self.position_seconds();
            self.hot_cues[slot as usize] = Some(position);
            log::debug!(
                "Deck {}: Hot cue {} set at {:.2}s",
                self.deck_id,
                slot + 1,
                position
            );
        }
    }

    /// Set a hot cue at the given slot to a specific position.
    pub fn set_hot_cue_at(&mut self, slot: u8, position_seconds: f64) {
        if slot < 4 {
            let position = position_seconds.clamp(0.0, self.duration_seconds());
            self.hot_cues[slot as usize] = Some(position);
            log::debug!(
                "Deck {}: Hot cue {} set at {:.2}s",
                self.deck_id,
                slot + 1,
                position
            );
        }
    }

    /// Clear a hot cue at the given slot.
    pub fn clear_hot_cue(&mut self, slot: u8) {
        if slot < 4 {
            self.hot_cues[slot as usize] = None;
            log::debug!("Deck {}: Hot cue {} cleared", self.deck_id, slot + 1);
        }
    }

    /// Jump to a hot cue and start playing.
    pub fn trigger_hot_cue(&mut self, slot: u8) {
        if slot < 4 {
            if let Some(position) = self.hot_cues[slot as usize] {
                self.seek(position);
                self.play();
                log::debug!(
                    "Deck {}: Triggered hot cue {} at {:.2}s",
                    self.deck_id,
                    slot + 1,
                    position
                );
            } else {
                // If hot cue not set, set it at current position
                self.set_hot_cue(slot);
            }
        }
    }

    /// Get the position of a hot cue.
    pub fn hot_cue(&self, slot: u8) -> Option<f64> {
        if slot < 4 {
            self.hot_cues[slot as usize]
        } else {
            None
        }
    }

    /// Get all hot cue positions.
    pub fn hot_cues(&self) -> &[Option<f64>; 4] {
        &self.hot_cues
    }

    /// Load hot cue positions from HotCue structs (from database).
    pub fn load_hot_cues(&mut self, hot_cues: &[crate::library::HotCue]) {
        self.hot_cues = [None; 4];
        for cue in hot_cues {
            if cue.slot < 4 {
                self.hot_cues[cue.slot as usize] = Some(cue.position_seconds);
            }
        }
        log::debug!("Deck {}: Loaded {} hot cues", self.deck_id, hot_cues.len());
    }

    // Loop methods

    /// Set the loop points in seconds.
    ///
    /// Converts the time positions to sample positions for accurate looping.
    pub fn set_loop(&mut self, loop_in: f64, loop_out: f64) {
        // Convert seconds to samples
        self.loop_in_sample = Some((loop_in * self.sample_rate as f64) as u64);
        self.loop_out_sample = Some((loop_out * self.sample_rate as f64) as u64);
        self.loop_active = true;
        log::debug!(
            "Deck {}: Loop set from {:.2}s to {:.2}s (samples {} to {})",
            self.deck_id,
            loop_in,
            loop_out,
            self.loop_in_sample.unwrap(),
            self.loop_out_sample.unwrap()
        );
    }

    /// Enable or disable the loop.
    pub fn set_loop_active(&mut self, active: bool) {
        if self.loop_in_sample.is_some() && self.loop_out_sample.is_some() {
            self.loop_active = active;
            log::debug!("Deck {}: Loop active = {}", self.deck_id, active);
        }
    }

    /// Clear the loop points.
    pub fn clear_loop(&mut self) {
        self.loop_in_sample = None;
        self.loop_out_sample = None;
        self.loop_active = false;
        log::debug!("Deck {}: Loop cleared", self.deck_id);
    }

    /// Check if a loop is defined (has IN and OUT points).
    pub fn is_loop_defined(&self) -> bool {
        self.loop_in_sample.is_some() && self.loop_out_sample.is_some()
    }

    /// Check if the loop is active.
    pub fn is_loop_active(&self) -> bool {
        self.loop_active
    }

    /// Get loop IN point in seconds.
    pub fn loop_in_seconds(&self) -> Option<f64> {
        self.loop_in_sample
            .map(|s| s as f64 / self.sample_rate as f64)
    }

    /// Get loop OUT point in seconds.
    pub fn loop_out_seconds(&self) -> Option<f64> {
        self.loop_out_sample
            .map(|s| s as f64 / self.sample_rate as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_player() {
        let player = DeckPlayer::new(DeckId::A);
        assert_eq!(player.state(), PlayerState::Empty);
        assert_eq!(player.position_seconds(), 0.0);
    }

    #[test]
    fn test_empty_player_samples() {
        let mut player = DeckPlayer::new(DeckId::A);
        let (left, right) = player.next_stereo_sample();
        assert_eq!(left, 0.0);
        assert_eq!(right, 0.0);
    }

    #[test]
    fn test_hot_cues() {
        let mut player = DeckPlayer::new(DeckId::A);

        // All hot cues should be empty initially
        assert!(player.hot_cue(0).is_none());
        assert!(player.hot_cue(1).is_none());
        assert!(player.hot_cue(2).is_none());
        assert!(player.hot_cue(3).is_none());

        // Simulate having a loaded track by setting total_samples
        player.total_samples = 44100 * 60; // 60 seconds at 44100Hz

        // Set hot cue at specific position
        player.set_hot_cue_at(0, 10.5);
        assert!((player.hot_cue(0).unwrap() - 10.5).abs() < 0.001);

        // Set another hot cue
        player.set_hot_cue_at(2, 30.0);
        assert!((player.hot_cue(2).unwrap() - 30.0).abs() < 0.001);

        // Clear hot cue
        player.clear_hot_cue(0);
        assert!(player.hot_cue(0).is_none());

        // Invalid slot should be ignored
        player.set_hot_cue_at(5, 100.0);
        assert!(player.hot_cue(5).is_none());
    }

    #[test]
    fn test_playback_rate() {
        let mut player = DeckPlayer::new(DeckId::A);

        // Default rate should be 1.0
        assert!((player.playback_rate() - 1.0).abs() < 0.001);

        // Set to 1.1 (10% faster)
        player.set_playback_rate(1.1);
        assert!((player.playback_rate() - 1.1).abs() < 0.001);

        // Clamping should work
        player.set_playback_rate(3.0);
        assert!((player.playback_rate() - 2.0).abs() < 0.001);

        player.set_playback_rate(0.1);
        assert!((player.playback_rate() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_pitch_control() {
        use crate::library::TempoRange;

        let mut player = DeckPlayer::new(DeckId::A);

        // Center pitch should be 1.0
        let rate = player.set_pitch(0.0, TempoRange::Range10);
        assert!((rate - 1.0).abs() < 0.001);

        // +10% at full pitch with Range10
        let rate = player.set_pitch(1.0, TempoRange::Range10);
        assert!((rate - 1.1).abs() < 0.001);

        // -10% at full negative pitch with Range10
        let rate = player.set_pitch(-1.0, TempoRange::Range10);
        assert!((rate - 0.9).abs() < 0.001);

        // +6% at full pitch with Range6
        let rate = player.set_pitch(1.0, TempoRange::Range6);
        assert!((rate - 1.06).abs() < 0.001);

        // Half pitch position with Range10 should give +5%
        let rate = player.set_pitch(0.5, TempoRange::Range10);
        assert!((rate - 1.05).abs() < 0.001);
    }

    #[test]
    fn test_beat_sync() {
        use chrono::Utc;

        use crate::library::{BeatGrid, TempoRange, TrackId};

        let mut player = DeckPlayer::new(DeckId::A);

        // Set up a beat grid at 120 BPM
        let bpm = 120.0;
        let beat_grid = BeatGrid {
            track_id: TrackId(1),
            first_beat_offset_ms: 0.0,
            beat_positions: vec![],
            confidence: 0.95,
            analyzed_at: Utc::now(),
        };
        player.set_beat_grid(beat_grid, bpm);

        // Original BPM should be 120
        assert!((player.original_bpm().unwrap() - 120.0).abs() < 0.001);

        // Sync to 126 BPM (5% faster)
        let result = player.sync_to_bpm(126.0, TempoRange::Range10);
        assert!(result);

        // Effective BPM should now be 126
        assert!((player.effective_bpm().unwrap() - 126.0).abs() < 0.5);

        // Playback rate should be 1.05
        assert!((player.playback_rate() - 1.05).abs() < 0.001);

        // Sync to 130 BPM (should be within range)
        let result = player.sync_to_bpm(130.0, TempoRange::Range10);
        assert!(result);

        // Sync to 140 BPM with Range10 (out of range - would need 16.7% increase)
        let result = player.sync_to_bpm(140.0, TempoRange::Range10);
        assert!(!result);

        // But Wide should work (120 * 2.0 = 240, so 140 is within range)
        let result = player.sync_to_bpm(140.0, TempoRange::Wide);
        assert!(result);

        // Verify effective BPM is now 140
        assert!((player.effective_bpm().unwrap() - 140.0).abs() < 0.5);
    }
}
