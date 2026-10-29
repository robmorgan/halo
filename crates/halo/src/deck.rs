//! Per-deck worker ownership. The session owns media semantics; Halo owns
//! scheduling, output reserve and delivery receipts. No session enters cpal.

use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use timestretch::{
    Parameters, PreAnalysisArtifact, PreparedSession, SessionConfig, SessionError, SourceAccess,
    StreamSession, TempoRate,
};

use crate::delivery::{DeliveryHandoff, Playback};
use crate::output_queue::{self, Producer};
use crate::render_worker::{fill_reserve, output_reserve_frames};
use crate::state::{DeckShared, ScrubPhase, StopFlag, Transport};
use crate::worker_priority::AudioPriority;

pub const CHANNELS: usize = 2;

pub struct Track {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub samples: Arc<Vec<f32>>,
    #[allow(dead_code)]
    pub num_frames: usize,
}

pub struct Deck {
    pub shared: Arc<DeckShared>,
    pub handoff: Arc<DeliveryHandoff>,
    pub track: Option<Track>,
    /// Display/library metadata only; never passed into DSP.
    pub pre_analysis: Option<Arc<PreAnalysisArtifact>>,
    worker_stop: Option<Arc<StopFlag>>,
    worker_handle: Option<thread::JoinHandle<()>>,
    errors: Option<mpsc::Receiver<String>>,
}

