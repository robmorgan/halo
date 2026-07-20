//! Per-deck engine ownership and the feed/control thread.
//!
//! The audio callback owns each deck's
//! [`EngineProcessor`](timestretch::engine::EngineProcessor) (handed over
//! through a lock-free slot); this thread keeps the engine's source ring
//! topped up and publishes the playhead. Seeks feed the preroll preceding
//! the target and request warm-start priming, exactly as in the timestretch
//! desktop reference deck.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use timestretch::PreAnalysisArtifact;
use timestretch::engine::{
    Engine, EngineConfig, EngineController, EngineProcessor, EngineProfile, SourceProducer,
};

use crate::state::{DeckShared, ScrubPhase, StopFlag, Transport};

pub const CHANNELS: usize = 2;
/// Interleaved samples pushed per feed batch.
const FEED_BATCH_SAMPLES: usize = 2048 * CHANNELS;
/// Ring occupancy (frames) the feeder tops up to.
const TARGET_OCCUPANCY_FRAMES: usize = 16_384;
/// Occupancy (frames) required before output unmutes after start/seek.
const PREROLL_FRAMES: usize = 4_096;

/// Hand-off slot for moving an `EngineProcessor` into the audio callback.
/// The callback `try_lock`s it each block and adopts any waiting processor.
pub type ProcessorSlot = Arc<Mutex<Option<EngineProcessor>>>;

/// Hand-off slot for the raw interleaved track samples the audio callback
/// reads directly during a scratch (bypassing the engine). Same discipline
/// as [`ProcessorSlot`]: the callback only ever `try_lock`s.
pub type SampleSlot = Arc<Mutex<Option<Arc<Vec<f32>>>>>;

/// One source-timeline discontinuity: at cumulative consumed frame
/// `anchor`, playback continued from source frame `target`.
#[derive(Debug, Clone, Copy)]
struct Jump {
    anchor: f64,
    target: f64,
}

/// Maps the engine's cumulative consumed-source position to an absolute
/// source frame across feed-cursor jumps (loop wraps, seeks). Ported from
/// the timestretch desktop reference deck.
#[derive(Debug)]
struct JumpMap {
    jumps: Vec<Jump>,
}

impl JumpMap {
    fn starting_at(source_frame: f64) -> Self {
        Self {
            jumps: vec![Jump {
                anchor: 0.0,
                target: source_frame,
            }],
        }
    }

    fn record(&mut self, anchor: f64, target: f64) {
        self.jumps.push(Jump { anchor, target });
    }

    fn map(&self, cumulative: f64) -> f64 {
        let jump = self
            .jumps
            .iter()
            .rev()
            .find(|j| j.anchor <= cumulative)
            .or(self.jumps.first())
            .copied()
            .unwrap_or(Jump {
                anchor: 0.0,
                target: 0.0,
            });
        jump.target + (cumulative - jump.anchor)
    }

    /// Drops jumps well behind the playhead, keeping one anchor.
    fn prune(&mut self, cumulative: f64) {
        while self.jumps.len() >= 2 && self.jumps[1].anchor <= cumulative {
            self.jumps.remove(0);
        }
    }
}

pub struct Track {
    #[allow(dead_code)] // browser/library metadata from Phase 6
    pub name: String,
    /// Kept for waveform rendering and analysis from Phase 2 on.
    #[allow(dead_code)]
    pub samples: Arc<Vec<f32>>,
    #[allow(dead_code)]
    pub num_frames: usize,
}

/// UI-side deck object. Owns the feed thread and the engine controller;
/// the processor lives in the audio callback.
pub struct Deck {
    pub shared: Arc<DeckShared>,
    pub processor_slot: ProcessorSlot,
    /// Old processors handed back by the callback so they drop off the
    /// audio thread.
    pub retired_slot: ProcessorSlot,
    /// Current track samples for the callback's scratch reader.
    pub scratch_source: SampleSlot,
    /// Old sample Arcs handed back by the callback so the track buffer
    /// never deallocates on the audio thread.
    pub scratch_retired: SampleSlot,
    pub reset_request: Arc<AtomicBool>,
    pub track: Option<Track>,
    /// Offline analysis artifact for the loaded track, once it has landed
    /// (steers the engine's transient handling at non-unity tempo).
    pub pre_analysis: Option<Arc<PreAnalysisArtifact>>,
    feed_stop: Option<Arc<StopFlag>>,
    feed_handle: Option<thread::JoinHandle<()>>,
}

