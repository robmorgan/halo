//! Device-side delivery and mixer. All storage is prepared before cpal calls
//! render; track handoffs return retired storage to the UI for destruction.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use crossbeam_queue::ArrayQueue;
use timestretch::{MomentaryLoudness, TransitionEnvelope};

use crate::dsp::{ChannelStrip, FilterMode, Limiter, StripParams};
use crate::output_queue::{Consumer, Cursor};
use crate::scrub::ScrubVoice;
use crate::state::{DeckShared, DeliveryClock, MixerShared, ScrubPhase, Transport};
use crate::sync::{BeatMap, Lock, SyncState, correction, crossing, phase_error};

const BLOCK_SAMPLES: usize = 4096;

fn requested_rate(shared: &DeckShared, rate: f64) -> f32 {
    let min = if shared.keylock.load(Ordering::Relaxed) {
        1.0 / 32.0
    } else {
        0.25
    };
    rate.clamp(min, 4.0) as f32
}

pub struct Playback {
    pub consumer: Consumer,
    pub samples: Arc<Vec<f32>>,
    pub generation: u64,
}

pub struct DeliveryHandoff {
    pending: ArrayQueue<Playback>,
    retired: ArrayQueue<Playback>,
    grids: ArrayQueue<BeatMap>,
    retired_grids: ArrayQueue<BeatMap>,
}

impl DeliveryHandoff {
    pub fn new() -> Self {
        Self {
            pending: ArrayQueue::new(1),
            retired: ArrayQueue::new(2),
            grids: ArrayQueue::new(1),
            retired_grids: ArrayQueue::new(2),
        }
    }

    /// UI only. Superseded, unadopted deliveries are destroyed here.
    pub fn publish(&self, playback: Playback) {
        drop(self.pending.force_push(playback));
    }

    pub fn publish_grid(&self, map: BeatMap) {
        drop(self.grids.force_push(map));
    }

    pub fn reclaim(&self) {
        while self.retired_grids.pop().is_some() {}
        while self.retired.pop().is_some() {}
    }
}

pub struct DeckAudio {
    pub shared: Arc<DeckShared>,
    pub handoff: Arc<DeliveryHandoff>,
}