impl Deck {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(DeckShared::new()),
            handoff: Arc::new(DeliveryHandoff::new()),
            track: None,
            pre_analysis: None,
            worker_stop: None,
            worker_handle: None,
            errors: None,
        }
    }

    /// Reclaim retired callback storage and report worker failures on the UI.
    pub fn poll(&mut self) -> Option<String> {
        self.handoff.reclaim();
        self.errors.as_ref().and_then(|rx| rx.try_recv().ok())
    }

    pub fn load(
        &mut self,
        name: String,
        samples: Arc<Vec<f32>>,
        device_sample_rate: u32,
        pre_analysis: Option<Arc<PreAnalysisArtifact>>,
    ) -> Result<(), String> {
        let num_frames = samples.len() / CHANNELS;
        if !samples.len().is_multiple_of(CHANNELS)
            || !(32_000..=192_000).contains(&device_sample_rate)
        {
            return Err("Unsupported playback format".into());
        }
        self.stop_worker();
        self.handoff.reclaim();
        self.shared.set_transport(Transport::Stopped);
        self.shared.stream_active.store(false, Ordering::Release);
        self.shared.worker_failed.store(false, Ordering::Relaxed);
        self.shared.take_seek();
        self.shared.playhead.store(0, Ordering::Relaxed);
        self.shared.cue_point.store(0, Ordering::Relaxed);
        self.shared.set_loop(None);
        self.shared
            .total_frames
            .store(num_frames as u64, Ordering::Relaxed);
        self.shared.scrub.cancel();
        self.shared.scrub_ready.store(0, Ordering::Relaxed);
        self.shared.underrun_frames.store(0, Ordering::Relaxed);
        let generation = self.shared.track_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let reserve = output_reserve_frames(device_sample_rate);
        let (producer, consumer) = output_queue::channel((reserve * 4).max(8192));
        self.handoff.publish(Playback {
            consumer,
            samples: samples.clone(),
            generation,
        });
        let stop = Arc::new(StopFlag::new());
        let shared = self.shared.clone();
        let source = samples.clone();
        let worker_stop = stop.clone();
        let (tx, rx) = mpsc::channel();
        let landing_seen = self.shared.scrub.landing().0;
        self.worker_handle = Some(thread::spawn(move || {
            let result = run_worker(
                &shared,
                &source,
                producer,
                &worker_stop,
                device_sample_rate,
                landing_seen,
            );
            shared.stream_active.store(false, Ordering::Release);
            shared.worker_load.store(0.0);
            if let Err(error) = result {
                shared.worker_failed.store(true, Ordering::Release);
                shared.set_transport(Transport::Stopped);
                let _ = tx.send(format!("Playback error: {error}"));
            }
        }));
        self.errors = Some(rx);
        self.worker_stop = Some(stop);
        self.pre_analysis = pre_analysis;
        self.track = Some(Track {
            name,
            samples,
            num_frames,
        });
        Ok(())
    }

    fn stop_worker(&mut self) {
        if let Some(stop) = self.worker_stop.take() {
            stop.set();
        }
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Deck {
    fn drop(&mut self) {
        self.stop_worker();
    }
}

/// Clamp host intent using the public joint parameter limits. Zero speed is
/// handled separately as a transport pause, never submitted as a DSP ratio.
pub fn playback_parameters(rate: f64, keylock: bool) -> Result<Parameters, SessionError> {
    let min = if keylock {
        TempoRate::MIN.get()
    } else {
        timestretch::PitchRatio::MIN.get()
    };
    let tempo = TempoRate::new(rate.clamp(min, TempoRate::MAX.get()))?;
    if keylock {
        Ok(Parameters::keylock(tempo))
    } else {
        Parameters::linked(tempo)
    }
}

/// Track only one outstanding control. Continuous UI edits coalesce until its
/// acknowledgement, so a paused deck cannot overflow the session command queue.
struct Controls {
    submitted: Parameters,
    pending: bool,
    next_id: u64,
}

impl Controls {
    fn update(
        &mut self,
        session: &mut StreamSession,
        parameters: Parameters,
        epoch_changed: bool,
    ) -> Result<(), SessionError> {
        while session.next_applied().is_some() {
            self.pending = false;
        }
        if epoch_changed {
            self.pending = false;
        }
        if !self.pending && (parameters != self.submitted || epoch_changed) {
            self.next_id += 1;
            session.schedule(self.next_id, session.commit_horizon(), parameters)?;
            self.submitted = parameters;
            self.pending = true;
        }
        Ok(())
    }
}

fn run_worker(
    shared: &DeckShared,
    source: &[f32],
    mut output: Producer,
    stop: &StopFlag,
    sample_rate: u32,
    mut landing_seen: u64,
) -> Result<(), SessionError> {
    let parameters = playback_parameters(
        shared.tempo_rate.load() as f64,
        shared.keylock.load(Ordering::Relaxed),
    )?;
    let mut session = PreparedSession::new(SessionConfig {
        source_sample_rate: sample_rate,
        output_sample_rate: sample_rate,
        source_frames: Some((source.len() / CHANNELS) as u64),
        source_access: SourceAccess::Seekable,
        parameters,
        ..Default::default()
    })?
    .activate();
    let reserve = output_reserve_frames(sample_rate);
    shared
        .queue_latency
        .store(reserve as f64 / sample_rate as f64);
    let mut controls = Controls {
        submitted: parameters,
        pending: false,
        next_id: 0,
    };
    let mut scratch = vec![0.0; 4096];
    let mut primed = false;
    let mut priority = None;
    let mut priority_requested = false;
    let mut landing_pending = None;
    let mut last_tick = Instant::now();
    output.publish_epoch(session.epoch());
    while !stop.is_set() {
        let tick = Instant::now();
        if let Some(receipt) = output.receipt() {
            session.acknowledge(receipt)?;
        }
        let previous_epoch = session.epoch();
        let mut region = shared.loop_region().map(|(a, b)| (a as u64, b as u64));
        let mut seek = shared
            .take_seek()
            .map(|n| n.min(source.len() / CHANNELS) as u64);
        let (sequence, landing) = shared.scrub.landing();
        if sequence != landing_seen {
            landing_seen = sequence;
            if shared.scrub.phase() == ScrubPhase::Settling {
                seek = Some((landing.max(0.0) as u64).min((source.len() / CHANNELS) as u64));
                landing_pending = Some(sequence);
            }
        }
        if seek.is_some_and(|target| {
            region.is_some_and(|(start, end)| target < start || target >= end)
        }) {
            region = None;
            shared.set_loop(None);
        }
        if region != session.loop_region() || seek.is_some() {
            shared.stream_active.store(false, Ordering::Release);
            // Restart is heavyweight and must run under ordinary worker policy.
            drop(priority.take());
            priority_requested = false;
            if region != session.loop_region() {
                session.set_loop(region)?;
            }
            if let Some(target) = seek {
                session.seek(target)?;
            }
        }
        let epoch_changed = session.epoch() != previous_epoch;
        if epoch_changed {
            output.publish_epoch(session.epoch());
            primed = false;
        }
        let requested = playback_parameters(
            shared.tempo_rate.load() as f64,
            shared.keylock.load(Ordering::Relaxed),
        )?;
        controls.update(&mut session, requested, epoch_changed)?;
        let playing = shared.transport() == Transport::Playing
            && !shared.tempo_stopped.load(Ordering::Relaxed);
        let scrubbing = shared.scrub.phase();
        // Prepare a paused scrub's landing too, but never deliver it until play.
        if scrubbing == ScrubPhase::Active {
            landing_pending = None;
        }
        let should_render =
            (playing && scrubbing != ScrubPhase::Active) || landing_pending.is_some();
        session.set_transport(if should_render {
            timestretch::Transport::Playing
        } else {
            timestretch::Transport::Paused
        });
        if should_render {
            if !priority_requested {
                priority = match AudioPriority::new(sample_rate, reserve) {
                    Ok(guard) => {
                        log::debug!("Deck worker scheduling: {}", AudioPriority::label());
                        Some(guard)
                    }
                    Err(error) => {
                        log::warn!("Deck worker priority unavailable: {error}");
                        None
                    }
                };
                priority_requested = true;
            }
            fill_reserve(&mut session, source, &mut output, &mut scratch, reserve, 8)?;
            if output.occupied_frames() >= reserve || session.rendered_finished() {
                primed = true;
            }
        } else {
            drop(priority.take());
            priority_requested = false;
        }
        shared.stream_active.store(
            playing && primed && scrubbing != ScrubPhase::Active && !session.finished(),
            Ordering::Release,
        );
        if scrubbing == ScrubPhase::Idle {
            shared
                .playhead
                .store(session.position() as u64, Ordering::Relaxed);
        } else {
            shared.playhead.store(
                shared.scrub.voice_frame().max(0.0) as u64,
                Ordering::Relaxed,
            );
        }
        if session.finished() && scrubbing == ScrubPhase::Idle {
            shared.set_transport(Transport::Stopped);
        }
        shared
            .achieved_tempo
            .store(session.applied_parameters().tempo());
        if primed && let Some(sequence) = landing_pending.take() {
            // Publish readiness after the delivery gate and achieved rate.
            shared.scrub_ready.store(sequence, Ordering::Release);
        }
        let timing = session.timing();
        shared.control_latency.store(
            (timing.control_quantum_frames
                + timing.pending_output_frames
                + timing.unacknowledged_output_frames) as f64
                / sample_rate as f64,
        );
        shared
            .underrun_frames
            .store(output.underrun_frames(), Ordering::Relaxed);
        let elapsed = tick.duration_since(last_tick).as_secs_f32();
        if elapsed > 0.0 {
            let load = tick.elapsed().as_secs_f32() / elapsed;
            shared
                .worker_load
                .store(shared.worker_load.load() * 0.9 + load * 0.1);
        }
        last_tick = tick;
        thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delivery::{DeckAudio, MixerRenderer};
    use crate::state::MixerShared;

    fn audio(decks: &[Deck; 3], rate: u32) -> MixerRenderer {
        MixerRenderer::new(
            std::array::from_fn(|i| DeckAudio {
                shared: decks[i].shared.clone(),
                handoff: decks[i].handoff.clone(),
            }),
            Arc::new(MixerShared::new()),
            rate,
        )
    }

    #[track_caller]
    fn wait(mut ready: impl FnMut() -> bool) {
        // Functional tests also run with the portable backend under emulation
        // and concurrent debug tests. Timing budgets belong to qualification.
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ready() {
            assert!(
                Instant::now() < deadline,
                "worker did not reach requested state"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn three_workers_track_consumption_pause_seek_loop_and_eof() {
        let rate = 48_000;
        let mut decks = std::array::from_fn(|_| Deck::new());
        for deck in &mut decks {
            let source = Arc::new(
                (0..24_000)
                    .flat_map(|i| {
                        let x = (i as f32 * 0.05).sin() * 0.1;
                        [x, x]
                    })
                    .collect(),
            );
            deck.load("test".into(), source, rate, None).unwrap();
            deck.shared.set_transport(Transport::Playing);
        }
        let mut renderer = audio(&decks, rate);
        wait(|| {
            decks
                .iter()
                .all(|d| d.shared.stream_active.load(Ordering::Acquire))
        });
        for deck in &decks {
            assert_eq!(
                deck.shared.playhead_frames(),
                0,
                "render-ahead advanced playback"
            );
        }
        let mut block = [0.0; 512];
        renderer.render(&mut block);
        wait(|| decks.iter().all(|d| d.shared.playhead_frames() > 0));
        for deck in &decks {
            deck.shared.set_transport(Transport::Paused);
        }
        thread::sleep(Duration::from_millis(10));
        let paused = decks.each_ref().map(|d| d.shared.playhead_frames());
        for _ in 0..4 {
            renderer.render(&mut block);
        }
        thread::sleep(Duration::from_millis(10));
        assert_eq!(paused, decks.each_ref().map(|d| d.shared.playhead_frames()));

        // Paused seeks must publish and discard the old queue without play.
        for deck in &decks {
            deck.shared.request_seek(4000);
        }
        wait(|| {
            renderer.render(&mut block);
            decks.iter().all(|d| d.shared.playhead_frames() == 4000)
        });
        for deck in &decks {
            deck.shared.set_loop(Some((4000, 6000)));
            deck.shared.set_transport(Transport::Playing);
        }
        let mut wrapped = false;
        let mut last = 4000;
        wait(|| {
            renderer.render(&mut block);
            let position = decks[0].shared.playhead_frames();
            wrapped |= position < last;
            last = position;
            wrapped
        });
        assert!((4000..6000).contains(&decks[0].shared.playhead_frames()));
        // Seeking outside the loop disables it before asking the library.
        for deck in &decks {
            deck.shared.request_seek(22_000);
        }
        wait(|| {
            renderer.render(&mut block);
            decks
                .iter()
                .all(|d| d.shared.transport() == Transport::Stopped)
        });
        for deck in &mut decks {
            assert_eq!(deck.shared.loop_region(), None);
            assert!(deck.shared.playhead_frames() >= 23_999);
            assert_eq!(deck.poll(), None);
            assert!(!deck.shared.worker_failed.load(Ordering::Acquire));
        }
    }

    #[test]
    fn paused_controls_coalesce_and_seek_reissues_the_latest_intent() {
        let mut session = PreparedSession::new(SessionConfig {
            source_frames: Some(48_000),
            source_access: SourceAccess::Seekable,
            max_commands: 1,
            ..Default::default()
        })
        .unwrap()
        .activate();
        session.set_transport(timestretch::Transport::Paused);
        let mut controls = Controls {
            submitted: Parameters::default(),
            pending: false,
            next_id: 0,
        };
        for i in 0..1000 {
            controls
                .update(
                    &mut session,
                    playback_parameters(0.8 + (i % 10) as f64 * 0.01, true).unwrap(),
                    false,
                )
                .unwrap();
        }
        assert_eq!(session.counters().rejected_commands, 0);
        session.seek(1000).unwrap();
        let latest = playback_parameters(1.25, false).unwrap();
        controls.update(&mut session, latest, true).unwrap();
        session.set_transport(timestretch::Transport::Playing);
        for _ in 0..100 {
            session.advance(1);
            if let Some(request) = session.source_request() {
                session
                    .supply(request, &vec![0.0; request.frames * 2])
                    .unwrap();
            }
            if session.applied_parameters().requested == latest {
                break;
            }
        }
        assert_eq!(session.applied_parameters().requested, latest);
    }

    #[test]
    fn scrub_release_primes_without_any_ui_polling() {
        let mut decks = std::array::from_fn(|_| Deck::new());
        decks[0]
            .load("scrub".into(), Arc::new(vec![0.1; 96_000]), 48_000, None)
            .unwrap();
        let mut renderer = audio(&decks, 48_000);
        let shared = decks[0].shared.clone();
        shared.scrub.begin(4000.0, 0.0);
        renderer.render(&mut [0.0; 1024]);
        shared.scrub.release(0.0);
        wait(|| {
            renderer.render(&mut [0.0; 1024]);
            shared.scrub.phase() == ScrubPhase::Idle
        });
        assert!(shared.scrub.landing().0 > 0);
        assert_eq!(
            shared.scrub.landing().0,
            shared.scrub_ready.load(Ordering::Acquire)
        );
        assert_eq!(shared.transport(), Transport::Stopped);
        assert!(!shared.worker_failed.load(Ordering::Acquire));
    }
    #[test]
    fn device_reload_uses_fresh_delivery_and_restores_position_at_new_rate() {
        let mut decks = std::array::from_fn(|_| Deck::new());
        for rate in [44_100, 96_000] {
            for deck in &mut decks {
                deck.load(
                    "device change".into(),
                    Arc::new(vec![0.1; rate as usize * 2]),
                    rate,
                    None,
                )
                .unwrap();
                deck.shared.request_seek(rate as usize / 4);
            }
            let mut renderer = audio(&decks, rate);
            wait(|| {
                renderer.render(&mut [0.0; 256]);
                decks
                    .iter()
                    .all(|d| d.shared.playhead_frames() == rate as usize / 4)
            });
            for deck in &decks {
                deck.shared.set_transport(Transport::Playing);
            }
            wait(|| {
                renderer.render(&mut [0.0; 256]);
                decks
                    .iter()
                    .all(|d| d.shared.playhead_frames() > rate as usize / 4)
            });
            for deck in &mut decks {
                assert_eq!(deck.poll(), None);
                deck.shared.set_transport(Transport::Paused);
            }
            // Dropping the stream/renderer occurs outside its render callback.
            drop(renderer);
        }
        for deck in &decks {
            assert_eq!(deck.shared.track_generation.load(Ordering::Acquire), 2);
        }
    }

    /// Host qualification, kept out of normal parallel CI because it measures
    /// wall-clock scheduling. No audio device or private music corpus required.
    #[test]
    #[ignore = "run alone in release mode to qualify host scheduling"]
    fn three_deck_realtime_qualification() {
        for rate in [44_100, 48_000, 96_000] {
            for frames in [128, 512, 2048] {
                let mut decks = std::array::from_fn(|_| Deck::new());
                let source: Arc<Vec<f32>> = Arc::new(
                    (0..rate * 4)
                        .flat_map(|i| {
                            let t = i as f32 / rate as f32;
                            let x = (t * 440.0 * std::f32::consts::TAU).sin() * 0.1
                                + (t * 61.0 * std::f32::consts::TAU).sin() * 0.1;
                            [x, x * 0.9]
                        })
                        .collect(),
                );
                for (i, deck) in decks.iter_mut().enumerate() {
                    deck.shared.tempo_rate.store([0.92, 1.08, 0.75][i]);
                    deck.load("qualification".into(), source.clone(), rate, None)
                        .unwrap();
                    deck.shared.set_transport(Transport::Playing);
                }
                let mut renderer = audio(&decks, rate);
                wait(|| {
                    decks
                        .iter()
                        .all(|d| d.shared.stream_active.load(Ordering::Acquire))
                });
                let analysis_stop = Arc::new(StopFlag::new());
                let analysis_flag = analysis_stop.clone();
                let analysis_source = source.clone();
                let analysis = thread::spawn(move || {
                    let mono = timestretch::downmix_to_mid(&analysis_source, 2);
                    while !analysis_flag.is_set() {
                        std::hint::black_box(timestretch::analyze(&mono, rate));
                    }
                });
                let period = Duration::from_secs_f64(frames as f64 / rate as f64);
                let blocks = (rate as usize).div_ceil(frames);
                let mut block = vec![0.0; frames * 2];
                let mut deadline = Instant::now();
                let mut max_callback = Duration::ZERO;
                for _ in 0..blocks {
                    let start = Instant::now();
                    renderer.render(&mut block);
                    max_callback = max_callback.max(start.elapsed());
                    assert!(block.iter().all(|x| x.is_finite()));
                    deadline += period;
                    if let Some(delay) = deadline.checked_duration_since(Instant::now()) {
                        thread::sleep(delay);
                    }
                }
                analysis_stop.set();
                analysis.join().unwrap();
                let underruns = decks
                    .each_ref()
                    .map(|d| d.shared.underrun_frames.load(Ordering::Relaxed));
                let loads = decks
                    .each_ref()
                    .map(|d| d.shared.worker_load.load() * 100.0);
                println!(
                    "rate={rate} block={frames} underruns={underruns:?} worker_pct={loads:?} max_callback_us={}",
                    max_callback.as_micros()
                );
                assert_eq!(underruns, [0; 3]);
                assert!(max_callback < period, "callback exceeded device budget");
                for deck in &mut decks {
                    assert_eq!(deck.poll(), None);
                }
            }
        }
    }
}
