use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};

use eframe::egui;
use halo_light::artnet::{ARTNET_PORT, ArtNetMode, NetworkConfig};
use halo_light::cues::{ALL_LANES, CueSet, LANE_COUNT, Lane};
use halo_light::fixture::{ALL_KINDS, Rig, RigFile, default_rig};
use halo_light::fixture_library::FixtureLibrary;
use halo_light::programmer::{
    self, LaneOutput, LaneSource, ParamView, Programmer, ProgrammerParams,
};
use timestretch::{AudioBuffer, BeatGrid, Channels, PreAnalysisArtifact};

use crate::audio::{AudioOutput, AudioSettings, DeckAudio, list_output_devices};
use crate::deck::Deck;
use crate::decoder::decode_file;
use crate::dmx;
use crate::fader::{Fader, Notches};
use crate::knob::{Knob, KnobArc};
use crate::library::{Library, PlaylistRow, SortColumn, TrackRow};
use crate::programmer_ui::{ProgrammerCtx, programmer_panel};
use crate::show::simulate_show_l3;
use crate::show_preview::{LOOK_PALETTE, LookId, ShowPreview, ShowSel};
use crate::state::{MixerShared, ScrubPhase, Transport};
use crate::waveform::{
    BandPeaks, GhostPlayhead, GridMarks, OverviewParams, OverviewTexture, ScrubGesture,
    ShowEditorInteraction, ShowEditorParams, ShowStripParams, ZoomSpan, ZoomedParams,
    paint_beat_counter, paint_overview, paint_show_strip, paint_zoomed, show_editor,
};
use crate::worker::{WorkerEvent, spawn_analysis_worker, spawn_folder_import};

/// Halo accent color (amber, CDJ-style).
const ACCENT: egui::Color32 = egui::Color32::from_rgb(255, 170, 40);
const DECK_NAMES: [&str; 2] = ["A", "B"];
/// Longest edge of the artwork texture.
const ARTWORK_MAX_PX: u32 = 256;
/// Artwork display size in the deck header.
const ARTWORK_SIZE: f32 = 56.0;

/// Everything the decode thread produces for a track load.
struct LoadedData {
    title: String,
    artist: Option<String>,
    key: Option<String>,
    track_id: Option<i64>,
    artwork: Option<egui::ColorImage>,
    /// Interleaved stereo, resampled to the device rate.
    samples: Arc<Vec<f32>>,
    peaks: BandPeaks,
    grid: BeatGrid,
    /// Library analysis artifact, already rescaled to the device rate.
    artifact: Option<Arc<PreAnalysisArtifact>>,
}

type DecodeResult = Result<LoadedData, String>;

/// Ghost-playhead slide-in duration after a sync-aligned play start.
const GHOST_ANIM_SECS: f32 = 0.4;
/// Skip the ghost when the align jump is smaller than this (source frames);
/// a sub-beat sliver would just flicker under the playhead.
const GHOST_MIN_DELTA_FRAMES: f64 = 256.0;

/// Sync-aligned play start animation: the pre-align playhead position
/// gliding into the centered playhead.
struct GhostAnim {
    /// Pre-align playhead minus the aligned seek target (source frames).
    delta_frames: f64,
    started: std::time::Instant,
}

impl GhostAnim {
    fn new(delta_frames: f64) -> Self {
        Self {
            delta_frames,
            started: std::time::Instant::now(),
        }
    }

    fn finished(&self) -> bool {
        self.started.elapsed().as_secs_f32() >= GHOST_ANIM_SECS
    }

    /// Cubic ease-out slide toward offset 0, linear fade.
    fn params(&self) -> GhostPlayhead {
        let t = (self.started.elapsed().as_secs_f32() / GHOST_ANIM_SECS).clamp(0.0, 1.0);
        let ease = 1.0 - (1.0 - t).powi(3);
        GhostPlayhead {
            offset_frames: self.delta_frames * (1.0 - f64::from(ease)),
            alpha: (1.0 - t) * 0.9,
        }
    }
}

struct DeckUi {
    deck: Deck,
    decode_rx: Option<mpsc::Receiver<DecodeResult>>,
    /// Library row of the loaded track (None only when the library is
    /// unavailable).
    track_id: Option<i64>,
    key: Option<String>,
    /// Artifact that landed while the deck was playing; applied at the next
    /// non-playing moment.
    pending_artifact: Option<Arc<PreAnalysisArtifact>>,
    peaks: Option<BandPeaks>,
    marks: GridMarks,
    /// Lighting/pixels/FX cues, loaded from the library on track load.
    /// No longer painted (the L3 strip replaced the classic lanes) but
    /// still the layer that feeds the DMX engine and STORE-from-live.
    cues: CueSet,
    /// Session-only L3 show preview (look / energy / accent lanes),
    /// seeded on track load, edited in Prepare, never persisted.
    show: ShowPreview,
    bpm: f64,
    overview: Option<OverviewTexture>,
    artwork: Option<egui::TextureHandle>,
    title: String,
    artist: Option<String>,
    zoom: ZoomSpan,
    /// Pointer-implied platter position while the zoomed waveform is
    /// dragged (None = not dragging); published to the audio callback's
    /// scrub voice as the chase target. Dips below 0 in the elastic
    /// lead-in.
    scrub_pos: Option<f64>,
    /// Elastic lead-in depth captured at Grab, so the drag clamp matches
    /// exactly the floor the audio voice saw even if the grid re-analyzes
    /// mid-gesture.
    scrub_lead_in: f64,
    /// Last consumed scrub-landing sequence number; each newly published
    /// landing fires one parallel engine warm-start seek.
    landing_seq_seen: u64,
    /// Previous frame's cue-button-held state, for press/release edges.
    cue_was_down: bool,
    /// True while the cue button is previewing (play-from-cue while held).
    cue_previewing: bool,
    /// Tempo slider value in percent, within ±`pitch_range`.
    pitch_percent: f32,
    /// Tempo slider range in percent (8 / 16 / 50).
    pitch_range: f32,
    keylock: bool,
    /// Following the master deck's tempo + beat phase.
    synced: bool,
    /// Momentary pitch-bend factor from the held nudge buttons (1.0 = none).
    bend: f32,
    /// Smoothed beat-phase error vs the master (beats, ±0.5), tracked only
    /// while synced and both decks play. Filters playhead-publish jitter so
    /// the sync PLL doesn't chase phantom errors.
    phase_err: Option<f64>,
    /// Ghost-playhead slide-in running after a sync-aligned jump.
    ghost: Option<GhostAnim>,
    /// Hot cue slots (source frames).
    hot_cues: [Option<usize>; 8],
    hotcue_was_down: [bool; 8],
    /// Gated hot-cue mode: play from the cue while held, pause on release.
    gated: bool,
    /// Slot currently held in gated mode.
    gated_held: Option<usize>,
    /// Quantize hot cues and loop points to the beat grid.
    quantize: bool,
    /// Auto cue: on load, park the deck at the first downbeat.
    auto_cue: bool,
    /// Cue frame last set by auto cue; guards re-application when async
    /// analysis refines the grid.
    last_auto_cue: Option<usize>,
    /// Header time readout shows remaining (true) or elapsed (false).
    show_remaining: bool,
    /// Staged loop-in point awaiting loop-out.
    loop_in_staged: Option<usize>,
    /// Active/last loop length in beats (resize anchor).
    loop_beats: f64,
    /// Restore the playhead to this track fraction after the next load
    /// (used when an audio-device change forces a reload).
    pending_seek_frac: Option<f64>,
}

impl DeckUi {
    fn new() -> Self {
        Self {
            deck: Deck::new(),
            decode_rx: None,
            track_id: None,
            key: None,
            pending_artifact: None,
            peaks: None,
            marks: GridMarks::empty(),
            cues: CueSet::empty(),
            show: ShowPreview::default(),
            bpm: 0.0,
            overview: None,
            artwork: None,
            title: String::new(),
            artist: None,
            zoom: ZoomSpan::default(),
            scrub_pos: None,
            scrub_lead_in: 0.0,
            landing_seq_seen: 0,
            cue_was_down: false,
            cue_previewing: false,
            pitch_percent: 0.0,
            pitch_range: 8.0,
            keylock: true,
            synced: false,
            bend: 1.0,
            phase_err: None,
            ghost: None,
            hot_cues: [None; 8],
            hotcue_was_down: [false; 8],
            gated: false,
            gated_held: None,
            quantize: true,
            auto_cue: true,
            last_auto_cue: None,
            show_remaining: false,
            loop_in_staged: None,
            loop_beats: 4.0,
            pending_seek_frac: None,
        }
    }

    fn playhead(&self) -> usize {
        self.deck
            .shared
            .playhead_frames()
            .min(self.deck.shared.total())
    }

    fn toggle_play(&mut self) {
        if self.deck.track.is_none() {
            return;
        }
        let shared = &self.deck.shared;
        self.cue_previewing = false;
        match shared.transport() {
            Transport::Playing => shared.set_transport(Transport::Paused),
            _ => {
                let total = shared.total();
                if total > 0 && shared.playhead_frames() >= total {
                    request_seek_guarded(shared, 0);
                }
                shared.set_transport(Transport::Playing);
            }
        }
    }

    /// CDJ cue, press edge: playing = return to cue and pause; paused = set
    /// the cue here and preview while held.
    fn cue_press(&mut self) {
        if self.deck.track.is_none() {
            return;
        }
        let playhead = self.playhead();
        let shared = &self.deck.shared;
        if shared.transport() == Transport::Playing {
            if !self.cue_previewing {
                shared.set_transport(Transport::Paused);
                request_seek_guarded(shared, shared.cue_point.load(Ordering::Relaxed) as usize);
            }
        } else {
            shared.cue_point.store(playhead as u64, Ordering::Relaxed);
            self.cue_previewing = true;
            shared.set_transport(Transport::Playing);
        }
    }

    /// CDJ cue, release edge: a preview ends back at the cue point, paused.
    fn cue_release(&mut self) {
        if self.cue_previewing {
            self.cue_previewing = false;
            let shared = &self.deck.shared;
            shared.set_transport(Transport::Paused);
            request_seek_guarded(shared, shared.cue_point.load(Ordering::Relaxed) as usize);
        }
    }

    /// Hot cue press: empty slot stores the (quantized) playhead, occupied
    /// slot jumps and plays. Returns true when it jumped (the button path
    /// uses this to engage gated mode).
    fn hot_cue_press(&mut self, slot: usize) -> bool {
        if self.deck.track.is_none() {
            return false;
        }
        let playhead = self.playhead();
        match self.hot_cues[slot] {
            None => {
                self.hot_cues[slot] = Some(quantize_frame(&self.marks, self.quantize, playhead));
                false
            }
            Some(frame) => {
                let shared = &self.deck.shared;
                request_seek_guarded(shared, frame);
                shared.set_transport(Transport::Playing);
                self.cue_previewing = false;
                true
            }
        }
    }

    /// Quantized autoloop of `beats` beats at the current position.
    fn autoloop(&mut self, beats: f64) {
        if self.deck.track.is_none() || !self.marks.is_usable() {
            return;
        }
        let start = quantize_frame(&self.marks, true, self.playhead());
        let end = loop_end_for(&self.marks, start, beats);
        if end > start {
            self.deck.shared.set_loop(Some((start, end)));
            self.loop_beats = beats;
            self.loop_in_staged = None;
        }
    }

    fn exit_loop(&mut self) {
        self.deck.shared.set_loop(None);
    }
}

/// UI state of the track browser.
struct BrowserState {
    rows: Vec<TrackRow>,
    playlists: Vec<PlaylistRow>,
    /// Selected playlist (None = whole library).
    selected: Option<i64>,
    search: String,
    sort: SortColumn,
    ascending: bool,
    /// Re-query the DB on the next frame.
    dirty: bool,
    /// Playlist being renamed inline: (id, buffer).
    rename: Option<(i64, String)>,
}

impl BrowserState {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            playlists: Vec::new(),
            selected: None,
            search: String::new(),
            sort: SortColumn::Title,
            ascending: true,
            dirty: true,
            rename: None,
        }
    }
}

/// State restored between sessions.
///
/// New fields MUST carry `#[serde(default)]`: the stored blob is decoded
/// with `unwrap_or_default()`, so a missing field would otherwise reset
/// every setting on upgrade.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Persisted {
    master_volume: f32,
    crossfader: f32,
    trims: [f32; 2],
    keylocks: [bool; 2],
    pitch_ranges: [f32; 2],
    quantize: [bool; 2],
    gated: [bool; 2],
    sort: Option<SortColumn>,
    ascending: bool,
    device_name: Option<String>,
    buffer_size: Option<u32>,
    #[serde(default)]
    view: View,
    #[serde(default)]
    audition_volume: f32,
    /// Inverted so the missing-field default (false) means snap ON.
    #[serde(default)]
    snap_off: bool,
    /// Inverted so the missing-field default (false) means auto cue ON.
    #[serde(default)]
    auto_cue_off: [bool; 2],
    #[serde(default)]
    footer_tab: FooterTab,
}

/// Which pane the bottom slide-up footer shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
enum FooterTab {
    #[default]
    Library,
    Programmer,
}

/// Persisted Art-Net destination choice. Lives in the library DB (show
/// config, tied to the venue), not eframe storage (UI state, tied to the
/// machine).
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct ArtNetSettings {
    /// Node IP for unicast; `None` broadcasts.
    unicast_ip: Option<String>,
}

/// Art-Net config for the current rig: one destination (broadcast, or
/// unicast when the settings name a node IP) with every rig universe
/// routed to it.
fn build_net(settings: &ArtNetSettings, rig: &Rig) -> std::sync::Arc<NetworkConfig> {
    let mode = match settings
        .unicast_ip
        .as_deref()
        .and_then(|ip| ip.parse::<std::net::IpAddr>().ok())
    {
        Some(ip) => ArtNetMode::Unicast(
            "0.0.0.0:0".parse().unwrap(),
            std::net::SocketAddr::new(ip, ARTNET_PORT),
        ),
        None => ArtNetMode::Broadcast,
    };
    let mut net = NetworkConfig::single("output", mode);
    for f in rig.iter() {
        net.route_universe(f.universe, 0);
    }
    std::sync::Arc::new(net)
}

/// Which top-level screen is showing. Purely a UI concern: the engine
/// (audio, decks, lighting) is untouched by the view, so switching never
/// interrupts playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
enum View {
    #[default]
    Perform,
    Prepare,
    /// Rig setup: the full-window patch sheet.
    Patch,
}

/// Default audition player volume (its channel bypasses the crossfader,
/// so the fader alone sets its level).
const AUDITION_VOLUME_DEFAULT: f32 = 0.85;

/// Prepare-view state: an independent audition player (a full third deck
/// mixed straight to master) whose loaded track is the cue-editing target,
/// plus the lane editor's selection and drag state.
struct PrepareState {
    audition: DeckUi,
    selection: std::collections::HashSet<ShowSel>,
    interaction: ShowEditorInteraction,
    /// Snap editor gestures to the beat grid.
    snap: bool,
    /// Palette look new look events are created with.
    armed_look: LookId,
}

impl PrepareState {
    fn new() -> Self {
        let mut audition = DeckUi::new();
        // No auto cue for previews: they start at the top of the file, and
        // the Prepare UI has no toggle for it.
        audition.auto_cue = false;
        audition.deck.shared.fader.store(AUDITION_VOLUME_DEFAULT);
        Self {
            audition,
            selection: std::collections::HashSet::new(),
            interaction: ShowEditorInteraction::default(),
            snap: true,
            armed_look: LookId(0),
        }
    }
}

const PERSIST_KEY: &str = "halo";

pub struct HaloApp {
    audio: Option<AudioOutput>,
    mixer: Arc<MixerShared>,
    decks: [DeckUi; 2],
    /// Index of the tempo-master deck.
    master: usize,
    /// Index of the deck driving the lighting rig.
    lighting_deck: usize,
    view: View,
    prepare: PrepareState,
    /// Live manual-override layer; beats the active deck's track cues.
    programmer: Programmer,
    /// Which pane the footer shows (library browser or programmer).
    footer_tab: FooterTab,
    /// The patched rig (simulated default until real patching exists).
    /// Shared with the DMX engine thread via snapshots; Arc so publishing
    /// is a pointer bump, not a rig clone.
    rig: std::sync::Arc<Rig>,
    /// Snapshot slot the DMX engine thread renders from.
    dmx: dmx::DmxShared,
    fixture_library: FixtureLibrary,
    /// Current Art-Net config. Replaced wholesale (new Arc) on any
    /// settings or rig-universe change — the engine rebuilds its sockets
    /// when the pointer changes.
    net: std::sync::Arc<NetworkConfig>,
    artnet: ArtNetSettings,
    /// Settings-window edit buffer for the unicast node IP.
    artnet_ip_edit: String,
    /// Staged patch-sheet edits: the live rig, DMX output, and programmer
    /// keep the last-applied patch until APPLY commits this draft to the
    /// show file. `None` = sheet is clean. In-memory only.
    patch_draft: Option<Rig>,
    /// Fixtures the programmer's grid has selected (empty = whole lanes).
    fixture_selection: std::collections::HashSet<u32>,
    /// Parameter-view values + effect configs (mockup state).
    programmer_params: ProgrammerParams,
    /// Keyboard FLASH state (Z/X/C), merged with the on-screen buttons.
    flash_key: [bool; LANE_COUNT],
    library: Option<Library>,
    browser: BrowserState,
    /// Wakes the analysis worker after imports.
    wake_tx: mpsc::Sender<()>,
    events_rx: mpsc::Receiver<WorkerEvent>,
    /// Prototype sender for the events channel (folder imports clone it).
    event_tx: mpsc::Sender<WorkerEvent>,
    audio_settings: AudioSettings,
    settings_open: bool,
    /// Track pending "Remove from library" confirmation: (id, title).
    pending_remove: Option<(i64, String)>,
    available_devices: Vec<String>,
    /// Per-deck previous key-down states for shortcut edge detection.
    kb_prev: [[bool; 8]; 2],
    /// Process CPU sampling: (last wall instant, last cpu seconds, percent).
    cpu_sample: (std::time::Instant, f64, f32),
    status: String,
    /// Absolute x of the mixer column's center, captured while rendering the
    /// central panel and read a frame later by the toolbar to center the
    /// master-BPM readout over the mixer (0 = not measured yet).
    mixer_center_x: f32,
}

impl HaloApp {
    fn deck_audio(deck_ui: &DeckUi) -> DeckAudio {
        DeckAudio {
            shared: deck_ui.deck.shared.clone(),
            slot: deck_ui.deck.processor_slot.clone(),
            retired: deck_ui.deck.retired_slot.clone(),
            scratch_source: deck_ui.deck.scratch_source.clone(),
            scratch_retired: deck_ui.deck.scratch_retired.clone(),
            reset_request: deck_ui.deck.reset_request.clone(),
        }
    }

