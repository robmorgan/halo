//! Lock-free state shared between the UI thread, the per-deck render workers,
//! and the audio callback. Controls and telemetry use atomics — the audio path never
//! takes a lock it can block on. Anchor commands use a bounded ownership queue.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

/// f32 stored as bits in an AtomicU32.
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }

    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    pub fn store(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
}

/// f64 stored as bits in an AtomicU64.
pub struct AtomicF64(AtomicU64);

impl AtomicF64 {
    pub fn new(v: f64) -> Self {
        Self(AtomicU64::new(v.to_bits()))
    }

    pub fn load(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }

    pub fn store(&self, v: f64) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Stopped = 0,
    Playing = 1,
    Paused = 2,
}

/// Bounded, value-only anchor messages. The published revision is advanced
/// AFTER insertion, so an acknowledgement can never refer to an unobserved seek.
#[derive(Clone, Copy)]
pub struct AnchorCommand {
    pub revision: u64,
    pub frame: Option<f64>,
    pub region: Option<(usize, usize)>,
}

/// Coherent observation of one software delivery boundary. No speaker-time
/// or exact sample-origin claim is made by these coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeliveryClock {
    pub output_frame: u64,
    pub position: f64,
    pub iteration: u64,
    pub track: u64,
    pub anchor: u64,
}

pub struct ClockSnapshot {
    sequence: AtomicU64,
    output_frame: AtomicU64,
    position: AtomicF64,
    iteration: AtomicU64,
    track: AtomicU64,
    anchor: AtomicU64,
}

impl ClockSnapshot {
    fn new() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            output_frame: AtomicU64::new(0),
            position: AtomicF64::new(0.0),
            iteration: AtomicU64::new(0),
            track: AtomicU64::new(0),
            anchor: AtomicU64::new(0),
        }
    }

    /// Callback only: the sole writer. Readers make one bounded attempt.
    pub fn publish(&self, clock: DeliveryClock) {
        self.sequence.fetch_add(1, Ordering::AcqRel);
        self.output_frame
            .store(clock.output_frame, Ordering::Relaxed);
        self.position.store(clock.position);
        self.iteration.store(clock.iteration, Ordering::Relaxed);
        self.track.store(clock.track, Ordering::Relaxed);
        self.anchor.store(clock.anchor, Ordering::Relaxed);
        self.sequence.fetch_add(1, Ordering::Release);
    }

    pub fn read(&self) -> Option<DeliveryClock> {
        let sequence = self.sequence.load(Ordering::Acquire);
        if sequence == 0 || sequence & 1 != 0 {
            return None;
        }
        let clock = DeliveryClock {
            output_frame: self.output_frame.load(Ordering::Relaxed),
            position: self.position.load(),
            iteration: self.iteration.load(Ordering::Relaxed),
            track: self.track.load(Ordering::Relaxed),
            anchor: self.anchor.load(Ordering::Relaxed),
        };
        std::sync::atomic::fence(Ordering::Acquire);
        (sequence == self.sequence.load(Ordering::Relaxed)).then_some(clock)
    }
}

