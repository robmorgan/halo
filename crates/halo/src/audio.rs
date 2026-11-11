//! cpal device setup. Sessions run on deck workers; the callback only delivers
//! their completed audio, scrub voices and the mixer through fixed storage.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};

pub use crate::delivery::DeckAudio;
use crate::delivery::MixerRenderer;
use crate::routing::{DeviceRouting, Layout};
use crate::state::MixerShared;

/// User-selectable output configuration (persisted between sessions).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AudioSettings {
    /// Output device name (None = system default).
    pub device_name: Option<String>,
    /// Requested buffer size in frames (None = device default).
    pub buffer_size: Option<u32>,
    /// Each edited device's routing; the rest get their default.
    pub routings: Vec<DeviceRouting>,
    /// The headphone cue was turned off before routing existed: new
    /// devices' defaults leave it off.
    pub cue_off_default: bool,
}

/// An output device: its name and how many outputs it has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputDevice {
    pub name: String,
    pub channels: u16,
}

/// The available output devices.
pub fn list_output_devices() -> Vec<OutputDevice> {
    let host = cpal::default_host();
    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| {
            let name = device_name(&device)?;
            let channels = max_channels(&device, None);
            Some(OutputDevice { name, channels })
        })
        .collect()
}

/// A device's name, as Options lists it and the settings store it.
fn device_name(device: &cpal::Device) -> Option<String> {
    Some(device.description().ok()?.name().to_string())
}

/// The system default output's name.
pub fn default_output_device() -> Option<String> {
    device_name(&cpal::default_host().default_output_device()?)
}

/// The supported output channel counts, at `rate` when given.
fn channel_counts(device: &cpal::Device, rate: Option<u32>) -> Vec<u16> {
    let Ok(configs) = device.supported_output_configs() else {
        return Vec::new();
    };
    configs
        .filter(|config| {
            rate.is_none_or(|rate| {
                (config.min_sample_rate()..=config.max_sample_rate()).contains(&rate)
            })
        })
        .map(|config| config.channels())
        .collect()
}

/// The most outputs `device` opens, at `rate` when given; 2 if it can't say.
fn max_channels(device: &cpal::Device, rate: Option<u32>) -> u16 {
    channel_counts(device, rate).into_iter().max().unwrap_or(2)
}

/// The fewest channels, from `wanted` up, that `device` opens at `rate`.
fn channels_at_least(device: &cpal::Device, rate: u32, wanted: u16) -> Option<u16> {
    channel_counts(device, Some(rate))
        .into_iter()
        .filter(|&channels| channels >= wanted)
        .min()
}

pub struct AudioOutput {
    _stream: Stream,
    pub sample_rate: u32,
    pub device_name: String,
    /// What the stream's pairs carry.
    pub layout: Layout,
    /// The device's routing, for its pair names.
    pub routing: DeviceRouting,
}

impl AudioOutput {
    pub fn new(
        decks: [DeckAudio; 3],
        mixer: Arc<MixerShared>,
        settings: &AudioSettings,
    ) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = match &settings.device_name {
            Some(wanted) => host
                .output_devices()
                .ok()
                .and_then(|mut devices| {
                    devices.find(|d| device_name(d).is_some_and(|n| &n == wanted))
                })
                .or_else(|| {
                    log::warn!("Output device {wanted:?} not found, using default");
                    host.default_output_device()
                }),
            None => host.default_output_device(),
        }
        .ok_or_else(|| "No audio output device found".to_string())?;
        let device_name = device_name(&device).unwrap_or_else(|| "Unknown".to_string());

        let default_config = device
            .default_output_config()
            .map_err(|e| format!("Failed to get default output config: {e}"))?;
        let sample_rate = default_config.sample_rate();
        let routing = DeviceRouting::for_device(
            &settings.routings,
            &device_name,
            max_channels(&device, None),
            settings.cue_off_default,
        );
        let mut layout = routing.resolve(max_channels(&device, Some(sample_rate)));
        if layout != routing.resolve(u16::MAX) {
            log::warn!("{device_name}: some outputs are past what it opens at {sample_rate} Hz");
        }
        match channels_at_least(&device, sample_rate, layout.channels) {
            Some(channels) => layout.channels = channels,
            None => layout = Layout::stereo(),
        }
        let channels = layout.channels;
        let cue = layout.phones().is_some();
        let external = layout.external();
        // Only a running stream has phones, or a pair per deck.
        mixer.cue_output.store(false, Ordering::Release);
        mixer.external.store(false, Ordering::Release);
        let shared = mixer.clone();

