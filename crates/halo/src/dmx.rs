//! The DMX engine thread: renders the lighting stack to Art-Net on its
//! own 44 Hz clock, independent of the UI.
//!
//! The UI publishes a [`DmxSnapshot`] of the cold inputs (cues,
//! programmer state, selection) once per frame; the thread reads the
//! *live* playhead from the deck's atomics every tick, so track cues keep
//! firing at full rate through UI stalls (live window resize, modal file
//! dialogs, an occluded window). A stalled UI freezes programmer *edits*,
//! never playback. The thread holds the last snapshot, so the rig keeps
//! its look — and keeps receiving frames — even when the UI goes quiet.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use halo_light::artnet::NetworkConfig;
use halo_light::cues::{CueSet, LANE_COUNT};
use halo_light::fixture::Rig;
use halo_light::fixture_library::FixtureLibrary;
use halo_light::output::render;
use halo_light::programmer::{LaneOverride, ProgrammerParams, resolve};

use crate::state::{DeckShared, MixerShared};

/// DMX refresh rate (full-universe Art-Net refresh convention).
pub const DMX_FPS: f64 = 44.0;

/// Linear beat map published by the UI: the thread extrapolates musical
/// time from the live playhead so effects stay beat-locked between UI
/// frames. `frames_per_beat <= 0` means no usable grid — hold `beat_t`.
#[derive(Clone, Copy)]
pub struct BeatRef {
    pub beat_t: f64,
    pub playhead: f64,
    pub frames_per_beat: f64,
}

/// Everything the engine needs except the playhead, which it reads live.
#[derive(Clone)]
pub struct DmxSnapshot {
    pub rig: Arc<Rig>,
    pub overrides: [LaneOverride; LANE_COUNT],
    pub params: ProgrammerParams,
    pub selection: HashSet<u32>,
    pub decks: [DmxDeck; 2],
    pub mixer: Arc<MixerShared>,
    /// Destinations + universe routing. Publish a *new* Arc to make the
    /// engine rebuild its sockets (it compares pointers, not contents).
    pub net: Arc<NetworkConfig>,
}

/// Keep both cue maps so automatic master handoff survives a stalled UI.
#[derive(Clone)]
pub struct DmxDeck {
    pub cues: Option<CueSet>,
    pub deck: Arc<DeckShared>,
    pub beat_ref: BeatRef,
}

impl DmxSnapshot {
    fn master_deck(&self) -> &DmxDeck {
        &self.decks[usize::from(self.mixer.effective_master.load(Ordering::Acquire)).min(1)]
    }
}

pub type DmxShared = Arc<Mutex<Option<DmxSnapshot>>>;

/// Spawn the engine. Sockets open lazily from the first snapshot's
/// network config and rebuild whenever a new config Arc is published.
/// The thread runs for the life of the process.
pub fn spawn_dmx_engine() -> DmxShared {
    let shared: DmxShared = Arc::new(Mutex::new(None));
    let out = Arc::clone(&shared);
    thread::spawn(move || {
        let library = FixtureLibrary::new();
        let mut current_net: Option<Arc<NetworkConfig>> = None;
        let mut connections = Vec::new();

        let tick = Duration::from_secs_f64(1.0 / DMX_FPS);
        let mut next = Instant::now() + tick;
        let mut send_errors: u64 = 0;
        loop {
            if let Some(wait) = next.checked_duration_since(Instant::now()) {
                thread::sleep(wait);
            }
            // Fixed cadence, but re-anchor rather than burst after a stall.
            next += tick;
            if next < Instant::now() {
                next = Instant::now() + tick;
            }

            let Some(snap) = shared.lock().unwrap().clone() else {
                continue;
            };
            if current_net
                .as_ref()
                .is_none_or(|n| !Arc::ptr_eq(n, &snap.net))
            {
                connections = match snap.net.connect() {
                    Ok(c) => {
                        log::info!("Art-Net up at {DMX_FPS} Hz: {}", snap.net.summary());
                        c
                    }
                    Err(e) => {
                        log::warn!("Art-Net socket setup failed ({e}); output paused");
                        Vec::new()
                    }
                };
                current_net = Some(Arc::clone(&snap.net));
            }
            let deck = snap.master_deck();
            let playhead = deck
                .deck
                .delivery_clock
                .read()
                .filter(|clock| clock.track == deck.deck.track_generation.load(Ordering::Acquire))
                .map_or_else(
                    || deck.deck.playhead_frames() as f64,
                    |clock| clock.position,
                );
            let lanes = resolve(&snap.overrides, deck.cues.as_ref(), playhead);
            let beat_t = if deck.beat_ref.frames_per_beat > 0.0 {
                deck.beat_ref.beat_t
                    + (playhead - deck.beat_ref.playhead) / deck.beat_ref.frames_per_beat
            } else {
                deck.beat_ref.beat_t
            };

            let frames = render(
                &snap.rig,
                &library,
                &lanes,
                &snap.params,
                &snap.selection,
                beat_t,
            );
            for (universe, frame) in &frames {
                let Some(dest) = snap.net.destination_for_universe(*universe) else {
                    continue;
                };
                let Some(conn) = connections.get(dest) else {
                    continue;
                };
                if let Err(e) = conn.send(*universe, frame) {
                    send_errors += 1;
                    // Log the 1st, 2nd, 4th, 8th... occurrence, not all 44/s.
                    if send_errors.is_power_of_two() {
                        log::debug!("Art-Net send error #{send_errors}: {e}");
                    }
                }
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use std::net::{SocketAddr, UdpSocket};

    use halo_light::artnet::ArtNetMode;
    use halo_light::fixture::default_rig;

    use super::*;

    /// End-to-end: real engine thread → resolve → render → Art-Net UDP,
    /// received by a local listener standing in for a node.
    #[test]
    fn engine_delivers_artnet_frames_to_a_destination() {
        let listener = UdpSocket::bind("127.0.0.1:0").expect("bind listener");
        listener
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let dst = listener.local_addr().unwrap();
        let src: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let mut net = NetworkConfig::single("test-node", ArtNetMode::Unicast(src, dst));
        net.route_universe(1, 0);

        let shared = spawn_dmx_engine();
        let library = FixtureLibrary::new();
        *shared.lock().unwrap() = Some(DmxSnapshot {
            rig: Arc::new(default_rig(&library)),
            overrides: Default::default(),
            params: ProgrammerParams::default(),
            selection: HashSet::new(),
            decks: std::array::from_fn(|_| DmxDeck {
                cues: None,
                deck: Arc::new(DeckShared::new()),
                beat_ref: BeatRef {
                    beat_t: 0.0,
                    playhead: 0.0,
                    frames_per_beat: 0.0,
                },
            }),
            mixer: Arc::new(MixerShared::new()),
            net: Arc::new(net),
        });

        let mut buf = [0u8; 1024];
        let (len, from) = listener
            .recv_from(&mut buf)
            .expect("engine should send a frame within the timeout");
        assert_eq!(&buf[..8], b"Art-Net\0", "packet id from {from}");
        // ArtDmx: 18-byte header + 512 channels.
        assert_eq!(len, 18 + 512);
        assert_eq!(u16::from_be_bytes([buf[9], buf[8]]), 0x5000, "OpDmx");
    }
}
