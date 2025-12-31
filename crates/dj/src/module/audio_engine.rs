//! Multi-channel audio engine for DJ deck output.
//!
//! Handles routing two stereo deck outputs to separate output channels
//! on a multi-channel audio interface (e.g., Motu M4).

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use parking_lot::RwLock;

use super::DeckPlayer;
use crate::deck::DeckId;

/// Audio engine configuration.
#[derive(Debug, Clone)]
pub struct AudioEngineConfig {
    /// Audio device name (empty for default).
    pub device_name: String,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Buffer size in samples.
    pub buffer_size: u32,
    /// Output channels for Deck A (left, right).
    pub deck_a_channels: (u16, u16),
    /// Output channels for Deck B (left, right).
    pub deck_b_channels: (u16, u16),
}

impl Default for AudioEngineConfig {
    fn default() -> Self {
        Self {
            device_name: String::new(),
            sample_rate: 44100,
            buffer_size: 512,
            // Deck A on outputs 1-2 (channels 0-1)
            deck_a_channels: (0, 1),
            // Deck B on outputs 3-4 (channels 2-3)
            deck_b_channels: (2, 3),
        }
    }
}

/// Multi-channel audio engine for DJ playback.
pub struct DjAudioEngine {
    /// Configuration.
    config: AudioEngineConfig,
    /// Audio output stream.
    stream: Option<Stream>,
    /// Deck A player.
    deck_a_player: Arc<RwLock<DeckPlayer>>,
    /// Deck B player.
    deck_b_player: Arc<RwLock<DeckPlayer>>,
    /// Number of output channels on the device.
    output_channels: u16,
    /// Which deck is the tempo master (None = auto-select playing deck).
    master_deck: Option<DeckId>,
}

impl DjAudioEngine {
    /// Create a new audio engine with the given configuration.
    pub fn new(config: AudioEngineConfig) -> Self {
        Self {
            config,
            stream: None,
            deck_a_player: Arc::new(RwLock::new(DeckPlayer::new(DeckId::A))),
            deck_b_player: Arc::new(RwLock::new(DeckPlayer::new(DeckId::B))),
            output_channels: 4, // Default to 4 channels
            master_deck: None,
        }
    }

    /// Get a reference to a deck player.
    pub fn deck_player(&self, id: DeckId) -> &Arc<RwLock<DeckPlayer>> {
        match id {
            DeckId::A => &self.deck_a_player,
            DeckId::B => &self.deck_b_player,
        }
    }

    /// Find the audio device by name.
    fn find_device(&self) -> Result<Device, anyhow::Error> {
        let host = cpal::default_host();

        if self.config.device_name.is_empty() {
            return host
                .default_output_device()
                .ok_or_else(|| anyhow::anyhow!("No default output device available"));
        }

        // Search for device by name
        for device in host.output_devices()? {
            if let Ok(name) = device.name() {
                if name.contains(&self.config.device_name) {
                    log::info!("Found audio device: {}", name);
                    return Ok(device);
                }
            }
        }

        // Fall back to default
        log::warn!(
            "Device '{}' not found, using default",
            self.config.device_name
        );
        host.default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No default output device available"))
    }

    /// Find a supported stream config with the required number of channels.
    fn find_config(&self, device: &Device) -> Result<StreamConfig, anyhow::Error> {
        let supported_configs = device.supported_output_configs()?;

        // Find the maximum channel count needed
        let max_channel = self
            .config
            .deck_a_channels
            .0
            .max(self.config.deck_a_channels.1)
            .max(self.config.deck_b_channels.0)
            .max(self.config.deck_b_channels.1)
            + 1;

        // Look for a config with enough channels
        for config_range in supported_configs {
            if config_range.channels() >= max_channel
                && config_range.sample_format() == SampleFormat::F32
            {
                // Check if our target sample rate is within the supported range
                let target_rate = self.config.sample_rate;

                if target_rate >= config_range.min_sample_rate()
                    && target_rate <= config_range.max_sample_rate()
                {
                    return Ok(config_range.with_sample_rate(target_rate).into());
                }
            }
        }

        // Fall back to default config
        let default_config = device.default_output_config()?;
        log::warn!(
            "Could not find {}-channel config, using default ({} channels)",
            max_channel,
            default_config.channels()
        );
        Ok(default_config.into())
    }