        let config = StreamConfig {
            channels,
            sample_rate,
            buffer_size: match settings.buffer_size {
                Some(frames) => cpal::BufferSize::Fixed(frames),
                None => cpal::BufferSize::Default,
            },
        };

        let mut renderer = MixerRenderer::new(decks, mixer, sample_rate, layout);
        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| renderer.render(data),
                move |err| {
                    log::error!("Audio output error: {err}");
                },
                None,
            )
            .map_err(|e| format!("Failed to build output stream: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start audio stream: {e}"))?;
        shared.cue_output.store(cue, Ordering::Release);
        shared.external.store(external, Ordering::Release);

        log::info!(
            "Audio output: {device_name}, {channels} channels @ {sample_rate} Hz, {:?}",
            layout.mixing
        );
        Ok(AudioOutput {
            _stream: stream,
            sample_rate,
            device_name,
            layout,
            routing,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::deck::Deck;
    use crate::state::Transport;

    fn wait(mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready() {
            assert!(Instant::now() < deadline, "audio device made no progress");
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// Exercises real device scheduling and receipts with silent master output.
    /// Run separately from benchmarks and normal CI, which has no audio device.
    #[test]
    #[ignore = "requires a real default output device; run alone in release mode"]
    fn default_device_transport_qualification() {
        for frames in [128, 512, 2048] {
            let mut decks: [Deck; 3] = std::array::from_fn(|_| Deck::new());
            let mixer = Arc::new(MixerShared::new());
            mixer.master.store(0.0);
            let output = AudioOutput::new(
                std::array::from_fn(|i| DeckAudio {
                    shared: decks[i].shared.clone(),
                    handoff: decks[i].handoff.clone(),
                }),
                mixer,
                &AudioSettings {
                    buffer_size: Some(frames),
                    ..Default::default()
                },
            )
            .unwrap();
            let rate = output.sample_rate;
            let source: Arc<Vec<f32>> = Arc::new(
                (0..rate * 8)
                    .flat_map(|i| {
                        let x =
                            (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.1;
                        [x, x * 0.9]
                    })
                    .collect(),
            );
            for (i, deck) in decks.iter_mut().enumerate() {
                deck.shared.tempo_rate.store([0.92, 1.08, 0.75][i]);
                deck.load("device qualification".into(), source.clone(), rate, None)
                    .unwrap();
                deck.shared.set_transport(Transport::Playing);
            }
            wait(|| {
                decks
                    .iter()
                    .all(|d| d.shared.playhead_frames() > rate as usize / 2)
            });
            for deck in &decks {
                deck.shared.set_transport(Transport::Stopped);
            }
            // Allow the final in-flight callback receipt to reach the worker.
            thread::sleep(Duration::from_millis(100));
            let parked = decks.each_ref().map(|d| d.shared.playhead_frames());
            thread::sleep(Duration::from_millis(100));
            assert_eq!(parked, decks.each_ref().map(|d| d.shared.playhead_frames()));

            for deck in &decks {
                deck.shared
                    .set_loop(Some((rate as usize, rate as usize * 2)));
                deck.shared.request_seek(rate as usize);
                deck.shared.set_transport(Transport::Playing);
            }
            wait(|| {
                decks
                    .iter()
                    .all(|d| d.shared.playhead_frames() > rate as usize * 5 / 4)
            });
            thread::sleep(Duration::from_millis(1500));
            for deck in &mut decks {
                assert_eq!(deck.poll(), None);
                assert!(
                    (rate as usize..rate as usize * 2).contains(&deck.shared.playhead_frames())
                );
                assert_eq!(deck.shared.underrun_frames.load(Ordering::Relaxed), 0);
            }
            println!(
                "device={} rate={rate} buffer={frames}: three decks, pause, seek, loop, zero underruns",
                output.device_name
            );
        }
    }
}