/// Per-deck state shared across the UI, render worker, and audio callback.
pub struct DeckShared {
    transport: AtomicU64,
    /// Persistent intent is independent of master promotion and runtime state.
    pub sync_enabled: AtomicBool,
    pub sync_state: AtomicU8,
    pub sync_remaining: AtomicF64,
    pub manual_tempo: AtomicF32,
    pub bend: AtomicF32,
    pub matched_tempo: AtomicF64,
    pub delivered_tempo: AtomicF64,
    pub controls_pending: AtomicBool,
    pub anchor_revision: AtomicU64,
    pub prepared_revision: AtomicU64,
    pub delivery_position: AtomicF64,
    pub delivery_clock: ClockSnapshot,
    pub loop_iteration: AtomicU64,
    pub launch_frame: AtomicU64,
    /// Playhead in source frames, published at software delivery boundaries.
    pub playhead: AtomicU64,
    /// Total source frames of the loaded track.
    pub total_frames: AtomicU64,
    /// Cue point in source frames.
    pub cue_point: AtomicU64,
    /// Bounded seek/loop requests. UI and callback publish, worker consumes.
    anchor_commands: crossbeam_queue::ArrayQueue<AnchorCommand>,
    anchor_counter: AtomicU64,
    /// True once the render worker has filled the output reserve —
    /// the audio callback only consumes the deck while this is set.
    pub stream_active: AtomicBool,
    /// Channel trim (gain), linear. 1.0 = unity.
    pub trim: AtomicF32,
    /// Channel fader, 0..1.
    pub fader: AtomicF32,
    /// Pre-fader (post-trim) deck level (linear peak, 0..1), published by
    /// the audio callback for the channel meter — shows the track's level
    /// regardless of fader/crossfader. Fast attack, slow release.
    pub meter: AtomicF32,
    /// Pre-fader (post-trim) momentary loudness in LUFS (BS.1770 400 ms
    /// window), published by the audio callback. Floor -100.0 (silence).
    pub meter_lufs: AtomicF32,
    /// Isolator EQ band gains, linear 0..2 (0 = kill, 1 = unity).
    pub eq_low: AtomicF32,
    pub eq_mid: AtomicF32,
    pub eq_high: AtomicF32,
    /// Filter mode as u8 (see `dsp::FilterMode::from_u8`).
    pub filter_mode: AtomicU8,
    /// Normalized filter cutoff 0..1 (log-mapped 20 Hz → 20 kHz).
    pub filter_cutoff: AtomicF32,
    /// Engine tempo rate (playback speed; 1.0 = original). The callback
    /// resolves manual/matched tempo and nudge; the worker forwards it to DSP.
    pub tempo_rate: AtomicF32,
    /// Zero-speed fader detent pauses delivery without changing play intent.
    pub tempo_stopped: AtomicBool,
    /// Generation of the loaded track; stale callback deliveries stay muted.
    pub track_generation: AtomicU64,
    /// Worker processing time / elapsed wall time (EMA).
    pub worker_load: AtomicF32,
    /// Applied DSP tempo, reported separately from requested controls.
    pub achieved_tempo: AtomicF64,
    /// Conservative control boundary + pending/unacknowledged output, seconds.
    pub control_latency: AtomicF64,
    /// Host reserve in seconds, separate from device latency.
    pub queue_latency: AtomicF64,
    pub underrun_frames: AtomicU64,
    /// True after a worker failure; UI drains its error channel.
    pub worker_failed: AtomicBool,
    /// Scrub landing sequence primed by the worker.
    pub scrub_ready: AtomicU64,
    /// Keylock: pitch stays constant while tempo changes (Tape mode when
    /// off — pitch follows tempo).
    pub keylock: AtomicBool,
    /// Active loop region packed as `start << 32 | end` (frames, u32 each)
    /// so the render worker can never read a torn start/end pair.
    /// `u64::MAX` = no loop.
    loop_region: AtomicU64,
    /// Audible-scrub handshake between the UI, render worker, and audio
    /// callback.
    pub scrub: ScrubState,
}

impl DeckShared {
    pub fn new() -> Self {
        Self {
            transport: AtomicU64::new(Transport::Stopped as u64),
            sync_enabled: AtomicBool::new(false),
            sync_state: AtomicU8::new(0),
            sync_remaining: AtomicF64::new(0.0),
            manual_tempo: AtomicF32::new(1.0),
            bend: AtomicF32::new(1.0),
            matched_tempo: AtomicF64::new(1.0),
            delivered_tempo: AtomicF64::new(1.0),
            controls_pending: AtomicBool::new(false),
            anchor_revision: AtomicU64::new(0),
            prepared_revision: AtomicU64::new(0),
            delivery_position: AtomicF64::new(0.0),
            delivery_clock: ClockSnapshot::new(),
            loop_iteration: AtomicU64::new(0),
            launch_frame: AtomicU64::new(u64::MAX),
            playhead: AtomicU64::new(0),
            total_frames: AtomicU64::new(0),
            cue_point: AtomicU64::new(0),
            anchor_commands: crossbeam_queue::ArrayQueue::new(8),
            anchor_counter: AtomicU64::new(0),
            stream_active: AtomicBool::new(false),
            trim: AtomicF32::new(1.0),
            fader: AtomicF32::new(1.0),
            meter: AtomicF32::new(0.0),
            meter_lufs: AtomicF32::new(-100.0),
            eq_low: AtomicF32::new(1.0),
            eq_mid: AtomicF32::new(1.0),
            eq_high: AtomicF32::new(1.0),
            filter_mode: AtomicU8::new(0),
            filter_cutoff: AtomicF32::new(1.0),
            tempo_rate: AtomicF32::new(1.0),
            tempo_stopped: AtomicBool::new(false),
            track_generation: AtomicU64::new(0),
            worker_load: AtomicF32::new(0.0),
            achieved_tempo: AtomicF64::new(1.0),
            control_latency: AtomicF64::new(0.0),
            queue_latency: AtomicF64::new(0.0),
            underrun_frames: AtomicU64::new(0),
            worker_failed: AtomicBool::new(false),
            scrub_ready: AtomicU64::new(0),
            keylock: AtomicBool::new(true),
            loop_region: AtomicU64::new(u64::MAX),
            scrub: ScrubState::new(),
        }
    }

