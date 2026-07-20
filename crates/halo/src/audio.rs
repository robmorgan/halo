//! The single cpal output stream and the mixer callback.
//!
//! The stream runs at the device's default sample rate for the whole
//! session; tracks are resampled to that rate at load time, so the per-deck
//! engines are never rebuilt for rate reasons. The callback owns each
//! deck's [`EngineProcessor`] (adopted from lock-free hand-off slots),
//! renders each live deck into a scratch buffer, and sums them through
//! trim × fader × constant-power crossfader × master with per-sample gain
//! smoothing. Channels 0/1 are performance decks A/B on the crossfader;
//! channel 2 is the Prepare view's audition player, which bypasses the
//! crossfader (its fader alone is its volume).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleRate, Stream, StreamConfig};
use timestretch::engine::EngineProcessor;

use crate::deck::{ProcessorSlot, SampleSlot};
use crate::dsp::{ChannelStrip, FilterMode, Limiter, StripParams};
use crate::scrub::ScrubVoice;
use crate::state::{DeckShared, MixerShared, ScrubPhase};

/// Gain smoothing time constant in seconds (anti-zipper).
const GAIN_SMOOTH_SECS: f32 = 0.005;
/// Engine ↔ scrub-voice crossfade time constant in seconds.
const SCRUB_MIX_SECS: f32 = 0.005;
/// EMA weight for the CPU load meter.
const CPU_EMA_ALPHA: f32 = 0.1;
/// Release time constant for the per-deck level meter (fast attack, slow
/// release so the bar falls smoothly rather than flickering).
const METER_RELEASE_SECS: f32 = 0.3;

/// Everything the callback needs from one deck.
pub struct DeckAudio {
    pub shared: Arc<DeckShared>,
    pub slot: ProcessorSlot,
    pub retired: ProcessorSlot,
    /// Raw track samples for the scratch reader (bypasses the engine).
    pub scratch_source: SampleSlot,
    /// Old sample Arcs handed back so the track buffer never drops here.
    pub scratch_retired: SampleSlot,
    pub reset_request: Arc<AtomicBool>,
}

/// User-selectable output configuration (persisted between sessions).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioSettings {
    /// Output device name (None = system default).
    pub device_name: Option<String>,
    /// Requested buffer size in frames (None = device default).
    pub buffer_size: Option<u32>,
}