    pub fn new(cc: &eframe::CreationContext<'_>, initial_file: Option<PathBuf>) -> Self {
        apply_theme(&cc.egui_ctx);

        let persisted: Persisted = cc
            .storage
            .and_then(|s| eframe::get_value(s, PERSIST_KEY))
            .unwrap_or_default();

        let mixer = Arc::new(MixerShared::new());
        let mut decks = [DeckUi::new(), DeckUi::new()];
        let mut prepare = PrepareState::new();

        // Restore session state before the audio stream starts.
        if persisted.master_volume > 0.0 {
            mixer.master.store(persisted.master_volume);
            mixer.crossfader.store(persisted.crossfader.clamp(0.0, 1.0));
            for (i, deck_ui) in decks.iter_mut().enumerate() {
                deck_ui.deck.shared.trim.store(persisted.trims[i]);
                deck_ui.keylock = persisted.keylocks[i];
                deck_ui.pitch_range = persisted.pitch_ranges[i].max(8.0);
                deck_ui.quantize = persisted.quantize[i];
                deck_ui.auto_cue = !persisted.auto_cue_off[i];
                deck_ui.gated = persisted.gated[i];
            }
        }
        if persisted.audition_volume > 0.0 {
            prepare
                .audition
                .deck
                .shared
                .fader
                .store(persisted.audition_volume.clamp(0.0, 1.0));
        }
        prepare.snap = !persisted.snap_off;
        let audio_settings = AudioSettings {
            device_name: persisted.device_name,
            buffer_size: persisted.buffer_size,
        };

        let audio = match AudioOutput::new(
            [
                Self::deck_audio(&decks[0]),
                Self::deck_audio(&decks[1]),
                Self::deck_audio(&prepare.audition),
            ],
            mixer.clone(),
            &audio_settings,
        ) {
            Ok(a) => Some(a),
            Err(e) => {
                log::error!("{e}");
                None
            }
        };

        let status = match &audio {
            Some(a) => format!("Output: {} @ {} Hz", a.device_name, a.sample_rate),
            None => "No audio output".to_string(),
        };

        let db_path = Library::default_path();
        let library = match Library::open(&db_path) {
            Ok(l) => Some(l),
            Err(e) => {
                log::error!("library: {e}");
                None
            }
        };
        let (wake_tx, wake_rx) = mpsc::channel();
        let (event_tx, events_rx) = mpsc::channel();
        if library.is_some() {
            spawn_analysis_worker(db_path, wake_rx, event_tx.clone());
        }

        let mut browser = BrowserState::new();
        if let Some(sort) = persisted.sort {
            browser.sort = sort;
            browser.ascending = persisted.ascending;
        }

        // Rig patch + Art-Net settings from the library DB, falling back
        // to the default rig / broadcast when absent or unparsable.
        let fixture_library = FixtureLibrary::new();
        let rig = std::sync::Arc::new(
            library
                .as_ref()
                .and_then(|l| l.setting("rig_patch").ok().flatten())
                .and_then(|json| serde_json::from_str::<RigFile>(&json).ok())
                .map(RigFile::into_rig)
                .unwrap_or_else(|| default_rig(&fixture_library)),
        );
        let artnet: ArtNetSettings = library
            .as_ref()
            .and_then(|l| l.setting("artnet").ok().flatten())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        let artnet_ip_edit = artnet
            .unicast_ip
            .clone()
            .unwrap_or_else(|| "10.0.0.10".to_string());
        let net = build_net(&artnet, &rig);
        let dmx = dmx::spawn_dmx_engine();

        let mut app = Self {
            audio,
            mixer,
            decks,
            master: 0,
            lighting_deck: 0,
            view: persisted.view,
            prepare,
            programmer: Programmer::default(),
            footer_tab: persisted.footer_tab,
            rig,
            dmx,
            fixture_library,
            net,
            artnet,
            artnet_ip_edit,
            patch_draft: None,
            fixture_selection: std::collections::HashSet::new(),
            programmer_params: ProgrammerParams::default(),
            flash_key: [false; LANE_COUNT],
            library,
            browser,
            wake_tx,
            events_rx,
            event_tx,
            audio_settings,
            settings_open: false,
            pending_remove: None,
            available_devices: Vec::new(),
            kb_prev: [[false; 8]; 2],
            cpu_sample: (std::time::Instant::now(), process_cpu_secs(), 0.0),
            status,
            mixer_center_x: 0.0,
        };
        // Dev smoke-test hook: import a folder at startup.
        if let Some(dir) = std::env::var_os("HALO_IMPORT") {
            app.import_folder(PathBuf::from(dir));
        }
        if let Some(path) = initial_file {
            // Dev smoke-test hook: also open the same track in the Prepare
            // audition player (import is idempotent).
            if std::env::var_os("HALO_AUDITION").is_some()
                && let Some(lib) = &app.library
                && let Ok(id) = lib.import_file(&path)
            {
                app.load_audition(id);
            }
            app.import_and_load(0, path);
        }
        // Dev smoke-test hook: latch programmer lanes (comma-separated
        // indices) with the footer on the programmer tab.
        if let Ok(l) = std::env::var("HALO_LATCH") {
            for idx in l.split(',').filter_map(|s| s.trim().parse::<usize>().ok()) {
                if let Some(o) = app.programmer.get_mut(idx) {
                    o.latched = true;
                }
            }
            app.footer_tab = FooterTab::Programmer;
        }
        // Dev smoke-test hook: preselect a fixture group ("all" or a
        // group label like "pixels") in the programmer grid.
        if let Ok(sel) = std::env::var("HALO_FIXSEL") {
            app.fixture_selection = if sel.eq_ignore_ascii_case("all") {
                app.rig.ids().collect()
            } else {
                ALL_KINDS
                    .iter()
                    .find(|k| k.group_label().eq_ignore_ascii_case(&sel))
                    .map(|&k| app.rig.ids_of_kind(k).collect())
                    .unwrap_or_default()
            };
            app.footer_tab = FooterTab::Programmer;
        }
        // Dev smoke-test hook: open a specific programmer parameter view.
        if let Ok(v) = std::env::var("HALO_PVIEW") {
            app.programmer_params.view = match v.to_ascii_lowercase().as_str() {
                "color" => ParamView::Color,
                "position" => ParamView::Position,
                "beam" => ParamView::Beam,
                "pixel" => ParamView::PixelFx,
                _ => ParamView::Intensity,
            };
            app.footer_tab = FooterTab::Programmer;
        }
        // Dev smoke-test hook: start in a specific view.
        if let Ok(v) = std::env::var("HALO_VIEW") {
            app.view = if v.eq_ignore_ascii_case("prepare") {
                View::Prepare
            } else if v.eq_ignore_ascii_case("patch") {
                View::Patch
            } else {
                View::Perform
            };
        }
        app
    }

    fn import_folder(&mut self, dir: PathBuf) {
        if self.library.is_none() {
            return;
        }
        self.status = format!("Importing {}…", dir.display());
        spawn_folder_import(
            Library::default_path(),
            dir,
            self.wake_tx.clone(),
            self.event_tx.clone(),
        );
    }

    fn device_rate(&self) -> u32 {
        self.audio.as_ref().map(|a| a.sample_rate).unwrap_or(44_100)
    }

    /// File picked via dialog / CLI: register it in the library first so it
    /// gains analysis + browser presence, then load.
    fn import_and_load(&mut self, deck_idx: usize, path: PathBuf) {
        let mut track_id = None;
        if let Some(lib) = &self.library {
            match lib.import_file(&path) {
                Ok(id) => track_id = Some(id),
                Err(e) => log::warn!("import {}: {e}", path.display()),
            }
        }
        if track_id.is_some() {
            let _ = self.wake_tx.send(());
            self.browser.dirty = true;
        }
        self.start_decode(deck_idx, path, track_id);
    }

    /// Load a library row onto a deck.
    fn load_track_row(&mut self, deck_idx: usize, track_id: i64) {
        let row = self
            .library
            .as_ref()
            .and_then(|lib| lib.track(track_id).ok().flatten());
        match row {
            Some(row) => self.start_decode(deck_idx, row.path, Some(track_id)),
            None => self.status = "Track not found in library".to_string(),
        }
    }

    fn start_decode(&mut self, deck_idx: usize, path: PathBuf, track_id: Option<i64>) {
        self.status = format!("Deck {}: loading {}…", DECK_NAMES[deck_idx], path.display());
        self.decks[deck_idx].decode_rx = Some(self.spawn_decode(path, track_id));
    }

    /// Load a library row into the Prepare view's audition player.
    fn load_audition(&mut self, track_id: i64) {
        let row = self
            .library
            .as_ref()
            .and_then(|lib| lib.track(track_id).ok().flatten());
        match row {
            Some(row) => {
                self.status = format!("Prepare: loading {}…", row.path.display());
                self.prepare.audition.decode_rx = Some(self.spawn_decode(row.path, Some(track_id)));
            }
            None => self.status = "Track not found in library".to_string(),
        }
    }

