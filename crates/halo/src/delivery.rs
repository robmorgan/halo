//! Device-side delivery and mixer. All storage is prepared before cpal calls
//! render; track handoffs return retired storage to the UI for destruction.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use crossbeam_queue::ArrayQueue;
use timestretch::{MomentaryLoudness, TransitionEnvelope};

use crate::dsp::{ChannelStrip, FilterMode, Limiter, StripParams};
use crate::output_queue::Consumer;
use crate::scrub::ScrubVoice;
use crate::state::{DeckShared, MixerShared, ScrubPhase, Transport};

const BLOCK_SAMPLES: usize = 4096;

pub struct Playback {
    pub consumer: Consumer,
    pub samples: Arc<Vec<f32>>,
    pub generation: u64,
}

pub struct DeliveryHandoff {
    pending: ArrayQueue<Playback>,
    retired: ArrayQueue<Playback>,
}

impl DeliveryHandoff {
    pub fn new() -> Self {
        Self {
            pending: ArrayQueue::new(1),
            retired: ArrayQueue::new(2),
        }
    }

    /// UI only. Superseded, unadopted deliveries are destroyed here.
    pub fn publish(&self, playback: Playback) {
        drop(self.pending.force_push(playback));
    }

    pub fn reclaim(&self) {
        while self.retired.pop().is_some() {}
    }
}

pub struct DeckAudio {
    pub shared: Arc<DeckShared>,
    pub handoff: Arc<DeliveryHandoff>,
}

struct DeckVoice {
    input: DeckAudio,
    playback: Option<Playback>,
    retiring: Option<Playback>,
    strip: ChannelStrip,
    scrub_strip: ChannelStrip,
    voice: ScrubVoice,
    phase: ScrubPhase,
    scrub_mix: f32,
    gain: f32,
    pre_gain: f32,
    meter: f32,
    lufs: Option<MomentaryLoudness>,
    transition: TransitionEnvelope,
    last_frame: [f32; 2],
    was_live: bool,
    engine: Box<[f32; BLOCK_SAMPLES]>,
    scratch: Box<[f32; BLOCK_SAMPLES]>,
    sample_rate: u32,
}

impl DeckVoice {
    fn new(input: DeckAudio, sample_rate: u32) -> Self {
        Self {
            input,
            playback: None,
            retiring: None,
            strip: ChannelStrip::new(sample_rate),
            scrub_strip: ChannelStrip::new(sample_rate),
            voice: ScrubVoice::new(sample_rate),
            phase: ScrubPhase::Idle,
            scrub_mix: 0.0,
            gain: 0.0,
            pre_gain: 0.0,
            meter: 0.0,
            lufs: MomentaryLoudness::new(sample_rate, 2),
            transition: Default::default(),
            last_frame: [0.0; 2],
            was_live: false,
            engine: Box::new([0.0; BLOCK_SAMPLES]),
            scratch: Box::new([0.0; BLOCK_SAMPLES]),
            sample_rate,
        }
    }

    fn adopt(&mut self) {
        // Backpressure retains ownership locally; it must never destroy the
        // previous source/queue on this thread, even if the UI stops polling.
        if let Some(old) = self.retiring.take() {
            self.retiring = self.input.handoff.retired.push(old).err();
        }
        if self.retiring.is_none()
            && let Some(next) = self.input.handoff.pending.pop()
        {
            self.retiring = self.playback.replace(next);
            self.strip.reset();
            self.scrub_strip.reset();
            self.voice.seed(0.0);
            self.phase = ScrubPhase::Idle;
            self.scrub_mix = 0.0;
            self.transition.begin(self.last_frame);
            if let Some(lufs) = &mut self.lufs {
                lufs.reset();
            }
        }
        if let Some(playback) = &mut self.playback
            && playback.consumer.sync_epoch()
        {
            self.strip.reset();
            self.transition.begin(self.last_frame);
            if let Some(lufs) = &mut self.lufs {
                lufs.reset();
            }
        }
    }