    pub fn set_loop(&self, region: Option<(usize, usize)>) {
        let packed = match region {
            Some((start, end)) => ((start as u64) << 32) | (end as u64 & 0xFFFF_FFFF),
            None => u64::MAX,
        };
        if self.loop_region.swap(packed, Ordering::AcqRel) != packed {
            self.publish_anchor(None);
        }
    }

    pub fn loop_region(&self) -> Option<(usize, usize)> {
        let packed = self.loop_region.load(Ordering::Relaxed);
        if packed == u64::MAX {
            return None;
        }
        Some(((packed >> 32) as usize, (packed & 0xFFFF_FFFF) as usize))
    }

    pub fn filter_mode_u8(&self) -> u8 {
        self.filter_mode.load(Ordering::Relaxed)
    }

    pub fn set_filter_mode(&self, mode: u8) {
        self.filter_mode.store(mode, Ordering::Relaxed);
    }

    pub fn transport(&self) -> Transport {
        match self.transport.load(Ordering::Acquire) & 3 {
            1 => Transport::Playing,
            2 => Transport::Paused,
            _ => Transport::Stopped,
        }
    }

    pub fn set_transport(&self, t: Transport) {
        let _ = self
            .transport
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                ((old & 3) != t as u64).then_some(((old >> 2) + 1) << 2 | t as u64)
            });
    }

    pub fn transport_revision(&self) -> u64 {
        self.transport.load(Ordering::Acquire) >> 2
    }

    pub fn request_seek(&self, frame: usize) {
        self.request_anchor(frame as f64);
    }

    pub fn request_anchor(&self, frame: f64) {
        if frame.is_finite() && frame >= 0.0 {
            self.publish_anchor(Some(frame));
        }
    }

    fn publish_anchor(&self, frame: Option<f64>) {
        let revision = self.anchor_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.anchor_commands.force_push(AnchorCommand {
            revision,
            frame,
            region: self.loop_region(),
        });
        self.anchor_revision.fetch_max(revision, Ordering::Release);
        self.stream_active.store(false, Ordering::Release);
    }

    /// Worker only. Coalesce a bounded batch; a delayed older writer cannot
    /// supersede a newer revision. Keep the newest seek across loop edits.
    pub fn take_anchor(&self, after_revision: u64) -> Option<AnchorCommand> {
        let mut newest: Option<AnchorCommand> = None;
        let mut seek: Option<(u64, f64)> = None;
        for _ in 0..8 {
            let Some(command) = self.anchor_commands.pop() else {
                break;
            };
            if command.revision <= after_revision {
                continue;
            }
            if let Some(frame) = command.frame
                && seek.is_none_or(|(revision, _)| command.revision > revision)
            {
                seek = Some((command.revision, frame));
            }
            if newest.is_none_or(|old| command.revision > old.revision) {
                newest = Some(command);
            }
        }
        newest.map(|mut command| {
            command.frame = seek.map(|(_, frame)| frame);
            command
        })
    }

    #[cfg(test)]
    pub fn take_seek(&self) -> Option<f64> {
        self.take_anchor(0).and_then(|command| command.frame)
    }

    /// An explicit seek outside the loop disables that observed loop as part
    /// of the same transaction, without superseding concurrent UI loop edits.
    pub fn clear_loop_for_seek(&self, region: (usize, usize)) {
        let packed = ((region.0 as u64) << 32) | region.1 as u64;
        let _ = self.loop_region.compare_exchange(
            packed,
            u64::MAX,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub fn playhead_frames(&self) -> usize {
        self.playhead.load(Ordering::Relaxed) as usize
    }

    pub fn total(&self) -> usize {
        self.total_frames.load(Ordering::Relaxed) as usize
    }
}

/// Where the audible scrub currently stands. Ported from the timestretch
/// desktop reference deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrubPhase {
    /// No scrub engaged; the engine owns playback.
    Idle = 0,
    /// The pointer holds the platter: the callback's varispeed voice
    /// chases the published target.
    Active = 1,
    /// The drag dropped: the voice glides its momentum toward the settle
    /// rate, then hands back to the engine at the predicted landing.
    Settling = 2,
}