    fn spawn_decode(&self, path: PathBuf, track_id: Option<i64>) -> mpsc::Receiver<DecodeResult> {
        let device_rate = self.device_rate();
        // Stored analysis (native rate) and key come from the library; both
        // queries are cheap enough for the UI thread.
        let (artifact, key) = match (&self.library, track_id) {
            (Some(lib), Some(id)) => (
                lib.analysis(id).ok().flatten(),
                lib.track(id).ok().flatten().and_then(|r| r.key),
            ),
            _ => (None, None),
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(load_track_data(&path, device_rate, track_id, key, artifact));
        });
        rx
    }

    /// Handle finished decode threads: install the track on its deck (A, B,
    /// or the Prepare audition player) and kick off background pre-analysis.
    fn poll_decodes(&mut self, ctx: &egui::Context) {
        let device_rate = self.device_rate();
        for i in 0..self.decks.len() {
            if let Some(status) = Self::poll_deck_decode(
                &mut self.decks[i],
                DECK_NAMES[i],
                ctx,
                device_rate,
                self.library.as_ref(),
                &self.wake_tx,
            ) {
                self.status = status;
                // The show preview is session-only: a fresh load adopts
                // any edits a sibling player holds for the same track,
                // instead of keeping its own fresh seed.
                if self.decks[i].track_id.is_some()
                    && self.decks[i].track_id == self.prepare.audition.track_id
                {
                    self.decks[i].show = self.prepare.audition.show.clone();
                }
            }
        }
        if let Some(status) = Self::poll_deck_decode(
            &mut self.prepare.audition,
            "PREP",
            ctx,
            device_rate,
            self.library.as_ref(),
            &self.wake_tx,
        ) {
            self.status = status;
            if let Some(deck) = self
                .decks
                .iter()
                .find(|d| d.track_id.is_some() && d.track_id == self.prepare.audition.track_id)
            {
                self.prepare.audition.show = deck.show.clone();
            }
        }
    }

    /// One deck's decode-completion handling; returns a status line when a
    /// load finished (or failed).
    fn poll_deck_decode(
        deck_ui: &mut DeckUi,
        label: &str,
        ctx: &egui::Context,
        device_rate: u32,
        library: Option<&Library>,
        wake_tx: &mpsc::Sender<()>,
    ) -> Option<String> {
        // Drop sample Arcs the callback retired, off the audio thread.
        if let Ok(mut retired) = deck_ui.deck.scratch_retired.try_lock() {
            retired.take();
        }
        let rx = deck_ui.decode_rx.as_ref()?;
        let Ok(result) = rx.try_recv() else {
            return None;
        };
        deck_ui.decode_rx = None;
        match result {
            Ok(data) => {
                match deck_ui.deck.load(
                    data.title.clone(),
                    data.samples.clone(),
                    device_rate,
                    data.artifact.clone(),
                ) {
                    Ok(()) => {
                        deck_ui.overview = Some(OverviewTexture::from_peaks(ctx, &data.peaks));
                        deck_ui.artwork = data.artwork.map(|img| {
                            ctx.load_texture(
                                format!("artwork_{label}"),
                                img,
                                egui::TextureOptions::LINEAR,
                            )
                        });
                        deck_ui.peaks = Some(data.peaks);
                        deck_ui.marks = GridMarks::from_grid(&data.grid);
                        deck_ui.cues = data
                            .track_id
                            .and_then(|id| library?.cues(id).ok().flatten())
                            .map(|f| CueSet::from_file(&f, device_rate))
                            .unwrap_or_else(CueSet::empty);
                        // Session-only L3 preview: seed the role lanes
                        // deterministically per track so both views show
                        // content immediately (poll_decodes adopts edits
                        // from a sibling player holding the same track).
                        deck_ui.show = simulate_show_l3(
                            &deck_ui.marks,
                            deck_ui.deck.shared.total(),
                            device_rate,
                            data.track_id.unwrap_or(1) as u64,
                        );
                        deck_ui.bpm = data.grid.bpm;
                        deck_ui.title = data.title;
                        deck_ui.artist = data.artist;
                        deck_ui.key = data.key;
                        deck_ui.track_id = data.track_id;
                        deck_ui.pending_artifact = None;
                        deck_ui.scrub_pos = None;
                        deck_ui.hot_cues = [None; 8];
                        deck_ui.gated_held = None;
                        deck_ui.loop_in_staged = None;
                        deck_ui.loop_beats = 4.0;
                        // No stored analysis yet: the worker will send
                        // an Analyzed event when it lands.
                        if data.artifact.is_none() && data.track_id.is_some() {
                            let _ = wake_tx.send(());
                        }
                        // Auto cue: park the deck at the first downbeat.
                        // Ceil so the parked position sits at/after the
                        // grid frame and the bar readout says 1.1, not 0.4.
                        deck_ui.last_auto_cue = None;
                        if deck_ui.auto_cue
                            && deck_ui.marks.is_usable()
                            && let Some(frame) = deck_ui.marks.first_downbeat_frame()
                        {
                            let frame = frame.ceil() as usize;
                            deck_ui
                                .deck
                                .shared
                                .cue_point
                                .store(frame as u64, Ordering::Relaxed);
                            deck_ui.deck.shared.request_seek(frame);
                            deck_ui.last_auto_cue = Some(frame);
                        }
                        // Device-change reload: restore the playhead.
                        if let Some(frac) = deck_ui.pending_seek_frac.take() {
                            let total = deck_ui.deck.shared.total();
                            deck_ui
                                .deck
                                .shared
                                .request_seek((frac * total as f64) as usize);
                        }
                        // Dev smoke-test hooks: start playback
                        // immediately, optionally at a tempo offset
                        // and/or with a 4-beat loop engaged.
                        if std::env::var_os("HALO_AUTOPLAY").is_some() {
                            if let Ok(p) = std::env::var("HALO_PITCH") {
                                deck_ui.pitch_percent = p.parse().unwrap_or(0.0);
                            }
                            if std::env::var_os("HALO_LOOP").is_some() && deck_ui.marks.is_usable()
                            {
                                let start = quantize_frame(&deck_ui.marks, true, 0);
                                let end = loop_end_for(&deck_ui.marks, start, 4.0);
                                deck_ui.deck.shared.set_loop(Some((start, end)));
                            }
                            deck_ui.deck.shared.set_transport(Transport::Playing);
                        }
                        Some(format!(
                            "Deck {label}: {} ({:.1} BPM)",
                            deck_ui.title, deck_ui.bpm
                        ))
                    }
                    Err(e) => Some(format!("Deck {label}: {e}")),
                }
            }
            Err(e) => Some(format!("Deck {label}: load failed: {e}")),
        }
    }

    /// Drain analysis-worker events and apply pending artifacts. A freshly
    /// analyzed track that's sitting on a deck gets its display grid/BPM
    /// immediately; the engine upgrade waits for the next moment the deck
    /// isn't playing (rebuilding a live engine would audibly interrupt it).
    fn poll_worker_events(&mut self) {
        let device_rate = self.device_rate();
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                WorkerEvent::Analyzed(id) => {
                    self.browser.dirty = true;
                    // No `pre_analysis.is_none()` guard: a reanalyzed track
                    // already on a deck must refresh too. The engine-side
                    // apply still waits for a non-playing moment via
                    // `pending_artifact`.
                    for deck_ui in self.decks.iter_mut() {
                        if deck_ui.track_id == Some(id)
                            && let Some(lib) = &self.library
                            && let Ok(Some(native)) = lib.analysis(id)
                        {
                            let resampled = native.resample_to(device_rate);
                            deck_ui.marks = GridMarks::from_grid(&grid_from_artifact(&resampled));
                            deck_ui.bpm = resampled.bpm;
                            deck_ui.pending_artifact = Some(Arc::new(resampled));
                            // The refined grid may move the first downbeat:
                            // re-apply auto cue, but never disturb a playing
                            // or scrubbing deck, a user-moved cue, or a
                            // user-moved playhead.
                            let shared = &deck_ui.deck.shared;
                            if deck_ui.auto_cue
                                && shared.transport() != Transport::Playing
                                && deck_ui.scrub_pos.is_none()
                                && let Some(prev) = deck_ui.last_auto_cue
                                && shared.cue_point.load(Ordering::Relaxed) as usize == prev
                                && let Some(frame) = deck_ui.marks.first_downbeat_frame()
                            {
                                let frame = frame.ceil() as usize;
                                shared.cue_point.store(frame as u64, Ordering::Relaxed);
                                if shared.playhead_frames() == prev {
                                    shared.request_seek(frame);
                                }
                                deck_ui.last_auto_cue = Some(frame);
                            }
                        }
                    }
                    if let Some(lib) = &self.library
                        && let Ok(n) = lib.unanalyzed_count()
                    {
                        self.status = if n > 0 {
                            format!("Analyzing… {n} track(s) remaining")
                        } else {
                            "Analysis complete".to_string()
                        };
                    }
                }
                WorkerEvent::Imported(n) => {
                    self.browser.dirty = true;
                    self.status = format!("Imported {n} audio file(s)");
                }
            }
        }

        for deck_ui in self.decks.iter_mut() {
            if deck_ui.pending_artifact.is_some()
                && deck_ui.deck.shared.transport() != Transport::Playing
            {
                let artifact = deck_ui.pending_artifact.take().unwrap();
                if let Err(e) = deck_ui.deck.apply_pre_analysis(artifact, device_rate) {
                    log::error!("apply_pre_analysis: {e}");
                }
            }
        }
    }

    /// Recompute and publish each deck's tempo rate + keylock.
    ///
    /// A synced deck follows the master's effective BPM and writes the
    /// matching pitch back to its own slider (expanding the range if it
    /// doesn't fit) so the control shows the real tempo; while both decks
    /// run, a gentle proportional rate correction (smoothed error, capped
    /// at ±1.5%) chases the master's beat phase — a continuous PLL, like a
    /// DJ riding the platter, so the lock survives grid drift and seeks.
    /// The PLL nudge is deliberately left out of the displayed pitch to
    /// keep the slider steady. Pitch-bend multiplies on top either way.
    /// Global/master BPM for the header readout: the pitch-adjusted BPM of the
    /// deck driving the mix. A deck is "live" if it's playing with its channel
    /// fader up (> 50%); the live deck wins, the master deck breaks ties (both
    /// live) and is the fallback (neither live). Excludes momentary bend so the
    /// clock reads steady.
    /// Deck the master BPM (and the lighting rig) is sourced from: the live
    /// deck (playing with fader > 50%), the master deck breaking ties / as the
    /// fallback.
    fn master_source(&self) -> usize {
        let live = |i: usize| {
            let d = &self.decks[i];
            d.deck.shared.transport() == Transport::Playing && d.deck.shared.fader.load() > 0.5
        };
        match (live(0), live(1)) {
            (true, false) => 0,
            (false, true) => 1,
            _ => self.master,
        }
    }

    fn global_bpm(&self) -> f64 {
        let d = &self.decks[self.master_source()];
        d.bpm * (1.0 + d.pitch_percent as f64 / 100.0)
    }

    fn update_tempo(&mut self, dt: f64) {
        // EMA weight for a ~150 ms error-smoothing time constant at the
        // actual frame rate.
        let alpha = 1.0 - (-dt / 0.15).exp();
        let master = self.master;
        let master_base = self.decks[master].bpm;
        let master_rate = 1.0 + self.decks[master].pitch_percent as f64 / 100.0;
        let master_playing = self.decks[master].deck.shared.transport() == Transport::Playing;
        let master_phase = beat_phase(
            &self.decks[master].marks,
            self.decks[master].deck.shared.playhead_frames() as f64,
        );

        for i in 0..2 {
            let d = &mut self.decks[i];
            let manual = 1.0 + d.pitch_percent as f64 / 100.0;
            let mut rate = if d.synced && i != master && d.bpm > 0.0 && master_base > 0.0 {
                let mut r = master_base * master_rate / d.bpm;
                let sync_pct = ((r - 1.0) * 100.0) as f32;
                d.pitch_range = d.pitch_range.max(range_for_pitch(sync_pct));
                d.pitch_percent = sync_pct.clamp(-50.0, 50.0);
                // A scrub (grab or glide) freezes or overrides the deck's
                // playhead — suspend the phase chase so it doesn't poison
                // the smoothed error; the PLL re-acquires once it ends.
                if master_playing
                    && d.deck.shared.scrub.phase() == ScrubPhase::Idle
                    && d.deck.shared.transport() == Transport::Playing
                    && let Some(mp) = master_phase
                    && let Some(dp) = beat_phase(&d.marks, d.deck.shared.playhead_frames() as f64)
                {
                    let err = smooth_phase_err(d.phase_err, wrap_phase_err(mp, dp), alpha);
                    d.phase_err = Some(err);
                    r *= 1.0 + phase_correction(err);
                } else {
                    d.phase_err = None;
                }
                r
            } else {
                d.phase_err = None;
                manual
            };
            rate *= d.bend as f64;
            d.deck.shared.tempo_rate.store(rate.clamp(0.25, 4.0) as f32);
            d.deck.shared.keylock.store(d.keylock, Ordering::Relaxed);
        }
    }

    /// Tear down and rebuild the output stream with the current settings.
    /// The old callback owned the deck processors, so loaded decks always
    /// reload afterwards (restoring their playhead position).
    fn rebuild_audio(&mut self) {
        self.audio = None;
        match AudioOutput::new(
            [
                Self::deck_audio(&self.decks[0]),
                Self::deck_audio(&self.decks[1]),
                Self::deck_audio(&self.prepare.audition),
            ],
            self.mixer.clone(),
            &self.audio_settings,
        ) {
            Ok(a) => {
                self.status = format!("Output: {} @ {} Hz", a.device_name, a.sample_rate);
                self.audio = Some(a);
            }
            Err(e) => {
                self.status = format!("Audio error: {e}");
                return;
            }
        }
        for i in 0..2 {
            if let Some(id) = self.decks[i].track_id {
                let shared = &self.decks[i].deck.shared;
                shared.set_transport(Transport::Stopped);
                let total = shared.total().max(1);
                self.decks[i].pending_seek_frac =
                    Some(shared.playhead_frames() as f64 / total as f64);
                self.load_track_row(i, id);
            }
        }
        if let Some(id) = self.prepare.audition.track_id {
            let shared = &self.prepare.audition.deck.shared;
            shared.set_transport(Transport::Stopped);
            let total = shared.total().max(1);
            self.prepare.audition.pending_seek_frac =
                Some(shared.playhead_frames() as f64 / total as f64);
            self.load_audition(id);
        }
    }

    /// Keyboard shortcuts: deck A = Q (play) W (cue) E (4-beat loop)
    /// R (exit loop) 1–4 (hot cues); deck B = P O I U 7–0. Suppressed while
    /// a text field has focus.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            // Don't leave a FLASH stuck on when focus moves to a text field.
            self.flash_key = [false; LANE_COUNT];
            for o in self.programmer.iter_mut() {
                o.flash_held = false;
            }
            return;
        }
        use egui::Key;
        // V toggles Perform ↔ Prepare; the engine is untouched by the view.
        // Patch is a setup screen, not part of the performance flip — V
        // just returns to Perform from there.
        if ctx.input(|i| i.key_pressed(Key::V)) {
            self.view = match self.view {
                View::Perform => View::Prepare,
                View::Prepare | View::Patch => View::Perform,
            };
        }
        // Programmer: Z/X/C = momentary FLASH per lane (suppressed under
        // ⌘ so ⌘C copy doesn't fire the FX lane); Esc = CLEAR, except in
        // Prepare where a non-empty editor selection clears first.
        const FLASH_KEYS: [Key; LANE_COUNT] = [Key::Z, Key::X, Key::C];
        self.flash_key = ctx.input(|i| {
            if i.modifiers.command {
                [false; LANE_COUNT]
            } else {
                FLASH_KEYS.map(|k| i.key_down(k))
            }
        });
        for (i, o) in self.programmer.iter_mut().enumerate() {
            o.flash_held = self.flash_key[i];
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.view == View::Prepare && !self.prepare.selection.is_empty() {
                self.prepare.selection.clear();
            } else {
                programmer::clear(&mut self.programmer);
            }
        }
        if self.view == View::Prepare {
            self.prepare_editor_keys(ctx);
        }
        const KEYS: [[Key; 8]; 2] = [
            [
                Key::Q,
                Key::W,
                Key::E,
                Key::R,
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
            ],
            [
                Key::P,
                Key::O,
                Key::I,
                Key::U,
                Key::Num7,
                Key::Num8,
                Key::Num9,
                Key::Num0,
            ],
        ];
        let down = ctx.input(|i| KEYS.map(|deck| deck.map(|k| i.key_down(k))));
        for d in 0..2 {
            let prev = self.kb_prev[d];
            let now = down[d];
            if now[0] && !prev[0] {
                self.toggle_play_synced(d);
            }
            let deck_ui = &mut self.decks[d];
            if now[1] && !prev[1] {
                deck_ui.cue_press();
            }
            if !now[1] && prev[1] {
                deck_ui.cue_release();
            }
            if now[2] && !prev[2] {
                deck_ui.autoloop(4.0);
            }
            if now[3] && !prev[3] {
                deck_ui.exit_loop();
            }
            for slot in 0..4 {
                if now[4 + slot] && !prev[4 + slot] {
                    deck_ui.hot_cue_press(slot);
                }
            }
            self.kb_prev[d] = now;
        }
    }

    /// Prepare-view editor keys: Delete removes the typed selection
    /// (look events, energy breakpoints, accents). Esc is handled by the
    /// caller, layered with programmer CLEAR. Cue copy/paste was dropped
    /// with the classic lanes; it returns with the full L3 landing.
    fn prepare_editor_keys(&mut self, ctx: &egui::Context) {
        use egui::Key;
        let del = ctx.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace));
        let PrepareState {
            audition,
            selection,
            ..
        } = &mut self.prepare;
        if !del || selection.is_empty() {
            return;
        }
        let mut accents = std::collections::HashSet::new();
        for &sel in selection.iter() {
            match sel {
                ShowSel::Look(id) => audition.show.looks.remove(id),
                ShowSel::Energy(id) => audition.show.energy.remove(id),
                ShowSel::Accent(id) => {
                    accents.insert(id);
                }
            }
        }
        audition.show.accents.remove(&accents);
        selection.clear();
        let (show, track_id) = (audition.show.clone(), audition.track_id);
        if let Some(id) = track_id {
            self.sync_show(id, &show);
        }
    }

    /// Publish the cold lighting inputs to the DMX engine thread. The
    /// thread reads the playhead atomics itself; the beat reference lets
    /// it extrapolate musical time between publishes so effects stay
    /// beat-locked through UI stalls.
    fn publish_dmx(&self, ctx: &egui::Context) {
        let d = &self.decks[self.lighting_deck];
        let playhead = d.playhead() as f64;
        let beat_ref = match (
            d.marks.beat_at_or_before(playhead),
            beat_phase(&d.marks, playhead),
        ) {
            (Some(i), Some(ph)) => dmx::BeatRef {
                beat_t: i as f64 + ph,
                playhead,
                frames_per_beat: d.marks.median_beat_frames(),
            },
            _ => {
                // No grid: wall-clock at the master BPM, matching the
                // programmer's effect previews. Advances per publish only.
                let bpm = {
                    let b = self.global_bpm();
                    if b > 0.0 { b } else { 120.0 }
                };
                dmx::BeatRef {
                    beat_t: ctx.input(|i| i.time) * bpm / 60.0,
                    playhead,
                    frames_per_beat: 0.0,
                }
            }
        };
        *self.dmx.lock().unwrap() = Some(dmx::DmxSnapshot {
            rig: self.rig.clone(),
            cues: d.deck.track.is_some().then(|| d.cues.clone()),
            overrides: self.programmer.clone(),
            params: self.programmer_params.clone(),
            selection: self.fixture_selection.clone(),
            deck: d.deck.shared.clone(),
            beat_ref,
            net: self.net.clone(),
        });
    }

    /// The PATCH view: console-style patch sheet, staged. Edits build a
    /// draft; the live rig, DMX output, and programmer keep the
    /// last-applied patch until APPLY swaps the draft in and persists it
    /// to the show file. REVERT discards the draft.
    fn patch_ui(&mut self, ui: &mut egui::Ui) {
        const WARN: egui::Color32 = egui::Color32::from_rgb(230, 90, 70);
        let mut work = self
            .patch_draft
            .clone()
            .unwrap_or_else(|| (*self.rig).clone());
        let dirty = self.patch_draft.is_some();
        let conflicts = work.conflicts(&self.fixture_library);
        // Stable, name-sorted profile list for the combos.
        let mut profiles: Vec<(String, String)> = self
            .fixture_library
            .profiles
            .iter()
            .map(|(id, p)| (id.clone(), p.to_string()))
            .collect();
        profiles.sort_by(|a, b| a.1.cmp(&b.1));

        let mut changed = false;
        let mut remove: Option<u32> = None;
        let mut do_apply = false;
        let mut do_revert = false;
        {
            let fl = &self.fixture_library;
            let rig = &mut work;
            ui.horizontal(|ui| {
                ui.menu_button("+ ADD FIXTURE", |ui| {
                    for kind in ALL_KINDS {
                        if ui.button(kind.group_label()).clicked() {
                            let id = rig.next_id();
                            let count = rig.iter().filter(|f| f.kind == kind).count();
                            let row = rig.extent().1;
                            // First free address on universe 1.
                            let addr = rig
                                .iter()
                                .filter(|f| f.universe == 1)
                                .filter_map(|f| {
                                    fl.get(&f.profile_id)
                                        .map(|p| f.start_address + p.footprint() as u16)
                                })
                                .max()
                                .unwrap_or(1);
                            rig.fixtures_mut().push(halo_light::fixture::Fixture {
                                id,
                                kind,
                                label: format!("{}{}", kind.short(), count + 1),
                                col: 0,
                                row,
                                profile_id: kind.default_profile_id().to_string(),
                                universe: 1,
                                start_address: addr.min(512),
                            });
                            changed = true;
                            ui.close_menu();
                        }
                    }
                });
                if ui.button("RESET TO DEFAULT RIG").clicked() {
                    *rig = default_rig(fl);
                    changed = true;
                }
                if !conflicts.is_empty() {
                    ui.colored_label(
                        WARN,
                        format!("{} fixtures with patch conflicts", conflicts.len()),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can_apply = dirty && conflicts.is_empty();
                    let apply = ui.add_enabled(
                        can_apply,
                        egui::Button::new(egui::RichText::new("APPLY").strong()),
                    );
                    let apply = if dirty && !conflicts.is_empty() {
                        apply.on_disabled_hover_text("Resolve patch conflicts first")
                    } else {
                        apply.on_disabled_hover_text("No unapplied changes")
                    };
                    if apply.clicked() {
                        do_apply = true;
                    }
                    if ui
                        .add_enabled(dirty, egui::Button::new("REVERT"))
                        .on_hover_text("Discard the draft; keep the applied patch")
                        .clicked()
                    {
                        do_revert = true;
                    }
                    if dirty {
                        ui.colored_label(
                            egui::Color32::from_rgb(240, 200, 90),
                            "● unapplied changes",
                        );
                    }
                });
            });
            ui.add_space(4.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("patch_sheet")
                    .striped(true)
                    .min_col_width(44.0)
                    .show(ui, |ui| {
                        for h in [
                            "FIXTURE", "KIND", "PROFILE", "UNIV", "ADDR", "COL", "ROW", "",
                        ] {
                            ui.label(egui::RichText::new(h).weak().size(10.0));
                        }
                        ui.end_row();
                        for f in rig.fixtures_mut().iter_mut() {
                            let bad = conflicts.contains(&f.id);
                            if bad {
                                ui.colored_label(WARN, format!("⚠ {}", f.label));
                            } else {
                                ui.label(&f.label);
                            }
                            egui::ComboBox::from_id_salt(("patch-kind", f.id))
                                .selected_text(f.kind.group_label())
                                .show_ui(ui, |ui| {
                                    for kind in ALL_KINDS {
                                        if ui
                                            .selectable_label(f.kind == kind, kind.group_label())
                                            .clicked()
                                            && f.kind != kind
                                        {
                                            f.kind = kind;
                                            changed = true;
                                        }
                                    }
                                });
                            let profile_text = fl
                                .get(&f.profile_id)
                                .map(|p| p.to_string())
                                .unwrap_or_else(|| format!("? {}", f.profile_id));
                            egui::ComboBox::from_id_salt(("patch-profile", f.id))
                                .selected_text(profile_text)
                                .width(230.0)
                                .show_ui(ui, |ui| {
                                    for (pid, name) in &profiles {
                                        if ui.selectable_label(&f.profile_id == pid, name).clicked()
                                            && &f.profile_id != pid
                                        {
                                            f.profile_id = pid.clone();
                                            changed = true;
                                        }
                                    }
                                });
                            changed |= ui
                                .add(egui::DragValue::new(&mut f.universe).range(1..=32))
                                .changed();
                            changed |= ui
                                .add(egui::DragValue::new(&mut f.start_address).range(1..=512))
                                .changed();
                            changed |= ui
                                .add(egui::DragValue::new(&mut f.col).range(0..=15))
                                .changed();
                            changed |= ui
                                .add(egui::DragValue::new(&mut f.row).range(0..=15))
                                .changed();
                            if ui.small_button("✕").on_hover_text("Unpatch").clicked() {
                                remove = Some(f.id);
                            }
                            ui.end_row();
                        }
                    });
            });

            if let Some(id) = remove {
                rig.fixtures_mut().retain(|f| f.id != id);
                changed = true;
            }
        }
        if do_revert {
            self.patch_draft = None;
        } else if do_apply {
            // Commit: swap the draft in, then the same live-update path
            // instant edits used to take — selection may reference
            // unpatched ids, routing may cover new universes.
            self.patch_draft = None;
            self.rig = std::sync::Arc::new(work);
            let rig = std::sync::Arc::clone(&self.rig);
            self.fixture_selection
                .retain(|id| rig.iter().any(|f| f.id == *id));
            self.net = build_net(&self.artnet, &rig);
            self.save_rig();
        } else if changed || dirty {
            self.patch_draft = Some(work);
        }
    }

    /// Persist the current rig patch to the library DB.
    fn save_rig(&self) {
        let Some(lib) = &self.library else { return };
        match serde_json::to_string(&RigFile::from_rig(&self.rig)) {
            Ok(json) => {
                if let Err(e) = lib.store_setting("rig_patch", &json) {
                    log::warn!("save rig patch: {e}");
                }
            }
            Err(e) => log::warn!("serialize rig patch: {e}"),
        }
    }

    /// Persist the Art-Net settings and rebuild the engine's config.
    fn apply_artnet(&mut self) {
        self.net = build_net(&self.artnet, &self.rig);
        if let Some(lib) = &self.library {
            match serde_json::to_string(&self.artnet) {
                Ok(json) => {
                    if let Err(e) = lib.store_setting("artnet", &json) {
                        log::warn!("save artnet settings: {e}");
                    }
                }
                Err(e) => log::warn!("serialize artnet settings: {e}"),
            }
        }
    }

    /// The footer's Programmer tab: group-select row, the fixture grid
    /// (select fixtures → apply values, console-style; value application
    /// comes with the fixture engine), and the lane override controls.
    /// Returns true when STORE was pressed.
    fn programmer_ui(&mut self, ui: &mut egui::Ui, outputs: &[LaneOutput; LANE_COUNT]) -> bool {
        let deck = &self.decks[self.lighting_deck];
        let can_store = deck.track_id.is_some() && deck.marks.is_usable();
        let deck_name = DECK_NAMES[self.lighting_deck];

        // Musical time in beats for the effect previews: the lighting
        // deck's grid when available, wall-clock at the master BPM
        // otherwise (so previews always animate).
        let playhead = deck.playhead() as f64;
        let beat_t = match (
            deck.marks.beat_at_or_before(playhead),
            beat_phase(&deck.marks, playhead),
        ) {
            (Some(i), Some(ph)) => i as f64 + ph,
            _ => {
                let bpm = {
                    let b = self.global_bpm();
                    if b > 0.0 { b } else { 120.0 }
                };
                ui.input(|i| i.time) * bpm / 60.0
            }
        };

        let mut cx = ProgrammerCtx {
            rig: &self.rig,
            selection: &mut self.fixture_selection,
            overrides: &mut self.programmer,
            params: &mut self.programmer_params,
            outputs,
            can_store,
            deck_name,
            beat_t,
        };
        programmer_panel(ui, &mut cx)
    }

    /// STORE: write every active programmer lane into the lighting deck's
    /// track as a cue at the current bar (1 bar for Lighting/Pixels — they
    /// read as states — 1 beat for FX hits), then persist + sync.
    fn store_programmer(&mut self) {
        let deck = &self.decks[self.lighting_deck];
        let Some(track_id) = deck.track_id else {
            return;
        };
        let Some(start) = deck.marks.bar_start(deck.playhead() as f64) else {
            return;
        };
        let beat = deck.marks.median_beat_frames();
        if beat <= 0.0 {
            return;
        }
        let mut cues = deck.cues.clone();
        let mut stored = 0;
        for lane in ALL_LANES {
            let o = &self.programmer[lane as usize];
            if o.active()
                && cues
                    .insert(
                        lane,
                        start,
                        match lane {
                            Lane::Fx => beat,
                            _ => 4.0 * beat,
                        },
                        o.intensity,
                    )
                    .is_some()
            {
                stored += 1;
            }
        }
        if stored > 0 {
            let bar = deck.marks.bar_beat(start).map(|(b, _)| b).unwrap_or(0);
            self.commit_cues(track_id, &cues);
            self.status = format!("Stored {stored} cue(s) at bar {bar}");
        }
    }

    /// Sample process-wide CPU roughly once a second.
    fn update_cpu(&mut self) {
        let (last_at, last_cpu, _) = self.cpu_sample;
        let elapsed = last_at.elapsed().as_secs_f64();
        if elapsed >= 1.0 {
            let cpu_now = process_cpu_secs();
            let pct = ((cpu_now - last_cpu) / elapsed * 100.0) as f32;
            self.cpu_sample = (std::time::Instant::now(), cpu_now, pct.max(0.0));
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Settings")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Audio output").strong());
                let current = self
                    .audio_settings
                    .device_name
                    .clone()
                    .unwrap_or_else(|| "System default".to_string());
                egui::ComboBox::from_label("Device")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                self.audio_settings.device_name.is_none(),
                                "System default",
                            )
                            .clicked()
                        {
                            self.audio_settings.device_name = None;
                        }
                        for name in &self.available_devices {
                            if ui
                                .selectable_label(
                                    self.audio_settings.device_name.as_deref() == Some(name),
                                    name,
                                )
                                .clicked()
                            {
                                self.audio_settings.device_name = Some(name.clone());
                            }
                        }
                    });

                let buffer_label = match self.audio_settings.buffer_size {
                    Some(n) => format!("{n} frames"),
                    None => "Default".to_string(),
                };
                egui::ComboBox::from_label("Buffer size")
                    .selected_text(buffer_label)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.audio_settings.buffer_size.is_none(), "Default")
                            .clicked()
                        {
                            self.audio_settings.buffer_size = None;
                        }
                        for n in [64u32, 128, 256, 512, 1024, 2048] {
                            if ui
                                .selectable_label(
                                    self.audio_settings.buffer_size == Some(n),
                                    format!("{n} frames"),
                                )
                                .clicked()
                            {
                                self.audio_settings.buffer_size = Some(n);
                            }
                        }
                    });

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Applying restarts the audio stream and reloads decks.")
                        .weak()
                        .size(11.0),
                );
                if ui.button("Apply").clicked() {
                    apply = true;
                }

                ui.add_space(12.0);
                ui.separator();
                ui.label(egui::RichText::new("Art-Net output").strong());
                let was_broadcast = self.artnet.unicast_ip.is_none();
                let mut artnet_dirty = false;
                ui.horizontal(|ui| {
                    if ui.selectable_label(was_broadcast, "Broadcast").clicked() && !was_broadcast {
                        self.artnet.unicast_ip = None;
                        artnet_dirty = true;
                    }
                    if ui.selectable_label(!was_broadcast, "Unicast").clicked() && was_broadcast {
                        self.artnet.unicast_ip = Some(self.artnet_ip_edit.clone());
                        artnet_dirty = true;
                    }
                });
                if self.artnet.unicast_ip.is_some() {
                    ui.horizontal(|ui| {
                        ui.label("Node IP");
                        let resp = ui.text_edit_singleline(&mut self.artnet_ip_edit);
                        let valid = self.artnet_ip_edit.parse::<std::net::IpAddr>().is_ok();
                        if !valid {
                            ui.colored_label(
                                egui::Color32::from_rgb(230, 90, 70),
                                "invalid address",
                            );
                        } else if resp.lost_focus()
                            && self.artnet.unicast_ip.as_deref()
                                != Some(self.artnet_ip_edit.as_str())
                        {
                            self.artnet.unicast_ip = Some(self.artnet_ip_edit.clone());
                            artnet_dirty = true;
                        }
                    });
                }
                ui.label(egui::RichText::new(self.net.summary()).weak().size(11.0));
                if artnet_dirty {
                    self.apply_artnet();
                }
            });
        self.settings_open = open;
        if apply {
            self.rebuild_audio();
        }
    }

    fn refresh_browser(&mut self) {
        if !self.browser.dirty {
            return;
        }
        self.browser.dirty = false;
        if let Some(lib) = &self.library {
            self.browser.playlists = lib.playlists().unwrap_or_default();
            self.browser.rows = lib
                .tracks(
                    self.browser.selected,
                    &self.browser.search,
                    self.browser.sort,
                    self.browser.ascending,
                )
                .unwrap_or_default();
        }
    }

    /// The bottom slide-up footer, home of both the library browser and
    /// the programmer, toggled by tabs so the programmer lives in one
    /// consistent place across views. Returns true when STORE was pressed.
    fn footer_panel(&mut self, ctx: &egui::Context, lighting: &[LaneOutput; LANE_COUNT]) -> bool {
        let mut actions: Vec<BrowserAction> = Vec::new();
        let mut store = false;
        egui::TopBottomPanel::bottom("browser")
            .resizable(true)
            .default_height(300.0)
            .min_height(140.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for (tab, label) in [
                        (FooterTab::Library, "LIBRARY"),
                        (FooterTab::Programmer, "PROGRAMMER"),
                    ] {
                        if ui
                            .selectable_label(
                                self.footer_tab == tab,
                                egui::RichText::new(label).size(11.0),
                            )
                            .clicked()
                        {
                            self.footer_tab = tab;
                        }
                    }
                    lighting_leds(ui, lighting);
                });
                ui.add_space(4.0);
                match self.footer_tab {
                    FooterTab::Library => {
                        if self.library.is_none() {
                            ui.centered_and_justified(|ui| {
                                ui.label(egui::RichText::new("Library unavailable").weak());
                            });
                            return;
                        }
                        let view = self.view;
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(190.0);
                                playlist_tree(ui, &mut self.browser, &mut actions);
                            });
                            ui.separator();
                            ui.vertical(|ui| {
                                track_table(ui, &mut self.browser, &mut actions, view);
                            });
                        });
                    }
                    FooterTab::Programmer => {
                        ui.add_space(4.0);
                        store = self.programmer_ui(ui, lighting);
                    }
                }
            });

        for action in actions {
            self.apply_browser_action(action);
        }
        store
    }

    /// Single write path for every cue mutation: persist to the library,
    /// then propagate clones to every player holding the same track — this
    /// is what keeps a performance deck's lane strip live-updating while
    /// the same track is edited in Prepare.
    fn commit_cues(&mut self, track_id: i64, cues: &CueSet) {
        if let Some(lib) = &self.library
            && let Err(e) = lib.store_cues(track_id, &cues.to_file(self.device_rate()))
        {
            log::error!("store cues: {e}");
        }
        for d in &mut self.decks {
            if d.track_id == Some(track_id) {
                d.cues = cues.clone();
            }
        }
        if self.prepare.audition.track_id == Some(track_id) {
            self.prepare.audition.cues = cues.clone();
        }
    }

    /// Session-only analogue of [`commit_cues`](Self::commit_cues) for the
    /// L3 show preview: propagate an edited show to every player holding
    /// the same track. No persistence — the preview dies with the session.
    fn sync_show(&mut self, track_id: i64, show: &ShowPreview) {
        for d in &mut self.decks {
            if d.track_id == Some(track_id) {
                d.show = show.clone();
            }
        }
        if self.prepare.audition.track_id == Some(track_id) {
            self.prepare.audition.show = show.clone();
        }
    }

    /// The Prepare view's central panel: audition transport plus the same
    /// waveform stack as a deck, with the direct-manipulation cue-lane
    /// editor in the middle.
    fn prepare_panel(&mut self, ui: &mut egui::Ui) {
        let sample_rate = self.device_rate();
        let has_audio = self.audio.is_some();
        let PrepareState {
            audition,
            selection,
            interaction,
            snap,
            armed_look,
        } = &mut self.prepare;
        let shared = audition.deck.shared.clone();
        let has_track = audition.deck.track.is_some();
        let total = shared.total();
        // Scrub-aware signed position, same as deck_panel: dips below 0 in
        // the elastic lead-in. Only the zoomed waveform + lanes editor see
        // the sign; every other consumer uses the clamped `playhead`.
        let display_pos = if let Some(pos) = audition.scrub_pos {
            pos.min(total as f64)
        } else if shared.scrub.phase() == ScrubPhase::Settling {
            shared.scrub.voice_frame().min(total as f64)
        } else {
            shared.playhead_frames().min(total) as f64
        };
        let playhead = display_pos.max(0.0) as usize;
        let playing = shared.transport() == Transport::Playing;

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if has_track {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&audition.title).strong().size(16.0));
                    ui.label(egui::RichText::new(audition.artist.as_deref().unwrap_or("—")).weak());
                });
            } else {
                ui.label(
                    egui::RichText::new(
                        "Load a track from the library below (EDIT or double-click) \
                         to author its lighting cues",
                    )
                    .weak(),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if audition.bpm > 0.0 {
                    ui.label(
                        egui::RichText::new(format!("{:.1} BPM", audition.bpm))
                            .monospace()
                            .color(ACCENT),
                    );
                    ui.add_space(10.0);
                }
                ui.label(
                    egui::RichText::new(format_time(playhead, sample_rate))
                        .monospace()
                        .size(16.0),
                );
            });
        });
        ui.add_space(6.0);

        // Transport + audition volume (this channel bypasses the
        // crossfader; the fader alone is its level).
        ui.add_enabled_ui(has_track, |ui| {
            ui.horizontal(|ui| {
                let play_label = if playing { "⏸" } else { "▶" };
                if ui
                    .add_sized(
                        [50.0, 30.0],
                        egui::Button::new(
                            egui::RichText::new(play_label)
                                .size(16.0)
                                .color(egui::Color32::from_rgb(90, 220, 120)),
                        )
                        .fill(egui::Color32::from_rgb(32, 56, 40)),
                    )
                    .clicked()
                {
                    audition.toggle_play();
                }
                let cue_resp = ui.add_sized(
                    [50.0, 30.0],
                    egui::Button::new(
                        egui::RichText::new("CUE")
                            .size(13.0)
                            .color(egui::Color32::from_rgb(255, 215, 70)),
                    )
                    .fill(egui::Color32::from_rgb(58, 50, 26)),
                );
                let cue_down = cue_resp.is_pointer_button_down_on();
                let pressed = cue_down && !audition.cue_was_down;
                let released = !cue_down && audition.cue_was_down;
                audition.cue_was_down = cue_down;
                if pressed {
                    audition.cue_press();
                }
                if released {
                    audition.cue_release();
                }
                ui.separator();
                let mut vol = shared.fader.load();
                if ui
                    .add(
                        egui::Slider::new(&mut vol, 0.0..=1.0)
                            .show_value(false)
                            .text("VOL"),
                    )
                    .on_hover_text("Audition volume (independent of the crossfader)")
                    .changed()
                {
                    shared.fader.store(vol);
                }
            });
        });
        ui.add_space(8.0);

        // Waveform stack — same painters as a deck, full width.
        let loop_region = shared.loop_region();
        let gesture = paint_zoomed(
            ui,
            ZoomedParams {
                peaks: audition.peaks.as_ref(),
                marks: &audition.marks,
                position_frames: display_pos,
                total_frames: total,
                sample_rate,
                loop_region,
                loop_in: audition.loop_in_staged,
                hot_cues: &[],
                cue_point: has_track.then(|| shared.cue_point.load(Ordering::Relaxed) as usize),
                ghost: None,
            },
            &mut audition.zoom,
        );
        handle_scrub_gesture(
            audition,
            gesture,
            has_track,
            has_audio,
            playing,
            display_pos,
            sample_rate,
        );

        ui.add_space(2.0);
        let mut mutated = show_editor(
            ui,
            ShowEditorParams {
                marks: &audition.marks,
                position_frames: display_pos,
                total_frames: total,
                sample_rate,
                snap: *snap,
                armed_look: *armed_look,
            },
            &audition.zoom,
            &mut audition.show,
            selection,
            interaction,
        );

        // Inspector row: snap, look palette, selection tools, reset/clear.
        ui.add_space(4.0);
        ui.add_enabled_ui(has_track, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(*snap, "SNAP")
                    .on_hover_text("Snap edits to the beat grid")
                    .clicked()
                {
                    *snap = !*snap;
                }
                ui.separator();
                // Look palette: click arms the look for new events and
                // reassigns any selected look events.
                let selected_looks: Vec<u64> = selection
                    .iter()
                    .filter_map(|&s| match s {
                        ShowSel::Look(id) => Some(id),
                        _ => None,
                    })
                    .collect();
                for (i, look) in LOOK_PALETTE.iter().enumerate() {
                    let armed = armed_look.0 == i;
                    let mut swatch = egui::Button::new("")
                        .fill(look.color.gamma_multiply(0.85))
                        .min_size(egui::vec2(18.0, 18.0));
                    if armed {
                        swatch = swatch.stroke(egui::Stroke::new(2.0_f32, egui::Color32::WHITE));
                    }
                    let resp = ui.add(swatch).on_hover_text(look.name);
                    if resp.clicked() {
                        *armed_look = LookId(i);
                        if !selected_looks.is_empty() {
                            for &id in &selected_looks {
                                audition.show.looks.set_look(id, LookId(i));
                            }
                            mutated = true;
                        }
                    }
                }
                ui.separator();
                let selected_accents: std::collections::HashSet<u64> = selection
                    .iter()
                    .filter_map(|&s| match s {
                        ShowSel::Accent(id) => Some(id),
                        _ => None,
                    })
                    .collect();
                if selection.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "Drag in LOOK to place the armed look · drag in ENERGY to shape \
                             the arc · draw one-shots in ACCENT",
                        )
                        .weak()
                        .size(11.0),
                    );
                } else {
                    if let [id] = selected_looks[..]
                        && let Some(ev) = audition.show.looks.find(id)
                    {
                        ui.label(
                            egui::RichText::new(ev.look.def().name)
                                .size(11.0)
                                .color(ev.look.def().color)
                                .strong(),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(format!("{} selected", selection.len()))
                                .size(11.0)
                                .strong(),
                        );
                    }
                    if !selected_accents.is_empty() {
                        let mut intensity = selected_accents
                            .iter()
                            .next()
                            .and_then(|&id| audition.show.accents.find(id))
                            .map(|(_, c)| c.intensity)
                            .unwrap_or(1.0);
                        let resp = ui
                            .add(
                                egui::Slider::new(&mut intensity, 0.0..=1.0)
                                    .show_value(false)
                                    .text("INT"),
                            )
                            .on_hover_text("Intensity of the selected accent(s)");
                        if resp.changed() {
                            for &id in &selected_accents {
                                audition.show.accents.set_intensity(id, intensity);
                            }
                        }
                        if resp.drag_stopped() {
                            mutated = true;
                        }
                    }
                    if ui.button("Delete").clicked() {
                        for &s in selection.iter() {
                            match s {
                                ShowSel::Look(id) => audition.show.looks.remove(id),
                                ShowSel::Energy(id) => audition.show.energy.remove(id),
                                ShowSel::Accent(_) => {}
                            }
                        }
                        audition.show.accents.remove(&selected_accents);
                        selection.clear();
                        mutated = true;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button("Clear ▾", |ui| {
                        if ui.button("Looks").clicked() {
                            audition.show.looks.clear();
                            mutated = true;
                            ui.close_menu();
                        }
                        if ui.button("Energy").clicked() {
                            audition.show.energy.clear();
                            mutated = true;
                            ui.close_menu();
                        }
                        if ui.button("Accents").clicked() {
                            audition.show.accents = CueSet::empty();
                            mutated = true;
                            ui.close_menu();
                        }
                        if ui.button("All").clicked() {
                            audition.show = ShowPreview::default();
                            selection.clear();
                            mutated = true;
                            ui.close_menu();
                        }
                    });
                    if ui
                        .button("Reset demo show")
                        .on_hover_text(
                            "Re-seed the look / energy / accent lanes with the simulated \
                             show, then edit",
                        )
                        .clicked()
                    {
                        audition.show = simulate_show_l3(
                            &audition.marks,
                            total,
                            sample_rate,
                            audition.track_id.unwrap_or(1) as u64,
                        );
                        selection.clear();
                        mutated = true;
                    }
                });
            });
        });
        ui.add_space(4.0);

        if let Some(frac) = paint_overview(
            ui,
            OverviewParams {
                texture: audition.overview.as_ref(),
                progress: if total > 0 {
                    playhead as f32 / total as f32
                } else {
                    0.0
                },
                total_frames: total,
                loop_region,
                loop_in: audition.loop_in_staged,
                hot_cues: &[],
                marks: &audition.marks,
            },
        ) && has_track
        {
            request_seek_guarded(&shared, (frac as f64 * total as f64) as usize);
        }

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            paint_beat_counter(ui, &audition.marks, display_pos);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("+").clicked() {
                    audition.zoom.zoom_in();
                }
                ui.label(
                    egui::RichText::new(audition.zoom.label(audition.marks.is_usable()))
                        .weak()
                        .size(11.0),
                );
                if ui.small_button("−").clicked() {
                    audition.zoom.zoom_out();
                }
            });
        });

        // Drag-and-drop target: the whole editor area accepts library
        // tracks (mirrors the deck columns in Perform).
        let mut dropped: Option<i64> = None;
        if egui::DragAndDrop::has_payload_of_type::<DragTrack>(ui.ctx()) {
            let rect = ui.min_rect();
            if ui.rect_contains_pointer(rect) {
                ui.painter().rect_stroke(
                    rect.expand(2.0),
                    6.0,
                    egui::Stroke::new(2.0, ACCENT),
                    egui::StrokeKind::Inside,
                );
                if ui.input(|i| i.pointer.any_released())
                    && let Some(drag) = egui::DragAndDrop::take_payload::<DragTrack>(ui.ctx())
                {
                    dropped = Some(drag.track_id);
                }
            }
        }

        // Session sync: every completed mutation propagates to any deck
        // holding the same track (no persistence — the L3 preview is
        // session-only).
        let track_id = audition.track_id;
        let dirty = if mutated {
            Some(audition.show.clone())
        } else {
            None
        };
        if let (Some(show), Some(id)) = (dirty, track_id) {
            self.sync_show(id, &show);
        }
        if let Some(id) = dropped {
            self.load_audition(id);
        }
    }

    /// Confirmation dialog for "Remove from library" — the FK cascade also
    /// deletes authored lighting cues, so this is not a one-click action.
    fn remove_confirm_window(&mut self, ctx: &egui::Context) {
        let Some((id, title)) = self.pending_remove.clone() else {
            return;
        };
        let mut open = true;
        let mut decided = false;
        let mut remove = false;
        egui::Window::new("Remove from library")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(format!("Remove “{title}” from the library?"));
                ui.label(
                    egui::RichText::new(
                        "Authored lighting cues will be deleted.\n\
                         The audio file on disk is not touched.",
                    )
                    .weak(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Remove").clicked() {
                        decided = true;
                        remove = true;
                    }
                    if ui.button("Cancel").clicked() {
                        decided = true;
                    }
                });
            });
        if !open || decided {
            self.pending_remove = None;
        }
        if remove {
            self.remove_track_from_library(id, &title);
        }
    }

    fn remove_track_from_library(&mut self, id: i64, title: &str) {
        let Some(lib) = &self.library else { return };
        if let Err(e) = lib.delete_track(id) {
            log::error!("{e}");
            return;
        }
        // Detach any deck still holding the row: audio keeps playing, it
        // is just no longer DB-backed (cue edits would otherwise hit the
        // dropped foreign key).
        for deck_ui in self.decks.iter_mut() {
            if deck_ui.track_id == Some(id) {
                deck_ui.track_id = None;
            }
        }
        if self.prepare.audition.track_id == Some(id) {
            self.prepare.audition.track_id = None;
        }
        self.status = format!("Removed {title} from library");
        self.browser.dirty = true;
    }

    fn apply_browser_action(&mut self, action: BrowserAction) {
        let Some(lib) = &self.library else { return };
        match action {
            BrowserAction::LoadDeck(deck, track) => {
                self.load_track_row(deck, track);
                return;
            }
            BrowserAction::LoadAudition(track) => {
                self.load_audition(track);
                return;
            }
            BrowserAction::SelectPlaylist(p) => self.browser.selected = p,
            BrowserAction::SortBy(col) => {
                if self.browser.sort == col {
                    self.browser.ascending = !self.browser.ascending;
                } else {
                    self.browser.sort = col;
                    self.browser.ascending = true;
                }
            }
            BrowserAction::SearchChanged => {}
            BrowserAction::NewPlaylist(is_folder) => {
                let base = if is_folder {
                    "New Folder"
                } else {
                    "New Playlist"
                };
                let name = format!("{base} {}", self.browser.playlists.len() + 1);
                let parent = self
                    .browser
                    .selected
                    .and_then(|id| self.browser.playlists.iter().find(|p| p.id == id))
                    .map(|p| if p.is_folder { Some(p.id) } else { p.parent_id })
                    .unwrap_or(None);
                if let Err(e) = lib.create_playlist(&name, parent, is_folder) {
                    log::error!("{e}");
                }
            }
            BrowserAction::CommitRename(id, name) => {
                if let Err(e) = lib.rename_playlist(id, &name) {
                    log::error!("{e}");
                }
                self.browser.rename = None;
            }
            BrowserAction::DeletePlaylist(id) => {
                if let Err(e) = lib.delete_playlist(id) {
                    log::error!("{e}");
                }
                if self.browser.selected == Some(id) {
                    self.browser.selected = None;
                }
            }
            BrowserAction::AddToPlaylist(playlist, track) => {
                if let Err(e) = lib.add_to_playlist(playlist, track) {
                    log::error!("{e}");
                }
            }
            BrowserAction::RemoveFromPlaylist(playlist, track) => {
                if let Err(e) = lib.remove_from_playlist(playlist, track) {
                    log::error!("{e}");
                }
            }
            BrowserAction::Reanalyze(track) => {
                if let Err(e) = lib.clear_analysis(track) {
                    log::error!("{e}");
                    return;
                }
                // The worker also polls every 5 s; the wake just makes it
                // immediate.
                let _ = self.wake_tx.send(());
                self.status = "Reanalyzing…".to_string();
            }
            BrowserAction::RemoveFromLibrary(track, title) => {
                // No DB change yet — the confirm dialog commits it.
                self.pending_remove = Some((track, title));
                return;
            }
            BrowserAction::ImportFolder(dir) => {
                self.import_folder(dir);
                return;
            }
        }
        self.browser.dirty = true;
    }

    fn any_active(&self) -> bool {
        self.decks
            .iter()
            .chain(std::iter::once(&self.prepare.audition))
            .any(|d| {
                d.deck.shared.transport() == Transport::Playing
                    || d.decode_rx.is_some()
                    // A scrub glide animates the playhead even while paused,
                    // so it keeps the repaint loop alive too.
                    || d.deck.shared.scrub.phase() != ScrubPhase::Idle
                    // A ghost playhead keeps fading even if the deck was
                    // paused right after the sync-aligned start.
                    || d.ghost.is_some()
            })
    }

    /// Play/pause for deck `i`. When starting a synced non-master deck while
    /// the master plays, seek to the nearest phase-aligned frame first so the
    /// audio starts on beat, and kick off the ghost-playhead slide-in.
    fn toggle_play_synced(&mut self, i: usize) {
        let d = &self.decks[i];
        let starting = d.deck.track.is_some() && d.deck.shared.transport() != Transport::Playing;
        let aligned_start = starting
            && d.synced
            && i != self.master
            // Don't fight a scrub glide's own landing seek.
            && d.deck.shared.scrub.phase() == ScrubPhase::Idle
            && self.decks[self.master].deck.shared.transport() == Transport::Playing;
        if !aligned_start {
            self.decks[i].toggle_play();
            return;
        }
        let m = &self.decks[self.master];
        let master_phase = beat_phase(&m.marks, m.playhead() as f64);
        let d = &self.decks[i];
        let total = d.deck.shared.total();
        // Mirror toggle_play's EOF rewind: align as if starting from zero.
        let from = if total > 0 && d.playhead() >= total {
            0.0
        } else {
            d.playhead() as f64
        };
        let target = master_phase.and_then(|mp| align_target_frame(&d.marks, from, mp, total));
        let Some(target) = target else {
            // No usable grid on one of the decks: plain start, no jump.
            self.decks[i].toggle_play();
            return;
        };
        let d = &mut self.decks[i];
        d.cue_previewing = false;
        request_seek_guarded(&d.deck.shared, target);
        d.deck.shared.set_transport(Transport::Playing);
        // The jump invalidates any smoothed error history (same as engaging
        // sync does).
        d.phase_err = None;
        let delta = from - target as f64;
        d.ghost = (delta.abs() >= GHOST_MIN_DELTA_FRAMES).then(|| GhostAnim::new(delta));
    }
}