    fn render_into(&mut self, out: &mut [f32], fader: f32) {
        self.adopt();
        let shared = &self.input.shared;
        let Some(playback) = &self.playback else {
            return;
        };
        if playback.generation != shared.track_generation.load(Ordering::Acquire) {
            return;
        }
        let mut phase = shared.scrub.phase();
        if self.phase == ScrubPhase::Idle && phase != ScrubPhase::Idle {
            self.voice.seed(shared.scrub.target());
            self.voice.set_lead_in(shared.scrub.lead_in());
            self.scrub_strip.reset();
        }
        if self.phase != ScrubPhase::Settling && phase == ScrubPhase::Settling {
            let landing = self
                .voice
                .begin_settle(shared.scrub.settle_rate_target(), &playback.samples);
            shared.scrub.publish_landing(landing);
        }
        self.phase = phase;
        let mut offset = 0;
        while offset < out.len() {
            // End a segment exactly at the scrub landing. The following segment
            // starts both voices at that source frame, even with a large callback.
            if phase == ScrubPhase::Settling && self.voice.settle_remaining_frames() == 0 {
                let (sequence, landing) = self.input.shared.scrub.landing();
                if self.input.shared.scrub_ready.load(Ordering::Acquire) == sequence {
                    let rate = if self.input.shared.transport() == Transport::Playing
                        && !self.input.shared.tempo_stopped.load(Ordering::Relaxed)
                    {
                        self.input.shared.achieved_tempo.load()
                    } else {
                        0.0
                    };
                    self.voice.align_landing(landing.floor(), rate);
                    self.input.shared.scrub.finish_settle();
                    phase = self.input.shared.scrub.phase();
                    self.phase = phase;
                }
            }
            let remaining = if phase == ScrubPhase::Settling {
                self.voice.settle_remaining_frames() as usize
            } else {
                0
            };
            let count = if remaining > 0 {
                (out.len() - offset).min(remaining * 2)
            } else {
                out.len() - offset
            };
            self.render_segment(&mut out[offset..offset + count], fader, phase);
            offset += count;
        }
    }