/// Names of the available output devices.
pub fn list_output_devices() -> Vec<String> {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    host.output_devices()
        .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

pub struct AudioOutput {
    _stream: Stream,
    pub sample_rate: u32,
    pub device_name: String,
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
                .and_then(|mut devices| devices.find(|d| d.name().is_ok_and(|n| &n == wanted)))
                .or_else(|| {
                    log::warn!("Output device {wanted:?} not found, using default");
                    host.default_output_device()
                }),
            None => host.default_output_device(),
        }
        .ok_or_else(|| "No audio output device found".to_string())?;
        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());

        let default_config = device
            .default_output_config()
            .map_err(|e| format!("Failed to get default output config: {e}"))?;
        let sample_rate = default_config.sample_rate().0;

        let config = StreamConfig {
            channels: 2,
            sample_rate: SampleRate(sample_rate),
            buffer_size: match settings.buffer_size {
                Some(frames) => cpal::BufferSize::Fixed(frames),
                None => cpal::BufferSize::Default,
            },
        };

        let gain_alpha = 1.0 - (-1.0 / (GAIN_SMOOTH_SECS * sample_rate as f32)).exp();
        let mut procs: [Option<EngineProcessor>; 3] = [None, None, None];
        let mut limiter = Limiter::new(sample_rate);
        let mut strips: [ChannelStrip; 3] = std::array::from_fn(|_| ChannelStrip::new(sample_rate));
        let mut was_rendering = [false; 3];
        // Deck gain is split around the meter tap: `pre_gains` (trim) is
        // applied to the metered signal, `gains` (fader × crossfader,
        // gated by audibility) after it — so the channel meters read
        // post-trim, pre-fader, like a DJ mixer.
        let mut pre_gains: [f32; 3] = [0.0; 3];
        let mut gains: [f32; 3] = [0.0; 3];
        let mut meters: [f32; 3] = [0.0; 3];
        let mut master_meter = 0.0f32;
        let mut scratch: Vec<f32> = vec![0.0; 16_384];
        // Scrub state: per-deck varispeed voice (raw-sample snapshot, its
        // own channel strip) and the engine↔voice crossfade mix.
        let mix_alpha = 1.0 - (-1.0 / (SCRUB_MIX_SECS * sample_rate as f32)).exp();
        let mut voices: [ScrubVoice; 3] = std::array::from_fn(|_| ScrubVoice::new(sample_rate));
        let mut scr_srcs: [Option<Arc<Vec<f32>>>; 3] = [None, None, None];
        let mut scrub_mix: [f32; 3] = [0.0; 3];
        let mut prev_phase = [ScrubPhase::Idle; 3];
        let mut scr_strips: [ChannelStrip; 3] =
            std::array::from_fn(|_| ChannelStrip::new(sample_rate));
        let mut scr_buf: Vec<f32> = vec![0.0; 16_384];

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let t0 = Instant::now();

                    data.fill(0.0);
                    if scratch.len() < data.len() {
                        scratch.resize(data.len(), 0.0);
                    }
                    if scr_buf.len() < data.len() {
                        scr_buf.resize(data.len(), 0.0);
                    }

                    let xf = mixer.crossfader.load().clamp(0.0, 1.0);
                    let master = mixer.master.load();
                    // Decks A/B ride the constant-power crossfader; the
                    // audition channel bypasses it.
                    let xfade_gains = [
                        (xf * std::f32::consts::FRAC_PI_2).cos(),
                        (xf * std::f32::consts::FRAC_PI_2).sin(),
                        1.0,
                    ];

                    // Real time this buffer represents; drives both the meter
                    // release ballistics and the CPU-load figure below.
                    let budget = (data.len() / 2) as f32 / sample_rate as f32;
                    let meter_release = 1.0 - (-budget / METER_RELEASE_SECS).exp();

                    for (i, deck) in decks.iter().enumerate() {
                        // Adopt a newly loaded processor; retire the old one
                        // so it drops off the audio thread. try_lock only —
                        // if the UI holds either slot this block, skip.
                        if let Ok(mut slot) = deck.slot.try_lock()
                            && slot.is_some()
                            && let Ok(mut retired) = deck.retired.try_lock()
                        {
                            *retired = std::mem::replace(&mut procs[i], slot.take());
                        }

                        // Acknowledge a pending warm-start reset before
                        // anything else so seeks work while muted.
                        if deck.reset_request.load(Ordering::Acquire) {
                            if let Some(p) = &mut procs[i] {
                                p.reset();
                            }
                            deck.reset_request.store(false, Ordering::Release);
                        }

                        // Adopt/refresh the raw-sample snapshot for the
                        // scrub voice; retire a stale Arc so the track
                        // buffer never deallocates on the audio thread.
                        if let Ok(src) = deck.scratch_source.try_lock() {
                            let differs = match (&scr_srcs[i], &*src) {
                                (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
                                (a, b) => a.is_some() != b.is_some(),
                            };
                            if differs && let Ok(mut retired) = deck.scratch_retired.try_lock() {
                                *retired = std::mem::replace(&mut scr_srcs[i], src.clone());
                            }
                        }
                        let source: &[f32] = scr_srcs[i].as_ref().map_or(&[], |s| s.as_slice());

                        // Scrub phase edges: seed on a fresh engage; start
                        // the release glide (and publish its predicted
                        // landing for the UI's parallel engine warm-start)
                        // on entry to Settling. A press+release inside one
                        // block arrives as Idle → Settling — seed first,
                        // then glide.
                        let phase = deck.shared.scrub.phase();
                        if prev_phase[i] == ScrubPhase::Idle && phase != ScrubPhase::Idle {
                            voices[i].seed(deck.shared.scrub.target());
                            scr_strips[i].reset();
                        }
                        if prev_phase[i] != ScrubPhase::Settling && phase == ScrubPhase::Settling {
                            let landing = voices[i]
                                .begin_settle(deck.shared.scrub.settle_rate_target(), source);
                            deck.shared.scrub.publish_landing(landing);
                        }
                        prev_phase[i] = phase;
                        let scrubbing = phase != ScrubPhase::Idle;

                        let live = deck.shared.stream_active.load(Ordering::Relaxed);
                        // Engine path. Keep consuming through the short
                        // fade-out after pause so the ramp lands on real
                        // audio, not an instant cut; once faded, freeze the
                        // engine (its state survives pause; a seek resets it
                        // anyway). While the scrub voice fully owns the mix
                        // the engine is likewise left unconsumed (frozen) —
                        // its state is discarded by the release-time
                        // warm-start seek.
                        let rendering = (live || gains[i] > 1e-4) && scrub_mix[i] < 1.0;
                        let buf = &mut scratch[..data.len()];
                        if rendering {
                            if let Some(p) = &mut procs[i] {
                                p.process(buf);
                            } else {
                                buf.fill(0.0);
                            }
                            // Channel strip: isolator EQ then LP/HP filter.
                            if !was_rendering[i] {
                                strips[i].reset();
                            }
                            strips[i].process(
                                buf,
                                StripParams {
                                    eq: [
                                        deck.shared.eq_low.load(),
                                        deck.shared.eq_mid.load(),
                                        deck.shared.eq_high.load(),
                                    ],
                                    filter_mode: FilterMode::from_u8(deck.shared.filter_mode_u8()),
                                    cutoff: deck.shared.filter_cutoff.load(),
                                },
                            );
                        } else {
                            buf.fill(0.0);
                        }
                        was_rendering[i] = rendering;

                        // Scrub voice: chases the hand while `Active`, then
                        // glides its momentum through `Settling` and the
                        // post-settle mix ramp-out (past the landing it holds
                        // the settle rate, time-aligned with the engine
                        // warm-started there). Its own strip keeps EQ/filter
                        // applying to scratch audio.
                        let vbuf = &mut scr_buf[..data.len()];
                        if scrubbing || scrub_mix[i] > 0.0 {
                            match phase {
                                ScrubPhase::Active => {
                                    voices[i].render(deck.shared.scrub.target(), source, vbuf);
                                }
                                ScrubPhase::Settling | ScrubPhase::Idle => {
                                    if voices[i].render_settle(source, vbuf)
                                        && phase == ScrubPhase::Settling
                                    {
                                        deck.shared.scrub.finish_settle();
                                        prev_phase[i] = ScrubPhase::Idle;
                                    }
                                }
                            }
                            deck.shared.scrub.publish_voice_frame(voices[i].position());
                            scr_strips[i].process(
                                vbuf,
                                StripParams {
                                    eq: [
                                        deck.shared.eq_low.load(),
                                        deck.shared.eq_mid.load(),
                                        deck.shared.eq_high.load(),
                                    ],
                                    filter_mode: FilterMode::from_u8(deck.shared.filter_mode_u8()),
                                    cutoff: deck.shared.filter_cutoff.load(),
                                },
                            );
                        } else {
                            vbuf.fill(0.0);
                        }

                        // Blend engine ↔ voice per frame and apply the deck
                        // gain in two stages: trim first (the meter reads
                        // the post-trim signal), fader × crossfader after,
                        // gated by audibility — it stays up during a
                        // paused-deck scrub (`scrubbing`) so the voice is
                        // audible even though the engine is muted.
                        let target_pre = deck.shared.trim.load();
                        let target_gain = if live || scrubbing || scrub_mix[i] > 0.0 {
                            deck.shared.fader.load() * xfade_gains[i]
                        } else {
                            0.0
                        };
                        let mut g_pre = pre_gains[i];
                        let mut g = gains[i];
                        let mut mix = scrub_mix[i];
                        let mix_target: f32 = if scrubbing { 1.0 } else { 0.0 };
                        let mut peak = 0.0f32;
                        for ((out, e), v) in data
                            .chunks_exact_mut(2)
                            .zip(buf.chunks_exact(2))
                            .zip(vbuf.chunks_exact(2))
                        {
                            g_pre += (target_pre - g_pre) * gain_alpha;
                            g += (target_gain - g) * gain_alpha;
                            mix += (mix_target - mix) * mix_alpha;
                            let pl = (e[0] * (1.0 - mix) + v[0] * mix) * g_pre;
                            let pr = (e[1] * (1.0 - mix) + v[1] * mix) * g_pre;
                            out[0] += pl * g;
                            out[1] += pr * g;
                            peak = peak.max(pl.abs()).max(pr.abs());
                        }
                        pre_gains[i] = g_pre;
                        gains[i] = if !live && !scrubbing && g < 1e-4 {
                            0.0
                        } else {
                            g
                        };
                        // Snap the asymptotic one-pole at the rails: without
                        // this, mix stalls one ulp below 1.0 and the engine
                        // keeps being consumed (unfed, at inaudible gain) for
                        // the whole drag, draining its ring into underruns.
                        if scrubbing && mix > 0.999 {
                            mix = 1.0;
                        } else if !scrubbing && mix < 1e-4 {
                            mix = 0.0;
                        }
                        scrub_mix[i] = mix;

                        // Pre-fader channel meter: instant attack, slow release.
                        let m = &mut meters[i];
                        *m = if peak >= *m {
                            peak
                        } else {
                            *m + (peak - *m) * meter_release
                        };
                        deck.shared.meter.store(*m);
                    }

                    for s in data.iter_mut() {
                        *s *= master;
                    }
                    limiter.process(data);

                    // Master output meter: peak of what actually leaves the
                    // stream (post-master, post-limiter), same ballistics as
                    // the channel meters.
                    let mut out_peak = 0.0f32;
                    for &s in data.iter() {
                        out_peak = out_peak.max(s.abs());
                    }
                    master_meter = if out_peak >= master_meter {
                        out_peak
                    } else {
                        master_meter + (out_peak - master_meter) * meter_release
                    };
                    mixer.master_meter.store(master_meter);

                    // CPU load: render time over the real time this buffer
                    // represents (`budget`, computed above), EMA-smoothed.
                    if budget > 0.0 {
                        let load = t0.elapsed().as_secs_f32() / budget;
                        let old = mixer.cpu_load.load();
                        mixer.cpu_load.store(old + (load - old) * CPU_EMA_ALPHA);
                    }
                },
                move |err| {
                    log::error!("Audio output error: {err}");
                },
                None,
            )
            .map_err(|e| format!("Failed to build output stream: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start audio stream: {e}"))?;

        Ok(AudioOutput {
            _stream: stream,
            sample_rate,
            device_name,
        })
    }
}