impl eframe::App for HaloApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(
            storage,
            PERSIST_KEY,
            &Persisted {
                master_volume: self.mixer.master.load(),
                crossfader: self.mixer.crossfader.load(),
                trims: [
                    self.decks[0].deck.shared.trim.load(),
                    self.decks[1].deck.shared.trim.load(),
                ],
                keylocks: [self.decks[0].keylock, self.decks[1].keylock],
                pitch_ranges: [self.decks[0].pitch_range, self.decks[1].pitch_range],
                quantize: [self.decks[0].quantize, self.decks[1].quantize],
                gated: [self.decks[0].gated, self.decks[1].gated],
                sort: Some(self.browser.sort),
                ascending: self.browser.ascending,
                device_name: self.audio_settings.device_name.clone(),
                buffer_size: self.audio_settings.buffer_size,
                view: self.view,
                audition_volume: self.prepare.audition.deck.shared.fader.load(),
                snap_off: !self.prepare.snap,
                auto_cue_off: [!self.decks[0].auto_cue, !self.decks[1].auto_cue],
                footer_tab: self.footer_tab,
            },
        );
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Scrub glide bookkeeping: each newly published landing fires the
        // parallel engine warm-start, so the engine is primed at the
        // predicted frame by the time the glide hands back to it.
        for deck_ui in self
            .decks
            .iter_mut()
            .chain(std::iter::once(&mut self.prepare.audition))
        {
            let shared = &deck_ui.deck.shared;
            let (landing_seq, landing) = shared.scrub.landing();
            if landing_seq != deck_ui.landing_seq_seen {
                deck_ui.landing_seq_seen = landing_seq;
                // `.max(0.0)`: a landing is >= 0 by contract, but a negative
                // f64 cast to usize would wrap into a garbage seek.
                request_seek_guarded(shared, landing.max(0.0) as usize);
            }
            if deck_ui.ghost.as_ref().is_some_and(GhostAnim::finished) {
                deck_ui.ghost = None;
            }
        }
        self.poll_decodes(ctx);
        self.poll_worker_events();
        self.refresh_browser();
        self.handle_shortcuts(ctx);
        self.update_tempo(ctx.input(|i| i.stable_dt).min(0.1) as f64);
        // Lighting auto-follows the deck the master BPM is sourced from.
        self.lighting_deck = self.master_source();
        self.update_cpu();

        if self.any_active() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        } else if self.footer_tab == FooterTab::Programmer {
            // Only the effect preview is animating: a slower tick keeps
            // the dot moving without paying the full-window 30 fps
            // repaint cost (which re-tessellates the deck waveforms and
            // was burning ~20% CPU on idle decks).
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        } else {
            // Idle tick so worker events (analysis, imports) still land.
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }

        // Resolve the lighting output stack once per frame — every
        // indicator (toolbar LEDs, lane tints, hollow bars) derives from
        // this one result so provenance stays consistent.
        let lighting = {
            let d = &self.decks[self.lighting_deck];
            let cues = d.deck.track.is_some().then_some(&d.cues);
            programmer::resolve(&self.programmer, cues, d.playhead() as f64)
        };
        // Same inputs, other consumer: the DMX engine thread re-resolves
        // on its own clock between UI frames.
        self.publish_dmx(ctx);

        let global_bpm = self.global_bpm();
        let master_src = self.master_source();
        let mixer_cx = self.mixer_center_x;
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(6.0);
            let toolbar_row = ui
                .horizontal(|ui| {
                    ui.heading(egui::RichText::new("HALO").color(ACCENT).strong());
                    ui.add_space(10.0);
                    for (v, label) in [
                        (View::Perform, "PERFORM"),
                        (View::Prepare, "PREPARE"),
                        (View::Patch, "PATCH"),
                    ] {
                        if ui
                            .selectable_label(self.view == v, egui::RichText::new(label).size(11.0))
                            .on_hover_text("Switch view (V) — playback is unaffected")
                            .clicked()
                        {
                            self.view = v;
                        }
                    }
                    // The audition player keeps playing across view
                    // switches; outside Prepare it has no other UI, so
                    // surface it with a pause chip.
                    if self.view != View::Prepare
                        && self.prepare.audition.deck.shared.transport() == Transport::Playing
                        && ui
                            .button(egui::RichText::new("AUDITION ▶").size(11.0).color(ACCENT))
                            .on_hover_text(
                                "The Prepare audition player is running — click to pause",
                            )
                            .clicked()
                    {
                        self.prepare.audition.toggle_play();
                    }
                    ui.separator();
                    lighting_leds(ui, &lighting);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("⚙").on_hover_text("Audio settings").clicked() {
                            self.settings_open = !self.settings_open;
                            if self.settings_open {
                                self.available_devices = list_output_devices();
                            }
                        }
                        ui.separator();
                        cpu_meter(ui, self.mixer.cpu_load.load(), self.cpu_sample.2);
                        ui.separator();
                        // Right-to-left layout: the output bar lands between
                        // the DSP cluster and the master knob, i.e. just to
                        // the knob's right.
                        master_level_meter(ui, self.mixer.master_meter.load());
                        ui.add_space(6.0);
                        let mut master = self.mixer.master.load();
                        if ui
                            .add(
                                Knob::new(&mut master, 0.0..=1.0, ACCENT)
                                    .arc(KnobArc::Unipolar)
                                    .default_value(1.0)
                                    .diameter(24.0),
                            )
                            .on_hover_text(format!("Master: {:.0}%", master * 100.0))
                            .changed()
                        {
                            self.mixer.master.store(master);
                        }
                        ui.label(egui::RichText::new("Master").weak().size(11.0));
                    });
                })
                .response
                .rect;
            ui.add_space(6.0);

            // Global/master BPM, centered over the mixer. Uses the mixer's
            // actual center-x captured last frame in the central panel (egui
            // rects are absolute screen coords); falls back to the panel
            // mid-x until measured. Painted, not laid out, to dodge egui's
            // horizontal-centering quirks.
            let cx = if mixer_cx > 0.0 {
                mixer_cx
            } else {
                ui.max_rect().center().x
            };
            let cy = toolbar_row.center().y;
            let bpm_text = if global_bpm > 0.0 {
                format!("{global_bpm:.1}")
            } else {
                "—".to_string()
            };
            // Boxed readout: "A  MASTER BPM / value  B". A/B light blue for
            // the deck the master BPM (and lighting rig) is sourced from —
            // automatic, not clickable.
            let border = ui.visuals().widgets.noninteractive.bg_stroke.color;
            let dim = ui.visuals().weak_text_color().gamma_multiply(0.6);
            let lit = crate::waveform::palette::LANE_LIGHTING;
            let box_rect =
                egui::Rect::from_center_size(egui::pos2(cx, cy), egui::vec2(132.0, 30.0));
            let painter = ui.painter();
            painter.rect_stroke(
                box_rect,
                4.0,
                egui::Stroke::new(1.0, border),
                egui::StrokeKind::Outside,
            );
            painter.text(
                egui::pos2(cx, box_rect.top() + 7.0),
                egui::Align2::CENTER_CENTER,
                "MASTER BPM",
                egui::FontId::proportional(8.0),
                dim,
            );
            let vy = box_rect.top() + 20.0;
            painter.text(
                egui::pos2(box_rect.left() + 13.0, vy),
                egui::Align2::CENTER_CENTER,
                "A",
                egui::FontId::proportional(13.0),
                if master_src == 0 { lit } else { dim },
            );
            painter.text(
                egui::pos2(cx, vy),
                egui::Align2::CENTER_CENTER,
                bpm_text,
                egui::FontId::monospace(18.0),
                egui::Color32::WHITE,
            );
            painter.text(
                egui::pos2(box_rect.right() - 13.0, vy),
                egui::Align2::CENTER_CENTER,
                "B",
                egui::FontId::proportional(13.0),
                if master_src == 1 { lit } else { dim },
            );
        });

        self.settings_window(ctx);
        self.remove_confirm_window(ctx);

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&self.status).weak());
            ui.add_space(4.0);
        });

        // The patch sheet gets the whole window: no slide-up footer there.
        if self.view != View::Patch && self.footer_panel(ctx, &lighting) {
            self.store_programmer();
        }

        if self.view == View::Prepare {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.prepare_panel(ui);
            });
            return;
        }

        if self.view == View::Patch {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add_space(6.0);
                self.patch_ui(ui);
            });
            return;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let full_width = ui.available_width();
            // Sized to just fit the mixer content (119 px strip cluster) with
            // ~5 px breathing room each side, rather than a wide centered panel.
            let mixer_width = 130.0;
            // The row holds 5 children (deck | sep | mixer | sep | deck):
            // 4 item-spacing gaps plus 2 separators (6 pt each in egui).
            let spacing = ui.spacing().item_spacing.x;
            let chrome = 4.0 * spacing + 2.0 * 6.0;
            let deck_width = ((full_width - mixer_width - chrome) / 2.0)
                .floor()
                .max(280.0);

            let sample_rate = self.device_rate();
            let master = self.master;
            let lighting_deck = self.lighting_deck;
            let has_audio = self.audio.is_some();
            let mut responses: [DeckPanelResponse; 2] = Default::default();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(deck_width);
                    responses[0] = deck_panel(
                        ui,
                        &mut self.decks[0],
                        0,
                        sample_rate,
                        master == 0,
                        (lighting_deck == 0).then_some(&lighting),
                        has_audio,
                    );
                });
                ui.separator();
                let mixer_rect = ui
                    .vertical(|ui| {
                        ui.set_width(mixer_width);
                        mixer_panel(ui, &self.mixer, &self.decks);
                    })
                    .response
                    .rect;
                self.mixer_center_x = mixer_rect.center().x;
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(deck_width);
                    responses[1] = deck_panel(
                        ui,
                        &mut self.decks[1],
                        1,
                        sample_rate,
                        master == 1,
                        (lighting_deck == 1).then_some(&lighting),
                        has_audio,
                    );
                });
            });
            for (i, resp) in responses.into_iter().enumerate() {
                if resp.master_clicked {
                    self.master = i;
                    // The master leads; it can't also follow.
                    self.decks[i].synced = false;
                }
                if resp.play_toggled {
                    self.toggle_play_synced(i);
                }
                // Engaging sync jumps straight onto the master's beat (at
                // most half a beat, the short way); the PLL holds the lock
                // from there.
                if resp.sync_engaged && i != self.master {
                    let m = &self.decks[self.master];
                    let master_phase = beat_phase(&m.marks, m.playhead() as f64);
                    if let Some(mp) = master_phase {
                        let d = &self.decks[i];
                        let from = d.playhead() as f64;
                        if let Some(target) =
                            align_target_frame(&d.marks, from, mp, d.deck.shared.total())
                        {
                            request_seek_guarded(&d.deck.shared, target);
                            // Same visual jump as an aligned play start, so
                            // it gets the same ghost slide-in.
                            let delta = from - target as f64;
                            self.decks[i].ghost = (delta.abs() >= GHOST_MIN_DELTA_FRAMES)
                                .then(|| GhostAnim::new(delta));
                        }
                    }
                    // The jump invalidates any smoothed error history.
                    self.decks[i].phase_err = None;
                }
                if let Some(path) = resp.load_path {
                    self.import_and_load(i, path);
                }
                if let Some(track_id) = resp.load_track_id {
                    self.load_track_row(i, track_id);
                }
            }
        });
    }
}