    fn render_segment(&mut self, out: &mut [f32], fader: f32, phase: ScrubPhase) {
        let Some(playback) = &mut self.playback else {
            return;
        };
        let shared = &self.input.shared;
        let live = shared.stream_active.load(Ordering::Acquire)
            && shared.transport() == Transport::Playing
            && !shared.tempo_stopped.load(Ordering::Relaxed);
        if live != self.was_live {
            self.transition.begin(self.last_frame);
            self.was_live = live;
        }
        let buf = &mut self.engine[..out.len()];
        if live
            && (phase == ScrubPhase::Idle || self.scrub_mix < 1.0)
            && phase != ScrubPhase::Settling
        {
            let frames = playback.consumer.read(buf);
            playback.consumer.acknowledge_frames(frames);
        } else {
            buf.fill(0.0);
        }
        let params = StripParams {
            eq: [
                shared.eq_low.load(),
                shared.eq_mid.load(),
                shared.eq_high.load(),
            ],
            filter_mode: FilterMode::from_u8(shared.filter_mode_u8()),
            cutoff: shared.filter_cutoff.load(),
        };
        self.strip.process(buf, params);
        let vbuf = &mut self.scratch[..out.len()];
        let scrubbing = phase != ScrubPhase::Idle;
        if scrubbing || self.scrub_mix > 0.0 {
            match phase {
                ScrubPhase::Active => {
                    self.voice
                        .render(shared.scrub.target(), &playback.samples, vbuf)
                }
                ScrubPhase::Settling if self.voice.settle_remaining_frames() == 0 => vbuf.fill(0.0),
                _ => {
                    self.voice.render_settle(&playback.samples, vbuf);
                }
            }
            shared.scrub.publish_voice_frame(self.voice.position());
            self.scrub_strip.process(vbuf, params);
        } else {
            vbuf.fill(0.0);
        }
        let alpha = 1.0 - (-1.0 / (0.005 * self.sample_rate as f32)).exp();
        let target_gain = if live || scrubbing || self.scrub_mix > 0.0 {
            shared.fader.load() * fader
        } else {
            0.0
        };
        let target_pre = shared.trim.load();
        let mix_target = if scrubbing { 1.0 } else { 0.0 };
        let mut peak = 0.0_f32;
        // Mix into the engine scratch first so transition ramps also cover
        // transport discontinuities before the channel's gain/meter tap.
        for (frame, voice) in buf.chunks_exact_mut(2).zip(vbuf.chunks_exact(2)) {
            self.scrub_mix += (mix_target - self.scrub_mix) * alpha;
            for (sample, voice_sample) in frame.iter_mut().zip(voice) {
                *sample = *sample * (1.0 - self.scrub_mix) + voice_sample * self.scrub_mix;
            }
        }
        self.transition.apply(buf, 2);
        if let Some(last) = buf.chunks_exact(2).last() {
            self.last_frame.copy_from_slice(last);
        }
        for (frame, samples) in out.chunks_exact_mut(2).zip(buf.chunks_exact(2)) {
            self.pre_gain += (target_pre - self.pre_gain) * alpha;
            self.gain += (target_gain - self.gain) * alpha;
            let left = samples[0] * self.pre_gain;
            let right = samples[1] * self.pre_gain;
            frame[0] += left * self.gain;
            frame[1] += right * self.gain;
            peak = peak.max(left.abs()).max(right.abs());
            if let Some(lufs) = &mut self.lufs {
                lufs.push_stereo(left, right);
            }
        }
        if scrubbing && self.scrub_mix > 0.999 {
            self.scrub_mix = 1.0;
        }
        if !scrubbing && self.scrub_mix < 1e-4 {
            self.scrub_mix = 0.0;
        }
        let release = 1.0 - (-((out.len() / 2) as f32) / (0.3 * self.sample_rate as f32)).exp();
        self.meter = if peak >= self.meter {
            peak
        } else {
            self.meter + (peak - self.meter) * release
        };
        shared.meter.store(self.meter);
        shared.meter_lufs.store(self.lufs.as_mut().map_or(
            MomentaryLoudness::SILENCE_LUFS,
            MomentaryLoudness::momentary_lufs,
        ));
    }
}

/// Shared by the real device callback and deterministic delivery tests.
pub struct MixerRenderer {
    decks: [DeckVoice; 3],
    mixer: Arc<MixerShared>,
    limiter: Limiter,
    master_meter: f32,
    sample_rate: u32,
}

impl MixerRenderer {
    pub fn new(decks: [DeckAudio; 3], mixer: Arc<MixerShared>, sample_rate: u32) -> Self {
        Self {
            decks: decks.map(|deck| DeckVoice::new(deck, sample_rate)),
            mixer,
            limiter: Limiter::new(sample_rate),
            master_meter: 0.0,
            sample_rate,
        }
    }

    pub fn render(&mut self, data: &mut [f32]) {
        let started = Instant::now();
        data.fill(0.0);
        let xf = self.mixer.crossfader.load().clamp(0.0, 1.0);
        let faders = [
            (xf * std::f32::consts::FRAC_PI_2).cos(),
            (xf * std::f32::consts::FRAC_PI_2).sin(),
            1.0,
        ];
        let whole = data.len() / 2 * 2;
        let mut peak = 0.0_f32;
        for block in data[..whole].chunks_mut(BLOCK_SAMPLES) {
            for (deck, fader) in self.decks.iter_mut().zip(faders) {
                deck.render_into(block, fader);
            }
            let master = self.mixer.master.load();
            for sample in block.iter_mut() {
                *sample *= master;
            }
            self.limiter.process(block);
            for &sample in block.iter() {
                peak = peak.max(sample.abs());
            }
        }
        let budget = (whole / 2) as f32 / self.sample_rate as f32;
        let release = 1.0 - (-budget / 0.3).exp();
        self.master_meter = if peak >= self.master_meter {
            peak
        } else {
            self.master_meter + (peak - self.master_meter) * release
        };
        self.mixer.master_meter.store(self.master_meter);
        if budget > 0.0 {
            let load = started.elapsed().as_secs_f32() / budget;
            let old = self.mixer.cpu_load.load();
            self.mixer.cpu_load.store(old + (load - old) * 0.1);
        }
    }
}

