//! Device-side delivery and mixer. All storage is prepared before cpal calls
//! render; track handoffs return retired storage to the UI for destruction.

use std::slice::SliceIndex;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use crossbeam_queue::ArrayQueue;
use timestretch::{MomentaryLoudness, TransitionEnvelope};

use crate::dsp::{ChannelStrip, FilterMode, Limiter, StripParams};
use crate::output_queue::{Consumer, Cursor};
use crate::routing::{Layout, Mixing, Pair};
use crate::scrub::ScrubVoice;
use crate::state::{
    DEFAULT_MASTER_BPM, DeckShared, DeliveryClock, MixerShared, ScrubPhase, Transport,
};
use crate::sync::{
    BeatMap, ClockBlock, DRIFT_TOLERANCE_BEATS, Lock, PhaseLock, SyncState, clock_crossing,
    correction, phase_error,
};

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

/// One block's stereo buses: the master and the phones' cue, or for a
/// hardware mixer deck A, deck B and the audition player.
struct Bus<'a> {
    main: &'a mut [f32],
    cue: &'a mut [f32],
    aux: &'a mut [f32],
}

impl Bus<'_> {
    fn len(&self) -> usize {
        self.main.len()
    }

    fn range<R>(&mut self, range: R) -> Bus<'_>
    where
        R: SliceIndex<[f32], Output = [f32]> + Clone,
    {
        Bus {
            main: &mut self.main[range.clone()],
            cue: &mut self.cue[range.clone()],
            aux: &mut self.aux[range],
        }
    }
}

/// Where a voice's channel goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// The master only: the stream has no phones.
    Main,
    /// The master, and the phones pre-fader while the deck is cued.
    MainCued,
    /// The phones only, after the channel's volume: the audition player,
    /// so previews never reach the room.
    Phones,
    /// Straight to one bus for a hardware mixer: trim only, with no EQ,
    /// filter or fader, which the mixer has.
    Direct(Slot),
    /// The audition player on one bus for a hardware mixer, after its
    /// own volume.
    Preview(Slot),
}

/// One of a block's buses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Main,
    Cue,
    Aux,
}

impl Slot {
    const ALL: [Slot; 3] = [Slot::Main, Slot::Cue, Slot::Aux];

    /// `gain` into this bus, nothing into the others.
    fn gains(self, gain: f32) -> [f32; 3] {
        match self {
            Slot::Main => [gain, 0.0, 0.0],
            Slot::Cue => [0.0, gain, 0.0],
            Slot::Aux => [0.0, 0.0, gain],
        }
    }
}