impl Deck {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(DeckShared::new()),
            processor_slot: Arc::new(Mutex::new(None)),
            retired_slot: Arc::new(Mutex::new(None)),
            scratch_source: Arc::new(Mutex::new(None)),
            scratch_retired: Arc::new(Mutex::new(None)),
            reset_request: Arc::new(AtomicBool::new(false)),
            track: None,
            pre_analysis: None,
            feed_stop: None,
            feed_handle: None,
        }
    }

    /// Load a track (already decoded and resampled to the device rate),
    /// with its analysis artifact (already rescaled to the device rate)
    /// when the library has one. Builds a fresh engine, hands its processor
    /// to the audio callback, and starts the feed thread.
    pub fn load(
        &mut self,
        name: String,
        samples: Arc<Vec<f32>>,
        device_sample_rate: u32,
        pre_analysis: Option<Arc<PreAnalysisArtifact>>,
    ) -> Result<(), String> {
        let num_frames = samples.len() / CHANNELS;
        self.shared.set_transport(Transport::Stopped);
        self.shared.playhead.store(0, Ordering::Relaxed);
        self.shared.cue_point.store(0, Ordering::Relaxed);
        self.shared.set_loop(None);
        self.shared
            .total_frames
            .store(num_frames as u64, Ordering::Relaxed);
        // End any in-flight scrub and hand the new samples to the
        // callback; drain retired Arcs here on the UI thread.
        self.shared.scrub.cancel();
        *self.scratch_source.lock().unwrap() = Some(samples.clone());
        self.scratch_retired.lock().unwrap().take();

        self.pre_analysis = pre_analysis;
        self.start_engine(samples.clone(), device_sample_rate)?;

        self.track = Some(Track {
            name,
            samples,
            num_frames,
        });
        Ok(())
    }

    /// Rebuild the engine with the freshly landed analysis artifact,
    /// preserving playhead and cue. Only call while the deck is not
    /// playing — the rebuild swaps the processor out from under the
    /// callback.
    pub fn apply_pre_analysis(
        &mut self,
        artifact: Arc<PreAnalysisArtifact>,
        device_sample_rate: u32,
    ) -> Result<(), String> {
        let Some(track) = &self.track else {
            return Ok(());
        };
        let samples = track.samples.clone();
        let playhead = self.shared.playhead_frames();
        self.pre_analysis = Some(artifact);
        self.start_engine(samples, device_sample_rate)?;
        if playhead > 0 {
            self.shared.request_seek(playhead);
        }
        Ok(())
    }

    /// (Re)build the engine and feed thread for `samples`, using the deck's
    /// current `pre_analysis` if any.
    fn start_engine(
        &mut self,
        samples: Arc<Vec<f32>>,
        device_sample_rate: u32,
    ) -> Result<(), String> {
        self.stop_feed_thread();
        // Drop any processor retired by a previous engine.
        *self.retired_slot.lock().unwrap() = None;
        self.shared.stream_active.store(false, Ordering::Relaxed);
        self.shared.take_seek();

        let config = EngineConfig {
            sample_rate: device_sample_rate,
            channels: 2,
            profile: EngineProfile::Keylock,
            initial_tempo_rate: 1.0,
            max_block_frames: 2048,
            source_capacity_frames: 65_536,
            pre_analysis: self.pre_analysis.clone(),
        };
        let handles = Engine::build(config).map_err(|e| format!("Engine error: {e}"))?;
        let warm_start_preroll = handles.processor.warm_start_preroll_frames();

        self.reset_request.store(false, Ordering::Relaxed);
        *self.processor_slot.lock().unwrap() = Some(handles.processor);

        let stop_flag = Arc::new(StopFlag::new());
        let handle = start_feed_thread(
            self.shared.clone(),
            samples,
            handles.source,
            handles.controller,
            self.reset_request.clone(),
            stop_flag.clone(),
            warm_start_preroll,
        );
        self.feed_stop = Some(stop_flag);
        self.feed_handle = Some(handle);
        Ok(())
    }

    fn stop_feed_thread(&mut self) {
        if let Some(flag) = self.feed_stop.take() {
            flag.set();
        }
        if let Some(handle) = self.feed_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Deck {
    fn drop(&mut self) {
        self.stop_feed_thread();
    }
}

/// Feed/control thread: tops up the engine's source ring, executes
/// warm-start seeks, publishes the playhead, and gates `stream_active`.
#[allow(clippy::too_many_arguments)]
fn start_feed_thread(
    shared: Arc<DeckShared>,
    source_audio: Arc<Vec<f32>>,
    mut source: SourceProducer,
    controller: EngineController,
    reset_request: Arc<AtomicBool>,
    stop_flag: Arc<StopFlag>,
    warm_start_preroll: usize,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let total_frames = source_audio.len() / CHANNELS;
        // Interleaved read offset into the source.
        let mut cursor: usize = 0;
        // Maps consumed-source position to absolute source frames across
        // seeks and loop wraps.
        let mut jumps = JumpMap::starting_at(0.0);
        // Frames fed to the engine since the last reset (jump anchors).
        let mut fed_frames: f64 = 0.0;
        let mut finished = false;
        let mut prerolled = false;
        let mut last_underruns = 0u64;
        let mut last_rate = f64::NAN;
        let mut last_keylock: Option<bool> = None;

        shared.stream_active.store(false, Ordering::Relaxed);
        // Anchor the artifact timeline: the first pushed frame is track 0.
        source.set_track_position(0);

        loop {
            if stop_flag.is_set() {
                break;
            }

            // Forward tempo and keylock on change. The engine's mailbox is
            // wait-free and clamps values; the epsilon just avoids spamming
            // identical events every 2 ms.
            let rate = (shared.tempo_rate.load() as f64).clamp(0.25, 4.0);
            if last_rate.is_nan() || (rate - last_rate).abs() > 1e-6 {
                controller.set_tempo_rate(rate);
                last_rate = rate;
            }
            let keylock = shared.keylock.load(Ordering::Relaxed);
            if last_keylock != Some(keylock) {
                controller.set_keylock(keylock);
                last_keylock = Some(keylock);
            }

            if let Some(seek_frame) = shared.take_seek() {
                // Warm-start seek: mute, have the audio callback reset the
                // engine (which discards in-flight source), then feed the
                // preroll PRECEDING the target and request priming — the
                // graph runs the history through and resumes converged.
                shared.stream_active.store(false, Ordering::Relaxed);
                prerolled = false;
                reset_request.store(true, Ordering::Release);
                let mut spins = 0;
                while reset_request.load(Ordering::Acquire) && spins < 500 {
                    thread::sleep(Duration::from_millis(1));
                    spins += 1;
                }
                let target = seek_frame.min(total_frames);
                let preroll = warm_start_preroll.min(target);
                let feed_from = target - preroll;
                cursor = feed_from * CHANNELS;
                fed_frames = 0.0;
                jumps = JumpMap::starting_at(feed_from as f64);
                source.set_track_position(feed_from as u64);
                controller.warm_start(preroll as u32);
                finished = false;
                shared.playhead.store(target as u64, Ordering::Relaxed);
            }

            if shared.transport() != Transport::Playing {
                shared.stream_active.store(false, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(10));
                continue;
            }

            // Audible scrub: while the pointer holds the platter (`Active`)
            // the audio callback plays its own varispeed voice and leaves
            // the engine unconsumed, and the UI owns the displayed position
            // — don't feed, don't publish a stale engine playhead, don't
            // drive EOF logic. During the release glide (`Settling`) the
            // loop must keep running so the landing seek (handled above)
            // resets, feeds preroll, and primes the engine in parallel with
            // the glide — only the playhead publish stays yielded.
            let scrub_phase = shared.scrub.phase();
            if scrub_phase == ScrubPhase::Active {
                thread::sleep(Duration::from_millis(10));
                continue;
            }

            // Loop wrap: jump the feed cursor and re-anchor the timeline.
            // The engine streams straight across the seam — no reset.
            let loop_region = shared.loop_region();
            if let Some((loop_start, loop_end)) = loop_region
                && cursor >= loop_end * CHANNELS
            {
                cursor = loop_start * CHANNELS;
                jumps.record(fed_frames, loop_start as f64);
                source.set_track_position(loop_start as u64);
                finished = false;
            }

            // End of stream: flush the resampler lookahead once, then stop
            // the transport when the buffered tail has drained.
            if cursor >= source_audio.len() && loop_region.is_none() {
                if !finished {
                    finished = source.finish();
                } else if source.occupied_frames() == 0 {
                    thread::sleep(Duration::from_millis(100));
                    shared.stream_active.store(false, Ordering::Relaxed);
                    shared.set_transport(Transport::Stopped);
                    shared
                        .playhead
                        .store(total_frames as u64, Ordering::Relaxed);
                    continue;
                }
            } else if source.occupied_frames() < TARGET_OCCUPANCY_FRAMES {
                // Top up the ring, clamping each batch to the loop end (the
                // wrap above fires on the next iteration) and to EOF.
                let mut end = (cursor + FEED_BATCH_SAMPLES).min(source_audio.len());
                if let Some((_, loop_end)) = loop_region {
                    let loop_end = loop_end * CHANNELS;
                    if cursor < loop_end {
                        end = end.min(loop_end);
                    }
                }
                if end > cursor {
                    let accepted = source.push(&source_audio[cursor..end]);
                    cursor += accepted * CHANNELS;
                    fed_frames += accepted as f64;
                }
            }

            if !prerolled
                && (source.occupied_frames() >= PREROLL_FRAMES || cursor >= source_audio.len())
            {
                prerolled = true;
            }
            shared.stream_active.store(prerolled, Ordering::Relaxed);

            // Playhead: map the engine's cumulative consumed-source position
            // through the jump timeline to an absolute source frame. The
            // glide display belongs to the scrub voice, so don't fight it.
            let consumed = controller.source_position();
            jumps.prune(consumed);
            let playhead = jumps.map(consumed).clamp(0.0, total_frames as f64);
            if scrub_phase == ScrubPhase::Idle {
                shared.playhead.store(playhead as u64, Ordering::Relaxed);
            }

            let underruns = controller.underrun_frames();
            if underruns > last_underruns && !finished {
                log::warn!(
                    "deck: {} underrun frames (total {underruns})",
                    underruns - last_underruns
                );
                last_underruns = underruns;
            }

            thread::sleep(Duration::from_millis(2));
        }

        shared.stream_active.store(false, Ordering::Relaxed);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jump_map_identity_without_jumps() {
        let map = JumpMap::starting_at(0.0);
        assert_eq!(map.map(0.0), 0.0);
        assert_eq!(map.map(1234.5), 1234.5);
    }

    #[test]
    fn jump_map_seek_start_offsets_position() {
        let map = JumpMap::starting_at(44_100.0);
        assert_eq!(map.map(0.0), 44_100.0);
        assert_eq!(map.map(100.0), 44_200.0);
    }

    #[test]
    fn jump_map_loop_wrap_re_anchors() {
        // Fed 1000 frames, then wrapped back to source frame 200.
        let mut map = JumpMap::starting_at(0.0);
        map.record(1000.0, 200.0);
        assert_eq!(map.map(999.0), 999.0); // pre-wrap audio still playing
        assert_eq!(map.map(1000.0), 200.0); // seam
        assert_eq!(map.map(1300.0), 500.0); // inside the loop
    }

    #[test]
    fn jump_map_prune_keeps_active_anchor() {
        let mut map = JumpMap::starting_at(0.0);
        map.record(1000.0, 200.0);
        map.record(2000.0, 200.0);
        map.prune(2500.0);
        assert_eq!(map.map(2500.0), 700.0);
    }
}