/// Decode + resample + tag-read, all off the UI thread. With a stored
/// library artifact the beat grid comes straight from it (rescaled to the
/// device rate); otherwise a quick detection fills in until the analysis
/// worker delivers the real thing.
fn load_track_data(
    path: &Path,
    device_rate: u32,
    track_id: Option<i64>,
    key: Option<String>,
    stored: Option<PreAnalysisArtifact>,
) -> DecodeResult {
    let decoded = decode_file(path)?;
    let buffer = AudioBuffer::new(decoded.samples, decoded.sample_rate, Channels::Stereo)
        .resample(device_rate);

    let peaks = BandPeaks::compute(&buffer.data, 2, device_rate);
    let (grid, artifact) = match stored {
        Some(native) => {
            let resampled = native.resample_to(device_rate);
            (grid_from_artifact(&resampled), Some(Arc::new(resampled)))
        }
        None => {
            let grid = timestretch::detect_beat_grid_buffer(&buffer);
            log::info!(
                "Quick BPM: {:.1} ({} beats, confidence {:.2})",
                grid.bpm,
                grid.beats.len(),
                grid.confidence
            );
            (grid, None)
        }
    };

    let (title, artist, tag_key, artwork) = read_tags(path);
    let title = title.unwrap_or_else(|| {
        path.file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string())
    });

    Ok(LoadedData {
        title,
        artist,
        key: key.or(tag_key),
        track_id,
        artwork,
        samples: Arc::new(buffer.into_data()),
        peaks,
        grid,
        artifact,
    })
}

/// Display beat grid from a stored analysis artifact (same rate domain).
fn grid_from_artifact(artifact: &PreAnalysisArtifact) -> BeatGrid {
    let mut grid = BeatGrid::empty(artifact.sample_rate);
    grid.beats = if !artifact.beat_positions_fractional.is_empty() {
        artifact.beat_positions_fractional.clone()
    } else {
        artifact.beat_positions.iter().map(|&p| p as f64).collect()
    };
    grid.downbeats = artifact.downbeat_beat_indices.clone();
    grid.bpm = artifact.bpm;
    grid.confidence = artifact.confidence;
    grid
}

/// Best-effort tag read: title, artist, musical key, and embedded artwork.
fn read_tags(
    path: &Path,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<egui::ColorImage>,
) {
    use lofty::prelude::*;

    let tagged = match lofty::probe::Probe::open(path).and_then(|p| p.read()) {
        Ok(t) => t,
        Err(e) => {
            log::info!("No readable tags in {}: {e}", path.display());
            return (None, None, None, None);
        }
    };
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return (None, None, None, None);
    };

    let title = tag.title().map(|s| s.into_owned());
    let artist = tag.artist().map(|s| s.into_owned());
    let key = tag
        .get_string(&lofty::tag::ItemKey::InitialKey)
        .map(|s| s.to_string());
    let artwork =
        tag.pictures()
            .first()
            .and_then(|pic| match image::load_from_memory(pic.data()) {
                Ok(img) => {
                    let thumb = img.thumbnail(ARTWORK_MAX_PX, ARTWORK_MAX_PX).to_rgba8();
                    let size = [thumb.width() as usize, thumb.height() as usize];
                    Some(egui::ColorImage::from_rgba_unmultiplied(
                        size,
                        thumb.as_raw(),
                    ))
                }
                Err(e) => {
                    log::info!("Could not decode artwork: {e}");
                    None
                }
            });
    (title, artist, key, artwork)
}

/// Drag-and-drop payload for a library row dragged from the browser.
#[derive(Clone)]
struct DragTrack {
    track_id: i64,
    title: String, // carried so the drag preview needs no library lookup
}