/// Shared scrub state machine: the UI publishes the pointer-implied source
/// position while the zoomed waveform is dragged; the audio callback chases
/// it with a raw varispeed reader (bypassing the engine), then owns the
/// post-release momentum glide. The callback publishes the voice position;
/// the render worker stops rendering while `Active`.
pub struct ScrubState {
    /// `ScrubPhase` as u8 (0/1/2).
    phase: AtomicU8,
    /// Pointer-target source frame (valid while `Active`).
    target_frame: AtomicF64,
    /// Elastic lead-in depth for the current gesture (source frames):
    /// positions in `[-lead_in, 0)` are draggable silence before the track
    /// start. Published by the UI at engage, copied into the voice's floor.
    lead_in_frames: AtomicF64,
    /// Rate the release glide eases toward: the deck's tempo rate resumes
    /// playback speed, 0.0 spins down to rest.
    settle_rate_target: AtomicF64,
    /// Voice read position, published by the audio callback every rendered
    /// block while engaged; the UI displays it during the glide and uses
    /// it as the re-grab base.
    voice_frame: AtomicF64,
    /// Predicted settle landing frame, published by the callback when a
    /// glide starts.
    landing: AtomicF64,
    /// Bumped with each published landing; the worker consumes each sequence
    /// number exactly once to prepare the session landing.
    landing_seq: AtomicU64,
}

impl ScrubState {
    pub fn new() -> Self {
        Self {
            phase: AtomicU8::new(ScrubPhase::Idle as u8),
            target_frame: AtomicF64::new(0.0),
            lead_in_frames: AtomicF64::new(0.0),
            settle_rate_target: AtomicF64::new(0.0),
            voice_frame: AtomicF64::new(0.0),
            landing: AtomicF64::new(0.0),
            landing_seq: AtomicU64::new(0),
        }
    }

    pub fn phase(&self) -> ScrubPhase {
        match self.phase.load(Ordering::Acquire) {
            1 => ScrubPhase::Active,
            2 => ScrubPhase::Settling,
            _ => ScrubPhase::Idle,
        }
    }

    /// Engage the scrub at `frame` (the playhead where the drag started, or
    /// the gliding voice position on a mid-settle re-grab). The target and
    /// lead-in are published before the phase so the audio callback never
    /// sees stale values on engage.
    pub fn begin(&self, frame: f64, lead_in_frames: f64) {
        self.lead_in_frames.store(lead_in_frames);
        self.target_frame.store(frame);
        self.voice_frame.store(frame);
        self.phase
            .store(ScrubPhase::Active as u8, Ordering::Release);
    }

    /// Elastic lead-in depth for the current gesture (source frames).
    pub fn lead_in(&self) -> f64 {
        self.lead_in_frames.load()
    }

    pub fn update_target(&self, frame: f64) {
        self.target_frame.store(frame);
    }

    /// Release the drag into a momentum glide easing toward `rate_target`.
    pub fn release(&self, rate_target: f64) {
        self.settle_rate_target.store(rate_target);
        self.phase
            .store(ScrubPhase::Settling as u8, Ordering::Release);
    }

    /// Abort the gesture without a glide (no audio stream to render it).
    pub fn cancel(&self) {
        self.phase.store(ScrubPhase::Idle as u8, Ordering::Release);
    }

    /// Callback-side: the glide reached its landing; hand back to the
    /// engine. CAS so a simultaneous re-grab (`begin` on the UI thread)
    /// wins over the completion.
    pub fn finish_settle(&self) {
        let _ = self.phase.compare_exchange(
            ScrubPhase::Settling as u8,
            ScrubPhase::Idle as u8,
            Ordering::AcqRel,
            Ordering::Relaxed,
        );
    }

    pub fn target(&self) -> f64 {
        self.target_frame.load()
    }

    pub fn settle_rate_target(&self) -> f64 {
        self.settle_rate_target.load()
    }

    pub fn publish_voice_frame(&self, frame: f64) {
        self.voice_frame.store(frame);
    }

    pub fn voice_frame(&self) -> f64 {
        self.voice_frame.load()
    }

    /// Callback-side: publish the predicted glide landing. The frame is
    /// stored before the sequence bump so a consumer that sees the new
    /// sequence reads the matching landing.
    pub fn publish_landing(&self, frame: f64) {
        self.landing.store(frame);
        self.landing_seq.fetch_add(1, Ordering::Release);
    }

