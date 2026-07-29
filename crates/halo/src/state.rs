//! Lock-free state shared between the UI thread, the per-deck feed threads,
//! and the audio callback. Everything here is atomics — the audio path never
//! takes a lock it can block on.

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

/// Sentinel meaning "no seek requested".
const NO_SEEK: u64 = u64::MAX;

/// Per-deck state shared across the UI, feed thread, and audio callback.
pub struct DeckShared {
    transport: AtomicU8,
    /// Playhead in source frames, published by the feed thread.
    pub playhead: AtomicU64,
    /// Total source frames of the loaded track.
    pub total_frames: AtomicU64,
    /// Cue point in source frames.
    pub cue_point: AtomicU64,
    /// Seek request in source frames (NO_SEEK = none). UI writes, feed
    /// thread consumes.
    seek_request: AtomicU64,
    /// True once the feed thread has prerolled the engine's source ring —
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
    /// Isolator EQ band gains, linear 0..2 (0 = kill, 1 = unity).
    pub eq_low: AtomicF32,
    pub eq_mid: AtomicF32,
    pub eq_high: AtomicF32,
    /// Filter mode as u8 (see `dsp::FilterMode::from_u8`).
    pub filter_mode: AtomicU8,
    /// Normalized filter cutoff 0..1 (log-mapped 20 Hz → 20 kHz).
    pub filter_cutoff: AtomicF32,
    /// Engine tempo rate (playback speed; 1.0 = original). Written by the
    /// UI's tempo logic, forwarded to the engine by the feed thread.
    pub tempo_rate: AtomicF32,
    /// Keylock: pitch stays constant while tempo changes (Tape mode when
    /// off — pitch follows tempo).
    pub keylock: AtomicBool,
    /// Active loop region packed as `start << 32 | end` (frames, u32 each)
    /// so the feed thread can never read a torn start/end pair.
    /// `u64::MAX` = no loop.
    loop_region: AtomicU64,
    /// Audible-scrub handshake between the UI, feed thread, and audio
    /// callback.
    pub scrub: ScrubState,
}

impl DeckShared {
    pub fn new() -> Self {
        Self {
            transport: AtomicU8::new(Transport::Stopped as u8),
            playhead: AtomicU64::new(0),
            total_frames: AtomicU64::new(0),
            cue_point: AtomicU64::new(0),
            seek_request: AtomicU64::new(NO_SEEK),
            stream_active: AtomicBool::new(false),
            trim: AtomicF32::new(1.0),
            fader: AtomicF32::new(1.0),
            meter: AtomicF32::new(0.0),
            eq_low: AtomicF32::new(1.0),
            eq_mid: AtomicF32::new(1.0),
            eq_high: AtomicF32::new(1.0),
            filter_mode: AtomicU8::new(0),
            filter_cutoff: AtomicF32::new(1.0),
            tempo_rate: AtomicF32::new(1.0),
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
        self.loop_region.store(packed, Ordering::Relaxed);
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
        match self.transport.load(Ordering::Relaxed) {
            1 => Transport::Playing,
            2 => Transport::Paused,
            _ => Transport::Stopped,
        }
    }

    pub fn set_transport(&self, t: Transport) {
        self.transport.store(t as u8, Ordering::Relaxed);
    }

    pub fn request_seek(&self, frame: usize) {
        self.seek_request.store(frame as u64, Ordering::Relaxed);
    }

    pub fn take_seek(&self) -> Option<usize> {
        let v = self.seek_request.swap(NO_SEEK, Ordering::Relaxed);
        (v != NO_SEEK).then_some(v as usize)
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
/// post-release momentum glide. The feed thread yields the playhead while
/// any phase is engaged and additionally stops feeding while `Active`.
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
    /// Bumped with each published landing; the UI consumes each sequence
    /// number exactly once to fire the engine warm-start seek.
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
            crossfader: AtomicF32::new(0.5),
            master: AtomicF32::new(1.0),
            master_meter: AtomicF32::new(0.0),
            cpu_load: AtomicF32::new(0.0),
        }
    }
}

/// Flag for signaling a feed thread to stop.
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