enum BrowserAction {
    LoadDeck(usize, i64),
    /// Load into the Prepare view's audition player for cue editing.
    LoadAudition(i64),
    SelectPlaylist(Option<i64>),
    SortBy(SortColumn),
    SearchChanged,
    NewPlaylist(bool),
    CommitRename(i64, String),
    DeletePlaylist(i64),
    AddToPlaylist(i64, i64),
    RemoveFromPlaylist(i64, i64),
    /// Clear the stored analysis and wake the worker to redo it.
    Reanalyze(i64),
    /// Ask to delete a track row (id, title) — confirmed via a dialog
    /// because the cascade also drops authored lighting cues.
    RemoveFromLibrary(i64, String),
    ImportFolder(PathBuf),
}

/// Left side of the browser: Library root + playlist tree with inline
/// rename and context menus.
fn playlist_tree(ui: &mut egui::Ui, browser: &mut BrowserState, actions: &mut Vec<BrowserAction>) {
    if ui
        .selectable_label(browser.selected.is_none(), "🗄 Library")
        .clicked()
    {
        actions.push(BrowserAction::SelectPlaylist(None));
    }

    // Folders first, then root-level playlists.
    let folders: Vec<PlaylistRow> = browser
        .playlists
        .iter()
        .filter(|p| p.is_folder)
        .cloned()
        .collect();
    let playlists: Vec<PlaylistRow> = browser
        .playlists
        .iter()
        .filter(|p| !p.is_folder)
        .cloned()
        .collect();

    for folder in &folders {
        egui::CollapsingHeader::new(format!("🗀 {}", folder.name))
            .id_salt(folder.id)
            .default_open(true)
            .show(ui, |ui| {
                for pl in playlists.iter().filter(|p| p.parent_id == Some(folder.id)) {
                    playlist_row(ui, browser, pl, actions);
                }
            })
            .header_response
            .context_menu(|ui| {
                if ui.button("Delete folder").clicked() {
                    actions.push(BrowserAction::DeletePlaylist(folder.id));
                    ui.close_menu();
                }
            });
    }
    for pl in playlists
        .iter()
        .filter(|p| p.parent_id.is_none() || !folders.iter().any(|f| Some(f.id) == p.parent_id))
    {
        playlist_row(ui, browser, pl, actions);
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.small_button("+ List").clicked() {
            actions.push(BrowserAction::NewPlaylist(false));
        }
        if ui.small_button("+ Folder").clicked() {
            actions.push(BrowserAction::NewPlaylist(true));
        }
    });
    if ui.small_button("⬇ Import folder…").clicked()
        && let Some(dir) = rfd::FileDialog::new().pick_folder()
    {
        actions.push(BrowserAction::ImportFolder(dir));
    }
}

fn playlist_row(
    ui: &mut egui::Ui,
    browser: &mut BrowserState,
    pl: &PlaylistRow,
    actions: &mut Vec<BrowserAction>,
) {
    // Inline rename in progress?
    if let Some((rename_id, buffer)) = &mut browser.rename
        && *rename_id == pl.id
    {
        let resp = ui.text_edit_singleline(buffer);
        if resp.lost_focus() {
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !buffer.is_empty() {
                actions.push(BrowserAction::CommitRename(pl.id, buffer.clone()));
            } else {
                browser.rename = None;
            }
        }
        return;
    }

    let resp = ui.selectable_label(browser.selected == Some(pl.id), format!("♪ {}", pl.name));
    if resp.clicked() {
        actions.push(BrowserAction::SelectPlaylist(Some(pl.id)));
    }
    resp.context_menu(|ui| {
        if ui.button("Rename").clicked() {
            browser.rename = Some((pl.id, pl.name.clone()));
            ui.close_menu();
        }
        if ui.button("Delete").clicked() {
            actions.push(BrowserAction::DeletePlaylist(pl.id));
            ui.close_menu();
        }
    });
}

/// Right side of the browser: search box + sortable track table.
fn track_table(
    ui: &mut egui::Ui,
    browser: &mut BrowserState,
    actions: &mut Vec<BrowserAction>,
    view: View,
) {
    ui.horizontal(|ui| {
        ui.label("🔍");
        if ui
            .add(
                egui::TextEdit::singleline(&mut browser.search)
                    .hint_text("Search title, artist, album")
                    .desired_width(240.0),
            )
            .changed()
        {
            actions.push(BrowserAction::SearchChanged);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{} tracks", browser.rows.len()))
                    .weak()
                    .size(11.0),
            );
        });
    });
    ui.add_space(4.0);

    use egui_extras::{Column, TableBuilder};
    let header = |ui: &mut egui::Ui,
                  label: &str,
                  col: SortColumn,
                  browser: &BrowserState,
                  actions: &mut Vec<BrowserAction>| {
        let arrow = if browser.sort == col {
            if browser.ascending { " ▲" } else { " ▼" }
        } else {
            ""
        };
        if ui
            .add(
                egui::Label::new(egui::RichText::new(format!("{label}{arrow}")).strong())
                    .sense(egui::Sense::click()),
            )
            .clicked()
        {
            actions.push(BrowserAction::SortBy(col));
        }
    };

    let playlists: Vec<(i64, String)> = browser
        .playlists
        .iter()
        .filter(|p| !p.is_folder)
        .map(|p| (p.id, p.name.clone()))
        .collect();
    let selected_playlist = browser.selected;

    // Text selection in cells would show an I-beam cursor and steal drags
    // from the row; the whole row must act as one draggable unit.
    ui.style_mut().interaction.selectable_labels = false;

    TableBuilder::new(ui)
        .striped(true)
        .sense(egui::Sense::click_and_drag())
        .column(Column::exact(58.0)) // load buttons
        .column(Column::remainder().at_least(140.0)) // title
        .column(Column::initial(140.0).at_least(80.0)) // artist
        .column(Column::initial(120.0).at_least(60.0)) // album
        .column(Column::exact(52.0)) // bpm
        .column(Column::exact(44.0)) // key
        .column(Column::exact(48.0)) // time
        .header(20.0, |mut h| {
            h.col(|_| {});
            h.col(|ui| header(ui, "Title", SortColumn::Title, browser, actions));
            h.col(|ui| header(ui, "Artist", SortColumn::Artist, browser, actions));
            h.col(|ui| header(ui, "Album", SortColumn::Album, browser, actions));
            h.col(|ui| header(ui, "BPM", SortColumn::Bpm, browser, actions));
            h.col(|ui| header(ui, "Key", SortColumn::Key, browser, actions));
            h.col(|ui| header(ui, "Time", SortColumn::Duration, browser, actions));
        })
        .body(|body| {
            body.rows(20.0, browser.rows.len(), |mut row| {
                let track = &browser.rows[row.index()];
                row.col(|ui| {
                    ui.horizontal(|ui| match view {
                        View::Perform | View::Patch => {
                            if ui
                                .small_button("A")
                                .on_hover_text("Load to deck A")
                                .clicked()
                            {
                                actions.push(BrowserAction::LoadDeck(0, track.id));
                            }
                            if ui
                                .small_button("B")
                                .on_hover_text("Load to deck B")
                                .clicked()
                            {
                                actions.push(BrowserAction::LoadDeck(1, track.id));
                            }
                        }
                        View::Prepare => {
                            if ui
                                .small_button("EDIT")
                                .on_hover_text("Open in the cue editor")
                                .clicked()
                            {
                                actions.push(BrowserAction::LoadAudition(track.id));
                            }
                        }
                    });
                });
                row.col(|ui| {
                    ui.label(&track.title);
                });
                row.col(|ui| {
                    ui.label(track.artist.as_deref().unwrap_or("—"));
                });
                row.col(|ui| {
                    ui.label(track.album.as_deref().unwrap_or("—"));
                });
                row.col(|ui| {
                    ui.label(
                        egui::RichText::new(
                            track
                                .bpm
                                .map(|b| format!("{b:.1}"))
                                .unwrap_or_else(|| "—".to_string()),
                        )
                        .monospace(),
                    );
                });
                row.col(|ui| {
                    ui.label(track.key.as_deref().unwrap_or("—"));
                });
                row.col(|ui| {
                    ui.label(
                        egui::RichText::new(
                            track
                                .duration_secs
                                .map(|s| format!("{}:{:02}", s as u64 / 60, s as u64 % 60))
                                .unwrap_or_else(|| "—".to_string()),
                        )
                        .monospace(),
                    );
                });
                let row_resp = row.response();
                // Right-click anywhere on the row (the menu used to live on
                // the Title cell only).
                row_resp.context_menu(|ui| {
                    ui.menu_button("Add to playlist", |ui| {
                        for (id, name) in &playlists {
                            if ui.button(name).clicked() {
                                actions.push(BrowserAction::AddToPlaylist(*id, track.id));
                                ui.close_menu();
                            }
                        }
                        if playlists.is_empty() {
                            ui.label(egui::RichText::new("No playlists").weak());
                        }
                    });
                    if let Some(pl) = selected_playlist
                        && ui.button("Remove from this playlist").clicked()
                    {
                        actions.push(BrowserAction::RemoveFromPlaylist(pl, track.id));
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Reanalyze").clicked() {
                        actions.push(BrowserAction::Reanalyze(track.id));
                        ui.close_menu();
                    }
                    if ui.button("Remove from library…").clicked() {
                        actions.push(BrowserAction::RemoveFromLibrary(
                            track.id,
                            track.title.clone(),
                        ));
                        ui.close_menu();
                    }
                });
                if view == View::Prepare && row_resp.double_clicked() {
                    actions.push(BrowserAction::LoadAudition(track.id));
                }
                // Rows drag in both views: onto a deck in Perform, onto
                // the cue editor in Prepare.
                if row_resp.drag_started() {
                    egui::DragAndDrop::set_payload(
                        &row_resp.ctx,
                        DragTrack {
                            track_id: track.id,
                            title: track.title.clone(),
                        },
                    );
                }
            });
        });

    // Floating chip that follows the cursor while a track is being dragged.
    if let Some(drag) = egui::DragAndDrop::payload::<DragTrack>(ui.ctx())
        && let Some(pos) = ui.ctx().pointer_interact_pos()
    {
        egui::Area::new(egui::Id::new("track_drag_preview"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pos + egui::vec2(14.0, 10.0))
            .interactable(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(format!("♪ {}", drag.title));
                });
            });
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
}

#[derive(Default)]
struct DeckPanelResponse {
    load_path: Option<PathBuf>,
    load_track_id: Option<i64>,
    master_clicked: bool,
    sync_engaged: bool,
    /// Play/pause was pressed; handled at app level so a synced start can
    /// beat-align against the master deck first.
    play_toggled: bool,
}

/// Three always-visible dots answering "what is the rig doing right now":
/// lane color at output level; a white ring marks a programmer override
/// (vs a solid track cue).
fn lighting_leds(ui: &mut egui::Ui, outputs: &[LaneOutput; LANE_COUNT]) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(52.0, 16.0), egui::Sense::hover());
    resp.on_hover_text("Lighting output: solid = track cue, ring = programmer override");
    let painter = ui.painter();
    for (i, out) in outputs.iter().enumerate() {
        let (_, _, color) = crate::waveform::LANES[i];
        let c = egui::pos2(rect.left() + 8.0 + i as f32 * 18.0, rect.center().y);
        painter.circle_filled(c, 5.0, color.gamma_multiply(0.15 + 0.85 * out.level));
        if out.source == LaneSource::Programmer {
            painter.circle_stroke(c, 6.5, egui::Stroke::new(1.5_f32, egui::Color32::WHITE));
        }
    }
}

/// Apply a zoomed-waveform drag gesture to a deck's scrub state: grab the
/// platter, chase the hand while dragging, release into a momentum glide
/// (shared by the performance decks and the Prepare audition player).
/// Elastic lead-in depth for platter scrubs: one beat, or ~0.5 s of frames
/// when the track has no usable grid.
fn scrub_lead_in(marks: &GridMarks, sample_rate: u32) -> f64 {
    let beat = marks.median_beat_frames();
    if beat > 0.0 {
        beat
    } else {
        0.5 * sample_rate as f64
    }
}

fn handle_scrub_gesture(
    deck_ui: &mut DeckUi,
    gesture: Option<ScrubGesture>,
    has_track: bool,
    has_audio: bool,
    playing: bool,
    grab_pos: f64,
    sample_rate: u32,
) {
    let shared = deck_ui.deck.shared.clone();
    let total = shared.total();
    match gesture {
        Some(ScrubGesture::Grab) => {
            if has_track && total > 0 {
                // `grab_pos` is scrub-aware at the caller, so re-grabbing a
                // mid-glide platter continues from the voice's gliding
                // position — including inside the lead-in — not the stale
                // engine playhead.
                let lead_in = scrub_lead_in(&deck_ui.marks, sample_rate);
                deck_ui.scrub_lead_in = lead_in;
                deck_ui.scrub_pos = Some(grab_pos);
                shared.scrub.begin(grab_pos, lead_in);
            }
        }
        Some(ScrubGesture::Drag(delta)) => {
            if let Some(pos) = deck_ui.scrub_pos {
                let target =
                    (pos + delta).clamp(-deck_ui.scrub_lead_in, total.saturating_sub(1) as f64);
                shared.scrub.update_target(target);
                deck_ui.scrub_pos = Some(target);
            }
        }
        Some(ScrubGesture::Release) => {
            if let Some(frame) = deck_ui.scrub_pos.take() {
                if has_audio {
                    // Momentum glide: the audio callback eases the voice
                    // toward play speed (or rest), predicts the landing,
                    // and the landing consumer in `update` warm-starts the
                    // engine there in parallel.
                    let rate = if playing {
                        shared.tempo_rate.load() as f64
                    } else {
                        0.0
                    };
                    shared.scrub.release(rate);
                } else {
                    // No audio stream to render a glide — land instantly.
                    shared.scrub.cancel();
                    request_seek_guarded(&shared, frame.max(0.0) as usize);
                }
            }
        }
        None => {}
    }
}