impl Route {
    /// Halo's EQ and filter are the hardware mixer's job.
    fn bypasses_strip(self) -> bool {
        matches!(self, Route::Direct(_) | Route::Preview(_))
    }
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
    route: Route,
    cue_gain: f32,
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
    fn new(input: DeckAudio, sample_rate: u32, route: Route) -> Self {
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
            route,
            cue_gain: 0.0,
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
            self.lock.phase = None;
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

    /// Render a deck that may be synced to the master clock. An unsynced
    /// deck plays at its own tempo; a synced one plays at the clock's BPM,
    /// waits for the clock's next matching beat to start, and is held to
    /// its phase once it has.
    fn render_following(&mut self, mut out: Bus<'_>, clock: &ClockBlock, frame: u64) {
        // The clone cannot be the final owner: input retains the same Arc.
        let shared = self.input.shared.clone();
        let enabled = shared.sync_enabled.load(Ordering::Acquire);
        let frames = out.len() / 2;
        let version = [
            shared.track_generation.load(Ordering::Acquire),
            shared.transport_revision(),
            shared.anchor_revision.load(Ordering::Acquire),
            self.grid_revision,
        ];
        let newly_enabled = enabled && !self.lock.enabled;
        let changed = version != self.lock.version;
        let mut force = newly_enabled && self.was_live;
        if changed {
            // A grid change can invalidate a playing lock. A new anchor or
            // transport already closes delivery and is prepared by the worker.
            force |= self.was_live && version[..3] == self.lock.version[..3];
            if version[1] != self.lock.version[1] || version[0] != self.lock.version[0] {
                self.lock.reprime = true;
                self.lock.fading_out = 0;
                self.lock.fading_in = 0;
            }
            self.lock.launched = false;
            self.lock.phase = None;
            self.lock.drift_beats = 0.0;
            self.lock.version = version;
        }
        self.lock.enabled = enabled;
        if !enabled {
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
            self.status(SyncState::Off);
            self.render_into(out);
            return;
        }
        let cursor = self
            .playback
            .as_ref()
            .map_or(Cursor::default(), |p| p.consumer.cursor);
        let matched_rate = self.map().and_then(|grid| {
            let rate = grid.rate_for(clock.bpm)?;
            let min = if shared.keylock.load(Ordering::Relaxed) {
                1.0 / 32.0
            } else {
                0.25
            };
            (rate >= min && rate <= 4.0).then_some(rate)
        });
        // Tempo matching is independent of phase availability: pre-roll or an
        // incompatible loop can prevent alignment without changing the BPM.
        shared
            .matched_tempo
            .store(matched_rate.unwrap_or(shared.manual_tempo.load() as f64));
        let mapping = self.map().and_then(|grid| {
            if !grid.supports_loop(cursor.region) {
                return None;
            }
            let (beat, length, clock_beat) = if self.lock.launched
                && let Some(lock) = &self.lock.phase
            {
                let (clock_phase, phase) = lock.phases(clock.beats, cursor);
                (phase, grid.beat_frames(self.sample_rate), clock_phase)
            } else {
                let (beat, length) = grid.at(cursor.position)?;
                (beat, length, clock.beats)
            };
            Some((
                beat.rem_euclid(1.0),
                length,
                clock_beat.rem_euclid(1.0),
                matched_rate?,
            ))
        });
        let Some((phase, length, clock_phase, rate)) = mapping else {
            self.delivery_open = true;
            self.lock.launched = false;
            self.lock.fading_out = 0;
            self.lock.fading_in = 0;
            shared.tempo_rate.store(requested_rate(
                &shared,
                shared.matched_tempo.load() * shared.bend.load() as f64,
            ));
            self.status(SyncState::Unavailable);
            self.render_into(out);
            return;
        };
        let nudge = (shared.bend.load() - 1.0).abs() > f32::EPSILON;
        let error = phase_error(clock_phase, phase);
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
            // Paused, nothing is delivered either way; a scrub while playing
            // holds delivery until it lands and relaunches on the beat.
            self.delivery_open = !self.playing();
            self.status(SyncState::Preparing);
            self.render_into(out);
            return;
        }
        if newly_enabled && !self.was_live {
            self.lock.reprime = true;
        }
        if (self.lock.launched || was_nudging) && !nudge {
            force |= error.abs() > 0.125;
            if error.abs() <= DRIFT_TOLERANCE_BEATS {
                self.lock.drift_beats = 0.0;
            } else if !shared.controls_pending.load(Ordering::Acquire)
                && (shared.delivered_tempo.load() - shared.tempo_rate.load() as f64).abs()
                    < rate * 0.001
            {
                self.lock.drift_beats += frames as f64 * clock.beats_per_frame;
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
            let count = self.lock.fading_out.min(frames);
            self.delivery_open = true;
            self.status(SyncState::Preparing);
            self.render_into(out.range(..count * 2));
            if self.lock.fading_out == 0 {
                let target = self.map().and_then(|grid| {
                    grid.aligned_target(
                        self.playback.as_ref()?.consumer.cursor,
                        clock.at(count).rem_euclid(1.0),
                    )
                });
                if let Some(target) = target {
                    shared.request_anchor(target);
                }
                self.lock.launched = false;
                self.lock.drift_beats = 0.0;
                self.delivery_open = false;
                self.render_into(out.range(count * 2..));
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
            self.render_into(out);
            return;
        }
        if self.lock.launched {
            self.delivery_open = true;
            self.status(if nudge {
                SyncState::Nudging
            } else {
                SyncState::Locked
            });
            self.render_into(out);
            return;
        }
        let launch = clock_crossing(clock, frames, phase);
        let waiting_from = clock.at(launch.unwrap_or(frames)).rem_euclid(1.0);
        shared
            .sync_remaining
            .store((phase - waiting_from).rem_euclid(1.0) * length);
        self.status(SyncState::Waiting);
        self.delivery_open = false;
        let offset = launch.unwrap_or(frames);
        self.render_into(out.range(..offset * 2));
        if let Some(offset) = launch {
            // Recheck cancellation after the bounded prefix. The next
            // render_segment also checks readiness and the anchor generation.
            if shared.transport_revision() != version[1]
                || shared.anchor_revision.load(Ordering::Acquire) != version[2]
            {
                return;
            }
            self.lock.launched = true;
            self.lock.phase = self.map().map(|grid| {
                PhaseLock::new(clock.at(offset), cursor, grid.beat_frames(self.sample_rate))
            });
            self.lock.fading_in = (self.sample_rate as usize).div_ceil(200);
            self.delivery_open = true;
            shared
                .launch_frame
                .store(frame + offset as u64, Ordering::Relaxed);
            self.status(if nudge {
                SyncState::Nudging
            } else {
                SyncState::Locked
            });
            self.render_into(out.range(offset * 2..));
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

    fn render_into(&mut self, mut out: Bus<'_>) {
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
            self.render_segment(out.range(offset..offset + count), phase, offset / 2);
            offset += count;
        }
    }

    fn render_segment(&mut self, out: Bus<'_>, phase: ScrubPhase, offset: usize) {
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
        if !self.route.bypasses_strip() {
            self.strip.process(buf, params);
        }
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
            if !self.route.bypasses_strip() {
                self.scrub_strip.process(vbuf, params);
            }
        } else {
            vbuf.fill(0.0);
        }
        let alpha = 1.0 - (-1.0 / (0.005 * self.sample_rate as f32)).exp();
        let target_gain = if live || scrubbing || self.scrub_mix > 0.0 {
            match self.route {
                Route::Direct(_) => 1.0,
                _ => shared.fader.load(),
            }
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
        let target_cue = match self.route {
            Route::MainCued if shared.pfl.load(Ordering::Relaxed) => 1.0,
            _ => 0.0,
        };
        let buses = out
            .main
            .chunks_exact_mut(2)
            .zip(out.cue.chunks_exact_mut(2))
            .zip(out.aux.chunks_exact_mut(2));
        for (((frame, cue), aux), samples) in buses.zip(buf.chunks_exact(2)) {
            self.pre_gain += (target_pre - self.pre_gain) * alpha;
            self.gain += (target_gain - self.gain) * alpha;
            self.cue_gain += (target_cue - self.cue_gain) * alpha;
            let left = samples[0] * self.pre_gain;
            let right = samples[1] * self.pre_gain;
            // The phones take the deck pre-fader, or the audition player
            // after its own volume.
            let [main, phones, extra] = match self.route {
                Route::Phones => Slot::Cue.gains(self.gain),
                Route::Direct(slot) | Route::Preview(slot) => slot.gains(self.gain),
                Route::Main | Route::MainCued => [self.gain, self.cue_gain, 0.0],
            };
            frame[0] += left * main;
            frame[1] += right * main;
            cue[0] += left * phones;
            cue[1] += right * phones;
            aux[0] += left * extra;
            aux[1] += right * extra;
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
/// One output pair: the buses it carries, summed, behind its own ceiling.
struct OutputMix {
    /// Its left channel in a frame.
    offset: usize,
    /// Which of main, cue and aux it carries.
    slots: [bool; 3],
    limiter: Limiter,
    /// Whether the master meter reads it.
    metered: bool,
}

pub struct MixerRenderer {
    decks: [DeckVoice; 3],
    mixer: Arc<MixerShared>,
    master_meter: f32,
    sample_rate: u32,
    /// The stream's channels; pairs without a role are silent.
    channels: usize,
    /// A hardware mixer does the mixing: the buses are deck A, deck B and
    /// the audition player.
    external: bool,
    /// Halo's phones have a pair: the cue bus gets CUE MIX.
    phones: bool,
    /// The pairs something plays on, at most one per bus.
    outputs: Vec<OutputMix>,
    main: Box<[f32; BLOCK_SAMPLES]>,
    cue: Box<[f32; BLOCK_SAMPLES]>,
    aux: Box<[f32; BLOCK_SAMPLES]>,
    /// One pair's sum, before its limiter.
    mix: Box<[f32; BLOCK_SAMPLES]>,
    /// The master clock's beat position: synced decks lock to it.
    clock_beats: f64,
    output_frame: u64,
}

impl MixerRenderer {
    pub fn new(
        decks: [DeckAudio; 3],
        mixer: Arc<MixerShared>,
        sample_rate: u32,
        layout: Layout,
    ) -> Self {
        let channels = usize::from(layout.channels).max(2);
        // A role past the stream's channels plays nowhere.
        let fits = |pair: Pair| pair.offset() + 2 <= channels;
        let (routes, roles) = match layout.mixing {
            Mixing::Internal { master, phones } => {
                let phones = phones.filter(|&p| fits(p) && p != master);
                let routes = if phones.is_some() {
                    [Route::MainCued, Route::MainCued, Route::Phones]
                } else {
                    [Route::Main; 3]
                };
                (routes, [Some(master).filter(|&p| fits(p)), phones, None])
            }
            Mixing::External {
                deck_a,
                deck_b,
                audition,
            } => (
                [
                    Route::Direct(Slot::Main),
                    Route::Direct(Slot::Cue),
                    Route::Preview(Slot::Aux),
                ],
                [deck_a, deck_b, audition].map(|role| role.filter(|&p| fits(p))),
            ),
        };
        let external = layout.external();
        let mut outputs: Vec<OutputMix> = Vec::with_capacity(Slot::ALL.len());
        for (i, role) in roles.into_iter().enumerate() {
            let Some(pair) = role else {
                continue;
            };
            let index = outputs.iter().position(|o| o.offset == pair.offset());
            let output = match index {
                Some(index) => &mut outputs[index],
                None => {
                    outputs.push(OutputMix {
                        offset: pair.offset(),
                        slots: [false; 3],
                        limiter: Limiter::new(sample_rate),
                        metered: false,
                    });
                    let last = outputs.len() - 1;
                    &mut outputs[last]
                }
            };
            output.slots[i] = true;
            // The meter reads the room: the master, or every channel
            // going to a hardware mixer.
            output.metered |= external || i == 0;
        }
        let mut routes = routes.into_iter();
        Self {
            decks: decks.map(|deck| {
                DeckVoice::new(deck, sample_rate, routes.next().unwrap_or(Route::Main))
            }),
            mixer,
            master_meter: 0.0,
            sample_rate,
            channels,
            external,
            phones: roles[1].is_some() && !external,
            outputs,
            main: Box::new([0.0; BLOCK_SAMPLES]),
            cue: Box::new([0.0; BLOCK_SAMPLES]),
            aux: Box::new([0.0; BLOCK_SAMPLES]),
            mix: Box::new([0.0; BLOCK_SAMPLES]),
            clock_beats: 0.0,
            output_frame: 0,
        }
    }

    pub fn render(&mut self, data: &mut [f32]) {
        let started = Instant::now();
        let channels = self.channels;
        let frames = data.len() / channels;
        let (whole, rest) = data.split_at_mut(frames * channels);
        rest.fill(0.0);
        let mut peak = 0.0_f32;
        for chunk in whole.chunks_mut(BLOCK_SAMPLES / 2 * channels) {
            let samples = chunk.len() / channels * 2;
            let main = &mut self.main[..samples];
            let cue = &mut self.cue[..samples];
            let aux = &mut self.aux[..samples];
            main.fill(0.0);
            cue.fill(0.0);
            aux.fill(0.0);
            for deck in &mut self.decks {
                deck.frame_clock = self.output_frame;
                deck.adopt();
            }
            // The UI holds the tempo to MASTER_BPM_RANGE; here only an
            // unusable value falls back.
            let bpm = self.mixer.master_bpm.load();
            let bpm = if bpm.is_finite() && bpm > 0.0 {
                bpm
            } else {
                DEFAULT_MASTER_BPM
            };
            let clock = ClockBlock::new(self.clock_beats, bpm, self.sample_rate);
            let (decks, audition) = self.decks.split_at_mut(2);
            for deck in decks {
                deck.render_following(
                    Bus {
                        main: &mut *main,
                        cue: &mut *cue,
                        aux: &mut *aux,
                    },
                    &clock,
                    self.output_frame,
                );
            }
            audition[0].render_into(Bus {
                main: &mut *main,
                cue: &mut *cue,
                aux: &mut *aux,
            });
            self.clock_beats = clock.at(samples / 2);
            self.mixer.clock_beats.store(self.clock_beats);
            self.output_frame += (samples / 2) as u64;
            self.mixer
                .output_frame
                .store(self.output_frame, Ordering::Release);
            if self.phones {
                // CUE MIX: an equal-power blend of the cue and the master
                // as it is before the master volume, so turning the room
                // down leaves the phones alone.
                let angle = self.mixer.cue_mix.load().clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
                let (to_cue, to_master) = (angle.cos(), angle.sin());
                for (phones, &master) in cue.iter_mut().zip(main.iter()) {
                    *phones = *phones * to_cue + master * to_master;
                }
            }
            // A hardware mixer has the master.
            if !self.external {
                let master = self.mixer.master.load();
                for sample in main.iter_mut() {
                    *sample *= master;
                }
            }
            chunk.fill(0.0);
            let mix = &mut self.mix[..samples];
            for output in &mut self.outputs {
                mix.fill(0.0);
                for (&on, bus) in output.slots.iter().zip([&*main, &*cue, &*aux]) {
                    if on {
                        for (sum, &sample) in mix.iter_mut().zip(bus.iter()) {
                            *sum += sample;
                        }
                    }
                }
                output.limiter.process(mix);
                if output.metered {
                    for &sample in mix.iter() {
                        peak = peak.max(sample.abs());
                    }
                }
                let at = output.offset..output.offset + 2;
                for (frame, pair) in chunk.chunks_exact_mut(channels).zip(mix.chunks_exact(2)) {
                    if let Some(out) = frame.get_mut(at.clone()) {
                        out.copy_from_slice(pair);
                    }
                }
            }
        }
        let budget = frames as f32 / self.sample_rate as f32;
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
    fn rounded_tempo_is_corrected_without_restarting_an_established_lock() {
        const MASTER_BPM: f64 = 118.00116088848971;
        const FOLLOWER_BPM: f64 = 130.01179245283018;
        let matched = MASTER_BPM / FOLLOWER_BPM;
        // Measured at 96 kHz with keylock: the native engine returns a nearby
        // representable tempo, rather than the exact requested BPM ratio.
        let rounding = 0.9078014184397163 - matched;
        for bias in [rounding, -rounding] {
            let (mut renderer, shared, handoffs, mut producers) = fixture();
            renderer.mixer.master_bpm.store(MASTER_BPM);
            // Deck B alone is heard.
            shared[0].fader.store(0.0);
            shared[2].set_transport(Transport::Paused);
            shared[1].sync_enabled.store(true, Ordering::Release);
            shared[1].tempo_rate.store(matched as f32);
            for (i, bpm) in [MASTER_BPM, FOLLOWER_BPM].into_iter().enumerate() {
                handoffs[i].publish_grid(BeatMap::new(
                    (0..300).map(|beat| beat as f64 * 60.0 * 48_000.0 / bpm),
                    1,
                    bpm,
                ));
            }
            let mut worst_error = 0.0_f64;
            let mut corrected = false;
            let media = [0.1; 6144];
            for block in 0..(120 * 48_000 / 512) {
                // A 64 ms reserve delays new controls before delivery. A fixed
                // applied-rate bias models rounding in either direction even
                // after the callback starts asking for small corrections.
                for (i, producer) in producers[..2].iter_mut().enumerate() {
                    let frames = if block == 0 { 3072 } else { 512 };
                    let start_frame = if block == 0 {
                        0
                    } else {
                        3072 + (block - 1) * 512
                    };
                    let requested = shared[i].tempo_rate.load() as f64;
                    let delivered = requested + if i == 1 && block > 0 { bias } else { 0.0 };
                    producer.retain_applied(timestretch::AppliedParameters {
                        requested: crate::deck::playback_parameters(requested, true).unwrap(),
                        achieved_stretch: 1.0 / delivered,
                        achieved_pitch: 1.0,
                        command_id: block,
                        requested_at: block * 512,
                        applied_at: start_frame,
                    });
                    assert!(producer.push(
                        &media[..frames * 2],
                        OutputStamp {
                            epoch: Epoch(1),
                            start_frame,
                            frames
                        },
                    ));
                }
                let mut out = [0.0; 1024];
                renderer.render(&mut out);
                assert_eq!(
                    SyncState::load(&shared[1]),
                    SyncState::Locked,
                    "block={block} bias={bias}"
                );
                assert_eq!(shared[1].take_seek(), None);
                assert_eq!(shared[1].launch_frame.load(Ordering::Relaxed), 0);
                assert_eq!(producers[1].receipt().unwrap().end_frame, (block + 1) * 512);
                assert_eq!(producers[1].underrun_frames(), 0);
                let lock = renderer.decks[1].lock.phase.as_ref().unwrap();
                let (clock, follower) = lock.phases(
                    renderer.clock_beats,
                    renderer.decks[1].playback.as_ref().unwrap().consumer.cursor,
                );
                worst_error = worst_error.max(phase_error(clock, follower).abs());
                corrected |= (shared[1].tempo_rate.load() as f64 - matched).abs() > 1e-5;
                if block > 10 {
                    assert!(
                        out.iter().all(|&sample| sample > 0.01),
                        "audio was interrupted"
                    );
                }
            }
            assert!(corrected, "rounded tempo never received correction");
            assert!(
                worst_error < DRIFT_TOLERANCE_BEATS,
                "drift reached the re-lock threshold: {worst_error}"
            );
        }
    }

    #[test]
    fn breakdown_grid_wander_does_not_interrupt_an_established_lock() {
        for wandering_deck in [0, 1] {
            for cut_other in [false, true] {
                let (mut renderer, shared, handoffs, mut producers) = fixture();
                shared[1].sync_enabled.store(true, Ordering::Release);
                for (i, handoff) in handoffs[..2].iter().enumerate() {
                    // Onsets drift by half a beat and disappear during a sparse
                    // passage. The source transport and nominal BPM stay steady.
                    handoff.publish_grid(BeatMap::new(
                        (0..22)
                            .filter(|&beat| i != wandering_deck || !(14..18).contains(&beat))
                            .map(|beat| {
                                let drift = if i == wandering_deck {
                                    match beat {
                                        5..=9 => (beat - 4) as f64 * 100.0,
                                        10..=13 => (14 - beat) as f64 * 100.0,
                                        _ => 0.0,
                                    }
                                } else {
                                    0.0
                                };
                                beat as f64 * 1000.0 + drift
                            }),
                        1,
                        2880.0,
                    ));
                    prime_fake(
                        &mut producers[i],
                        &shared[i],
                        2,
                        i as f64 * 2000.0,
                        1.0,
                        4096,
                    );
                    assert!(producers[i].push(
                        &[0.0; 4096],
                        OutputStamp {
                            epoch: Epoch(2),
                            start_frame: 4096,
                            frames: 2048,
                        }
                    ));
                    assert!(producers[i].push(
                        &vec![0.1; 48000],
                        OutputStamp {
                            epoch: Epoch(2),
                            start_frame: 6144,
                            frames: 24000,
                        }
                    ));
                }
                for block in 0..100 {
                    if cut_other && block == 8 {
                        shared[0].eq_low.store(0.0);
                        shared[0].eq_mid.store(0.0);
                        shared[0].filter_mode.store(2, Ordering::Relaxed);
                        shared[0].filter_cutoff.store(2000.0);
                    }
                    let allocations =
                        allocation_counter::measure(|| renderer.render(&mut [0.0; 512]));
                    assert_eq!(allocations.count_total, 0);
                    assert_eq!(allocations.count_current, 0);
                    assert_eq!(
                        SyncState::load(&shared[1]),
                        SyncState::Locked,
                        "false re-lock at block {block}, grid on deck {wandering_deck}, EQ cut {cut_other}"
                    );
                    assert_eq!(shared[1].launch_frame.load(Ordering::Relaxed), 0);
                    assert_eq!(shared[1].take_seek(), None);
                    assert_eq!(shared[1].tempo_rate.load(), 1.0);
                    assert_eq!(
                        shared[1].delivery_position.load(),
                        2000.0 + (block + 1) as f64 * 256.0
                    );
                    assert_eq!(producers[1].receipt().unwrap().end_frame, (block + 1) * 256);
                }
            }
        }
    }

    #[test]
    fn uneven_detected_intervals_never_change_the_matched_bpm() {
        // Reduced cached-analysis example: a 130 BPM track with per-beat
        // onset jitter, synced to a 127.4 BPM clock.
        let beats = [
            20215.2256,
            40699.7146,
            61177.5614,
            81156.5636,
            101638.7939,
            122107.7101,
            142599.3014,
            162563.5545,
            183032.4931,
            203511.3947,
            223993.5318,
            244487.7003,
            264456.3430,
            284923.7041,
            305401.3910,
            325880.9168,
        ];
        let track_bpm = 130.01179245283018;
        let clock_bpm = 127.4;
        let (mut renderer, shared, handoffs, mut producers) = fixture();
        renderer.mixer.master_bpm.store(clock_bpm);
        shared[1].set_transport(Transport::Paused);
        shared[1].sync_enabled.store(true, Ordering::Release);
        handoffs[1].publish_grid(BeatMap::new(beats.into_iter(), 1, track_bpm));
        prime_fake(&mut producers[1], &shared[1], 2, beats[0], 1.0, 8192);
        renderer.render(&mut [0.0; 128]);
        let expected = clock_bpm / track_bpm;
        for pair in beats.windows(2) {
            renderer.decks[1]
                .playback
                .as_mut()
                .unwrap()
                .consumer
                .cursor
                .position = (pair[0] + pair[1]) / 2.0;
            renderer.render(&mut [0.0; 128]);
            let matched = shared[1].matched_tempo.load();
            assert!(
                (matched - expected).abs() < 1e-12,
                "matched rate wandered to {matched}"
            );
            assert!((shared[1].tempo_rate.load() as f64 - expected).abs() < 1e-6);
            assert_eq!(shared[1].take_seek(), None, "Sync moved a paused deck");
        }
        // A new master BPM retunes it, while a held nudge does not redefine
        // the underlying matched rate or move the parked deck.
        renderer.mixer.master_bpm.store(133.0);
        shared[1].bend.store(1.04);
        renderer.render(&mut [0.0; 128]);
        assert!((shared[1].matched_tempo.load() - 133.0 / track_bpm).abs() < 1e-12);
        assert_eq!(shared[1].take_seek(), None);
    }

    #[test]
    fn a_lone_synced_deck_waits_for_the_clocks_beat_and_plays_at_its_tempo() {
        let (mut renderer, shared, handoffs, mut producers) = fixture();
        grid(&handoffs[1], 0.0, 1000.0);
        renderer.mixer.master_bpm.store(GRID_BPM * 1.23);
        shared[0].set_transport(Transport::Paused);
        shared[1].set_transport(Transport::Paused);
        shared[1].sync_enabled.store(true, Ordering::Release);
        // Parked half a beat in.
        prime_fake(&mut producers[1], &shared[1], 2, 1500.0, 1.23, 8192);
        renderer.render(&mut [0.0; 128]);
        assert!((shared[1].matched_tempo.load() - 1.23).abs() < 1e-9);
        assert_eq!(shared[1].take_seek(), None);
        shared[1].set_transport(Transport::Playing);
        shared[1]
            .prepared_revision
            .store(shared[1].transport_revision(), Ordering::Release);
        let clock = ClockBlock::new(renderer.clock_beats, GRID_BPM * 1.23, 48_000);
        let offset = clock_crossing(&clock, 1024, 0.5).unwrap() as u64;
        let launch = renderer.output_frame + offset;
        renderer.render(&mut [0.0; 2048]);
        assert_eq!(SyncState::load(&shared[1]), SyncState::Locked);
        assert_eq!(shared[1].launch_frame.load(Ordering::Relaxed), launch);
        assert!((shared[1].tempo_rate.load() - 1.23).abs() < 1e-6);
        assert_eq!(shared[1].take_seek(), None);
        assert_eq!(producers[1].receipt().unwrap().end_frame, 1024 - offset);
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
        shared[2].set_transport(Transport::Paused);
        let mut start_frame = 0;
        let mut render = |renderer: &mut MixerRenderer, apply_controls: bool| {
            for (i, producer) in producers[..2].iter_mut().enumerate() {
                let rate = if apply_controls {
                    shared[i].tempo_rate.load() as f64
                } else {
                    1.0
                };
                producer.retain_applied(timestretch::AppliedParameters {
                    requested: crate::deck::playback_parameters(rate, true).unwrap(),
                    achieved_stretch: 1.0 / rate,
                    achieved_pitch: 1.0,
                    command_id: start_frame,
                    requested_at: start_frame,
                    applied_at: start_frame,
                });
                assert!(producer.push(
                    &[0.1; 256],
                    OutputStamp {
                        epoch: Epoch(1),
                        start_frame,
                        frames: 128
                    },
                ));
            }
            renderer.render(&mut [0.0; 256]);
            start_frame += 128;
        };
        render(&mut renderer, false);
        assert_eq!(SyncState::load(&shared[1]), SyncState::Locked);
        // This is within the gentle correction range, but large enough to
        // remain outside tolerance for two beats after controls reach output.
        renderer.decks[1]
            .playback
            .as_mut()
            .unwrap()
            .consumer
            .cursor
            .position += 50.0;
        shared[1].controls_pending.store(true, Ordering::Release);
        for _ in 0..20 {
            render(&mut renderer, false);
        }
        assert_eq!(
            shared[1].take_seek(),
            None,
            "pending controls must not start drift timeout"
        );
        shared[1].controls_pending.store(false, Ordering::Release);
        for _ in 0..14 {
            render(&mut renderer, true);
        }
        assert_eq!(shared[1].take_seek(), None);
        for _ in 0..4 {
            render(&mut renderer, true);
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

    /// A beat every 1000 frames at 48 kHz.
    const GRID_BPM: f64 = 2880.0;

    fn grid(handoff: &DeliveryHandoff, offset: f64, length: f64) {
        handoff.publish_grid(BeatMap::new(
            (0..2000).map(|i| offset + i as f64 * length),
            1,
            60.0 * 48_000.0 / length,
        ));
    }

    #[test]
    fn scheduled_launches_preserve_fractional_parking_through_worker_and_ui_stalls() {
        let mut worst_error = 0.0_f64;
        let mut internal = 0;
        for rate in [0.5, 1.0, 1.25] {
            for clock_phase in [0.0, 0.19, 0.5, 0.91] {
                for follower_phase in [0.0, 0.07, 0.5, 0.79, 0.999] {
                    let (mut renderer, shared, handoffs, mut producers) = fixture();
                    let parked = 997.0 * (2.0 + follower_phase);
                    renderer
                        .mixer
                        .master_bpm
                        .store(60.0 * 48_000.0 / 997.0 * rate);
                    renderer.clock_beats = clock_phase;
                    shared[1].set_transport(Transport::Paused);
                    shared[1].sync_enabled.store(true, Ordering::Release);
                    for handoff in &handoffs[..2] {
                        grid(handoff, 0.0, 997.0);
                    }
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
                            let clock_at_launch = clock_phase + launch as f64 * rate / 997.0;
                            let error =
                                phase_error(clock_at_launch, follower_phase).abs() * 997.0 / rate;
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
    fn unavailable_sync_allows_plain_playback_and_keeps_intent() {
        let (mut renderer, shared, handoffs, mut producers) = fixture();
        shared[1].sync_enabled.store(true, Ordering::Release);
        prime_fake(&mut producers[0], &shared[0], 2, 0.0, 1.0, 8192);
        prime_fake(&mut producers[1], &shared[1], 2, 2000.0, 1.0, 8192);
        renderer.render(&mut [0.0; 128]);
        assert_eq!(SyncState::load(&shared[1]), SyncState::Unavailable);
        assert_eq!(producers[1].receipt().unwrap().end_frame, 64);
        renderer.mixer.master_bpm.store(GRID_BPM * 10.0);
        grid(&handoffs[1], 0.0, 1000.0);
        renderer.render(&mut [0.0; 128]);
        assert_eq!(
            SyncState::load(&shared[1]),
            SyncState::Unavailable,
            "unsupported tempo"
        );
        renderer.mixer.master_bpm.store(GRID_BPM);
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
            mixer.master.store(1.0);
            // Deck A alone is heard.
            shared[1].fader.store(0.0);
            let mut renderer = MixerRenderer::new(
                std::array::from_fn(|i| DeckAudio {
                    shared: shared[i].clone(),
                    handoff: handoffs[i].clone(),
                }),
                mixer,
                RATE,
                Layout::stereo(),
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
        fixture_with(2)
    }

    /// A fixture whose stream has `channels`: 4 and up have the phones,
    /// on 3–4.
    fn fixture_with(channels: u16) -> Fixture {
        fixture_layout(Layout {
            channels,
            mixing: Mixing::Internal {
                master: Pair(0),
                phones: (channels >= 4).then_some(Pair(1)),
            },
        })
    }

    fn fixture_layout(layout: Layout) -> Fixture {
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
        let mixer = Arc::new(MixerShared::new());
        // The master clock at the test grids' tempo: a beat every 1000
        // output frames, from beat 0.
        mixer.master_bpm.store(GRID_BPM);
        let renderer = MixerRenderer::new(
            std::array::from_fn(|i| DeckAudio {
                shared: shared[i].clone(),
                handoff: handoffs[i].clone(),
            }),
            mixer,
            48_000,
            layout,
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

    /// Queue `frames` of a steady `level` on each of A, B and the audition.
    fn push_levels(producers: &mut [Producer; 3], levels: [f32; 3], frames: usize) {
        for (producer, level) in producers.iter_mut().zip(levels) {
            assert!(producer.push(
                &vec![level; frames * 2],
                OutputStamp {
                    epoch: Epoch(1),
                    start_frame: 0,
                    frames
                }
            ));
        }
    }

    /// The last frame of an interleaved buffer.
    fn last_frame(out: &[f32], channels: usize) -> &[f32] {
        &out[out.len() - channels..]
    }

    #[test]
    fn the_phones_hear_cued_decks_pre_fader_and_the_audition_player_alone() {
        let (mut renderer, shared, _, mut producers) = fixture_with(4);
        // A cued with its fader down, B in the room only, the audition
        // player at half volume.
        shared[0].fader.store(0.0);
        shared[0].pfl.store(true, Ordering::Relaxed);
        shared[2].fader.store(0.5);
        renderer.mixer.master.store(0.5);
        renderer.mixer.cue_mix.store(0.0);
        push_levels(&mut producers, [0.1, 0.2, 0.3], 8192);
        let mut out = vec![0.0; 4096 * 4];
        let allocation = allocation_counter::measure(|| renderer.render(&mut out));
        assert_eq!(allocation.count_total, 0);
        assert_eq!(allocation.count_current, 0);
        let frame = last_frame(&out, 4);
        // The room: B alone, through the master volume.
        assert!((frame[0] - 0.1).abs() < 0.01, "{frame:?}");
        assert!((frame[1] - 0.1).abs() < 0.01, "{frame:?}");
        // The phones, all cue: A pre-fader and the audition at its volume.
        assert!((frame[2] - 0.25).abs() < 0.01, "{frame:?}");
        assert!((frame[3] - 0.25).abs() < 0.01, "{frame:?}");

        // All master: the room's mix before the master volume.
        renderer.mixer.cue_mix.store(1.0);
        renderer.render(&mut out);
        let frame = last_frame(&out, 4);
        assert!((frame[0] - 0.1).abs() < 0.01, "{frame:?}");
        assert!((frame[2] - 0.2).abs() < 0.01, "{frame:?}");
    }

    #[test]
    fn the_phones_can_take_1_2_with_the_master_on_3_4() {
        let (mut renderer, shared, _, mut producers) = fixture_layout(Layout {
            channels: 4,
            mixing: Mixing::Internal {
                master: Pair(1),
                phones: Some(Pair(0)),
            },
        });
        // As above: A cued with its fader down, B in the room only.
        shared[0].fader.store(0.0);
        shared[0].pfl.store(true, Ordering::Relaxed);
        shared[2].fader.store(0.5);
        renderer.mixer.master.store(0.5);
        renderer.mixer.cue_mix.store(0.0);
        push_levels(&mut producers, [0.1, 0.2, 0.3], 8192);
        let mut out = vec![0.0; 4096 * 4];
        renderer.render(&mut out);
        let frame = last_frame(&out, 4);
        for (i, want) in [0.25, 0.25, 0.1, 0.1].into_iter().enumerate() {
            assert!((frame[i] - want).abs() < 0.01, "{frame:?}");
        }
    }

    /// A hardware mixer on a 4-output stream: A on 1–2, B on 3–4.
    fn external(audition: Pair) -> Fixture {
        fixture_layout(Layout {
            channels: 4,
            mixing: Mixing::External {
                deck_a: Some(Pair(0)),
                deck_b: Some(Pair(1)),
                audition: Some(audition),
            },
        })
    }

    #[test]
    fn external_sends_each_deck_unmixed_to_its_own_pair() {
        let (mut renderer, shared, _, mut producers) = external(Pair(1));
        // Halo's mixer is ignored: A's fader is down, its lows killed and
        // its filter closed, the master is off and B is cued. Only the
        // trim counts.
        shared[0].fader.store(0.0);
        shared[0].eq_low.store(0.0);
        shared[0].set_filter_mode(1);
        shared[0].filter_cutoff.store(0.0);
        shared[0].trim.store(0.5);
        shared[1].pfl.store(true, Ordering::Relaxed);
        shared[2].fader.store(0.0);
        renderer.mixer.master.store(0.0);
        push_levels(&mut producers, [0.2, 0.3, 0.4], 8192);
        let mut out = vec![0.0; 4096 * 4];
        let allocation = allocation_counter::measure(|| renderer.render(&mut out));
        assert_eq!(allocation.count_total, 0);
        assert_eq!(allocation.count_current, 0);
        let frame = last_frame(&out, 4);
        for (i, want) in [0.1, 0.1, 0.3, 0.3].into_iter().enumerate() {
            assert!((frame[i] - want).abs() < 0.01, "{frame:?}");
        }
    }

    #[test]
    fn external_plays_the_audition_player_on_its_pair_at_its_volume() {
        let (mut renderer, shared, _, mut producers) = external(Pair(0));
        shared[2].fader.store(0.5);
        push_levels(&mut producers, [0.0, 0.0, 0.4], 8192);
        let mut out = vec![0.0; 4096 * 4];
        renderer.render(&mut out);
        let frame = last_frame(&out, 4);
        assert!((frame[0] - 0.2).abs() < 0.01, "{frame:?}");
        assert_eq!(&frame[2..], &[0.0, 0.0]);
    }

    #[test]
    fn external_pairs_each_keep_their_own_ceiling() {
        let (mut renderer, shared, _, mut producers) = external(Pair(1));
        shared[0].trim.store(2.0);
        push_levels(&mut producers, [0.9, 0.9, 0.9], 4096);
        let mut out = vec![0.0; 4096 * 4];
        renderer.render(&mut out);
        let mut heard = [0.0_f32; 2];
        for frame in out.chunks_exact(4) {
            assert!(frame.iter().all(|s| s.abs() <= 0.981), "{frame:?}");
            heard[0] = heard[0].max(frame[0].abs());
            heard[1] = heard[1].max(frame[2].abs());
        }
        assert!(
            heard.iter().all(|&h| h > 0.9),
            "a pair went quiet: {heard:?}"
        );
    }

    #[test]
    fn external_reaches_any_pair_and_leaves_the_rest_silent() {
        let (mut renderer, shared, _, mut producers) = fixture_layout(Layout {
            channels: 8,
            mixing: Mixing::External {
                deck_a: Some(Pair(2)),
                deck_b: Some(Pair(0)),
                audition: Some(Pair(3)),
            },
        });
        shared[2].fader.store(1.0);
        push_levels(&mut producers, [0.2, 0.3, 0.4], 8192);
        let mut out = vec![0.0; 4096 * 8];
        let allocation = allocation_counter::measure(|| renderer.render(&mut out));
        assert_eq!(allocation.count_total, 0);
        let frame = last_frame(&out, 8);
        for (i, want) in [0.3, 0.3, 0.0, 0.0, 0.2, 0.2, 0.4, 0.4]
            .into_iter()
            .enumerate()
        {
            assert!((frame[i] - want).abs() < 0.01, "{frame:?}");
        }
    }

    #[test]
    fn roles_sharing_a_pair_are_summed_under_one_ceiling() {
        let (mut renderer, shared, _, mut producers) = external(Pair(1));
        shared[2].fader.store(1.0);
        push_levels(&mut producers, [0.1, 0.2, 0.3], 8192);
        let mut out = vec![0.0; 4096 * 4];
        renderer.render(&mut out);
        let frame = last_frame(&out, 4);
        // Deck B and the audition player together on 3–4.
        assert!((frame[2] - 0.5).abs() < 0.01, "{frame:?}");
        // Together past the ceiling, they're held under it.
        let (mut renderer, shared, _, mut producers) = external(Pair(1));
        shared[2].fader.store(1.0);
        push_levels(&mut producers, [0.0, 0.7, 0.7], 8192);
        renderer.render(&mut out);
        assert!(out.iter().all(|s| s.abs() <= 0.981));
        assert!(last_frame(&out, 4)[2] > 0.9);
    }

    #[test]
    fn without_phones_cue_is_silent_and_the_audition_player_is_in_the_room() {
        let (mut renderer, shared, _, mut producers) = fixture_with(2);
        shared[0].fader.store(0.0);
        shared[0].pfl.store(true, Ordering::Relaxed);
        shared[1].fader.store(0.0);
        push_levels(&mut producers, [0.1, 0.2, 0.3], 4096);
        let mut out = vec![0.0; 4096 * 2];
        renderer.render(&mut out);
        let frame = last_frame(&out, 2);
        assert!((frame[0] - 0.3).abs() < 0.01, "{frame:?}");
    }

    #[test]
    fn the_phones_have_their_own_ceiling_and_channels_past_them_are_silent() {
        let (mut renderer, shared, _, mut producers) = fixture_with(6);
        for deck in &shared[..2] {
            deck.pfl.store(true, Ordering::Relaxed);
        }
        renderer.mixer.cue_mix.store(0.0);
        push_levels(&mut producers, [0.9, 0.9, 0.9], 4096);
        let mut out = vec![0.0; 4096 * 6];
        renderer.render(&mut out);
        let mut heard = 0.0_f32;
        for frame in out.chunks_exact(6) {
            assert!(frame.iter().all(|s| s.abs() <= 0.981), "{frame:?}");
            assert_eq!(&frame[4..], &[0.0, 0.0]);
            heard = heard.max(frame[2].abs());
        }
        assert!(heard > 0.9, "the phones went quiet: {heard}");
    }

    #[test]
    fn cueing_a_deck_fades_it_into_the_phones_without_a_click() {
        let (mut renderer, shared, _, mut producers) = fixture_with(4);
        renderer.mixer.cue_mix.store(0.0);
        push_levels(&mut producers, [0.5, 0.0, 0.0], 8192);
        let mut out = vec![0.0; 4096 * 4];
        renderer.render(&mut out);
        assert!(out.chunks_exact(4).all(|frame| frame[2] == 0.0));
        let before = last_frame(&out, 4)[2];
        shared[0].pfl.store(true, Ordering::Relaxed);
        renderer.render(&mut out);
        let mut previous = before;
        for frame in out.chunks_exact(4) {
            assert!(
                (frame[2] - previous).abs() < 0.005,
                "a step at {}",
                frame[2]
            );
            previous = frame[2];
        }
        assert!((previous - 0.5).abs() < 0.01, "{previous}");
    }

    #[test]
    fn audition_plays_and_muted_decks_still_consume() {
        let (mut renderer, shared, _, mut producers) = fixture();
        for deck in &shared[..2] {
            deck.fader.store(0.0);
        }
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