struct DeckVoice {
    grid: Option<BeatMap>,
    retiring_grid: Option<BeatMap>,
    grid_revision: u64,
    lock: Lock,
    delivery_open: bool,
    advances: Box<[f64; BLOCK_SAMPLES / 2]>,
    prefix_start: Cursor,
    frame_clock: u64,
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
            grid: None,
            retiring_grid: None,
            grid_revision: 0,
            lock: Lock::default(),
            delivery_open: true,
            advances: Box::new([0.0; BLOCK_SAMPLES / 2]),
            prefix_start: Cursor::default(),
            frame_clock: 0,
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
        if let Some(old) = self.retiring_grid.take() {
            self.retiring_grid = self.input.handoff.retired_grids.push(old).err();
        }
        if self.retiring_grid.is_none()
            && let Some(next) = self.input.handoff.grids.pop()
        {
            self.retiring_grid = self.grid.replace(next);
            self.grid_revision += 1;
        }
        // Backpressure retains ownership locally; it must never destroy the
        // previous source/queue on this thread, even if the UI stops polling.
        if let Some(old) = self.retiring.take() {
            self.retiring = self.input.handoff.retired.push(old).err();
        }
        if self.retiring.is_none()
            && let Some(next) = self.input.handoff.pending.pop()
        {
            self.retiring = self.playback.replace(next);
            self.lock = Lock::default();
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
            self.lock.launched = false;
            self.publish_cursor();
            self.strip.reset();
            self.transition.begin(self.last_frame);
            if let Some(lufs) = &mut self.lufs {
                lufs.reset();
            }
        }
        if let Some(playback) = &mut self.playback
            && playback.consumer.complete()
        {
            self.publish_cursor();
        }
    }

    fn render_following(
        &mut self,
        out: &mut [f32],
        fader: f32,
        master: &DeckVoice,
        master_revision: u64,
        clock: u64,
    ) {
        // The clone cannot be the final owner: input retains the same Arc.
        let shared = self.input.shared.clone();
        let enabled = shared.sync_enabled.load(Ordering::Acquire);
        let version = [
            shared.track_generation.load(Ordering::Acquire),
            shared.transport_revision(),
            shared.anchor_revision.load(Ordering::Acquire),
            self.grid_revision,
            master_revision,
            master.grid_revision,
        ];
        let newly_enabled = enabled && !self.lock.enabled;
        let changed = version != self.lock.version;
        let mut force = newly_enabled && self.was_live;
        if changed {
            // Grid/master changes can invalidate a playing lock. A new anchor
            // or transport already closes delivery and is prepared by the worker.
            force |= self.was_live && version[..3] == self.lock.version[..3];
            if version[1] != self.lock.version[1] || version[0] != self.lock.version[0] {
                self.lock.reprime = true;
                self.lock.fading_out = 0;
                self.lock.fading_in = 0;
            }
            self.lock.launched = false;
            self.lock.drift_beats = 0.0;
            self.lock.version = version;
        }
        self.lock.enabled = enabled;
        if !enabled || !master.playing() {
            shared
                .matched_tempo
                .store(shared.manual_tempo.load() as f64);
            self.delivery_open = true;
            self.lock.launched = false;
            self.lock.fading_out = 0;
            self.lock.fading_in = 0;
            shared.tempo_rate.store(requested_rate(
                &shared,
                shared.manual_tempo.load() as f64 * shared.bend.load() as f64,
            ));
            self.status(if enabled {
                SyncState::Preparing
            } else {
                SyncState::Off
            });
            self.render_into(out, fader);
            return;
        }
        let cursor = self
            .playback
            .as_ref()
            .map_or(Cursor::default(), |p| p.consumer.cursor);
        let mapping = self.map().and_then(|grid| {
            let master_grid = master.map()?;
            if !grid.supports_loop(cursor.region)
                || !master_grid.supports_loop(master.prefix_start.region)
            {
                return None;
            }
            let (beat, length) = grid.at(cursor.position)?;
            let (master_beat, master_length) = master_grid.at(master.prefix_start.position)?;
            let rate = master.input.shared.tempo_rate.load() as f64 * length / master_length;
            let min = if shared.keylock.load(Ordering::Relaxed) {
                1.0 / 32.0
            } else {
                0.25
            };
            (rate >= min && rate <= 4.0).then_some((
                beat.rem_euclid(1.0),
                length,
                master_beat.rem_euclid(1.0),
                master_length,
                rate,
            ))
        });
        let Some((phase, length, master_phase, master_length, rate)) = mapping else {
            shared
                .matched_tempo
                .store(shared.manual_tempo.load() as f64);
            self.delivery_open = true;
            self.lock.launched = false;
            self.lock.fading_out = 0;
            self.lock.fading_in = 0;
            shared.tempo_rate.store(requested_rate(
                &shared,
                shared.manual_tempo.load() as f64 * shared.bend.load() as f64,
            ));
            self.status(SyncState::Unavailable);
            self.render_into(out, fader);
            return;
        };
        shared.matched_tempo.store(rate);
        let nudge = (shared.bend.load() - 1.0).abs() > f32::EPSILON
            || master.input.shared.scrub.phase() != ScrubPhase::Idle;
        let error = phase_error(master_phase, phase);
        let was_nudging = self.lock.nudging;
        self.lock.nudging = nudge;
        let correction = if self.lock.launched && !nudge && self.lock.fading_out == 0 {
            correction(error)
        } else {
            0.0
        };
        shared.tempo_rate.store(requested_rate(
            &shared,
            rate * (1.0 + correction) * shared.bend.load() as f64,
        ));
        if !self.playing() || shared.scrub.phase() != ScrubPhase::Idle {
            self.lock.launched = false;
            self.lock.fading_out = 0;
            self.lock.fading_in = 0;
            self.lock.drift_beats = 0.0;
            self.delivery_open = false;
            self.status(SyncState::Preparing);
            self.render_into(out, fader);
            return;
        }
        if newly_enabled && !self.was_live {
            self.lock.reprime = true;
        }
        if (self.lock.launched || was_nudging) && !nudge {
            force |= error.abs() > 0.125;
            if error.abs() <= 1.0 / 64.0 {
                self.lock.drift_beats = 0.0;
            } else if !shared.controls_pending.load(Ordering::Acquire)
                && (shared.delivered_tempo.load() - shared.tempo_rate.load() as f64).abs()
                    < rate * 0.001
            {
                self.lock.drift_beats += master.advances[..out.len() / 2].iter().sum::<f64>()
                    * master.prefix_start.conversion
                    / master_length;
                force |= self.lock.drift_beats >= 2.0;
            }
        }
        if force && !nudge && self.lock.fading_out == 0 {
            shared.tempo_rate.store(requested_rate(&shared, rate));
            self.lock.fading_out = (self.sample_rate as usize).div_ceil(200);
            self.lock.fading_in = 0;
            self.lock.reprime = false;
        }
        if self.lock.fading_out > 0 {
            let count = self.lock.fading_out.min(out.len() / 2);
            self.delivery_open = true;
            self.status(SyncState::Preparing);
            self.render_into(&mut out[..count * 2], fader);
            if self.lock.fading_out == 0 {
                let mut master_cursor = master.prefix_start;
                for &advance in &master.advances[..count] {
                    master_cursor.advance(advance);
                }
                let target = master
                    .map()
                    .and_then(|grid| grid.at(master_cursor.position))
                    .and_then(|(beat, _)| {
                        self.map()?.aligned_target(
                            self.playback.as_ref()?.consumer.cursor,
                            beat.rem_euclid(1.0),
                        )
                    });
                if let Some(target) = target {
                    shared.request_anchor(target);
                }
                self.lock.launched = false;
                self.lock.drift_beats = 0.0;
                self.delivery_open = false;
                self.render_into(&mut out[count * 2..], fader);
            }
            return;
        }
        if self.lock.reprime
            && self.ready()
            && self
                .playback
                .as_ref()
                .and_then(|p| p.consumer.next_advance())
                .is_some_and(|advance| (advance - rate).abs() < rate * 1e-6)
        {
            self.lock.reprime = false;
        }
        if self.lock.reprime {
            // A Play edge prepares exactly the parked coordinate with the
            // matched tempo. Do not overwrite an outstanding external seek.
            if self
                .playback
                .as_ref()
                .is_some_and(|p| p.consumer.anchor_revision == version[2])
            {
                shared.request_anchor(cursor.position);
                self.lock.reprime = false;
            }
        }
        if !self.ready() || self.lock.reprime {
            self.delivery_open = false;
            self.status(SyncState::Preparing);
            self.render_into(out, fader);
            return;
        }
        if self.lock.launched {
            self.delivery_open = true;
            self.status(if nudge {
                SyncState::Nudging
            } else {
                SyncState::Locked
            });
            self.render_into(out, fader);
            return;
        }
        let distance = (phase - master_phase).rem_euclid(1.0);
        shared.sync_remaining.store(distance * length);
        self.status(SyncState::Waiting);
        let launch = if master.ready()
            && master.input.shared.scrub.phase() == ScrubPhase::Idle
            && master.advances[..out.len() / 2]
                .iter()
                .any(|&advance| advance > 0.0)
        {
            master.map().and_then(|grid| {
                crossing(
                    grid,
                    master.prefix_start,
                    &master.advances[..out.len() / 2],
                    phase,
                )
            })
        } else {
            None
        };
        if launch.is_none() {
            let mut end = master.prefix_start;
            for &advance in &master.advances[..out.len() / 2] {
                end.advance(advance);
            }
            if let Some((beat, _)) = master.map().and_then(|map| map.at(end.position)) {
                shared
                    .sync_remaining
                    .store((phase - beat.rem_euclid(1.0)).rem_euclid(1.0) * length);
            }
        }
        self.delivery_open = false;
        let offset = launch.unwrap_or(out.len() / 2);
        self.render_into(&mut out[..offset * 2], fader);
        if let Some(offset) = launch {
            // Recheck cancellation after scanning the bounded prefix. The next
            // render_segment also checks readiness and the anchor generation.
            if shared.transport_revision() != version[1]
                || shared.anchor_revision.load(Ordering::Acquire) != version[2]
            {
                return;
            }
            self.lock.launched = true;
            self.lock.fading_in = (self.sample_rate as usize).div_ceil(200);
            self.delivery_open = true;
            shared
                .launch_frame
                .store(clock + offset as u64, Ordering::Relaxed);
            self.status(if nudge {
                SyncState::Nudging
            } else {
                SyncState::Locked
            });
            self.render_into(&mut out[offset * 2..], fader);
        }
    }

    fn publish_cursor(&self) {
        if let Some(playback) = &self.playback
            && playback.generation == self.input.shared.track_generation.load(Ordering::Acquire)
            && playback.consumer.anchor_revision
                == self.input.shared.anchor_revision.load(Ordering::Acquire)
        {
            let cursor = playback.consumer.cursor;
            self.input.shared.delivery_position.store(cursor.position);
            self.input
                .shared
                .playhead
                .store(cursor.position.max(0.0) as u64, Ordering::Relaxed);
            self.input
                .shared
                .loop_iteration
                .store(cursor.iterations, Ordering::Relaxed);
        }
    }

    fn map(&self) -> Option<&BeatMap> {
        self.grid
            .as_ref()
            .filter(|grid| grid.track == self.input.shared.track_generation.load(Ordering::Acquire))
    }

    fn ready(&self) -> bool {
        let shared = &self.input.shared;
        shared.stream_active.load(Ordering::Acquire)
            && shared.prepared_revision.load(Ordering::Acquire) == shared.transport_revision()
            && self.playback.as_ref().is_some_and(|p| {
                p.generation == shared.track_generation.load(Ordering::Acquire)
                    && p.consumer.anchor_revision == shared.anchor_revision.load(Ordering::Acquire)
            })
    }

    fn playing(&self) -> bool {
        self.input.shared.transport() == Transport::Playing
            && !self.input.shared.tempo_stopped.load(Ordering::Relaxed)
            && self.playback.as_ref().is_some_and(|p| {
                p.generation == self.input.shared.track_generation.load(Ordering::Acquire)
            })
    }

    fn status(&self, state: SyncState) {
        self.input
            .shared
            .sync_state
            .store(state as u8, Ordering::Relaxed);
        if state != SyncState::Waiting {
            self.input.shared.sync_remaining.store(0.0);
        }
    }

    fn render_into(&mut self, out: &mut [f32], fader: f32) {
        self.advances.fill(0.0);
        self.prefix_start = self
            .playback
            .as_ref()
            .map_or(Cursor::default(), |p| p.consumer.cursor);
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
                    let rate = if self.delivery_open
                        && self.input.shared.transport() == Transport::Playing
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
            self.render_segment(&mut out[offset..offset + count], fader, phase, offset / 2);
            offset += count;
        }
    }

    fn render_segment(&mut self, out: &mut [f32], fader: f32, phase: ScrubPhase, offset: usize) {
        let ready = self.ready();
        let Some(playback) = &mut self.playback else {
            return;
        };
        let shared = &self.input.shared;
        let live = self.delivery_open
            && ready
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
            let tags = &mut self.advances[offset..offset + out.len() / 2];
            let frames = playback.consumer.read_timed(buf, tags);
            if let Some(&last) = tags.get(frames.wrapping_sub(1)) {
                shared.delivered_tempo.store(last);
            }
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
        let fade_frames = (self.sample_rate as usize).div_ceil(200);
        for frame in buf.chunks_exact_mut(2) {
            let envelope = if self.lock.fading_out > 0 {
                self.lock.fading_out -= 1;
                self.lock.fading_out as f32 / fade_frames as f32
            } else if self.lock.fading_in > 0 {
                self.lock.fading_in -= 1;
                1.0 - self.lock.fading_in as f32 / fade_frames as f32
            } else if !self.delivery_open && phase == ScrubPhase::Idle && self.scrub_mix == 0.0 {
                0.0
            } else {
                1.0
            };
            for sample in frame {
                *sample *= envelope;
            }
        }
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
        if scrubbing {
            shared.playhead.store(
                shared.scrub.voice_frame().max(0.0) as u64,
                Ordering::Relaxed,
            );
        } else {
            self.publish_cursor();
        }
        self.frame_clock += (out.len() / 2) as u64;
        if let Some(playback) = &self.playback {
            shared.delivery_clock.publish(DeliveryClock {
                output_frame: self.frame_clock,
                position: if scrubbing {
                    shared.scrub.voice_frame().max(0.0)
                } else {
                    playback.consumer.cursor.position
                },
                iteration: playback.consumer.cursor.iterations,
                track: playback.generation,
                anchor: playback.consumer.anchor_revision,
            });
        }
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
    master: usize,
    master_request: u64,
    master_revision: u64,
    output_frame: u64,
}