/// Renders one deck column.
fn deck_panel(
    ui: &mut egui::Ui,
    deck_ui: &mut DeckUi,
    idx: usize,
    sample_rate: u32,
    is_master: bool,
    // Some exactly when this deck drives the lighting rig.
    lighting_outputs: Option<&[LaneOutput; LANE_COUNT]>,
    has_audio: bool,
) -> DeckPanelResponse {
    let is_lighting = lighting_outputs.is_some();
    let mut response = DeckPanelResponse::default();

    ui.add_space(8.0);

    let shared = deck_ui.deck.shared.clone();
    let has_track = deck_ui.deck.track.is_some();
    let total = shared.total();
    // Scrub-aware position: during a drag the UI owns the displayed
    // position (the hand target); during the release glide the audio
    // callback's voice does. Everything downstream — waveform, overview,
    // time readouts, quantized cues — follows the platter. `display_pos`
    // keeps the sign (it dips below 0 in the elastic lead-in) for the
    // waveform painters; every other consumer uses the clamped `playhead`.
    let display_pos = if let Some(pos) = deck_ui.scrub_pos {
        pos.min(total as f64)
    } else if shared.scrub.phase() == ScrubPhase::Settling {
        shared.scrub.voice_frame().min(total as f64)
    } else {
        shared.playhead_frames().min(total) as f64
    };
    let playhead = display_pos.max(0.0) as usize;
    let transport = shared.transport();
    let playing = transport == Transport::Playing;
    // Display rate: slider pitch × bend × throw momentum, without the
    // sync PLL's micro corrections — those are meant to be inaudible and
    // invisible, and showing them makes a locked deck look like it's
    // hunting. The throw is shown deliberately: the BPM dipping and
    // settling is the platter feedback.
    let tempo_rate = (1.0 + deck_ui.pitch_percent as f64 / 100.0) * deck_ui.bend as f64;

    // Header: artwork | title/artist/key | elapsed + remaining readouts.
    ui.horizontal(|ui| {
        let art_size = egui::vec2(ARTWORK_SIZE, ARTWORK_SIZE);
        match &deck_ui.artwork {
            Some(tex) => {
                ui.add(
                    egui::Image::new((tex.id(), art_size))
                        .corner_radius(4.0)
                        .fit_to_exact_size(art_size),
                );
            }
            None => {
                let (rect, _) = ui.allocate_exact_size(art_size, egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, 4.0, egui::Color32::from_rgb(28, 28, 32));
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "♪",
                    egui::FontId::proportional(24.0),
                    egui::Color32::from_rgb(80, 80, 90),
                );
            }
        }
        ui.add_space(4.0);
        // Fixed value-column widths, sized to the widest values so the digits
        // don't jitter the layout (the "-" prefix only shows on REMAINING).
        let value_w = |text: &str| {
            ui.fonts(|f| {
                f.layout_no_wrap(
                    text.to_owned(),
                    egui::FontId::monospace(20.0),
                    egui::Color32::WHITE,
                )
                .size()
                .x
            })
        };
        let time_value_w = value_w("-88:88.8");
        let bpm_value_w = value_w("888.8");
        // Title/artist, width-constrained and truncated so a long name can't
        // grow into (and overlap) the right-aligned time + BPM readouts.
        let title_w = (ui.available_width() - time_value_w - bpm_value_w - 72.0).max(60.0);
        ui.allocate_ui_with_layout(
            egui::vec2(title_w, ARTWORK_SIZE),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.add_space(4.0);
                if has_track {
                    ui.add(
                        egui::Label::new(egui::RichText::new(&deck_ui.title).strong().size(13.0))
                            .truncate(),
                    );
                    if let Some(artist) = &deck_ui.artist {
                        ui.add(egui::Label::new(egui::RichText::new(artist).weak()).truncate());
                    }
                    if let Some(key) = &deck_ui.key {
                        ui.add_space(2.0);
                        key_badge(ui, key);
                    }
                } else {
                    ui.label(egui::RichText::new("No track loaded").weak().size(13.0));
                }
            },
        );
        // Right side (right-to-left): BPM readout rightmost, then the time
        // readout with the ⏱ button toggling elapsed/remaining. Each value is
        // a small caption over a big monospace number. BPM is amber while this
        // deck is the master tempo reference, white otherwise.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let value_label = |ui: &mut egui::Ui, w: f32, caption: &str, value: String, color| {
                ui.allocate_ui_with_layout(
                    egui::vec2(w, ARTWORK_SIZE),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.add_space(10.0);
                        section_caption(ui, caption);
                        ui.label(
                            egui::RichText::new(value)
                                .color(color)
                                .strong()
                                .monospace()
                                .size(20.0),
                        );
                    },
                );
            };

            ui.add_space(8.0);
            let bpm_text = if deck_ui.bpm > 0.0 && has_track {
                format!("{:.1}", deck_ui.bpm * tempo_rate)
            } else {
                "0.0".to_string()
            };
            let bpm_color = if is_master {
                ACCENT
            } else {
                egui::Color32::WHITE
            };
            value_label(ui, bpm_value_w, "BPM", bpm_text, bpm_color);

            ui.add_space(12.0);
            if ui
                .small_button("⏱")
                .on_hover_text("Toggle elapsed / remaining")
                .clicked()
            {
                deck_ui.show_remaining = !deck_ui.show_remaining;
            }
            ui.add_space(4.0);
            let (caption, value) = if deck_ui.show_remaining {
                (
                    "REMAINING",
                    format!(
                        "-{}",
                        format_time(total.saturating_sub(playhead), sample_rate)
                    ),
                )
            } else {
                ("ELAPSED", format_time(playhead, sample_rate))
            };
            value_label(ui, time_value_w, caption, value, egui::Color32::WHITE);
        });
    });
    ui.add_space(6.0);

    // Waveform on the left; the BPM / keylock / master-sync / pitch sidebar
    // carves out a fixed column on the right (like a hardware deck's pitch
    // strip).
    let loop_region = shared.loop_region();
    ui.horizontal(|ui| {
        const RIGHT_W: f32 = 60.0;
        let total_w = ui.available_width();
        let wave = ui.vertical(|ui| {
            ui.set_width((total_w - RIGHT_W - 8.0).max(200.0));

            // Zoomed scrolling waveform. Dragging grabs the platter: the
            // audio callback's varispeed voice audibly chases the hand
            // (both directions); the drop releases the momentum into a
            // glide that eases back to play speed (or rest), handing off
            // to a warm-started engine at the predicted landing.
            let gesture = paint_zoomed(
                ui,
                ZoomedParams {
                    peaks: deck_ui.peaks.as_ref(),
                    marks: &deck_ui.marks,
                    position_frames: display_pos,
                    total_frames: total,
                    sample_rate,
                    loop_region,
                    loop_in: deck_ui.loop_in_staged,
                    hot_cues: &deck_ui.hot_cues,
                    cue_point: has_track.then(|| shared.cue_point.load(Ordering::Relaxed) as usize),
                    ghost: deck_ui.ghost.as_ref().map(GhostAnim::params),
                },
                &mut deck_ui.zoom,
            );
            handle_scrub_gesture(
                deck_ui,
                gesture,
                has_track,
                has_audio,
                playing,
                display_pos,
                sample_rate,
            );

            // L3 show lanes (look / energy / accent), scrolling in
            // lockstep with the zoomed view above.
            ui.add_space(2.0);
            paint_show_strip(
                ui,
                ShowStripParams {
                    show: &deck_ui.show,
                    marks: &deck_ui.marks,
                    position_frames: display_pos,
                    total_frames: total,
                    sample_rate,
                    lighting_active: is_lighting,
                    programmer_override: lighting_outputs
                        .is_some_and(|o| o.iter().any(|l| l.source == LaneSource::Programmer)),
                },
                &deck_ui.zoom,
            );
            ui.add_space(4.0);

            // Full-track overview with click-to-seek.
            if let Some(frac) = paint_overview(
                ui,
                OverviewParams {
                    texture: deck_ui.overview.as_ref(),
                    progress: if total > 0 {
                        playhead as f32 / total as f32
                    } else {
                        0.0
                    },
                    total_frames: total,
                    loop_region,
                    loop_in: deck_ui.loop_in_staged,
                    hot_cues: &deck_ui.hot_cues,
                    marks: &deck_ui.marks,
                },
            ) && has_track
            {
                request_seek_guarded(&shared, (frac as f64 * total as f64) as usize);
            }

            // Beat counter | zoom | time readouts.
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                paint_beat_counter(ui, &deck_ui.marks, display_pos);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("+").clicked() {
                        deck_ui.zoom.zoom_in();
                    }
                    ui.label(
                        egui::RichText::new(deck_ui.zoom.label(deck_ui.marks.is_usable()))
                            .weak()
                            .size(11.0),
                    );
                    if ui.small_button("−").clicked() {
                        deck_ui.zoom.zoom_out();
                    }
                });
            });
        });
        let wave_h = wave.response.rect.height();
        ui.vertical(|ui| {
            ui.set_width(RIGHT_W);
            deck_sidebar(
                ui,
                deck_ui,
                idx,
                is_master,
                has_track,
                wave_h,
                &mut response,
            );
        });
    });

    ui.add_space(8.0);
    ui.add_enabled_ui(has_track, |ui| {
        // Row 1: captioned groups — transport (play / cue), nudge, loops —
        // all buttons at one consistent height.
        const GROUP_H: f32 = 50.0;
        ui.horizontal(|ui| {
            control_group(ui, "TRANSPORT", |ui| {
                // PLAY/PAUSE — green accents.
                let play_label = if playing { "⏸" } else { "▶" };
                if ui
                    .add_sized(
                        [70.0, 30.0],
                        egui::Button::new(
                            egui::RichText::new(play_label)
                                .size(18.0)
                                .color(egui::Color32::from_rgb(90, 220, 120)),
                        )
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgb(90, 220, 120),
                        ))
                        .fill(egui::Color32::from_rgb(32, 56, 40)),
                    )
                    .clicked()
                {
                    response.play_toggled = true;
                }

                // CUE — CDJ semantics on press/release edges; amber outline.
                let cue_resp = ui.add_sized(
                    [70.0, 30.0],
                    egui::Button::new(egui::RichText::new("CUE").size(15.0).color(ACCENT))
                        .stroke(egui::Stroke::new(1.0, ACCENT))
                        .fill(ACCENT_FILL),
                );
                let cue_down = cue_resp.is_pointer_button_down_on();
                let pressed = cue_down && !deck_ui.cue_was_down;
                let released = !cue_down && deck_ui.cue_was_down;
                deck_ui.cue_was_down = cue_down;

                if pressed {
                    deck_ui.cue_press();
                }
                if released {
                    deck_ui.cue_release();
                }
            });

            group_divider(ui, GROUP_H);

            control_group(ui, "NUDGE", |ui| {
                // Pitch bend: momentary ±4% while held.
                let bend_minus = ui
                    .add_sized([28.0, 30.0], egui::Button::new("−"))
                    .is_pointer_button_down_on();
                let bend_plus = ui
                    .add_sized([28.0, 30.0], egui::Button::new("+"))
                    .is_pointer_button_down_on();
                deck_ui.bend = if bend_minus {
                    0.96
                } else if bend_plus {
                    1.04
                } else {
                    1.0
                };
                if bend_minus || bend_plus {
                    ui.ctx().request_repaint();
                }
            });

            group_divider(ui, GROUP_H);

            control_group(ui, "LOOP", |ui| {
                // Loops: manual in/out, quantized autoloop at the shown
                // length, halve/double between 1/16 and 16 beats (gapless
                // feed-thread re-anchor). Every button is a fixed 45×30 to
                // match the play/cue height; only the 4 BEATS chip is wider.
                let has_loop = loop_region.is_some();
                let grid_ok = deck_ui.marks.is_usable();
                let btn = |ui: &mut egui::Ui, txt: &str, enabled: bool| {
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.add_sized([45.0, 30.0], egui::Button::new(txt.to_string()))
                    })
                    .inner
                };

                if btn(ui, "IN", true).clicked() && has_track {
                    deck_ui.loop_in_staged =
                        Some(quantize_frame(&deck_ui.marks, deck_ui.quantize, playhead));
                }
                if btn(ui, "OUT", true).clicked()
                    && has_track
                    && let Some(start) = deck_ui.loop_in_staged
                {
                    let end = quantize_frame(&deck_ui.marks, deck_ui.quantize, playhead);
                    if end > start {
                        shared.set_loop(Some((start, end)));
                        let median = deck_ui.marks.median_beat_frames();
                        deck_ui.loop_beats = if median > 0.0 {
                            ((end - start) as f64 / median).clamp(0.25, 64.0)
                        } else {
                            4.0
                        };
                        deck_ui.loop_in_staged = None;
                    }
                }

                if btn(ui, "÷2", has_loop).clicked()
                    && let Some((start, _)) = loop_region
                {
                    deck_ui.loop_beats = (deck_ui.loop_beats / 2.0).max(0.0625);
                    let end = loop_end_for(&deck_ui.marks, start, deck_ui.loop_beats);
                    shared.set_loop(Some((start, end.max(start + 1))));
                }

                // Length chip: autoloop at the shown length; lit while a
                // loop is active. Sits in the middle, between ÷2 and ×2.
                let chip = ui
                    .add_enabled_ui(grid_ok && has_track, |ui| {
                        let label = format!("{} BEATS", format_beats(deck_ui.loop_beats));
                        outlined_toggle(ui, has_loop, &label, [72.0, 30.0])
                    })
                    .inner;
                if chip.on_hover_text("Autoloop at this length").clicked() {
                    deck_ui.autoloop(deck_ui.loop_beats);
                }

                if btn(ui, "×2", has_loop).clicked()
                    && let Some((start, _)) = loop_region
                {
                    deck_ui.loop_beats = (deck_ui.loop_beats * 2.0).min(16.0);
                    let end = loop_end_for(&deck_ui.marks, start, deck_ui.loop_beats);
                    shared.set_loop(Some((start, end.max(start + 1))));
                }
                if btn(ui, "EXIT", has_loop).clicked() {
                    shared.set_loop(None);
                }
            });
        });

        // Row 2: hot-cue pads (8) + GATE / Q. Normal mode: empty = set at the
        // (quantized) playhead, occupied = jump and play, right-click deletes.
        // Gated mode: plays from the cue while held, pauses on release.
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            // The pads shrink when the deck column is narrow so the row
            // (8 pads + divider + GATE/QUANTIZE/AUTO CUE, ~175 pt of tail)
            // never widens the panel — at the minimum window size that
            // overflow would push deck B past the right edge and wrap the
            // AUTO CUE caption.
            let gap = ui.spacing().item_spacing.x;
            let pad_w = ((ui.available_width() - 175.0 - 8.0 * gap) / 8.0).clamp(18.0, 40.0);
            control_group(ui, "HOT CUES", |ui| {
                for i in 0..8 {
                    let set = deck_ui.hot_cues[i].is_some();
                    let mut button = egui::Button::new(
                        egui::RichText::new(format!("{}", i + 1))
                            .size(13.0)
                            .color(if set {
                                egui::Color32::BLACK
                            } else {
                                egui::Color32::from_rgb(140, 140, 150)
                            }),
                    );
                    if set {
                        button = button.fill(ACCENT);
                    }
                    let resp = ui.add_sized([pad_w, 30.0], button);
                    let down = resp.is_pointer_button_down_on();
                    let pressed = down && !deck_ui.hotcue_was_down[i];
                    deck_ui.hotcue_was_down[i] = down;

                    if resp.secondary_clicked() {
                        deck_ui.hot_cues[i] = None;
                        continue;
                    }
                    if pressed && deck_ui.hot_cue_press(i) && deck_ui.gated {
                        deck_ui.gated_held = Some(i);
                    }
                }
                // Gated release: the held slot's button is no longer down.
                if let Some(held) = deck_ui.gated_held
                    && !deck_ui.hotcue_was_down[held]
                {
                    deck_ui.gated_held = None;
                    shared.set_transport(Transport::Paused);
                }
            });

            group_divider(ui, 44.0);

            control_group(ui, "GATE", |ui| {
                state_toggle(ui, &mut deck_ui.gated, [40.0, 30.0])
                    .on_hover_text("Gated hot cues: play while held, stop on release");
            });
            control_group(ui, "QUANTIZE", |ui| {
                state_toggle(ui, &mut deck_ui.quantize, [40.0, 30.0])
                    .on_hover_text("Quantize hot cues and loops to the beat grid");
            });
            control_group(ui, "AUTO CUE", |ui| {
                state_toggle(ui, &mut deck_ui.auto_cue, [40.0, 30.0]).on_hover_text(
                    "On load, set the cue to the first downbeat and park the deck there",
                );
            });
        });
    });

    // Drag-and-drop target: the whole deck column accepts library tracks.
    if egui::DragAndDrop::has_payload_of_type::<DragTrack>(ui.ctx()) {
        let rect = ui.min_rect();
        if ui.rect_contains_pointer(rect) {
            ui.painter().rect_stroke(
                rect.expand(2.0),
                6.0,
                egui::Stroke::new(2.0, ACCENT),
                egui::StrokeKind::Inside,
            );
            if ui.input(|i| i.pointer.any_released())
                && let Some(drag) = egui::DragAndDrop::take_payload::<DragTrack>(ui.ctx())
            {
                response.load_track_id = Some(drag.track_id);
            }
        }
    }

    response
}

/// Right-of-waveform sidebar: big BPM readout, keylock, Master/Sync,
/// pitch readout + vertical tempo fader with a labeled scale, and the
/// range selector. `height` is the waveform stack height, used to size the
/// fader so the column spans it.
#[allow(clippy::too_many_arguments)]
fn deck_sidebar(
    ui: &mut egui::Ui,
    deck_ui: &mut DeckUi,
    deck_idx: usize,
    is_master: bool,
    has_track: bool,
    height: f32,
    response: &mut DeckPanelResponse,
) {
    // Faint divider between the waveform stack and the sidebar.
    let left_x = ui.max_rect().left() - 4.0;
    let top_y = ui.cursor().top();
    ui.painter().line_segment(
        [
            egui::pos2(left_x, top_y),
            egui::pos2(left_x, top_y + height),
        ],
        ui.visuals().widgets.noninteractive.bg_stroke,
    );

    let full = ui.available_width();

    ui.add_enabled_ui(has_track, |ui| {
        ui.add_space(4.0);
        ui.vertical_centered(|ui| section_caption(ui, "KEYLOCK"));
        state_toggle(ui, &mut deck_ui.keylock, [full, 20.0])
            .on_hover_text("Keylock: keep pitch constant while tempo changes");
        ui.add_space(4.0);
        // Master/Sync: mutually exclusive, at most one lit (master deck
        // shows MASTER with SYNC disabled; a follower shows SYNC; neither
        // lit = independent). Stacked — the narrow strip can't fit both
        // side by side.
        if outlined_toggle(ui, is_master, "MASTER", [full, 20.0])
            .on_hover_text("Make this deck the tempo reference")
            .clicked()
            && !is_master
        {
            response.master_clicked = true;
        }
        let sync = ui
            .add_enabled_ui(!is_master, |ui| {
                outlined_toggle(ui, deck_ui.synced, "SYNC", [full, 20.0])
                    .on_hover_text("Follow the master deck's tempo and beat phase")
            })
            .inner;
        if sync.clicked() {
            deck_ui.synced = !deck_ui.synced;
            if deck_ui.synced {
                response.sync_engaged = true;
            }
        }

        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            section_caption(ui, "PITCH");
            ui.label(
                egui::RichText::new(format!("{:+.1}%", deck_ui.pitch_percent))
                    .monospace()
                    .size(11.0),
            );
        });
        // Size the fader so the sidebar spans the waveform stack: subtract
        // what the column has consumed so far (measured from its absolute
        // top) and the RANGE caption + combo below.
        let used = ui.cursor().top() - top_y;
        let fader_h = (height - used - 56.0).max(80.0);
        let range = deck_ui.pitch_range;
        let mut pct = deck_ui.pitch_percent;
        ui.vertical_centered(|ui| {
            // Absolute-position fader: `.changed()` only fires on a real
            // grab, so touching it hands control back and drops sync
            // (matching the old slider). While synced, update_tempo keeps
            // writing pitch_percent and the fader just displays it.
            // Pitch-strip look: dark slot, chunky cap, dense tick ladder.
            let fader = ui.add(
                Fader::new(&mut pct, -range..=range, ACCENT)
                    .vertical(true)
                    .size([32.0, fader_h])
                    .groove_width(8.0)
                    .cap_size(30.0, 12.0)
                    .notches(Notches::Even(16))
                    .default_value(0.0),
            );
            if fader.changed() {
                deck_ui.pitch_percent = pct;
                deck_ui.synced = false;
            }
        });
        ui.vertical_centered(|ui| {
            ui.add_space(6.0);
            section_caption(ui, "RANGE");
            let prev = deck_ui.pitch_range;
            egui::ComboBox::from_id_salt(("pitch-range", deck_idx))
                .selected_text(format!("±{prev:.0}%"))
                .width(full)
                .show_ui(ui, |ui| {
                    for r in [8.0_f32, 16.0, 50.0] {
                        ui.selectable_value(&mut deck_ui.pitch_range, r, format!("±{r:.0}%"));
                    }
                });
            if deck_ui.pitch_range != prev {
                deck_ui.pitch_percent = deck_ui
                    .pitch_percent
                    .clamp(-deck_ui.pitch_range, deck_ui.pitch_range);
            }
        });
    });
}

/// One deck's knob strip (Trim/Hi/Mid/Low/Filter) with the volume fader
/// centered beneath it, in a fixed-width column so the knobs land directly
/// over the fader. Returns the column rect (used to size the level meter).
fn deck_channel_strip(ui: &mut egui::Ui, shared: &crate::state::DeckShared) -> egui::Rect {
    ui.scope(|ui| {
        ui.set_width(44.0);
        ui.vertical_centered(|ui| {
            // Trim + isolator EQ: all bipolar knobs centered at unity (1.0),
            // range 0..2 (kill hard left, +6 dB hard right).
            for (label, atom) in [
                ("Trim", &shared.trim),
                ("Hi", &shared.eq_high),
                ("Mid", &shared.eq_mid),
                ("Low", &shared.eq_low),
            ] {
                ui.add_space(2.0);
                ui.label(egui::RichText::new(label).weak().size(11.0));
                let mut v = atom.load();
                if ui
                    .add(
                        Knob::new(&mut v, 0.0..=2.0, ACCENT)
                            .arc(KnobArc::Bipolar { center: 1.0 })
                            .default_value(1.0),
                    )
                    .on_hover_text(format!("{label}: {:+.1} dB", 20.0 * v.max(1e-4).log10()))
                    .changed()
                {
                    atom.store(v);
                }
            }

            // Filter: one bipolar knob. Center (12 o'clock) = off; twist left =
            // low-pass sweeping closed, right = high-pass sweeping closed. Rides
            // a synthetic position t in [-1, 1] mapped to (mode, cutoff).
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Filter").weak().size(11.0));
            let mut t = filter_pos(shared.filter_mode_u8(), shared.filter_cutoff.load());
            if ui
                .add(
                    Knob::new(&mut t, -1.0..=1.0, ACCENT)
                        .arc(KnobArc::Bipolar { center: 0.0 })
                        .default_value(0.0),
                )
                .on_hover_text(filter_hint(t))
                .changed()
            {
                let (mode, cutoff) = filter_params(t);
                if shared.filter_mode_u8() != mode {
                    shared.set_filter_mode(mode);
                }
                shared.filter_cutoff.store(cutoff);
            }

            ui.add_space(6.0);
            let mut fader = shared.fader.load();
            if ui
                .add(
                    Fader::new(&mut fader, 0.0..=1.0, ACCENT)
                        .vertical(true)
                        .size([24.0, 120.0])
                        .notches(Notches::Even(10))
                        .default_value(1.0),
                )
                .changed()
            {
                shared.fader.store(fader);
            }
        });
    })
    .response
    .rect
}

fn mixer_panel(ui: &mut egui::Ui, mixer: &MixerShared, decks: &[DeckUi; 2]) {
    ui.add_space(8.0);

    // Two channel strips packed toward the center, with the pair of level
    // meters between them (classic DJ-mixer layout). `vertical_centered` won't
    // center a multi-widget horizontal row (egui seeds it at full width), so
    // pad-center it to the panel mid-line — the same axis the crossfader uses.
    const STRIP_W: f32 = 44.0;
    // 44 + 8 + 6 + 3 + 6 + 8 + 44
    const CLUSTER_W: f32 = STRIP_W + 8.0 + 6.0 + 3.0 + 6.0 + 8.0 + STRIP_W;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(((ui.available_width() - CLUSTER_W) * 0.5).max(0.0));
        let a = deck_channel_strip(ui, &decks[0].deck.shared);
        ui.add_space(8.0);
        deck_level_meter(ui, decks[0].deck.shared.meter.load(), a.height());
        ui.add_space(3.0);
        deck_level_meter(ui, decks[1].deck.shared.meter.load(), a.height());
        ui.add_space(8.0);
        deck_channel_strip(ui, &decks[1].deck.shared);
    });

    ui.add_space(12.0);
    ui.vertical_centered(|ui| {
        let mut xf = mixer.crossfader.load();
        if ui
            .add(
                Fader::new(&mut xf, 0.0..=1.0, ACCENT)
                    .vertical(false)
                    .size([120.0, 24.0])
                    .notches(Notches::Center)
                    .center_fill(0.5)
                    .default_value(0.5),
            )
            .changed()
        {
            mixer.crossfader.store(xf);
        }
    });
}