    /// `(sequence, landing frame)` of the most recent glide, for the UI to
    /// consume once per sequence.
    pub fn landing(&self) -> (u64, f64) {
        let seq = self.landing_seq.load(Ordering::Acquire);
        (seq, self.landing.load())
    }
}

/// Mixer state shared between the UI and the audio callback.
pub struct MixerShared {
    /// Callback-owned effective master; UI requests changes separately.
    pub effective_master: AtomicU8,
    pub master_request: AtomicU64,
    pub output_frame: AtomicU64,
    /// Crossfader position 0..1: 0 = full deck A, 1 = full deck B.
    pub crossfader: AtomicF32,
    /// Master volume, linear.
    pub master: AtomicF32,
    /// Master output level (linear peak, 0..1), measured post-master and
    /// post-limiter — what actually leaves the stream. Published by the
    /// audio callback; fast attack, slow release.
    pub master_meter: AtomicF32,
    /// Audio-callback load: processing time ÷ buffer duration (EMA),
    /// published by the callback.
    pub cpu_load: AtomicF32,
}

impl MixerShared {
    pub fn new() -> Self {
        Self {
            effective_master: AtomicU8::new(0),
            master_request: AtomicU64::new(0),
            output_frame: AtomicU64::new(0),
            crossfader: AtomicF32::new(0.5),
            master: AtomicF32::new(1.0),
            master_meter: AtomicF32::new(0.0),
            cpu_load: AtomicF32::new(0.0),
        }
    }
}

/// Flag for signaling a render worker to stop.
pub struct StopFlag(AtomicBool);

impl StopFlag {
    pub fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    pub fn set(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_snapshots_never_mix_adjacent_callback_boundaries() {
        let snapshot = std::sync::Arc::new(ClockSnapshot::new());
        let writer = snapshot.clone();
        let worker = std::thread::spawn(move || {
            for frame in 1..10_000 {
                writer.publish(DeliveryClock {
                    output_frame: frame,
                    position: frame as f64 * 0.5,
                    iteration: frame / 100,
                    track: 7,
                    anchor: 11,
                });
            }
        });
        while !worker.is_finished() {
            if let Some(clock) = snapshot.read() {
                assert_eq!(clock.position, clock.output_frame as f64 * 0.5);
                assert_eq!(clock.iteration, clock.output_frame / 100);
                assert_eq!((clock.track, clock.anchor), (7, 11));
            }
        }
        worker.join().unwrap();
        assert_eq!(snapshot.read().unwrap().output_frame, 9999);
    }

    #[test]
    fn anchor_batches_keep_the_newest_seek_and_reject_delayed_commands() {
        let shared = DeckShared::new();
        shared.request_anchor(123.75);
        shared.set_loop(Some((0, 1000)));
        let command = shared.take_anchor(0).unwrap();
        assert_eq!(command.frame, Some(123.75));
        assert_eq!(command.region, Some((0, 1000)));
        assert_eq!(
            command.revision,
            shared.anchor_revision.load(Ordering::Acquire)
        );
        shared
            .anchor_commands
            .push(AnchorCommand {
                revision: 1,
                frame: Some(9.0),
                region: None,
            })
            .ok()
            .unwrap();
        shared.set_loop(Some((0, 2000)));
        let next = shared.take_anchor(command.revision).unwrap();
        assert_eq!(
            next.frame, None,
            "delayed seek must not be replayed during a newer loop edit"
        );
        assert_eq!(next.region, Some((0, 2000)));
        for frame in 0..100 {
            shared.request_seek(frame);
        }
        let latest = shared.take_anchor(next.revision).unwrap();
        assert_eq!(latest.frame, Some(99.0));
        assert_eq!(
            latest.revision,
            shared.anchor_revision.load(Ordering::Acquire)
        );
    }

    #[test]
    fn transport_intent_and_generation_are_published_together() {
        let shared = DeckShared::new();
        shared.set_transport(Transport::Playing);
        let playing = shared.transport_revision();
        shared.set_transport(Transport::Playing);
        assert_eq!(shared.transport_revision(), playing);
        shared.set_transport(Transport::Paused);
        shared.set_transport(Transport::Playing);
        assert_eq!(shared.transport_revision(), playing + 2);
        assert_eq!(shared.transport(), Transport::Playing);
    }
}