#[cfg(test)]
mod tests {
    use timestretch::{Epoch, OutputStamp};

    use super::*;
    use crate::output_queue::{self, Producer};

    #[test]
    fn rendered_audio_preserves_bass_stereo_and_pitch_intent() {
        use timestretch::{PreparedSession, SessionConfig, SourceAccess};

        use crate::deck::playback_parameters;
        use crate::render_worker::fill_reserve;

        const RATE: u32 = 48_000;
        let source: Arc<Vec<f32>> = Arc::new(
            (0..RATE * 2)
                .flat_map(|i| {
                    let phase = i as f64 * std::f64::consts::TAU / RATE as f64;
                    [
                        (phase * 61.0).sin() as f32 * 0.1,
                        (phase * 997.0).sin() as f32 * 0.1,
                    ]
                })
                .collect(),
        );
        for (tempo, keylock) in [
            (0.75, true),
            (1.5, false),
            (1.0 / 32.0, true),
            (4.0, true),
            (0.25, false),
        ] {
            let shared: [Arc<DeckShared>; 3] = std::array::from_fn(|_| Arc::new(DeckShared::new()));
            let handoffs: [Arc<DeliveryHandoff>; 3] =
                std::array::from_fn(|_| Arc::new(DeliveryHandoff::new()));
            let (mut producer, consumer) = output_queue::channel(8192);
            let mut session = PreparedSession::new(SessionConfig {
                source_sample_rate: RATE,
                output_sample_rate: RATE,
                source_frames: Some((source.len() / 2) as u64),
                source_access: SourceAccess::Seekable,
                parameters: playback_parameters(tempo, keylock).unwrap(),
                ..Default::default()
            })
            .unwrap()
            .activate();
            // Measure a steady section, with the session's seek history, rather
            // than the source-start transient stretched out at 1/32 speed.
            session.seek(u64::from(RATE / 4)).unwrap();
            producer.publish_epoch(session.epoch());
            shared[0].track_generation.store(1, Ordering::Release);
            shared[0].set_transport(Transport::Playing);
            shared[0].stream_active.store(true, Ordering::Release);
            handoffs[0].publish(Playback {
                consumer,
                samples: source.clone(),
                generation: 1,
            });
            let mixer = Arc::new(MixerShared::new());
            mixer.crossfader.store(0.0);
            mixer.master.store(1.0);
            let mut renderer = MixerRenderer::new(
                std::array::from_fn(|i| DeckAudio {
                    shared: shared[i].clone(),
                    handoff: handoffs[i].clone(),
                }),
                mixer,
                RATE,
            );
            let mut scratch = [0.0; 4096];
            let mut block = [0.0; 512];
            let mut rendered = Vec::new();
            // Pull less than half a second so the 4x case cannot reach EOF.
            for _ in 0..80 {
                for _ in 0..1000 {
                    fill_reserve(&mut session, &source, &mut producer, &mut scratch, 4096, 8)
                        .unwrap();
                    if producer.occupied_frames() >= block.len() / 2 {
                        break;
                    }
                }
                assert!(
                    producer.occupied_frames() >= block.len() / 2,
                    "no output at tempo={tempo}, keylock={keylock}"
                );
                renderer.render(&mut block);
                session.acknowledge(producer.receipt().unwrap()).unwrap();
                rendered.extend_from_slice(&block);
            }
            assert!(rendered.iter().all(|x| x.is_finite()));
            let steady = &rendered[8192..];
            let amplitude = |channel: usize, frequency: f64| {
                // A narrow frequency search tolerates the library's documented
                // quantization without mistaking the other stereo tone for it.
                (-6..=6)
                    .map(|step| {
                        let omega =
                            (frequency + step as f64 * 0.5) * std::f64::consts::TAU / RATE as f64;
                        let (re, im) = steady.chunks_exact(2).enumerate().fold(
                            (0.0, 0.0),
                            |(re, im), (i, frame)| {
                                let phase = omega * i as f64;
                                (
                                    re + frame[channel] as f64 * phase.cos(),
                                    im + frame[channel] as f64 * phase.sin(),
                                )
                            },
                        );
                        2.0 * re.hypot(im) / (steady.len() / 2) as f64
                    })
                    .fold(0.0_f64, f64::max)
            };
            let pitch = if keylock { 1.0 } else { tempo };
            let bass = amplitude(0, 61.0 * pitch);
            let treble = amplitude(1, 997.0 * pitch);
            assert!(
                bass > 0.04,
                "missing bass: tempo={tempo}, keylock={keylock}, amplitude={bass}"
            );
            assert!(
                treble > 0.04,
                "wrong pitch: tempo={tempo}, keylock={keylock}, amplitude={treble}"
            );
            assert!(
                amplitude(1, 61.0 * pitch) < bass * 0.1,
                "left tone leaked right"
            );
            assert!(
                amplitude(0, 997.0 * pitch) < treble * 0.1,
                "right tone leaked left"
            );
        }
    }