/// Reconstruct the bipolar filter knob position `t` in [-1, 1] from the
/// stored (mode, cutoff): 0 at center (off / fully open), -1 at hard left
/// (low-pass fully closed), +1 at hard right (high-pass fully closed).
fn filter_pos(mode: u8, cutoff: f32) -> f32 {
    match mode {
        1 => cutoff - 1.0, // LowPass: open (1.0) → 0, closed (0.0) → -1
        2 => cutoff,       // HighPass: open (0.0) → 0, closed (1.0) → +1
        _ => 0.0,          // Off
    }
}

/// Decompose the filter knob position `t` back into (filter_mode, cutoff).
/// A small center detent snaps to Off so 12 o'clock is easy to hit.
fn filter_params(t: f32) -> (u8, f32) {
    const EPS: f32 = 0.02;
    if t < -EPS {
        (1, 1.0 + t) // LowPass, cutoff 1.0 (open) → 0.0 (closed)
    } else if t > EPS {
        (2, t) // HighPass, cutoff 0.0 (open) → 1.0 (closed)
    } else {
        (0, 0.0) // Off
    }
}

/// Hover text for the filter knob: mode plus cutoff frequency.
fn filter_hint(t: f32) -> String {
    let (mode, cutoff) = filter_params(t);
    match mode {
        1 => format!("Low-pass: {:.0} Hz", crate::dsp::filter_cutoff_hz(cutoff)),
        2 => format!("High-pass: {:.0} Hz", crate::dsp::filter_cutoff_hz(cutoff)),
        _ => "Filter: off".to_string(),
    }
}

/// Thin vertical channel meter: a dim track with a green bar rising from the
/// bottom to `level` (0..1, linear pre-fader / post-trim peak published by
/// the audio callback — the track's level regardless of fader position).
fn deck_level_meter(ui: &mut egui::Ui, level: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(6.0, height), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, 1.0, ui.visuals().extreme_bg_color);
    let h = level.clamp(0.0, 1.0) * height;
    if h > 0.0 {
        let fill = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - h),
            rect.right_bottom(),
        );
        painter.rect_filled(fill, 1.0, egui::Color32::from_rgb(64, 210, 96));
    }
}

/// Toolbar master output meter, styled like the DSP load bar: the summed
/// stream level measured post-master / post-limiter, so pulling the master
/// knob down visibly limits it. Red = pinned near the ceiling (limiting).
fn master_level_meter(ui: &mut egui::Ui, level: f32) {
    let color = if level < 0.7 {
        egui::Color32::from_rgb(110, 200, 110)
    } else if level < 0.95 {
        ACCENT
    } else {
        egui::Color32::from_rgb(230, 80, 80)
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(64.0, 10.0), egui::Sense::hover());
    response.on_hover_text(format!("Output: {:.0}%", level.clamp(0.0, 1.0) * 100.0));
    ui.painter()
        .rect_filled(rect, 2.0, egui::Color32::from_rgb(30, 30, 34));
    let fill = rect.width() * level.clamp(0.0, 1.0);
    if fill > 0.0 {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(fill, rect.height())),
            2.0,
            color,
        );
    }
}

/// Toolbar meter: DSP = audio-callback load (render time ÷ buffer time,
/// the number that matters for dropouts) with a bar; CPU = whole-process
/// usage.
fn cpu_meter(ui: &mut egui::Ui, dsp_load: f32, process_pct: f32) {
    let dsp_pct = (dsp_load * 100.0).clamp(0.0, 999.0);
    let color = if dsp_pct < 50.0 {
        egui::Color32::from_rgb(110, 200, 110)
    } else if dsp_pct < 80.0 {
        ACCENT
    } else {
        egui::Color32::from_rgb(230, 80, 80)
    };
    // Right-to-left layout: process CPU, then the DSP bar + label.
    ui.label(
        egui::RichText::new(format!("CPU {process_pct:>4.1}%"))
            .weak()
            .monospace(),
    );
    ui.separator();
    ui.label(
        egui::RichText::new(format!("DSP {dsp_pct:>4.1}%"))
            .color(color)
            .monospace(),
    );
    let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 10.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 2.0, egui::Color32::from_rgb(30, 30, 34));
    let fill = rect.width() * (dsp_load.clamp(0.0, 1.0));
    ui.painter().rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(fill, rect.height())),
        2.0,
        color,
    );
}

/// Cumulative user+system CPU time of this process, in seconds.
fn process_cpu_secs() -> f64 {
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) == 0 {
            let secs = |tv: libc::timeval| tv.tv_sec as f64 + tv.tv_usec as f64 * 1e-6;
            secs(usage.ru_utime) + secs(usage.ru_stime)
        } else {
            0.0
        }
    }
}

/// Small uppercase weak caption above a control group ("TRANSPORT", …).
fn section_caption(ui: &mut egui::Ui, text: &str) {
    // Lighter grey when the deck is live; falls back to the dim weak color
    // when the enclosing scope is disabled (no track loaded).
    let text = egui::RichText::new(text).size(9.0);
    let text = if ui.is_enabled() {
        text.color(egui::Color32::from_gray(170))
    } else {
        text.weak()
    };
    ui.label(text);
}

/// Caption above a horizontal control row; returns the row's inner value.
fn control_group<R>(
    ui: &mut egui::Ui,
    caption: &str,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.vertical(|ui| {
        section_caption(ui, caption);
        ui.add_space(2.0);
        ui.horizontal(content).inner
    })
    .inner
}

/// Faint vertical divider between captioned control groups.
fn group_divider(ui: &mut egui::Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(7.0, height), egui::Sense::hover());
    let x = rect.center().x;
    ui.painter().line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
}

/// Fill behind amber-outlined controls: a dark amber wash.
const ACCENT_FILL: egui::Color32 = egui::Color32::from_rgb(46, 36, 16);

/// ON/OFF toggle whose label is the state; amber-outlined when on.
fn state_toggle(ui: &mut egui::Ui, on: &mut bool, size: [f32; 2]) -> egui::Response {
    let resp = outlined_toggle(ui, *on, if *on { "ON" } else { "OFF" }, size);
    if resp.clicked() {
        *on = !*on;
    }
    resp
}

/// Latching button with an amber outline when active (MASTER / SYNC / the
/// loop-length chip).
fn outlined_toggle(ui: &mut egui::Ui, active: bool, label: &str, size: [f32; 2]) -> egui::Response {
    let (color, stroke, fill) = if active {
        (ACCENT, egui::Stroke::new(1.0, ACCENT), ACCENT_FILL)
    } else {
        (
            ui.visuals().weak_text_color(),
            ui.visuals().widgets.inactive.bg_stroke,
            ui.visuals().widgets.inactive.weak_bg_fill,
        )
    };
    ui.add_sized(
        size,
        egui::Button::new(egui::RichText::new(label).size(10.0).color(color))
            .stroke(stroke)
            .fill(fill),
    )
}

/// Musical-key badge in the deck header: a bordered blue chip around the
/// raw tag string.
fn key_badge(ui: &mut egui::Ui, key: &str) {
    const KEY_BLUE: egui::Color32 = egui::Color32::from_rgb(120, 190, 255);
    let galley =
        ui.painter()
            .layout_no_wrap(key.to_owned(), egui::FontId::proportional(10.0), KEY_BLUE);
    let (rect, _) =
        ui.allocate_exact_size(galley.size() + egui::vec2(10.0, 4.0), egui::Sense::hover());
    ui.painter().rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(1.0, KEY_BLUE),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, KEY_BLUE);
}

fn apply_theme(ctx: &egui::Context) {
    // Pin to dark regardless of the OS appearance setting; set_visuals only
    // styles the active theme, so following the system would fall back to
    // egui's stock light visuals.
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = egui::Color32::from_rgb(16, 16, 18);
    visuals.window_fill = egui::Color32::from_rgb(16, 16, 18);
    visuals.extreme_bg_color = egui::Color32::from_rgb(8, 8, 10);
    visuals.selection.bg_fill = ACCENT.linear_multiply(0.4);
    visuals.slider_trailing_fill = true;
    ctx.set_visuals(visuals);
}

/// Seek, exiting any active loop the target falls outside of (otherwise
/// the feed thread would immediately wrap the playhead back in).
fn request_seek_guarded(shared: &crate::state::DeckShared, target: usize) {
    if let Some((start, end)) = shared.loop_region()
        && (target < start || target >= end)
    {
        shared.set_loop(None);
    }
    shared.request_seek(target);
}

/// Snap a frame to the nearest grid beat when quantize is on (and a grid
/// exists); otherwise pass it through.
fn quantize_frame(marks: &GridMarks, quantize: bool, frame: usize) -> usize {
    if !quantize || !marks.is_usable() {
        return frame;
    }
    let f = frame as f64;
    let Some(i) = marks.beat_at_or_before(f) else {
        // Before the first beat: snap forward to it.
        return marks.frame(0) as usize;
    };
    let a = marks.frame(i);
    let b = if i + 1 < marks.len() {
        marks.frame(i + 1)
    } else {
        a
    };
    if f - a <= b - f {
        a as usize
    } else {
        b as usize
    }
}

/// Loop end for `beats` beats starting at `start`: exact grid frames for
/// whole-beat lengths inside the grid, median beat interval otherwise.
fn loop_end_for(marks: &GridMarks, start: usize, beats: f64) -> usize {
    if marks.is_usable() {
        let whole = beats.fract() == 0.0;
        if whole && let Some(i) = marks.beat_at_or_before(start as f64) {
            let target = i + beats as usize;
            // Only use the grid when start sits on beat i exactly (it does
            // for quantized loops) and the grid reaches far enough.
            if (marks.frame(i) - start as f64).abs() < 1.0 && target < marks.len() {
                return marks.frame(target) as usize;
            }
        }
        let median = marks.median_beat_frames();
        if median > 0.0 {
            return start + (beats * median) as usize;
        }
    }
    start
}

/// "1/16", "1/4", "4", "3.7" — beat count for the loop-length readout.
fn format_beats(beats: f64) -> String {
    for (value, label) in [
        (0.0625, "1/16"),
        (0.125, "1/8"),
        (0.25, "1/4"),
        (0.5, "1/2"),
    ] {
        if (beats - value).abs() < 1e-9 {
            return label.to_string();
        }
    }
    if beats.fract() == 0.0 {
        format!("{beats:.0}")
    } else {
        format!("{beats:.1}")
    }
}

/// Fractional position within the current beat (0..1), from the display
/// beat grid.
fn beat_phase(marks: &GridMarks, frame: f64) -> Option<f64> {
    if !marks.is_usable() {
        return None;
    }
    let i = marks.beat_at_or_before(frame)?;
    if i + 1 >= marks.len() {
        return None;
    }
    let (a, b) = (marks.frame(i), marks.frame(i + 1));
    if b <= a {
        return None;
    }
    Some(((frame - a) / (b - a)).clamp(0.0, 1.0))
}

/// Smallest standard pitch range (8/16/50) that fits `pct`; saturates at 50.
fn range_for_pitch(pct: f32) -> f32 {
    let a = pct.abs();
    if a <= 8.0 {
        8.0
    } else if a <= 16.0 {
        16.0
    } else {
        50.0
    }
}

/// Seek target that puts the deck's beat phase at `master_phase`, taking
/// the nearest alignment (offset wrapped to ±half a beat) using the deck's
/// local beat length at the playhead. None without a usable grid reading.
fn align_target_frame(
    marks: &GridMarks,
    playhead: f64,
    master_phase: f64,
    total: usize,
) -> Option<usize> {
    let dp = beat_phase(marks, playhead)?;
    let i = marks.beat_at_or_before(playhead)?;
    // beat_phase succeeding proves i + 1 is in range and the interval > 0.
    let beat_len = marks.frame(i + 1) - marks.frame(i);
    let mut off = master_phase - dp;
    off -= off.round();
    let target = playhead + off * beat_len;
    Some((target.round().max(0.0) as usize).min(total.saturating_sub(1)))
}

/// Beat-phase error in beats, wrapped to ±half a beat so the deck always
/// takes the short way round.
fn wrap_phase_err(master_phase: f64, deck_phase: f64) -> f64 {
    let mut err = master_phase - deck_phase;
    err -= err.round();
    err
}

/// Low-pass the phase error across frames. Playheads are published in
/// audio-callback quanta on independent threads, so a single reading can be
/// off by a couple hundredths of a beat; the EMA averages that phantom
/// error away while real drift passes through. A jump bigger than a quarter
/// beat (seek, loop wrap, grid seam) restarts the filter instead of slewing
/// through stale history.
fn smooth_phase_err(prev: Option<f64>, raw: f64, alpha: f64) -> f64 {
    match prev {
        Some(p) if (raw - p).abs() <= 0.25 => p + (raw - p) * alpha,
        _ => raw,
    }
}

/// Proportional rate correction toward the master's beat phase, from an
/// already-smoothed error. Soft-knee deadband: corrections ramp from zero
/// past 0.02 beats (no step at the threshold), capped at an inaudible
/// ±1.5% — the align-on-engage seek means the PLL only ever counters slow
/// drift, never performs big chases.
fn phase_correction(err: f64) -> f64 {
    let past_knee = err.abs() - 0.02;
    if past_knee <= 0.0 {
        return 0.0;
    }
    (err.signum() * past_knee * 0.3).clamp(-0.015, 0.015)
}

/// `m:ss.t` for a frame count at the given sample rate.
pub fn format_time(frames: usize, sample_rate: u32) -> String {
    let secs = frames as f64 / sample_rate.max(1) as f64;
    let m = (secs / 60.0) as u64;
    let s = secs % 60.0;
    format!("{m}:{s:04.1}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_err_takes_short_way_round() {
        // Master just past a beat (0.1), deck late in its beat (0.9): the
        // short way is forward +0.2 beats, so the deck speeds up.
        assert!((wrap_phase_err(0.1, 0.9) - 0.2).abs() < 1e-9);
        assert!(phase_correction(wrap_phase_err(0.1, 0.9)) > 0.0);
        // Mirror case slows down.
        assert!(phase_correction(wrap_phase_err(0.9, 0.1)) < 0.0);
    }

    #[test]
    fn phase_correction_knee_and_clamp() {
        // Inside the knee: no correction.
        assert_eq!(phase_correction(0.005), 0.0);
        assert_eq!(phase_correction(-0.02), 0.0);
        // Ramps from zero past the knee — no step at the threshold.
        let c = phase_correction(0.03);
        assert!((c - 0.003).abs() < 1e-9);
        // Large errors cap at an inaudible ±1.5%.
        assert_eq!(phase_correction(0.5), 0.015);
        assert_eq!(phase_correction(-0.5), -0.015);
    }

    #[test]
    fn phase_err_smoothing_filters_jitter_but_resets_on_jumps() {
        // First reading passes straight through.
        assert_eq!(smooth_phase_err(None, 0.04, 0.1), 0.04);
        // Small readings blend toward the new value.
        let s = smooth_phase_err(Some(0.0), 0.04, 0.1);
        assert!((s - 0.004).abs() < 1e-9);
        // A jump past a quarter beat restarts the filter.
        assert_eq!(smooth_phase_err(Some(0.0), 0.4, 0.1), 0.4);
    }

    fn grid_100(sr: u32, n: usize) -> GridMarks {
        let mut grid = timestretch::BeatGrid::empty(sr);
        grid.beats = (0..n).map(|i| i as f64 * 100.0).collect();
        GridMarks::from_grid(&grid)
    }

    #[test]
    fn quantize_snaps_to_nearest_beat() {
        let marks = grid_100(48_000, 8);
        assert_eq!(quantize_frame(&marks, true, 140), 100);
        assert_eq!(quantize_frame(&marks, true, 160), 200);
        assert_eq!(quantize_frame(&marks, true, 150), 100); // ties go early
        assert_eq!(quantize_frame(&marks, false, 140), 140);
    }

    #[test]
    fn loop_end_uses_grid_for_whole_beats() {
        let marks = grid_100(48_000, 16);
        assert_eq!(loop_end_for(&marks, 200, 4.0), 600);
        // Fractional beats fall back to the median interval.
        assert_eq!(loop_end_for(&marks, 200, 0.5), 250);
    }

    #[test]
    fn beats_format_fractions() {
        assert_eq!(format_beats(0.0625), "1/16");
        assert_eq!(format_beats(0.125), "1/8");
        assert_eq!(format_beats(0.25), "1/4");
        assert_eq!(format_beats(0.5), "1/2");
        assert_eq!(format_beats(4.0), "4");
    }

    #[test]
    fn halving_ladder_floors_at_sixteenth() {
        let mut beats = 4.0f64;
        for _ in 0..10 {
            beats = (beats / 2.0).max(0.0625);
        }
        assert_eq!(beats, 0.0625);
        assert_eq!(format_beats(beats), "1/16");
    }

    #[test]
    fn pitch_range_expands_to_fit() {
        assert_eq!(range_for_pitch(5.0), 8.0);
        assert_eq!(range_for_pitch(-8.0), 8.0);
        assert_eq!(range_for_pitch(8.1), 16.0);
        assert_eq!(range_for_pitch(16.0), 16.0);
        assert_eq!(range_for_pitch(-20.0), 50.0);
        assert_eq!(range_for_pitch(74.0), 50.0); // saturates
    }

    #[test]
    fn align_seeks_nearest_beat_offset() {
        let marks = grid_100(48_000, 8);
        // Already in phase: no movement.
        assert_eq!(align_target_frame(&marks, 250.0, 0.5, 800), Some(250));
        // Deck late (0.9), master early (0.1): forward to the next beat's
        // 0.1, not back a near-full beat.
        assert_eq!(align_target_frame(&marks, 290.0, 0.1, 800), Some(310));
        // Mirror case wraps backwards.
        assert_eq!(align_target_frame(&marks, 210.0, 0.9, 800), Some(190));
        // Target clamped inside the track.
        assert_eq!(align_target_frame(&marks, 640.0, 0.8, 660), Some(659));
        // No grid: no seek.
        assert_eq!(
            align_target_frame(&GridMarks::empty(), 250.0, 0.5, 800),
            None
        );
    }

    #[test]
    fn beat_phase_interpolates_between_beats() {
        let mut grid = timestretch::BeatGrid::empty(48_000);
        grid.beats = vec![0.0, 100.0, 200.0, 300.0];
        let marks = GridMarks::from_grid(&grid);
        assert_eq!(beat_phase(&marks, 150.0), Some(0.5));
        assert_eq!(beat_phase(&marks, 100.0), Some(0.0));
        // Past the last interval or before the grid: no reading.
        assert_eq!(beat_phase(&marks, 350.0), None);
        assert_eq!(beat_phase(&marks, -1.0), None);
    }
}