impl MixerRenderer {
    pub fn new(decks: [DeckAudio; 3], mixer: Arc<MixerShared>, sample_rate: u32) -> Self {
        Self {
            decks: decks.map(|deck| DeckVoice::new(deck, sample_rate)),
            mixer,
            limiter: Limiter::new(sample_rate),
            master_meter: 0.0,
            sample_rate,
            master: 0,
            master_request: 0,
            master_revision: 0,
            output_frame: 0,
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
            for deck in &mut self.decks {
                deck.frame_clock = self.output_frame;
                deck.adopt();
            }
            let request = self.mixer.master_request.load(Ordering::Acquire);
            let previous = self.master;
            if request != self.master_request {
                self.master_request = request;
                self.master = (request & 1) as usize;
            }
            if !self.decks[self.master].playing() && self.decks[1 - self.master].playing() {
                self.master = 1 - self.master;
            }
            if previous != self.master {
                self.master_revision += 1;
                let shared = &self.decks[self.master].input.shared;
                if shared.sync_enabled.load(Ordering::Relaxed) {
                    shared
                        .manual_tempo
                        .store(shared.matched_tempo.load() as f32);
                }
            }
            self.mixer
                .effective_master
                .store(self.master as u8, Ordering::Release);
            let (a, rest) = self.decks.split_at_mut(1);
            let (b, audition) = rest.split_at_mut(1);
            let (master, follower) = if self.master == 0 {
                (&mut a[0], &mut b[0])
            } else {
                (&mut b[0], &mut a[0])
            };
            master.delivery_open = true;
            master.lock = Lock::default();
            let shared = &master.input.shared;
            shared.tempo_rate.store(requested_rate(
                shared,
                shared.manual_tempo.load() as f64 * shared.bend.load() as f64,
            ));
            shared
                .matched_tempo
                .store(shared.manual_tempo.load() as f64);
            master.status(if !shared.sync_enabled.load(Ordering::Relaxed) {
                SyncState::Off
            } else if master
                .map()
                .zip(master.playback.as_ref())
                .is_none_or(|(map, playback)| {
                    !map.supports_loop(playback.consumer.cursor.region)
                        || map.at(playback.consumer.cursor.position).is_none()
                })
            {
                SyncState::Unavailable
            } else if master.playing() {
                SyncState::Locked
            } else {
                SyncState::Preparing
            });
            master.render_into(block, faders[self.master]);
            follower.render_following(
                block,
                faders[1 - self.master],
                master,
                self.master_revision,
                self.output_frame,
            );
            audition[0].render_into(block, faders[2]);
            self.output_frame += (block.len() / 2) as u64;
            self.mixer
                .output_frame
                .store(self.output_frame, Ordering::Release);
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
    fn first_synced_deck_starts_as_master_at_its_existing_manual_tempo() {
        let (mut renderer, shared, _, mut producers) = fixture();
        shared[0].set_transport(Transport::Paused);
        shared[1].set_transport(Transport::Paused);
        shared[1].sync_enabled.store(true, Ordering::Release);
        shared[1].manual_tempo.store(1.23);
        prime_fake(&mut producers[1], &shared[1], 2, 1000.0, 1.23, 8192);
        renderer.render(&mut [0.0; 128]);
        assert!((shared[1].matched_tempo.load() - 1.23).abs() < 1e-6);
        assert_eq!(shared[1].take_seek(), None);
        shared[1].set_transport(Transport::Playing);
        shared[1]
            .prepared_revision
            .store(shared[1].transport_revision(), Ordering::Release);
        renderer.render(&mut [0.0; 128]);
        assert_eq!(renderer.master, 1);
        assert!((shared[1].tempo_rate.load() - 1.23).abs() < 1e-6);
        assert_eq!(producers[1].receipt().unwrap().end_frame, 64);
    }

    #[test]
    fn ready_and_aligned_play_starts_immediately_without_repreparation() {
        let (mut renderer, shared, handoffs, mut producers) = fixture();
        for handoff in &handoffs[..2] {
            grid(handoff, 0.0, 1000.0);
        }
        shared[1].set_transport(Transport::Paused);
        shared[1].sync_enabled.store(true, Ordering::Release);
        prime_fake(&mut producers[0], &shared[0], 2, 0.0, 1.0, 8192);
        prime_fake(&mut producers[1], &shared[1], 2, 2064.0, 1.0, 8192);
        renderer.render(&mut [0.0; 128]);
        shared[1].set_transport(Transport::Playing);
        shared[1]
            .prepared_revision
            .store(shared[1].transport_revision(), Ordering::Release);
        renderer.render(&mut [0.0; 128]);
        assert_eq!(shared[1].launch_frame.load(Ordering::Relaxed), 64);
        assert_eq!(shared[1].take_seek(), None);
        assert_eq!(producers[1].receipt().unwrap().end_frame, 64);
    }

    #[test]
    fn forced_relock_fades_then_prepares_and_reopens_only_at_matching_phase() {
        let (mut renderer, shared, handoffs, mut producers) = fixture();
        for handoff in &handoffs[..2] {
            grid(handoff, 0.0, 1000.0);
        }
        shared[1].set_loop(Some((2000, 4000)));
        prime_fake(&mut producers[0], &shared[0], 2, 0.0, 1.0, 30_000);
        prime_fake(&mut producers[1], &shared[1], 2, 2300.0, 1.0, 8192);
        renderer.render(&mut [0.0; 256]);
        shared[1].sync_enabled.store(true, Ordering::Release);
        let before = producers[1].receipt().unwrap().end_frame;
        renderer.render(&mut [0.0; 256]);
        assert_eq!(
            shared[1].take_seek(),
            None,
            "seek started before fade completed"
        );
        renderer.render(&mut [0.0; 256]);
        assert_eq!(
            producers[1].receipt().unwrap().end_frame - before,
            240,
            "fade must consume exactly 5 ms"
        );
        let target = shared[1].take_seek().expect("forced seek after fade");
        assert!((2000.0..4000.0).contains(&target));
        let parked = shared[1].delivery_position.load();
        for _ in 0..9 {
            renderer.render(&mut [0.0; 256]);
        }
        assert_eq!(shared[1].delivery_position.load(), parked);
        assert_eq!(SyncState::load(&shared[1]), SyncState::Preparing);
        prime_fake(&mut producers[1], &shared[1], 3, target, 1.0, 8192);
        for _ in 0..20 {
            renderer.render(&mut [0.0; 256]);
            if SyncState::load(&shared[1]) == SyncState::Locked {
                break;
            }
        }
        let launch = shared[1].launch_frame.load(Ordering::Relaxed);
        assert_ne!(launch, u64::MAX);
        assert!(phase_error(launch as f64 / 1000.0, target / 1000.0).abs() <= 0.001 + 1e-9);
        assert_eq!(shared[1].loop_region(), Some((2000, 4000)));
    }

    #[test]
    fn nudges_suspend_correction_and_release_relocks_large_errors() {
        let (mut renderer, shared, handoffs, mut producers) = fixture();
        for handoff in &handoffs[..2] {
            grid(handoff, 0.0, 1000.0);
        }
        shared[1].sync_enabled.store(true, Ordering::Release);
        shared[1].set_transport(Transport::Paused);
        prime_fake(&mut producers[0], &shared[0], 2, 0.0, 1.0, 30_000);
        prime_fake(&mut producers[1], &shared[1], 2, 2000.0, 1.0, 8192);
        renderer.render(&mut [0.0; 256]);
        shared[1].set_transport(Transport::Playing);
        renderer.render(&mut [0.0; 256]);
        let target = shared[1].take_seek().unwrap();
        prime_fake(&mut producers[1], &shared[1], 3, target, 1.0, 8192);
        for _ in 0..10 {
            renderer.render(&mut [0.0; 256]);
        }
        assert_eq!(SyncState::load(&shared[1]), SyncState::Locked);
        shared[1].bend.store(1.04);
        // Model drift accumulated by a held nudge without running a real DSP.
        renderer.decks[1]
            .playback
            .as_mut()
            .unwrap()
            .consumer
            .cursor
            .position += 200.0;
        for _ in 0..5 {
            renderer.render(&mut [0.0; 256]);
        }
        assert_eq!(SyncState::load(&shared[1]), SyncState::Nudging);
        assert_eq!(shared[1].take_seek(), None);
        assert!((shared[1].tempo_rate.load() - 1.04).abs() < 1e-6);
        shared[1].bend.store(1.0);
        renderer.render(&mut [0.0; 512]);
        assert!((shared[1].tempo_rate.load() - 1.0).abs() < 1e-6);
        assert!(shared[1].take_seek().is_some());
        assert_eq!(SyncState::load(&shared[1]), SyncState::Preparing);
    }

    #[test]
    fn persistent_small_drift_waits_for_controls_then_forces_after_two_beats() {
        let (mut renderer, shared, handoffs, mut producers) = fixture();
        for handoff in &handoffs[..2] {
            grid(handoff, 0.0, 1000.0);
        }
        shared[1].sync_enabled.store(true, Ordering::Release);
        shared[1].set_transport(Transport::Paused);
        prime_fake(&mut producers[0], &shared[0], 2, 0.0, 1.0, 30_000);
        prime_fake(&mut producers[1], &shared[1], 2, 2000.0, 1.0, 16_000);
        renderer.render(&mut [0.0; 256]);
        shared[1].set_transport(Transport::Playing);
        renderer.render(&mut [0.0; 256]);
        let target = shared[1].take_seek().unwrap();
        prime_fake(&mut producers[1], &shared[1], 3, target, 1.0, 16_000);
        for _ in 0..10 {
            renderer.render(&mut [0.0; 256]);
        }
        assert_eq!(SyncState::load(&shared[1]), SyncState::Locked);
        renderer.decks[1]
            .playback
            .as_mut()
            .unwrap()
            .consumer
            .cursor
            .position += 18.0;
        shared[1].controls_pending.store(true, Ordering::Release);
        for _ in 0..20 {
            renderer.render(&mut [0.0; 256]);
        }
        assert_eq!(
            shared[1].take_seek(),
            None,
            "pending controls must not start drift timeout"
        );
        shared[1].controls_pending.store(false, Ordering::Release);
        for _ in 0..14 {
            renderer.render(&mut [0.0; 256]);
        }
        assert_eq!(shared[1].take_seek(), None);
        for _ in 0..4 {
            renderer.render(&mut [0.0; 256]);
        }
        assert!(
            shared[1].take_seek().is_some(),
            "persistent error > 1/64 beat must force re-lock"
        );
    }

    fn prime_fake(
        producer: &mut Producer,
        shared: &DeckShared,
        epoch: u64,
        target: f64,
        rate: f64,
        frames: usize,
    ) {
        producer.publish_timing(
            Epoch(epoch),
            Cursor {
                position: target,
                region: shared.loop_region(),
                total: 1_000_000.0,
                ..Cursor::default()
            },
            shared.anchor_revision.load(Ordering::Acquire),
        );
        producer.initialize_advance(timestretch::AppliedParameters {
            requested: timestretch::Parameters::default(),
            achieved_stretch: 1.0 / rate,
            achieved_pitch: 1.0,
            command_id: 0,
            requested_at: 0,
            applied_at: 0,
        });
        assert!(producer.push(
            &vec![0.1; frames * 2],
            OutputStamp {
                epoch: Epoch(epoch),
                start_frame: 0,
                frames
            }
        ));
        shared
            .prepared_revision
            .store(shared.transport_revision(), Ordering::Release);
        shared.stream_active.store(true, Ordering::Release);
    }

    fn grid(handoff: &DeliveryHandoff, offset: f64, length: f64) {
        handoff.publish_grid(BeatMap::new(
            (0..2000).map(|i| offset + i as f64 * length),
            1,
        ));
    }

    #[test]
    fn scheduled_launches_preserve_fractional_parking_through_worker_and_ui_stalls() {
        let mut worst_error = 0.0_f64;
        let mut internal = 0;
        for rate in [0.5, 1.0, 1.25] {
            for master_phase in [0.0, 0.19, 0.5, 0.91] {
                for follower_phase in [0.0, 0.07, 0.5, 0.79, 0.999] {
                    let (mut renderer, shared, handoffs, mut producers) = fixture();
                    let parked = 997.0 * (2.0 + follower_phase);
                    shared[0].manual_tempo.store(rate as f32);
                    shared[1].set_transport(Transport::Paused);
                    shared[1].sync_enabled.store(true, Ordering::Release);
                    for handoff in &handoffs[..2] {
                        grid(handoff, 0.0, 997.0);
                    }
                    prime_fake(
                        &mut producers[0],
                        &shared[0],
                        2,
                        997.0 * master_phase,
                        rate,
                        30_000,
                    );
                    prime_fake(&mut producers[1], &shared[1], 2, parked, rate, 1024);
                    renderer.render(&mut [0.0; 254]);
                    assert_eq!(
                        shared[1].delivery_position.load(),
                        parked,
                        "arming moved a paused track"
                    );
                    assert_eq!(shared[1].take_seek(), None);
                    shared[1].set_transport(Transport::Playing);
                    renderer.render(&mut [0.0; 254]);
                    let target = shared[1].take_seek().expect("Play requests preparation");
                    assert_eq!(target, parked);
                    for _ in 0..11 {
                        renderer.render(&mut [0.0; 254]);
                    }
                    assert_eq!(shared[1].delivery_position.load(), parked);
                    assert_eq!(SyncState::load(&shared[1]), SyncState::Preparing);
                    assert_eq!(shared[1].launch_frame.load(Ordering::Relaxed), u64::MAX);
                    prime_fake(&mut producers[1], &shared[1], 3, target, rate, 8192);
                    for _ in 0..40 {
                        let allocation =
                            allocation_counter::measure(|| renderer.render(&mut [0.0; 254]));
                        assert_eq!(allocation.count_total, 0);
                        assert_eq!(allocation.count_current, 0);
                        let launch = shared[1].launch_frame.load(Ordering::Relaxed);
                        if launch != u64::MAX {
                            let master_at_launch = master_phase + launch as f64 * rate / 997.0;
                            let error =
                                phase_error(master_at_launch, follower_phase).abs() * 997.0 / rate;
                            worst_error = worst_error.max(error);
                            assert!(error <= 1.0 + 1e-9, "launch error {error} frames");
                            internal += usize::from(launch % 127 != 0);
                            let snapshot = shared[1].delivery_clock.read().unwrap();
                            assert_eq!(snapshot.output_frame, renderer.output_frame);
                            assert_eq!(snapshot.position, shared[1].delivery_position.load());
                            assert_eq!(snapshot.track, 1);
                            let consumed = producers[1].receipt().unwrap().end_frame;
                            assert_eq!(consumed, renderer.output_frame - launch);
                            assert!(
                                (shared[1].delivery_position.load()
                                    - parked
                                    - consumed as f64 * rate)
                                    .abs()
                                    < 1e-8
                            );
                            break;
                        }
                        assert_eq!(
                            shared[1].delivery_position.load(),
                            parked,
                            "waiting moved the playhead"
                        );
                        assert_eq!(SyncState::load(&shared[1]), SyncState::Waiting);
                    }
                    assert_ne!(shared[1].launch_frame.load(Ordering::Relaxed), u64::MAX);
                }
            }
        }
        assert!(internal > 40);
        println!(
            "60 scheduled phase/rate cases: maximum launch error {worst_error:.9} output frames; {internal} callback-internal starts"
        );
    }

    #[test]
    fn cancel_and_grid_replacement_reject_stale_launch_predictions() {
        let (mut renderer, shared, handoffs, mut producers) = fixture();
        for handoff in &handoffs[..2] {
            grid(handoff, 0.0, 1000.0);
        }
        shared[1].set_transport(Transport::Paused);
        shared[1].sync_enabled.store(true, Ordering::Release);
        prime_fake(&mut producers[0], &shared[0], 2, 0.0, 1.0, 20_000);
        prime_fake(&mut producers[1], &shared[1], 2, 2750.0, 1.0, 2048);
        renderer.render(&mut [0.0; 128]);
        shared[1].set_transport(Transport::Playing);
        renderer.render(&mut [0.0; 128]);
        let stale_transport = shared[1].transport_revision();
        let target = shared[1].take_seek().unwrap();
        shared[1].set_transport(Transport::Paused);
        prime_fake(&mut producers[1], &shared[1], 3, target, 1.0, 8192);
        shared[1]
            .prepared_revision
            .store(stale_transport, Ordering::Release);
        for _ in 0..20 {
            renderer.render(&mut [0.0; 128]);
        }
        assert_eq!(shared[1].launch_frame.load(Ordering::Relaxed), u64::MAX);
        assert_eq!(shared[1].delivery_position.load(), 2750.0);
        shared[1].set_transport(Transport::Playing);
        renderer.render(&mut [0.0; 128]);
        let target = shared[1].take_seek().unwrap();
        prime_fake(&mut producers[1], &shared[1], 4, target, 1.0, 8192);
        // Replacement changes the parked phase from .75 to .35 before readiness.
        grid(&handoffs[1], 400.0, 1000.0);
        for _ in 0..20 {
            renderer.render(&mut [0.0; 128]);
            if shared[1].launch_frame.load(Ordering::Relaxed) != u64::MAX {
                break;
            }
        }
        let launch = shared[1].launch_frame.load(Ordering::Relaxed);
        assert_ne!(launch, u64::MAX);
        assert!(phase_error(launch as f64 / 1000.0, 0.35).abs() <= 0.001 + 1e-9);
    }

    #[test]
    fn master_handoff_ignores_faders_and_underruns_and_preserves_sync_intent() {
        let (mut renderer, shared, _, _producers) = fixture();
        for deck in &shared[..2] {
            deck.sync_enabled.store(true, Ordering::Release);
        }
        shared[0].fader.store(0.0);
        renderer.render(&mut [0.0; 128]);
        assert_eq!(renderer.master, 0);
        shared[0].stream_active.store(false, Ordering::Release);
        renderer.render(&mut [0.0; 128]);
        assert_eq!(renderer.master, 0, "underrun changed master");
        shared[0].set_transport(Transport::Paused);
        renderer.render(&mut [0.0; 128]);
        assert_eq!(renderer.master, 1);
        shared[0].set_transport(Transport::Playing);
        renderer.render(&mut [0.0; 128]);
        assert_eq!(renderer.master, 1, "both playing must retain master");
        renderer.mixer.master_request.store(2, Ordering::Release);
        renderer.render(&mut [0.0; 128]);
        assert_eq!(renderer.master, 0);
        shared[0].track_generation.fetch_add(1, Ordering::AcqRel);
        renderer.render(&mut [0.0; 128]);
        assert_eq!(renderer.master, 1, "unloaded master did not transfer");
        assert!(
            shared[..2]
                .iter()
                .all(|s| s.sync_enabled.load(Ordering::Acquire))
        );
    }

    #[test]
    fn unavailable_sync_allows_plain_playback_and_keeps_intent() {
        let (mut renderer, shared, handoffs, mut producers) = fixture();
        shared[1].sync_enabled.store(true, Ordering::Release);
        prime_fake(&mut producers[0], &shared[0], 2, 0.0, 1.0, 8192);
        prime_fake(&mut producers[1], &shared[1], 2, 2000.0, 1.0, 8192);
        renderer.render(&mut [0.0; 128]);
        assert_eq!(SyncState::load(&shared[1]), SyncState::Unavailable);
        assert_eq!(producers[1].receipt().unwrap().end_frame, 64);
        grid(&handoffs[0], 0.0, 100.0);
        grid(&handoffs[1], 0.0, 1000.0);
        renderer.render(&mut [0.0; 128]);
        assert_eq!(
            SyncState::load(&shared[1]),
            SyncState::Unavailable,
            "unsupported tempo"
        );
        grid(&handoffs[0], 0.0, 1000.0);
        shared[1].set_loop(Some((2000, 2250)));
        prime_fake(&mut producers[1], &shared[1], 3, 2000.0, 1.0, 8192);
        renderer.render(&mut [0.0; 128]);
        assert_eq!(
            SyncState::load(&shared[1]),
            SyncState::Unavailable,
            "sub-beat loop cannot align continuously"
        );
        assert!(shared[1].sync_enabled.load(Ordering::Acquire));
        assert_eq!(shared[1].take_seek(), None);
    }

    #[test]
    fn grid_retirement_backpressure_never_destroys_callback_storage() {
        let (mut renderer, _, handoffs, _) = fixture();
        for offset in 0..10 {
            grid(&handoffs[0], offset as f64, 1000.0);
            let allocation = allocation_counter::measure(|| renderer.render(&mut [0.0; 128]));
            assert_eq!(allocation.count_total, 0);
            assert_eq!(allocation.count_current, 0);
        }
        handoffs[0].reclaim();
        let allocation = allocation_counter::measure(|| renderer.render(&mut [0.0; 128]));
        assert_eq!(allocation.count_total, 0);
        assert_eq!(allocation.count_current, 0);
        assert_eq!(
            renderer.decks[0].map().unwrap().at(9.0),
            Some((0.0, 1000.0))
        );
    }

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
            shared[0]
                .prepared_revision
                .store(shared[0].transport_revision(), Ordering::Release);
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
            shared[i]
                .prepared_revision
                .store(shared[i].transport_revision(), Ordering::Release);
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