    type Fixture = (
        MixerRenderer,
        [Arc<DeckShared>; 3],
        [Arc<DeliveryHandoff>; 3],
        [Producer; 3],
    );

    fn fixture() -> Fixture {
        let shared = std::array::from_fn(|_| Arc::new(DeckShared::new()));
        let handoffs = std::array::from_fn(|_| Arc::new(DeliveryHandoff::new()));
        let producers = std::array::from_fn(|i| {
            let (producer, consumer) = output_queue::channel(32_768);
            shared[i].track_generation.store(1, Ordering::Release);
            shared[i].stream_active.store(true, Ordering::Release);
            shared[i].set_transport(Transport::Playing);
            handoffs[i].publish(Playback {
                consumer,
                samples: Arc::new(vec![0.1; 96_000]),
                generation: 1,
            });
            producer
        });
        let renderer = MixerRenderer::new(
            std::array::from_fn(|i| DeckAudio {
                shared: shared[i].clone(),
                handoff: handoffs[i].clone(),
            }),
            Arc::new(MixerShared::new()),
            48_000,
        );
        (renderer, shared, handoffs, producers)
    }

    #[test]
    fn callback_handles_large_buffers_partial_delivery_and_pause_without_allocation() {
        let (mut renderer, shared, _, mut producers) = fixture();
        let media = vec![0.1; 20_000];
        for producer in &mut producers {
            assert!(producer.push(
                &media,
                OutputStamp {
                    epoch: Epoch(1),
                    start_frame: 0,
                    frames: 10_000
                }
            ));
        }
        let mut out = vec![0.0; 40_001];
        let allocation = allocation_counter::measure(|| renderer.render(&mut out));
        assert_eq!(allocation.count_total, 0);
        assert_eq!(
            allocation.count_current, 0,
            "callback destroyed owned storage"
        );
        assert!(out.iter().all(|s| s.is_finite()));
        assert!(out.iter().any(|s| s.abs() > 0.01));
        assert_eq!(*out.last().unwrap(), 0.0);
        for producer in &producers {
            assert_eq!(producer.receipt().unwrap().end_frame, 10_000);
        }
        for state in &shared {
            state.set_transport(Transport::Paused);
        }
        for producer in &mut producers {
            assert!(producer.push(
                &media[..1024],
                OutputStamp {
                    epoch: Epoch(1),
                    start_frame: 10_000,
                    frames: 512
                }
            ));
        }
        renderer.render(&mut out);
        for producer in &producers {
            assert_eq!(producer.receipt().unwrap().end_frame, 10_000);
        }
    }