    /// Start the audio engine.
    pub fn start(&mut self) -> Result<(), anyhow::Error> {
        let device = self.find_device()?;
        let config = self.find_config(&device)?;

        self.output_channels = config.channels;
        log::info!(
            "Starting audio engine: {} channels @ {} Hz",
            config.channels,
            config.sample_rate
        );

        let deck_a = Arc::clone(&self.deck_a_player);
        let deck_b = Arc::clone(&self.deck_b_player);
        let deck_a_channels = self.config.deck_a_channels;
        let deck_b_channels = self.config.deck_b_channels;
        let channels = config.channels as usize;

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // Fill buffer with silence first
                data.fill(0.0);

                // Process each frame
                for frame in data.chunks_mut(channels) {
                    // Get samples from deck players
                    let (a_left, a_right) = deck_a.write().next_stereo_sample();
                    let (b_left, b_right) = deck_b.write().next_stereo_sample();

                    // Route Deck A to configured channels
                    if (deck_a_channels.0 as usize) < frame.len() {
                        frame[deck_a_channels.0 as usize] = a_left;
                    }
                    if (deck_a_channels.1 as usize) < frame.len() {
                        frame[deck_a_channels.1 as usize] = a_right;
                    }

                    // Route Deck B to configured channels
                    if (deck_b_channels.0 as usize) < frame.len() {
                        frame[deck_b_channels.0 as usize] = b_left;
                    }
                    if (deck_b_channels.1 as usize) < frame.len() {
                        frame[deck_b_channels.1 as usize] = b_right;
                    }
                }
            },
            |err| {
                log::error!("Audio stream error: {}", err);
            },
            None,
        )?;

        stream.play()?;
        self.stream = Some(stream);

        log::info!("Audio engine started");
        Ok(())
    }

    /// Stop the audio engine.
    pub fn stop(&mut self) {
        if let Some(stream) = self.stream.take() {
            drop(stream);
            log::info!("Audio engine stopped");
        }
    }

    /// Check if the engine is running.
    pub fn is_running(&self) -> bool {
        self.stream.is_some()
    }

    /// Get the number of output channels.
    pub fn output_channels(&self) -> u16 {
        self.output_channels
    }

    // Master deck methods

    /// Set the master deck.
    ///
    /// The master deck provides the tempo reference for sync and lighting.
    /// Pass `None` to auto-select the playing deck.
    pub fn set_master_deck(&mut self, deck: Option<DeckId>) {
        self.master_deck = deck;
        log::debug!("Master deck set to: {:?}", deck);
    }

    /// Get the current master deck.
    ///
    /// Returns the explicitly set master deck, or auto-selects based on
    /// which deck is currently playing.
    pub fn master_deck(&self) -> Option<DeckId> {
        if let Some(deck) = self.master_deck {
            return Some(deck);
        }

        // Auto-select: prefer the deck that is playing
        let a_playing = self.deck_a_player.read().state() == super::PlayerState::Playing;
        let b_playing = self.deck_b_player.read().state() == super::PlayerState::Playing;

        match (a_playing, b_playing) {
            (true, false) => Some(DeckId::A),
            (false, true) => Some(DeckId::B),
            (true, true) => Some(DeckId::A), // Both playing: prefer A
            (false, false) => None,          // Neither playing
        }
    }

    /// Get the master BPM (effective BPM of the master deck).
    pub fn master_bpm(&self) -> Option<f64> {
        let master = self.master_deck()?;
        let player = self.deck_player(master).read();
        player.effective_bpm()
    }

    /// Get the master beat phase (0.0-1.0).
    pub fn master_beat_phase(&self) -> Option<f64> {
        let master = self.master_deck()?;
        let player = self.deck_player(master).read();
        player.beat_phase()
    }

    /// Get the master bar phase (0.0-1.0).
    pub fn master_bar_phase(&self) -> Option<f64> {
        let master = self.master_deck()?;
        let player = self.deck_player(master).read();
        player.bar_phase()
    }

    /// Get the master phrase phase (0.0-1.0).
    pub fn master_phrase_phase(&self) -> Option<f64> {
        let master = self.master_deck()?;
        let player = self.deck_player(master).read();
        player.phrase_phase()
    }

    /// Update sync corrections for all decks with sync enabled.
    ///
    /// This should be called periodically (e.g., 20-50 times per second) to maintain
    /// continuous beat lock between synced decks and the master.
    pub fn update_sync_corrections(&self) {
        let master = match self.master_deck() {
            Some(m) => m,
            None => return, // No master, nothing to sync
        };

        // Get master's bar phase
        let master_phase = {
            let player = self.deck_player(master).read();
            if player.state() != super::PlayerState::Playing {
                return; // Master not playing, skip sync
            }
            match player.bar_phase() {
                Some(p) => p,
                None => return,
            }
        };

        // Update sync correction for each non-master deck
        for deck in [DeckId::A, DeckId::B] {
            if deck == master {
                continue;
            }

            let mut player = self.deck_player(deck).write();

            // Only sync if deck has sync enabled and is playing
            if !player.is_sync_enabled() || player.state() != super::PlayerState::Playing {
                continue;
            }

            // Get this deck's bar phase
            let sync_phase = match player.bar_phase() {
                Some(p) => p,
                None => continue,
            };

            // Calculate phase difference (-0.5 to 0.5, wrapped)
            let mut phase_diff = master_phase - sync_phase;
            if phase_diff > 0.5 {
                phase_diff -= 1.0;
            } else if phase_diff < -0.5 {
                phase_diff += 1.0;
            }

            // Calculate correction factor (proportional control)
            // phase_diff of 0.25 (quarter bar behind) should speed up by ~1%
            // This creates a soft lock that gradually catches up
            let correction = phase_diff * 0.04; // 4% max correction for full quarter-bar offset

            player.set_sync_correction(correction);
        }
    }

    /// Sync a deck to the master deck's tempo and align beat phase.
    ///
    /// Returns true if sync was successful.
    pub fn sync_to_master(&self, deck: DeckId, tempo_range: crate::library::TempoRange) -> bool {
        // Get master deck
        let master = match self.master_deck() {
            Some(m) if m != deck => m,
            _ => return false, // Can't sync to self or no master
        };

        // Get master BPM and bar phase
        let (target_bpm, master_bar_phase) = {
            let player = self.deck_player(master).read();
            match player.effective_bpm() {
                Some(bpm) => (bpm, player.bar_phase()),
                None => return false,
            }
        };

        // Sync BPM first
        let mut player = self.deck_player(deck).write();
        if !player.sync_to_bpm(target_bpm, tempo_range) {
            return false;
        }

        // Enable continuous sync on this deck
        player.set_sync_enabled(true);

        // Now align beat phase if both decks have beat grids
        if let (Some(master_phase), Some(sync_phase)) = (master_bar_phase, player.bar_phase()) {
            // Calculate how far off we are (in bars, -0.5 to 0.5 wrapped)
            let mut phase_diff = master_phase - sync_phase;
            if phase_diff > 0.5 {
                phase_diff -= 1.0;
            } else if phase_diff < -0.5 {
                phase_diff += 1.0;
            }

            // Calculate the duration of one bar at current BPM
            let effective_bpm = player.effective_bpm().unwrap_or(target_bpm);
            let beat_duration = 60.0 / effective_bpm; // seconds per beat
            let bar_duration = beat_duration * 4.0; // seconds per bar

            // Calculate time offset needed to align
            let time_offset = phase_diff * bar_duration;

            // Only adjust if the offset is significant (> 10ms)
            if time_offset.abs() > 0.01 {
                let current_pos = player.position_seconds();
                let new_pos = (current_pos + time_offset).max(0.0);
                player.seek(new_pos);
                log::info!(
                    "Beat aligned: phase diff {:.3}, time offset {:.3}s, seeking to {:.3}s",
                    phase_diff,
                    time_offset,
                    new_pos
                );
            }
        }

        true
    }

    /// Disable sync on a deck.
    pub fn disable_sync(&self, deck: DeckId) {
        let mut player = self.deck_player(deck).write();
        player.set_sync_enabled(false);
    }
}