    #[test]
    fn paused_epoch_handoff_discards_stale_audio_without_advancing_position() {
        let (mut renderer, shared, _, mut producers) = fixture();
        renderer.render(&mut [0.0; 2]);
        for state in &shared {
            state.set_transport(Transport::Paused);
        }
        for producer in &mut producers {
            assert!(producer.push(
                &[0.7; 1024],
                OutputStamp {
                    epoch: Epoch(1),
                    start_frame: 0,
                    frames: 512
                }
            ));
            producer.publish_epoch(Epoch(2));
        }
        let allocation = allocation_counter::measure(|| renderer.render(&mut [0.0; 512]));
        assert_eq!(allocation.count_total, 0);
        assert_eq!(allocation.count_current, 0);
        for producer in &producers {
            let receipt = producer.receipt().unwrap();
            assert_eq!(receipt.epoch, Epoch(2));
            assert_eq!(receipt.end_frame, 0);
            assert_eq!(producer.occupied_frames(), 0);
        }
    }

    #[test]
    fn replacements_and_full_retirement_queue_never_drop_storage_in_callback() {
        let (mut renderer, shared, handoffs, _producers) = fixture();
        renderer.render(&mut [0.0; 512]);
        for generation in 2..8 {
            for i in 0..3 {
                let (_, consumer) = output_queue::channel(8192);
                shared[i]
                    .track_generation
                    .store(generation, Ordering::Release);
                handoffs[i].publish(Playback {
                    consumer,
                    samples: Arc::new(vec![0.0; 10_000]),
                    generation,
                });
            }
            // No UI reclamation until all retirement slots have filled.
            let allocation = allocation_counter::measure(|| renderer.render(&mut [0.0; 512]));
            assert_eq!(allocation.count_total, 0);
            assert_eq!(allocation.count_current, 0);
        }
        for handoff in &handoffs {
            handoff.reclaim();
        }
        let allocation = allocation_counter::measure(|| renderer.render(&mut [0.0; 512]));
        assert_eq!(allocation.count_total, 0);
        assert_eq!(allocation.count_current, 0);
        for voice in &renderer.decks {
            assert_eq!(voice.playback.as_ref().unwrap().generation, 7);
        }
    }

    #[test]
    fn audition_bypasses_crossfader_and_muted_decks_still_consume() {
        let (mut renderer, _, _, mut producers) = fixture();
        renderer.mixer.crossfader.store(0.0);
        for (i, producer) in producers.iter_mut().enumerate() {
            let value = if i == 2 { 0.1 } else { 0.0 };
            assert!(producer.push(
                &vec![value; 8192],
                OutputStamp {
                    epoch: Epoch(1),
                    start_frame: 0,
                    frames: 4096
                }
            ));
        }
        let mut out = vec![0.0; 8192];
        renderer.render(&mut out);
        assert!(out[4096..].iter().any(|s| s.abs() > 0.05));
        for producer in &producers {
            assert_eq!(producer.receipt().unwrap().end_frame, 4096);
        }
    }

    #[test]
    fn scrub_landing_waits_for_worker_and_hands_off_at_the_frame_boundary() {
        let (mut renderer, shared, _, mut producers) = fixture();
        for producer in &mut producers {
            assert!(producer.push(
                &vec![0.1; 16_000],
                OutputStamp {
                    epoch: Epoch(1),
                    start_frame: 0,
                    frames: 8000
                }
            ));
        }
        shared[0].scrub.begin(1000.0, 0.0);
        renderer.render(&mut [0.0; 4096]);
        shared[0].scrub.release(1.0);
        let allocation = allocation_counter::measure(|| {
            for _ in 0..100 {
                renderer.render(&mut [0.0; 4096]);
            }
        });
        assert_eq!(allocation.count_total, 0);
        assert_eq!(shared[0].scrub.phase(), ScrubPhase::Settling);
        let (sequence, landing) = shared[0].scrub.landing();
        assert!((renderer.decks[0].voice.position() - landing).abs() < 1e-6);
        let before = producers[0].receipt().unwrap().end_frame;
        shared[0].scrub_ready.store(sequence, Ordering::Release);
        renderer.render(&mut [0.0; 256]);
        assert_eq!(shared[0].scrub.phase(), ScrubPhase::Idle);
        // The first fade-back segment must advance BOTH voices, including
        // when the previous mix was pinned to 1.0.
        assert_eq!(producers[0].receipt().unwrap().end_frame - before, 128);
        assert!((renderer.decks[0].voice.position() - landing.floor() - 128.0).abs() < 0.1);
    }
}