impl Drop for DjAudioEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Information about an audio output device.
#[derive(Debug, Clone)]
pub struct AudioDeviceInfo {
    /// Device name.
    pub name: String,
    /// Maximum number of output channels.
    pub max_channels: u16,
    /// Whether this is the default device.
    pub is_default: bool,
}

/// List available audio output devices with their capabilities.
pub fn list_audio_devices() -> Vec<AudioDeviceInfo> {
    let host = cpal::default_host();
    let default_name = host
        .default_output_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    let mut devices = Vec::new();

    if let Ok(output_devices) = host.output_devices() {
        for device in output_devices {
            if let Ok(name) = device.name() {
                // Find maximum channel count from supported configs
                let max_channels = device
                    .supported_output_configs()
                    .ok()
                    .map(|configs| configs.map(|c| c.channels()).max().unwrap_or(2))
                    .unwrap_or(2);

                devices.push(AudioDeviceInfo {
                    is_default: name == default_name,
                    name,
                    max_channels,
                });
            }
        }
    }

    devices
}

/// Get the default audio device name.
pub fn default_device_name() -> Option<String> {
    let host = cpal::default_host();
    host.default_output_device().and_then(|d| d.name().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AudioEngineConfig::default();
        assert_eq!(config.deck_a_channels, (0, 1));
        assert_eq!(config.deck_b_channels, (2, 3));
        assert_eq!(config.sample_rate, 44100);
    }

    #[test]
    fn test_list_devices() {
        // This should not panic even if no devices are available
        let devices = list_audio_devices();
        println!("Available audio devices: {:?}", devices);
    }
}
