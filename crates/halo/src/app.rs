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
use crate::key::{self, Key, Lit};
use crate::knob::{Knob, KnobArc};
use crate::library::{Library, PlaylistRow, SortColumn, TrackRow};
use crate::programmer_ui::{ProgrammerCtx, programmer_panel};
use crate::show::simulate_show_l3;
use crate::show_preview::{LOOK_PALETTE, LookId, ShowPreview, ShowSel};
use crate::state::{MixerShared, ScrubPhase, Transport};
use crate::theme::{self, ACCENT};
use crate::waveform::{
    BandPeaks, GhostPlayhead, GridMarks, OverviewParams, OverviewTexture, STACKED_OVERVIEW_H,
    STACKED_ZOOMED_H, STRIP_HEIGHT, ScrubGesture, ShowEditorInteraction, ShowEditorParams,
    ShowStripParams, WaveStyle, ZoomSpan, ZoomedParams, paint_beat_counter, paint_overview,
    paint_show_strip, paint_zoomed, show_editor,
};
use crate::worker::{WorkerEvent, spawn_analysis_worker, spawn_import};

const DECK_NAMES: [&str; 2] = ["A", "B"];
/// Longest edge of the artwork texture.
const ARTWORK_MAX_PX: u32 = 256;
/// Padding inside the Perform view's deck and mixer panels (`space-16`).
const PERFORM_PANEL_PAD: f32 = theme::SPACE_16;
/// Ground between the window's panels (toolbar, decks, mixer, footer) and
/// round the window's edge (`space-2`): the sections sit almost touching.
const SECTION_GAP: f32 = theme::SPACE_2;

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

struct PendingDecode {
    path: PathBuf,
    track_id: Option<i64>,
    receiver: mpsc::Receiver<DecodeResult>,
}

struct DeckUi {
    deck: Deck,
    decode_rx: Option<PendingDecode>,
    /// Keep the source available for device changes even without a library row.
    source_path: Option<PathBuf>,
    /// Library row of the loaded track (None only when the library is
    /// unavailable).
    track_id: Option<i64>,
    key: Option<String>,
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
    /// Previous frame's cue-button-held state, for press/release edges.
    cue_was_down: bool,
    /// True while the cue button is previewing (play-from-cue while held).
    cue_previewing: bool,
    /// Tempo slider value in percent, within ±`pitch_range`.
    pitch_percent: f32,
    /// Tempo slider range in percent (8 / 16 / 50, or 300 in WIDE).
    pitch_range: f32,
    /// WIDE expands the fader range; all ranges use the same processor.
    wide: bool,
    keylock: bool,
    /// Following the master deck's tempo + beat phase.
    synced: bool,
    /// Momentary pitch-bend factor from the held nudge buttons (1.0 = none).
    bend: f32,
    /// Hot cue slots (source frames).
    hot_cues: [Option<usize>; 8],
    hotcue_was_down: [bool; 8],
    /// Gated hot-cue mode: play from the cue while held, pause on release.
    gated: bool,
    /// Slot currently held in gated mode.
    gated_held: Option<usize>,
    /// Quantize hot cues and loop points to the beat grid.
    quantize: bool,
    /// Auto cue: on load, park the deck at the first detected beat.
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
            source_path: None,
            track_id: None,
            key: None,
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
            cue_was_down: false,
            cue_previewing: false,
            pitch_percent: 0.0,
            pitch_range: 8.0,
            wide: false,
            keylock: true,
            synced: false,
            bend: 1.0,
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

    fn prepare_device_reload(&mut self) -> Option<(PathBuf, Option<i64>)> {
        let shared = &self.deck.shared;
        shared.set_transport(Transport::Stopped);
        shared.stream_active.store(false, Ordering::Release);
        shared.scrub.cancel();
        if let Some(pending) = &self.decode_rx {
            // A decode already in flight must restart at the new device rate.
            // Preserve an earlier device reload's position across rapid changes.
            Some((pending.path.clone(), pending.track_id))
        } else {
            let path = self.source_path.clone()?;
            self.pending_seek_frac =
                Some(shared.playhead_frames() as f64 / shared.total().max(1) as f64);
            Some((path, self.track_id))
        }
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
    /// The library zoom fader's position, −1..1 with 0 in the middle; see
    /// [`library_zoom_scale`].
    zoom: f32,
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
            zoom: 0.0,
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
    /// No longer restored: the crossfader has no control, so it stays
    /// centred. Kept so older builds still read this state.
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
    /// The library zoom fader's position, −1..1; the missing-field
    /// default (0) is the middle, today's row size.
    #[serde(default)]
    library_zoom: f32,
    /// Wide-range Master Tempo (WIDE) per deck.
    #[serde(default)]
    wides: [bool; 2],
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
    /// Per-track browser waveform thumbnails, rasterized lazily for
    /// visible rows. `None` = known-missing blob (negative cache, avoids
    /// a DB query per frame). Keyed by stable track id, so sorts and
    /// re-queries need no invalidation.
    waveform_thumbs: std::collections::HashMap<i64, Option<egui::TextureHandle>>,
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
    /// DSP load latched for the toolbar readout, on the same once-a-second
    /// tick as the CPU figure. The number would be unreadable at frame rate.
    dsp_display: f32,
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
            handoff: deck_ui.deck.handoff.clone(),
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
            for (i, deck_ui) in decks.iter_mut().enumerate() {
                deck_ui.deck.shared.trim.store(persisted.trims[i]);
                deck_ui.keylock = persisted.keylocks[i];
                deck_ui.pitch_range = persisted.pitch_ranges[i].max(8.0);
                deck_ui.wide = persisted.wides[i];
                if deck_ui.wide {
                    deck_ui.pitch_range = 300.0;
                }
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
        browser.zoom = persisted.library_zoom.clamp(-1.0, 1.0);

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
            waveform_thumbs: std::collections::HashMap::new(),
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
            dsp_display: 0.0,
            status,
            mixer_center_x: 0.0,
        };
        // Dev smoke-test hook: import a folder at startup.
        if let Some(dir) = std::env::var_os("HALO_IMPORT") {
            app.import_paths(vec![PathBuf::from(dir)], None);
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

    /// Import files and folders in the background, then add the tracks to
    /// `playlist` if it's set.
    fn import_paths(&mut self, paths: Vec<PathBuf>, playlist: Option<i64>) {
        if self.library.is_none() || paths.is_empty() {
            return;
        }
        self.status = match paths.as_slice() {
            [path] => format!("Importing {}…", path.display()),
            _ => format!("Importing {} items…", paths.len()),
        };
        spawn_import(
            Library::default_path(),
            paths,
            playlist,
            self.wake_tx.clone(),
            self.event_tx.clone(),
        );
    }

    /// Files and folders dropped from Finder join the library, and the
    /// selected playlist if there is one.
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let paths: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        self.import_paths(paths, self.browser.selected);
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
        self.decks[deck_idx].pending_seek_frac = None;
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
                self.prepare.audition.pending_seek_frac = None;
                self.prepare.audition.decode_rx = Some(self.spawn_decode(row.path, Some(track_id)));
            }
            None => self.status = "Track not found in library".to_string(),
        }
    }

    fn spawn_decode(&self, path: PathBuf, track_id: Option<i64>) -> PendingDecode {
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
        let source_path = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(load_track_data(&path, device_rate, track_id, key, artifact));
        });
        PendingDecode {
            path: source_path,
            track_id,
            receiver: rx,
        }
    }

    /// Handle finished decode threads: install the track on its deck (A, B,
    /// or the Prepare audition player) and kick off background pre-analysis.
    fn poll_decodes(&mut self, ctx: &egui::Context) {
        // Keep pending sources for a retry after a device-open failure; never
        // install audio decoded for an old rate while there is no output.
        if self.audio.is_none() {
            return;
        }
        let device_rate = self.device_rate();
        for (i, name) in DECK_NAMES.iter().enumerate().take(self.decks.len()) {
            if let Some(status) = Self::poll_deck_decode(
                &mut self.decks[i],
                name,
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
        if let Some(error) = deck_ui.deck.poll() {
            return Some(format!("Deck {label}: {error}"));
        }
        let pending = deck_ui.decode_rx.as_ref()?;
        let Ok(result) = pending.receiver.try_recv() else {
            return None;
        };
        let path = deck_ui.decode_rx.take()?.path;
        match result {
            Ok(data) => {
                match deck_ui.deck.load(
                    data.title.clone(),
                    data.samples.clone(),
                    device_rate,
                    data.artifact.clone(),
                ) {
                    Ok(()) => {
                        deck_ui.source_path = Some(path);
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
                        deck_ui.deck.handoff.publish_grid(crate::sync::BeatMap::new(
                            (0..deck_ui.marks.len()).map(|i| deck_ui.marks.frame(i)),
                            deck_ui.deck.shared.track_generation.load(Ordering::Acquire),
                            data.grid.bpm,
                        ));
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
                        // Auto cue: park at the first detected beat. Ceil so
                        // the parked position sits at/after its fractional
                        // grid frame, rather than just before the beat.
                        deck_ui.last_auto_cue = None;
                        if deck_ui.auto_cue
                            && deck_ui.marks.is_usable()
                            && let Some(frame) = deck_ui.marks.auto_cue_frame()
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

    /// Apply library metadata independently of the playback session.
    fn poll_worker_events(&mut self) {
        let device_rate = self.device_rate();
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                WorkerEvent::Analyzed(id) => {
                    self.browser.dirty = true;
                    // Analysis (first or re-) rewrote the thumbnail blob.
                    self.waveform_thumbs.remove(&id);
                    for deck_ui in self
                        .decks
                        .iter_mut()
                        .chain(std::iter::once(&mut self.prepare.audition))
                    {
                        if deck_ui.track_id == Some(id)
                            && let Some(lib) = &self.library
                            && let Ok(Some(native)) = lib.analysis(id)
                        {
                            let resampled = native.resample_to(device_rate);
                            deck_ui.marks = GridMarks::from_grid(&grid_from_artifact(&resampled));
                            deck_ui.deck.handoff.publish_grid(crate::sync::BeatMap::new(
                                (0..deck_ui.marks.len()).map(|i| deck_ui.marks.frame(i)),
                                deck_ui.deck.shared.track_generation.load(Ordering::Acquire),
                                resampled.bpm,
                            ));
                            deck_ui.bpm = resampled.bpm;
                            deck_ui.deck.pre_analysis = Some(Arc::new(resampled));
                            // The refined grid may move the first beat:
                            // re-apply auto cue, but never disturb a playing
                            // or scrubbing deck, a user-moved cue, or a
                            // user-moved playhead.
                            let shared = &deck_ui.deck.shared;
                            if deck_ui.auto_cue
                                && shared.transport() != Transport::Playing
                                && deck_ui.scrub_pos.is_none()
                                && let Some(prev) = deck_ui.last_auto_cue
                                && shared.cue_point.load(Ordering::Relaxed) as usize == prev
                                && let Some(frame) = deck_ui.marks.auto_cue_frame()
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
                WorkerEvent::WaveformReady(id) => {
                    // Backfill only touches the thumbnail: no row text
                    // changed, so no browser re-query.
                    self.waveform_thumbs.remove(&id);
                }
                WorkerEvent::Imported { tracks, playlist } => {
                    self.browser.dirty = true;
                    let n = tracks.len();
                    let noun = if n == 1 { "track" } else { "tracks" };
                    // The playlist may have been deleted while importing.
                    let target =
                        playlist.and_then(|id| self.browser.playlists.iter().find(|p| p.id == id));
                    self.status = match (target, &self.library) {
                        (Some(pl), Some(lib)) => match lib.add_tracks_to_playlist(pl.id, &tracks) {
                            Ok(added) => {
                                let name = &pl.name;
                                let already = n - added;
                                if already == 0 {
                                    format!("Imported {n} {noun} into {name}")
                                } else {
                                    format!(
                                        "Imported {n} {noun} into {name} ({already} already in it)"
                                    )
                                }
                            }
                            Err(e) => {
                                log::error!("{e}");
                                format!("Imported {n} {noun}")
                            }
                        },
                        _ => format!("Imported {n} {noun}"),
                    };
                }
            }
        }
    }

    /// One callback-owned master drives transport, displays and lighting.
    fn master_source(&self) -> usize {
        usize::from(self.mixer.effective_master.load(Ordering::Acquire)).min(1)
    }

    fn global_bpm(&self) -> f64 {
        let d = &self.decks[self.master_source()];
        d.bpm * d.deck.shared.matched_tempo.load()
    }

    fn update_tempo(&mut self) {
        let master = self.master_source();
        for (i, d) in self.decks.iter_mut().enumerate() {
            let shared = &d.deck.shared;
            shared.sync_enabled.store(d.synced, Ordering::Release);
            if d.synced
                && (i == master
                    || crate::sync::SyncState::load(shared) != crate::sync::SyncState::Off)
            {
                let pct = ((shared.matched_tempo.load() - 1.0) * 100.0) as f32;
                if pct.abs() > 50.0 {
                    d.wide = true;
                    d.pitch_range = 300.0;
                } else {
                    d.pitch_range = d.pitch_range.max(range_for_pitch(pct));
                }
                d.pitch_percent = pct;
            }
            let base = legal_tempo(1.0 + d.pitch_percent as f64 / 100.0, d.keylock);
            d.pitch_percent = ((base - 1.0) * 100.0) as f32;
            shared.manual_tempo.store(base.max(1.0 / 32.0) as f32);
            shared.bend.store(d.bend);
            shared.tempo_stopped.store(base <= 0.0, Ordering::Relaxed);
            shared.keylock.store(d.keylock, Ordering::Relaxed);
        }
    }

    /// Tear down and rebuild the output stream with the current settings.
    /// Loaded decks reload into fresh delivery queues at the new device rate,
    /// restoring their playhead position.
    fn rebuild_audio(&mut self) {
        let reloads = [
            self.decks[0].prepare_device_reload(),
            self.decks[1].prepare_device_reload(),
            self.prepare.audition.prepare_device_reload(),
        ];
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
        for (i, reload) in reloads.into_iter().enumerate() {
            if let Some((path, track_id)) = reload {
                let pending = self.spawn_decode(path, track_id);
                let deck = if i < 2 {
                    &mut self.decks[i]
                } else {
                    &mut self.prepare.audition
                };
                deck.decode_rx = Some(pending);
            }
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
        for (d, &now) in down.iter().enumerate() {
            let prev = self.kb_prev[d];
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
        let decks = std::array::from_fn(|i| {
            let d = &self.decks[i];
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
            dmx::DmxDeck {
                cues: d.deck.track.is_some().then(|| d.cues.clone()),
                deck: d.deck.shared.clone(),
                beat_ref,
            }
        });
        *self.dmx.lock().unwrap() = Some(dmx::DmxSnapshot {
            rig: self.rig.clone(),
            decks,
            mixer: self.mixer.clone(),
            overrides: self.programmer.clone(),
            params: self.programmer_params.clone(),
            selection: self.fixture_selection.clone(),
            net: self.net.clone(),
        });
    }

    /// The PATCH view: console-style patch sheet, staged. Edits build a
    /// draft; the live rig, DMX output, and programmer keep the
    /// last-applied patch until APPLY swaps the draft in and persists it
    /// to the show file. REVERT discards the draft.
    fn patch_ui(&mut self, ui: &mut egui::Ui) {
        const WARN: egui::Color32 = theme::WARN;
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
                ui.spacing_mut().item_spacing.x = theme::SPACE_8;
                ui.menu_button(theme::KEY_LABEL.rich("+ ADD FIXTURE"), |ui| {
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
                if ui.add(Key::new("RESET TO DEFAULT RIG").small()).clicked() {
                    *rig = default_rig(fl);
                    changed = true;
                }
                if !conflicts.is_empty() {
                    ui.label(
                        theme::BODY_TEXT
                            .rich(format!(
                                "⚠ {} fixtures with patch conflicts",
                                conflicts.len()
                            ))
                            .color(WARN),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can_apply = dirty && conflicts.is_empty();
                    let apply = ui.add_enabled(can_apply, Key::new("APPLY").small());
                    let apply = if dirty && !conflicts.is_empty() {
                        apply.on_disabled_hover_text("Resolve patch conflicts first")
                    } else {
                        apply.on_disabled_hover_text("No unapplied changes")
                    };
                    if apply.clicked() {
                        do_apply = true;
                    }
                    if ui
                        .add_enabled(dirty, Key::new("REVERT").small())
                        .on_hover_text("Discard the draft; keep the applied patch")
                        .clicked()
                    {
                        do_revert = true;
                    }
                    if dirty {
                        ui.label(theme::NOTE_TEXT.rich("● unapplied changes").color(ACCENT));
                    }
                });
            });
            ui.add_space(theme::SPACE_12);

            // The patch sheet sits in a well; DMX addresses are the one
            // place JetBrains Mono appears.
            let sheet_h = ui.available_height() - 2.0 * theme::SPACE_8;
            theme::well(ui, theme::SPACE_8, |ui| {
                ui.set_min_size(egui::vec2(ui.available_width(), sheet_h));
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("patch_sheet")
                        .striped(true)
                        .min_col_width(44.0)
                        .spacing([theme::SPACE_12, theme::SPACE_6])
                        .show(ui, |ui| {
                            for h in [
                                "FIXTURE", "KIND", "PROFILE", "UNIV", "ADDR", "COL", "ROW", "",
                            ] {
                                ui.label(theme::CAPTION.rich(h).color(theme::TEXT_MUTED));
                            }
                            ui.end_row();
                            for f in rig.fixtures_mut().iter_mut() {
                                let bad = conflicts.contains(&f.id);
                                if bad {
                                    ui.label(
                                        theme::BODY_TEXT.rich(format!("⚠ {}", f.label)).color(WARN),
                                    );
                                } else {
                                    ui.label(&f.label);
                                }
                                egui::ComboBox::from_id_salt(("patch-kind", f.id))
                                    .selected_text(f.kind.group_label())
                                    .show_ui(ui, |ui| {
                                        for kind in ALL_KINDS {
                                            if ui
                                                .selectable_label(
                                                    f.kind == kind,
                                                    kind.group_label(),
                                                )
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
                                            if ui
                                                .selectable_label(&f.profile_id == pid, name)
                                                .clicked()
                                                && &f.profile_id != pid
                                            {
                                                f.profile_id = pid.clone();
                                                changed = true;
                                            }
                                        }
                                    });
                                ui.scope(|ui| {
                                    ui.style_mut().drag_value_text_style =
                                        egui::TextStyle::Monospace;
                                    changed |= ui
                                        .add(egui::DragValue::new(&mut f.universe).range(1..=32))
                                        .changed();
                                });
                                ui.scope(|ui| {
                                    ui.style_mut().drag_value_text_style =
                                        egui::TextStyle::Monospace;
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut f.start_address)
                                                .range(1..=512),
                                        )
                                        .changed();
                                });
                                changed |= ui
                                    .add(egui::DragValue::new(&mut f.col).range(0..=15))
                                    .changed();
                                changed |= ui
                                    .add(egui::DragValue::new(&mut f.row).range(0..=15))
                                    .changed();
                                if ui
                                    .add(Key::new("−").small())
                                    .on_hover_text("Unpatch")
                                    .clicked()
                                {
                                    remove = Some(f.id);
                                }
                                ui.end_row();
                            }
                        });
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

    /// Sample process-wide CPU roughly once a second, and latch the DSP
    /// readout on the same tick.
    fn audio_load(&self) -> f32 {
        self.decks
            .iter()
            .chain(std::iter::once(&self.prepare.audition))
            .fold(self.mixer.cpu_load.load(), |load, deck| {
                load.max(deck.deck.shared.worker_load.load())
            })
    }

    fn update_cpu(&mut self) {
        let (last_at, last_cpu, _) = self.cpu_sample;
        let elapsed = last_at.elapsed().as_secs_f64();
        if elapsed >= 1.0 {
            let cpu_now = process_cpu_secs();
            let pct = ((cpu_now - last_cpu) / elapsed * 100.0) as f32;
            self.cpu_sample = (std::time::Instant::now(), cpu_now, pct.max(0.0));
            self.dsp_display = self.audio_load();
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
                ui.spacing_mut().item_spacing.y = theme::SPACE_8;
                ui.label(theme::CAPTION.rich("AUDIO OUTPUT").color(theme::TEXT_MUTED));
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

                ui.label(
                    theme::NOTE_TEXT
                        .rich("Applying restarts the audio stream and reloads decks.")
                        .color(theme::TEXT_MUTED),
                );
                if ui.add(Key::new("APPLY").small()).clicked() {
                    apply = true;
                }

                ui.add_space(theme::SPACE_12);
                ui.label(
                    theme::CAPTION
                        .rich("ART-NET OUTPUT")
                        .color(theme::TEXT_MUTED),
                );
                let was_broadcast = self.artnet.unicast_ip.is_none();
                let mut artnet_dirty = false;
                ui.horizontal(|ui| {
                    if ui
                        .add(Key::new("BROADCAST").small().selected(was_broadcast))
                        .clicked()
                        && !was_broadcast
                    {
                        self.artnet.unicast_ip = None;
                        artnet_dirty = true;
                    }
                    if ui
                        .add(Key::new("UNICAST").small().selected(!was_broadcast))
                        .clicked()
                        && was_broadcast
                    {
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
                            ui.label(theme::NOTE_TEXT.rich("invalid address").color(theme::WARN));
                        } else if resp.lost_focus()
                            && self.artnet.unicast_ip.as_deref()
                                != Some(self.artnet_ip_edit.as_str())
                        {
                            self.artnet.unicast_ip = Some(self.artnet_ip_edit.clone());
                            artnet_dirty = true;
                        }
                    });
                }
                ui.label(
                    theme::NOTE_TEXT
                        .rich(self.net.summary())
                        .color(theme::TEXT_MUTED),
                );
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
            .frame(theme::ground_frame(0.0).inner_margin(egui::Margin {
                top: 0,
                bottom: 0,
                ..egui::Margin::same(SECTION_GAP as i8)
            }))
            .show(ctx, |ui| {
                // The panel card fills the footer's (resizable) height
                // exactly, and its content lays out in a child that can't
                // grow the panel: a resizable panel stores the size its
                // content reached, so overflow would ratchet the footer up
                // a little every frame until it pushed the decks off screen.
                let card = ui.available_rect_before_wrap();
                ui.advance_cursor_after_rect(card);
                ui.painter().extend(theme::panel_shapes(ui.ctx(), card));
                let clip = card.intersect(ui.clip_rect());

                // The card's body: rail | divider | pane, each the body's
                // full height.
                let inner = egui::Rect::from_min_max(
                    card.min + egui::vec2(FOOTER_PAD_LEFT, PERFORM_PANEL_PAD),
                    card.max - egui::Vec2::splat(PERFORM_PANEL_PAD),
                );
                let rail =
                    egui::Rect::from_min_size(inner.min, egui::vec2(FOOTER_RAIL_W, inner.height()));
                let divider_x = rail.right() + FOOTER_RAIL_GAP;
                let pane =
                    egui::Rect::from_min_max(egui::pos2(divider_x + 1.0, inner.top()), inner.max);

                let mut rail_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rail)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                rail_ui.set_clip_rect(clip);
                rail_ui.spacing_mut().item_spacing.y = FOOTER_RAIL_KEY_GAP;
                for (tab, hover) in [
                    (FooterTab::Library, "Library — tracks and playlists"),
                    (
                        FooterTab::Programmer,
                        "Programmer — lane overrides, looks and effects",
                    ),
                ] {
                    let selected = self.footer_tab == tab;
                    let key = match tab {
                        FooterTab::Library => Key::new("🗄").text_style(FOOTER_RAIL_GLYPH),
                        FooterTab::Programmer => Key::new(""),
                    };
                    let resp = rail_ui
                        .add(
                            key.size(egui::Vec2::splat(FOOTER_RAIL_W))
                                .selected(selected),
                        )
                        .on_hover_text(hover);
                    if tab == FooterTab::Programmer {
                        // The label colour a key gives its own text.
                        let color =
                            if selected || resp.hovered() || resp.is_pointer_button_down_on() {
                                theme::TEXT_STRONG
                            } else {
                                theme::TEXT
                            };
                        paint_programmer_icon(rail_ui.painter(), resp.rect.center(), color);
                    }
                    if resp.clicked() {
                        self.footer_tab = tab;
                    }
                }

                ui.painter().with_clip_rect(clip).vline(
                    divider_x + 0.5,
                    inner.y_range(),
                    egui::Stroke::new(1.0, theme::TRAY_DIVIDER),
                );

                let mut body = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(pane)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                body.set_clip_rect(pane.intersect(clip));
                let ui = &mut body;
                match self.footer_tab {
                    FooterTab::Library => {
                        if self.library.is_none() {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    theme::BODY_TEXT
                                        .rich("Library unavailable")
                                        .color(theme::TEXT_MUTED),
                                );
                            });
                            return;
                        }
                        let view = self.view;
                        let body_h = ui.available_height();
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = theme::SPACE_8;
                            theme::well(ui, theme::SPACE_8, |ui| {
                                ui.set_width(PLAYLIST_WELL_W);
                                let inner_h = body_h - 2.0 * theme::SPACE_8;
                                ui.set_min_height(inner_h);
                                ui.vertical(|ui| {
                                    playlist_tree(ui, &mut self.browser, &mut actions, inner_h);
                                });
                            });
                            ui.vertical(|ui| {
                                track_table(
                                    ui,
                                    &mut self.browser,
                                    self.library.as_ref(),
                                    &mut self.waveform_thumbs,
                                    &mut actions,
                                    view,
                                );
                            });
                        });
                    }
                    FooterTab::Programmer => {
                        store = self.programmer_ui(ui, lighting);
                    }
                }

                // Files dragged in from Finder: outline the library pane
                // and say where they'll go. They're accepted anywhere in
                // the window.
                if self.footer_tab == FooterTab::Library
                    && self.library.is_some()
                    && ctx.input(|i| !i.raw.hovered_files.is_empty())
                {
                    ui.painter().with_clip_rect(clip).rect_stroke(
                        pane,
                        theme::RADIUS_CONTROL,
                        egui::Stroke::new(PLAYLIST_DROP_OUTLINE, ACCENT),
                        egui::StrokeKind::Inside,
                    );
                    let target = self
                        .browser
                        .selected
                        .and_then(|id| self.browser.playlists.iter().find(|p| p.id == id))
                        .map_or("the library".to_string(), |p| p.name.clone());
                    egui::Area::new(egui::Id::new("file_drop_hint"))
                        .order(egui::Order::Tooltip)
                        .pivot(egui::Align2::CENTER_CENTER)
                        .fixed_pos(pane.center())
                        .interactable(false)
                        .show(ctx, |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.label(format!("Drop to add to {target}"));
                            });
                        });
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

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if has_track {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme::SPACE_2;
                    ui.label(
                        theme::TRACK_TITLE
                            .rich(&audition.title)
                            .color(theme::TEXT_STRONG),
                    );
                    ui.label(
                        theme::BODY_TEXT
                            .rich(audition.artist.as_deref().unwrap_or("—"))
                            .color(theme::TEXT_MUTED),
                    );
                });
            } else {
                ui.label(
                    theme::BODY_TEXT
                        .rich(
                            "Load a track from the library below (EDIT or double-click) \
                             to author its lighting cues",
                        )
                        .color(theme::TEXT_MUTED),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let bpm = if audition.bpm > 0.0 {
                    format!("{:.1}", audition.bpm)
                } else {
                    "—".to_string()
                };
                theme::paint_readout(ui, "BPM", &bpm, theme::TEXT_STRONG, 5);
                ui.add_space(theme::SPACE_12);
                theme::paint_readout(
                    ui,
                    "ELAPSED",
                    &format_time(playhead, sample_rate),
                    theme::TEXT_STRONG,
                    8,
                );
            });
        });
        ui.add_space(theme::SPACE_12);

        // Transport + audition volume (this channel bypasses the
        // crossfader; the fader alone is its level).
        ui.add_enabled_ui(has_track, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                control_group(ui, "TRANSPORT", |ui| {
                    // The audition player's keys are 44 wide; its CUE is
                    // yellow, kept apart from the decks' amber.
                    let size = [44.0, theme::SIZE_KEY];
                    let play_label = if playing { "⏸" } else { "▶" };
                    if ui
                        .add(
                            Key::new(play_label)
                                .size(size)
                                .text_style(theme::KEY_LABEL.sized(16.0))
                                .label_color(theme::TEXT_STRONG)
                                .lit(playing.then_some(Lit::Play)),
                        )
                        .clicked()
                    {
                        audition.toggle_play();
                    }
                    let cue_resp = ui.add(
                        Key::new("CUE")
                            .size(size)
                            .label_color(theme::AUDITION)
                            .lit_when_held(Lit::Audition),
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
                });
                ui.add_space(theme::SPACE_12);
                group_divider(ui, 48.0);
                ui.add_space(theme::SPACE_12);
                control_group(ui, "VOLUME", |ui| {
                    let mut vol = shared.fader.load();
                    ui.spacing_mut().slider_width = 160.0;
                    if ui
                        .add_sized(
                            [160.0, theme::SIZE_KEY],
                            egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false),
                        )
                        .on_hover_text("Audition volume (independent of the crossfader)")
                        .changed()
                    {
                        shared.fader.store(vol);
                    }
                });
            });
        });
        ui.add_space(theme::SPACE_12);

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
                style: WaveStyle::Welled,
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
        ui.add_space(theme::SPACE_8);
        ui.add_enabled_ui(has_track, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_8;
                if outlined_toggle(ui, *snap, "SNAP", None)
                    .on_hover_text("Snap edits to the beat grid")
                    .clicked()
                {
                    *snap = !*snap;
                }
                group_divider(ui, theme::SIZE_KEY);
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
                    let resp = look_swatch(ui, look.color, armed).on_hover_text(look.name);
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
                group_divider(ui, theme::SIZE_KEY);
                let selected_accents: std::collections::HashSet<u64> = selection
                    .iter()
                    .filter_map(|&s| match s {
                        ShowSel::Accent(id) => Some(id),
                        _ => None,
                    })
                    .collect();
                if selection.is_empty() {
                    ui.label(
                        theme::NOTE_TEXT
                            .rich(
                                "Drag in LOOK to place the armed look · drag in ENERGY to shape \
                                 the arc · draw one-shots in ACCENT",
                            )
                            .color(theme::TEXT_MUTED),
                    );
                } else {
                    if let [id] = selected_looks[..]
                        && let Some(ev) = audition.show.looks.find(id)
                    {
                        // The look's colour as an LED; its name in
                        // `text-strong` (look colours aren't text colours).
                        let (dot, _) = ui.allocate_exact_size(
                            egui::vec2(theme::SIZE_LED, theme::SIZE_LED),
                            egui::Sense::hover(),
                        );
                        theme::paint_led(ui.painter(), dot.center(), true, ev.look.def().color);
                        ui.label(
                            theme::BODY_TEXT
                                .rich(ev.look.def().name)
                                .color(theme::TEXT_STRONG),
                        );
                    } else {
                        ui.label(
                            theme::BODY_TEXT
                                .rich(format!("{} selected", selection.len()))
                                .color(theme::TEXT_STRONG),
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
                    if ui.add(Key::new("DELETE").small()).clicked() {
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
                    ui.menu_button(theme::KEY_LABEL.rich("CLEAR"), |ui| {
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
                        .add(Key::new("RESET DEMO SHOW").small())
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
        ui.add_space(theme::SPACE_8);

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
                style: WaveStyle::Welled,
            },
        ) && has_track
        {
            request_seek_guarded(&shared, (frac as f64 * total as f64) as usize);
        }

        ui.add_space(2.0);
        ui.add_space(theme::SPACE_6);
        ui.horizontal(|ui| {
            paint_beat_counter(ui, &audition.marks, display_pos, theme::CAPTION, true);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(Key::new("+").small()).clicked() {
                    audition.zoom.zoom_in();
                }
                ui.label(
                    theme::CAPTION
                        .rich(audition.zoom.label(audition.marks.is_usable()))
                        .color(theme::TEXT_MUTED),
                );
                if ui.add(Key::new("−").small()).clicked() {
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
                    theme::RADIUS_PANEL,
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
                ui.label(
                    theme::BODY_TEXT
                        .rich(format!("Remove “{title}” from the library?"))
                        .color(theme::TEXT_STRONG),
                );
                ui.label(
                    theme::NOTE_TEXT
                        .rich(
                            "Authored lighting cues will be deleted.\n\
                             The audio file on disk is not touched.",
                        )
                        .color(theme::TEXT_MUTED),
                );
                ui.add_space(theme::SPACE_12);
                ui.horizontal(|ui| {
                    if ui.add(Key::new("REMOVE")).clicked() {
                        decided = true;
                        remove = true;
                    }
                    if ui.add(Key::new("CANCEL")).clicked() {
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
                let title = self.browser.rows.iter().find(|r| r.id == track);
                let name = self.browser.playlists.iter().find(|p| p.id == playlist);
                match lib.add_to_playlist(playlist, track) {
                    Ok(added) => {
                        if let (Some(t), Some(p)) = (title, name) {
                            let (title, name) = (&t.title, &p.name);
                            self.status = if added {
                                format!("Added “{title}” to {name}")
                            } else {
                                format!("“{title}” is already in {name}")
                            };
                        }
                    }
                    Err(e) => log::error!("{e}"),
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
                self.import_paths(vec![dir], None);
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
            })
    }

    /// Play intent is scheduled by the callback after worker preparation.
    /// Pressing again cancels Preparing/Waiting just like ordinary playback.
    fn toggle_play_synced(&mut self, i: usize) {
        self.decks[i].toggle_play();
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
                wides: [self.decks[0].wide, self.decks[1].wide],
                sort: Some(self.browser.sort),
                ascending: self.browser.ascending,
                device_name: self.audio_settings.device_name.clone(),
                buffer_size: self.audio_settings.buffer_size,
                view: self.view,
                audition_volume: self.prepare.audition.deck.shared.fader.load(),
                snap_off: !self.prepare.snap,
                auto_cue_off: [!self.decks[0].auto_cue, !self.decks[1].auto_cue],
                footer_tab: self.footer_tab,
                library_zoom: self.browser.zoom,
            },
        );
    }

    /// Tab flips the footer between Library and Programmer. It's taken out
    /// of the input here, before egui sees it: with nothing focused, egui
    /// would otherwise move focus to the first widget (the search field),
    /// which silences every shortcut. With a text field focused, Tab is left
    /// to egui.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if self.view == View::Patch || ctx.wants_keyboard_input() {
            return;
        }
        raw_input.events.retain(|event| match event {
            egui::Event::Key {
                key: egui::Key::Tab,
                pressed,
                repeat,
                modifiers,
                ..
            } if !(modifiers.command || modifiers.ctrl || modifiers.alt) => {
                if *pressed && !*repeat {
                    self.footer_tab = match self.footer_tab {
                        FooterTab::Library => FooterTab::Programmer,
                        FooterTab::Programmer => FooterTab::Library,
                    };
                }
                false
            }
            _ => true,
        });
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_decodes(ctx);
        self.poll_worker_events();
        self.refresh_browser();
        self.handle_dropped_files(ctx);
        self.handle_shortcuts(ctx);
        self.update_tempo();
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
        egui::TopBottomPanel::top("toolbar")
            .frame(theme::ground_frame(0.0).inner_margin(egui::Margin {
                bottom: 0,
                ..egui::Margin::same(SECTION_GAP as i8)
            }))
            .show(ctx, |ui| {
                let margin = egui::Margin::symmetric(theme::SPACE_16 as i8, theme::SPACE_8 as i8);
                let toolbar_row = theme::panel(ui, margin, |ui| {
                    ui.horizontal(|ui| {
                        ui.set_min_height(TOOLBAR_ROW_H);
                        ui.spacing_mut().item_spacing.x = theme::SPACE_8;
                        ui.label(theme::WORDMARK.rich("HALO").color(ACCENT));
                        ui.add_space(theme::SPACE_8);
                        // The view switcher: one strip, the current view
                        // pressed in.
                        let views = [
                            (View::Perform, "PERFORM"),
                            (View::Prepare, "PREPARE"),
                            (View::Patch, "PATCH"),
                        ];
                        let keys = views.map(|(v, label)| Key::new(label).pressed(self.view == v));
                        for ((v, _), resp) in views.into_iter().zip(key::segmented(ui, keys)) {
                            if resp
                                .on_hover_text("Switch view (V) — playback is unaffected")
                                .clicked()
                            {
                                self.view = v;
                            }
                        }
                        // The audition player keeps playing across view
                        // switches; outside Prepare it has no other UI, so
                        // surface it with a lit pause chip.
                        if self.view != View::Prepare
                            && self.prepare.audition.deck.shared.transport() == Transport::Playing
                            && ui
                                .add(Key::new("AUDITION ▶").small().lit(Some(Lit::Audition)))
                                .on_hover_text(
                                    "The Prepare audition player is running — click to pause",
                                )
                                .clicked()
                        {
                            self.prepare.audition.toggle_play();
                        }
                        group_divider(ui, theme::SIZE_KEY_SMALL);
                        lighting_leds(ui, &lighting);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(Key::new("⚙").small().text_style(theme::BODY_TEXT))
                                .on_hover_text("Audio settings")
                                .clicked()
                            {
                                self.settings_open = !self.settings_open;
                                if self.settings_open {
                                    self.available_devices = list_output_devices();
                                }
                            }
                            group_divider(ui, theme::SIZE_KEY_SMALL);
                            cpu_meter(ui, self.dsp_display, self.cpu_sample.2);
                            group_divider(ui, theme::SIZE_KEY_SMALL);
                            // Right-to-left layout: the output bar lands
                            // between the DSP cluster and the master knob,
                            // i.e. just to the knob's right.
                            master_level_meter(ui, self.mixer.master_meter.load());
                            let mut master = self.mixer.master.load();
                            if ui
                                .add(
                                    Knob::new(&mut master, 0.0..=1.0, ACCENT)
                                        .arc(KnobArc::Unipolar)
                                        .default_value(1.0)
                                        .diameter(theme::SIZE_KNOB_SMALL),
                                )
                                .on_hover_text(format!("Master: {:.0}%", master * 100.0))
                                .changed()
                            {
                                self.mixer.master.store(master);
                            }
                            ui.label(theme::CAPTION.rich("MASTER").color(theme::TEXT_MUTED));
                        });
                    })
                    .response
                    .rect
                })
                .inner;

                // Master BPM, centered over the mixer. Uses the mixer's actual
                // center-x captured last frame in the central panel (egui rects
                // are absolute screen coords); falls back to the toolbar's
                // mid-x until measured. Painted, not laid out, to dodge egui's
                // horizontal-centering quirks.
                let cx = if mixer_cx > 0.0 {
                    mixer_cx
                } else {
                    toolbar_row.center().x
                };
                paint_master_bpm(
                    ui,
                    egui::pos2(cx, toolbar_row.center().y),
                    global_bpm,
                    master_src,
                );
            });

        self.settings_window(ctx);
        self.remove_confirm_window(ctx);

        egui::TopBottomPanel::bottom("status")
            .frame(theme::ground_frame(theme::SPACE_8).inner_margin(egui::Margin::symmetric(12, 6)))
            .show(ctx, |ui| {
                ui.label(theme::NOTE_TEXT.rich(&self.status).color(theme::TEXT_MUTED));
            });

        // The patch sheet gets the whole window: no slide-up footer there.
        if self.view != View::Patch && self.footer_panel(ctx, &lighting) {
            self.store_programmer();
        }

        if self.view == View::Prepare {
            egui::CentralPanel::default()
                .frame(theme::ground_frame(SECTION_GAP))
                .show(ctx, |ui| {
                    theme::panel(ui, PERFORM_PANEL_PAD, |ui| {
                        ui.set_width(ui.available_width());
                        self.prepare_panel(ui);
                    });
                });
            return;
        }

        if self.view == View::Patch {
            egui::CentralPanel::default()
                .frame(theme::ground_frame(SECTION_GAP))
                .show(ctx, |ui| {
                    let fill = ui.available_size() - egui::Vec2::splat(2.0 * PERFORM_PANEL_PAD);
                    theme::panel(ui, PERFORM_PANEL_PAD, |ui| {
                        ui.set_min_size(fill);
                        self.patch_ui(ui);
                    });
                });
            return;
        }

        egui::CentralPanel::default()
            .frame(theme::ground_frame(SECTION_GAP))
            .show(ctx, |ui| {
                // Keep the `space-2` ground above the footer even when the
                // decks run taller than the room left for them (the 1px
                // allowance is the cards' outside edge).
                ui.set_clip_rect(ui.max_rect().expand(1.0).intersect(ui.clip_rect()));
                let full_width = ui.available_width();
                // Sized to just fit the mixer's strip cluster.
                let mixer_width = MIXER_W;
                // Three panels on the ground (deck | mixer | deck), `space-2`
                // apart: the decks in the quiet skin with `space-6` inside,
                // the mixer with `space-16` (`space-6` on top, so its
                // captions line up with the decks' headers).
                let gap = SECTION_GAP;
                let chrome = 2.0 * gap + 4.0 * DECK_PAD + 2.0 * PERFORM_PANEL_PAD;
                let deck_width = ((full_width - mixer_width - chrome) / 2.0)
                    .floor()
                    .max(280.0);
                // Panels in the row share one height: the tallest panel of the
                // previous frame.
                let row_h_id = egui::Id::new("perform-row-height");
                let row_h = ui.data(|d| d.get_temp::<f32>(row_h_id)).unwrap_or(0.0);
                let mut tallest = 0.0_f32;

                let sample_rate = self.device_rate();
                let master = self.master_source();
                let lighting_deck = self.lighting_deck;
                let has_audio = self.audio.is_some();
                let mut responses: [DeckPanelResponse; 2] = Default::default();
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    let mut card =
                        |ui: &mut egui::Ui,
                         width: f32,
                         (face, margin): (theme::Face, egui::Margin),
                         content: &mut dyn FnMut(&mut egui::Ui)| {
                            let chrome_h = margin.sum().y;
                            theme::panel_with(ui, margin, face, |ui| {
                                ui.set_width(width);
                                // Measured from the cursor, so set before
                                // the content.
                                ui.set_min_height(row_h - chrome_h);
                                let content_h =
                                    ui.vertical(|ui| content(ui)).response.rect.height();
                                tallest = tallest.max(content_h + chrome_h);
                            })
                            .response
                            .rect
                        };
                    let deck_skin = (theme::SKIN_QUIET.panel, egui::Margin::same(DECK_PAD as i8));
                    card(ui, deck_width, deck_skin, &mut |ui| {
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
                    let mixer_rect = card(
                        ui,
                        mixer_width,
                        (
                            theme::Face::Panel,
                            egui::Margin {
                                top: DECK_PAD as i8,
                                ..egui::Margin::same(PERFORM_PANEL_PAD as i8)
                            },
                        ),
                        &mut |ui| {
                            mixer_panel(ui, &self.decks);
                        },
                    );
                    self.mixer_center_x = mixer_rect.center().x;
                    card(ui, deck_width, deck_skin, &mut |ui| {
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
                if tallest != row_h {
                    ui.data_mut(|d| d.insert_temp(row_h_id, tallest));
                    ui.ctx().request_repaint();
                }
                for (i, resp) in responses.into_iter().enumerate() {
                    if resp.master_clicked {
                        let request = self.mixer.master_request.load(Ordering::Relaxed);
                        self.mixer
                            .master_request
                            .store((request & !1) + 2 + i as u64, Ordering::Release);
                    }
                    if resp.play_toggled {
                        self.toggle_play_synced(i);
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
    let cached_peaks = cached_tsa_peaks(path, &decoded);
    // Peaks and the fallback grid analyze the NATIVE signal: peak buckets
    // are per-second and grid positions rescale exactly, so the result is
    // identical — but a 96 kHz device no longer pays for analyzing the
    // 2.2x-larger resampled copy on every load.
    let native = AudioBuffer::new(decoded.samples, decoded.sample_rate, Channels::Stereo)
        .map_err(|e| format!("Invalid decoded audio: {e}"))?;
    let peaks =
        cached_peaks.unwrap_or_else(|| BandPeaks::compute(&native.data, 2, native.sample_rate));
    let (grid, artifact) = match stored {
        Some(artifact) => {
            let resampled = artifact.resample_to(device_rate);
            (grid_from_artifact(&resampled), Some(Arc::new(resampled)))
        }
        None => {
            let mono = timestretch::downmix_to_mid(&native.data, 2);
            let mut grid = timestretch::detect_beat_grid(&mono, native.sample_rate);
            log::info!(
                "Quick BPM: {:.1} ({} beats, confidence {:.2})",
                grid.bpm,
                grid.beats.len(),
                grid.confidence
            );
            // Rescale to the device rate (segments are beat-indexed and
            // BPM is time-based; only beat positions are frame-domain).
            let ratio = device_rate as f64 / native.sample_rate.max(1) as f64;
            for b in &mut grid.beats {
                *b *= ratio;
            }
            grid.sample_rate = device_rate;
            (grid, None)
        }
    };
    let buffer = native.resample(device_rate);

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

/// Precomputed waveform peaks from a `.tsa` sidecar (e.g. written by
/// `timestretch-cli analyze`), accepted only when the container's identity
/// matches the decoded native signal — sample rate, mono length, content
/// hash — so a re-encoded file can't paint a stale waveform. Peak buckets
/// are per-second of audio, so native-rate peaks paint correctly at any
/// device rate.
fn cached_tsa_peaks(path: &Path, decoded: &crate::decoder::DecodedAudio) -> Option<BandPeaks> {
    let sidecar = timestretch::analysis_file_path(path);
    if !sidecar.exists() {
        return None;
    }
    let mono = timestretch::downmix_to_mid(&decoded.samples, decoded.channels as usize);
    timestretch::read_analysis_file_validated(
        &sidecar,
        decoded.sample_rate,
        mono.len(),
        timestretch::hash_samples(&mono),
    )?
    .peaks
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

/// The footer's left inner padding (the rail sits close to the card's
/// edge; the other sides keep `PERFORM_PANEL_PAD`), the icon rail's width
/// and the gap between its keys, and the room between the rail and its
/// divider. The pane starts right at the divider.
const FOOTER_PAD_LEFT: f32 = theme::SPACE_4;
const FOOTER_RAIL_W: f32 = theme::SIZE_KEY;
const FOOTER_RAIL_KEY_GAP: f32 = theme::SPACE_4;
const FOOTER_RAIL_GAP: f32 = theme::SPACE_4;
/// The Library key's 🗄 glyph.
const FOOTER_RAIL_GLYPH: theme::TypeStyle = theme::BODY_TEXT.sized(16.0);
/// The programmer icon: 16 square, three fader slots at 55% with 4×2.5
/// caps at different heights (0 = top, 1 = bottom).
const PROGRAMMER_ICON: f32 = 16.0;
const PROGRAMMER_ICON_CAP: egui::Vec2 = egui::vec2(4.0, 2.5);
const PROGRAMMER_ICON_CAPS: [f32; 3] = [0.3, 0.7, 0.45];

/// The footer rail's Programmer icon, centred on `center` in `color`.
fn paint_programmer_icon(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    let half = PROGRAMMER_ICON / 2.0;
    let top = center.y - half;
    let travel = PROGRAMMER_ICON - PROGRAMMER_ICON_CAP.y;
    let step = (PROGRAMMER_ICON - PROGRAMMER_ICON_CAP.x) / 2.0;
    for (i, at) in PROGRAMMER_ICON_CAPS.iter().enumerate() {
        let x = center.x - half + PROGRAMMER_ICON_CAP.x / 2.0 + i as f32 * step;
        painter.vline(
            x,
            top..=top + PROGRAMMER_ICON,
            egui::Stroke::new(1.0, color.gamma_multiply(0.55)),
        );
        let cap = egui::Rect::from_min_size(
            egui::pos2(x - PROGRAMMER_ICON_CAP.x / 2.0, top + at * travel),
            PROGRAMMER_ICON_CAP,
        );
        painter.rect_filled(cap, 0.0, color);
    }
}

/// The playlist well's width.
const PLAYLIST_WELL_W: f32 = 190.0;
/// The outline a playlist row gets while a dragged track is over it (a lit
/// key's outline width).
const PLAYLIST_DROP_OUTLINE: f32 = 1.5;

/// Left side of the browser: Library root + playlist tree with inline
/// rename and context menus.
fn playlist_tree(
    ui: &mut egui::Ui,
    browser: &mut BrowserState,
    actions: &mut Vec<BrowserAction>,
    height: f32,
) {
    // The list scrolls in whatever height is left above + PLAYLIST, which
    // stays pinned to the bottom of the well.
    let list_h = (height - theme::SIZE_KEY_SMALL - theme::SPACE_8).max(0.0);
    let list_rect =
        egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), list_h));
    // Right-clicking the tree's background (not a row) offers the folder
    // actions. Registered before the rows, so the rows stay on top.
    let background = ui.interact(
        list_rect,
        ui.id().with("playlist_background"),
        egui::Sense::click(),
    );
    egui::ScrollArea::vertical()
        .id_salt("playlist_tree")
        .auto_shrink([false, false])
        .min_scrolled_height(list_h)
        .max_height(list_h)
        .show(ui, |ui| playlist_list(ui, browser, actions));
    background.context_menu(|ui| {
        if ui.button("New folder").clicked() {
            actions.push(BrowserAction::NewPlaylist(true));
            ui.close_menu();
        }
        if ui.button("Import folder…").clicked() {
            ui.close_menu();
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                actions.push(BrowserAction::ImportFolder(dir));
            }
        }
    });

    ui.add_space(theme::SPACE_8);
    if ui.add(Key::new("+ PLAYLIST").small()).clicked() {
        actions.push(BrowserAction::NewPlaylist(false));
    }
}

/// The Library root, then folders (each with its playlists), then
/// root-level playlists.
fn playlist_list(ui: &mut egui::Ui, browser: &mut BrowserState, actions: &mut Vec<BrowserAction>) {
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
    // A drop target for library rows: outlined while a track is over it,
    // the track appended on release.
    if egui::DragAndDrop::has_payload_of_type::<DragTrack>(ui.ctx())
        && ui.rect_contains_pointer(resp.rect)
    {
        ui.painter().rect_stroke(
            resp.rect,
            theme::RADIUS_CONTROL,
            egui::Stroke::new(PLAYLIST_DROP_OUTLINE, ACCENT),
            egui::StrokeKind::Inside,
        );
        if ui.input(|i| i.pointer.any_released())
            && let Some(drag) = egui::DragAndDrop::take_payload::<DragTrack>(ui.ctx())
        {
            actions.push(BrowserAction::AddToPlaylist(pl.id, drag.track_id));
        }
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
/// Library row height in points: tall enough for the waveform thumbnail
/// while keeping the table dense.
const TRACK_ROW_HEIGHT: f32 = 26.0;
/// Thumbnail cell rect in points (drawn centered in its column, 4 wider).
const THUMB_CELL_SIZE: egui::Vec2 = egui::vec2(92.0, 20.0);
/// The search field's clear button: a bare × of `SEARCH_CLEAR_ICON` square,
/// with a hit area as tall as the search row.
const SEARCH_CLEAR_ICON: f32 = 8.0;
const SEARCH_CLEAR_HIT: egui::Vec2 = egui::vec2(16.0, 22.0);

fn paint_clear_icon(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    let half = SEARCH_CLEAR_ICON / 2.0;
    let stroke = egui::Stroke::new(1.5, color);
    painter.line_segment(
        [
            center + egui::vec2(-half, -half),
            center + egui::vec2(half, half),
        ],
        stroke,
    );
    painter.line_segment(
        [
            center + egui::vec2(-half, half),
            center + egui::vec2(half, -half),
        ],
        stroke,
    );
}

/// The library zoom fader: 120×22, a 20×6 cap, a centre notch.
const LIBRARY_ZOOM_SIZE: egui::Vec2 = egui::vec2(120.0, 22.0);
const LIBRARY_ZOOM_CAP: egui::Vec2 = egui::vec2(20.0, 6.0);
/// The zoom's range either side of the middle (1×). Its floor keeps a
/// row at least as tall as the 22pt load keys in it.
const LIBRARY_ZOOM_MIN: f32 = 0.85;
const LIBRARY_ZOOM_MAX: f32 = 1.6;

/// The library zoom fader's −1..1 position as a scale on row height and
/// thumbnail size: `LIBRARY_ZOOM_MIN` at the left, 1× in the middle,
/// `LIBRARY_ZOOM_MAX` at the right.
fn library_zoom_scale(t: f32) -> f32 {
    if t < 0.0 {
        1.0 + (1.0 - LIBRARY_ZOOM_MIN) * t
    } else {
        1.0 + (LIBRARY_ZOOM_MAX - 1.0) * t
    }
}

fn track_table(
    ui: &mut egui::Ui,
    browser: &mut BrowserState,
    lib: Option<&Library>,
    thumbs: &mut std::collections::HashMap<i64, Option<egui::TextureHandle>>,
    actions: &mut Vec<BrowserAction>,
    view: View,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("🔍").color(theme::TEXT_MUTED));
        // A well field: `extreme_bg_color` is the well.
        if ui
            .add(
                egui::TextEdit::singleline(&mut browser.search)
                    .hint_text("Search title, artist, album")
                    .desired_width(240.0)
                    .margin(egui::vec2(theme::SPACE_8, 4.0)),
            )
            .changed()
        {
            actions.push(BrowserAction::SearchChanged);
        }
        // Back to the whole library: empties the search and drops the
        // playlist selection.
        let filtered = !browser.search.is_empty() || browser.selected.is_some();
        let sense = if filtered {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let (rect, resp) = ui.allocate_exact_size(SEARCH_CLEAR_HIT, sense);
        let color = if !filtered {
            theme::TEXT_DISABLED
        } else if resp.hovered() {
            theme::TEXT_STRONG
        } else {
            theme::TEXT_MUTED
        };
        paint_clear_icon(ui.painter(), rect.center(), color);
        let resp = if filtered {
            resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            resp
        };
        if resp
            .on_hover_text("Clear the search and show the whole library")
            .clicked()
        {
            browser.search.clear();
            actions.push(BrowserAction::SelectPlaylist(None));
        }
        let (rule, _) =
            ui.allocate_exact_size(egui::vec2(1.0, LIBRARY_ZOOM_SIZE.y), egui::Sense::hover());
        ui.painter().vline(
            rule.left() + 0.5,
            rule.y_range(),
            egui::Stroke::new(1.0, theme::TRAY_DIVIDER),
        );
        ui.add(
            Fader::new(&mut browser.zoom, -1.0..=1.0, ACCENT)
                .vertical(false)
                .size(LIBRARY_ZOOM_SIZE)
                .cap_size(LIBRARY_ZOOM_CAP.x, LIBRARY_ZOOM_CAP.y)
                .notches(Notches::Center)
                .default_value(0.0),
        )
        .on_hover_text("Library zoom — row height and waveform size");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let n = browser.rows.len();
            let noun = if n == 1 { "track" } else { "tracks" };
            ui.label(
                theme::NOTE_TEXT
                    .rich(format!("{n} {noun}"))
                    .color(theme::TEXT_MUTED),
            );
        });
    });
    ui.add_space(theme::SPACE_8);

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
        // Column headings stay Title Case (the one exception to uppercase
        // labels); the sorted column reads `text-strong`.
        let color = if browser.sort == col {
            theme::TEXT_STRONG
        } else {
            theme::TEXT_MUTED
        };
        if ui
            .add(
                egui::Label::new(
                    theme::KEY_LABEL
                        .rich(format!("{label}{arrow}"))
                        .color(color),
                )
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
    let zoom = library_zoom_scale(browser.zoom);
    let row_h = TRACK_ROW_HEIGHT * zoom;
    let thumb = THUMB_CELL_SIZE * zoom;

    // Text selection in cells would show an I-beam cursor and steal drags
    // from the row; the whole row must act as one draggable unit.
    ui.style_mut().interaction.selectable_labels = false;

    // The table sits in a well.
    theme::well(
        ui,
        egui::Margin::symmetric(theme::SPACE_8 as i8, theme::SPACE_4 as i8),
        |ui| {
            ui.set_min_size(ui.available_size());
            TableBuilder::new(ui)
                .striped(true)
                .sense(egui::Sense::click_and_drag())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::exact(58.0)) // load buttons
                .column(Column::exact(thumb.x + 4.0)) // waveform thumbnail
                // The text columns clip and their labels truncate, so a long
                // name ends in "…" in its own column instead of pushing the
                // rest of its row out of line.
                .column(Column::remainder().at_least(140.0).clip(true)) // title
                .column(Column::initial(140.0).at_least(80.0).clip(true)) // artist
                .column(Column::initial(120.0).at_least(60.0).clip(true)) // album
                .column(Column::exact(52.0)) // bpm
                .column(Column::exact(44.0)) // key
                .column(Column::exact(48.0)) // time
                .header(20.0, |mut h| {
                    h.col(|_| {});
                    h.col(|_| {}); // thumbnail: unlabeled, not sortable
                    h.col(|ui| header(ui, "Title", SortColumn::Title, browser, actions));
                    h.col(|ui| header(ui, "Artist", SortColumn::Artist, browser, actions));
                    h.col(|ui| header(ui, "Album", SortColumn::Album, browser, actions));
                    h.col(|ui| header(ui, "BPM", SortColumn::Bpm, browser, actions));
                    h.col(|ui| header(ui, "Key", SortColumn::Key, browser, actions));
                    h.col(|ui| header(ui, "Time", SortColumn::Duration, browser, actions));
                })
                .body(|body| {
                    body.rows(row_h, browser.rows.len(), |mut row| {
                        let track = &browser.rows[row.index()];
                        row.col(|ui| {
                            ui.horizontal(|ui| match view {
                                View::Perform | View::Patch => {
                                    ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                                    if ui
                                        .add(Key::new("A").small())
                                        .on_hover_text("Load to deck A")
                                        .clicked()
                                    {
                                        actions.push(BrowserAction::LoadDeck(0, track.id));
                                    }
                                    if ui
                                        .add(Key::new("B").small())
                                        .on_hover_text("Load to deck B")
                                        .clicked()
                                    {
                                        actions.push(BrowserAction::LoadDeck(1, track.id));
                                    }
                                }
                                View::Prepare => {
                                    if ui
                                        .add(Key::new("EDIT").small())
                                        .on_hover_text("Open in the cue editor")
                                        .clicked()
                                    {
                                        actions.push(BrowserAction::LoadAudition(track.id));
                                    }
                                }
                            });
                        });
                        row.col(|ui| {
                            // Painter-only (no widget) so the cell never competes
                            // with the table's row-level click/drag sense.
                            let rect = egui::Rect::from_center_size(ui.max_rect().center(), thumb);
                            if thumbs.len() >= 512 && !thumbs.contains_key(&track.id) {
                                // Cap: dropping the handles frees the textures;
                                // visible rows repopulate over the next frames.
                                thumbs.clear();
                            }
                            let entry = thumbs.entry(track.id).or_insert_with(|| {
                                lib.and_then(|l| l.waveform_blob(track.id).ok().flatten())
                                    .and_then(|data| crate::waveform::render_thumbnail(&data))
                                    .map(|img| {
                                        ui.ctx().load_texture(
                                            format!("track_thumb_{}", track.id),
                                            img,
                                            egui::TextureOptions::LINEAR,
                                        )
                                    })
                            });
                            match entry {
                                Some(tex) => {
                                    ui.painter().image(
                                        tex.id(),
                                        rect,
                                        egui::Rect::from_min_max(
                                            egui::pos2(0.0, 0.0),
                                            egui::pos2(1.0, 1.0),
                                        ),
                                        egui::Color32::WHITE,
                                    );
                                }
                                // No blob yet (unanalyzed / backfill pending /
                                // failed): a quiet baseline placeholder.
                                None => {
                                    ui.painter().hline(
                                        rect.x_range(),
                                        rect.bottom() - 1.0,
                                        egui::Stroke::new(
                                            1.0_f32,
                                            crate::waveform::palette::TEXT_DIM,
                                        ),
                                    );
                                }
                            }
                        });
                        row.col(|ui| {
                            ui.add(egui::Label::new(&track.title).truncate());
                        });
                        row.col(|ui| {
                            ui.add(
                                egui::Label::new(
                                    theme::BODY_TEXT
                                        .rich(track.artist.as_deref().unwrap_or("—"))
                                        .color(theme::TEXT_MUTED),
                                )
                                .truncate(),
                            );
                        });
                        row.col(|ui| {
                            ui.add(
                                egui::Label::new(
                                    theme::BODY_TEXT
                                        .rich(track.album.as_deref().unwrap_or("—"))
                                        .color(theme::TEXT_MUTED),
                                )
                                .truncate(),
                            );
                        });
                        row.col(|ui| {
                            ui.label(
                                theme::NUMERALS_SM.rich(
                                    track
                                        .bpm
                                        .map(|b| format!("{b:.1}"))
                                        .unwrap_or_else(|| "—".to_string()),
                                ),
                            );
                        });
                        row.col(|ui| {
                            key_badge(ui, track.key.as_deref().unwrap_or("—"));
                        });
                        row.col(|ui| {
                            ui.label(
                                theme::NUMERALS_SM.rich(
                                    track
                                        .duration_secs
                                        .map(|s| format!("{}:{:02}", s as u64 / 60, s as u64 % 60))
                                        .unwrap_or_else(|| "—".to_string()),
                                ),
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
                                    ui.label(
                                        theme::BODY_TEXT
                                            .rich("No playlists")
                                            .color(theme::TEXT_MUTED),
                                    );
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
        },
    );

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
    /// Play/pause was pressed; delivery schedules a synced start after priming.
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
        let color = ACCENT;
        let level = out.level.clamp(0.0, 1.0);
        let c = egui::pos2(rect.left() + 8.0 + i as f32 * 18.0, rect.center().y);
        // A well bezel with a 1px edge; the lens lights in the accent at
        // 15% + 85% × level.
        painter.circle(c, 6.5, theme::WELL, egui::Stroke::new(1.0, theme::EDGE));
        let lens = lerp_color(theme::LED_OFF, color, 0.15 + 0.85 * level);
        painter.circle_filled(c, 4.5, lens);
        if out.source == LaneSource::Programmer {
            painter.circle_stroke(c, 7.25, egui::Stroke::new(1.5, theme::TEXT_STRONG));
        }
    }
}

/// Mix two opaque colours in gamma space (CSS `color-mix in srgb`).
fn lerp_color(a: egui::Color32, b: egui::Color32, k: f32) -> egui::Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
    egui::Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
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
                    // The callback predicts the landing; the render worker
                    // primes it directly, including while the UI is stalled.
                    let rate = if playing {
                        if shared.tempo_stopped.load(Ordering::Relaxed) {
                            0.0
                        } else {
                            shared.tempo_rate.load() as f64
                        }
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
    let base_rate = if is_master
        || (deck_ui.synced && crate::sync::SyncState::load(&shared) != crate::sync::SyncState::Off)
    {
        shared.matched_tempo.load()
    } else {
        1.0 + deck_ui.pitch_percent as f64 / 100.0
    };
    let tempo_rate = base_rate * deck_ui.bend as f64;

    let readings = DeckReadings {
        has_track,
        is_master,
        playhead,
        total,
        sample_rate,
        tempo_rate,
    };
    ui.spacing_mut().item_spacing = egui::vec2(DECK_GAP, DECK_GAP);
    deck_header(ui, deck_ui, &readings);

    // The waveform well on the left; the tempo rail carves out a fixed
    // column on the right (like a hardware deck's pitch strip).
    let loop_region = shared.loop_region();
    ui.horizontal_top(|ui| {
        let wave_w = (ui.available_width() - TEMPO_RAIL_W - DECK_GAP).max(0.0);
        let (well, _) =
            ui.allocate_exact_size(egui::vec2(wave_w, WAVE_STACK_H), egui::Sense::hover());
        theme::paint_content_well(ui.painter(), well, theme::WAVE_GROUND);
        let mut stack = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(well)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        stack.set_clip_rect(well.intersect(ui.clip_rect()));
        stack.spacing_mut().item_spacing.y = 0.0;
        let stack = &mut stack;

        // Zoomed scrolling waveform. Dragging grabs the platter: the
        // audio callback's varispeed voice audibly chases the hand
        // (both directions); the drop releases the momentum into a
        // glide that eases back to play speed (or rest), handing off
        // to a primed session at the predicted landing.
        let gesture = paint_zoomed(
            stack,
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
                ghost: (crate::sync::SyncState::load(&shared) == crate::sync::SyncState::Waiting)
                    .then(|| GhostPlayhead {
                        offset_frames: -shared.sync_remaining.load(),
                        alpha: 0.85,
                    }),
                style: WaveStyle::Stacked,
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

        zoom_row(stack, deck_ui, idx, display_pos);
        wave_stack_gap(stack);

        // L3 show lanes (look / energy / accent), scrolling in
        // lockstep with the zoomed view above.
        paint_show_strip(
            stack,
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
        wave_stack_gap(stack);

        // Full-track overview with click-to-seek.
        if let Some(frac) = paint_overview(
            stack,
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
                style: WaveStyle::Stacked,
            },
        ) && has_track
        {
            request_seek_guarded(&shared, (frac as f64 * total as f64) as usize);
        }

        // One playhead, flagged at the top, through the waveform, the zoom
        // row and the lanes (the overview keeps its own cursor).
        if deck_ui.peaks.is_some() && total > 0 {
            let x = well.center().x;
            let bottom = well.top() + STACKED_ZOOMED_H + ZOOM_ROW_H + WAVE_STACK_GAP + STRIP_HEIGHT;
            let painter = ui.painter();
            // A black keyline either side keeps it clear of white peaks.
            painter.line_segment(
                [egui::pos2(x, well.top()), egui::pos2(x, bottom)],
                egui::Stroke::new(4.0, egui::Color32::BLACK),
            );
            painter.line_segment(
                [egui::pos2(x, well.top()), egui::pos2(x, bottom)],
                egui::Stroke::new(2.0, theme::PLAYHEAD),
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(x - PLAYHEAD_FLAG.x / 2.0, well.top()),
                    egui::pos2(x + PLAYHEAD_FLAG.x / 2.0, well.top()),
                    egui::pos2(x, well.top() + PLAYHEAD_FLAG.y),
                ],
                theme::PLAYHEAD,
                egui::Stroke::NONE,
            ));
        }

        ui.vertical(|ui| {
            ui.set_width(TEMPO_RAIL_W);
            deck_tempo_rail(ui, deck_ui, idx, is_master, has_track);
        });
    });

    ui.add_space(DECK_CONTROLS_GAP - DECK_GAP);

    // Hot-cue pads (8), joined into one strip across a tray. Normal mode:
    // empty = set at the (quantized) playhead, occupied = jump and play,
    // right-click deletes. Gated mode: plays from the cue while held,
    // pauses on release.
    theme::tray(ui, |ui| {
        ui.add_enabled_ui(has_track, |ui| {
            let strip_w = ui.available_width();
            let edge = |i: usize| (strip_w * i as f32 / 8.0).floor();
            // The slot number sits top right, leaving the centre free for a
            // cue's name; a set pad shows its slot colour in a strip along
            // the bottom.
            let pads: [Key; 8] = std::array::from_fn(|i| {
                let set = deck_ui.hot_cues[i].is_some();
                Key::new("")
                    .size([edge(i + 1) - edge(i), HOT_CUE_H])
                    .corner_label(
                        format!("{}", i + 1),
                        if set {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        },
                    )
                    .strip(if set {
                        theme::CUE[i]
                    } else {
                        theme::SEGMENT_OFF
                    })
                    .quiet()
            });
            for (i, resp) in key::segmented(ui, pads).into_iter().enumerate() {
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
    });

    ui.add_space(DECK_CONTROLS_GAP - DECK_GAP);

    // The key row, in a tray: PLAY, CUE | nudge | LOOP | GATE, GATE under
    // the tempo rail.
    theme::tray(ui, |ui| {
        ui.add_enabled_ui(has_track, |ui| {
            // PLAY/PAUSE: a green triangle; the whole face lights green
            // while playing.
            let play_label = if playing { "⏸" } else { "▶" };
            if ui
                .add(
                    Key::new(play_label)
                        .size([TRANSPORT_W, theme::SIZE_KEY])
                        .text_style(theme::QUIET_KEY_LABEL.sized(16.0))
                        .label_color(theme::TEXT_STRONG)
                        .lit(playing.then_some(Lit::Play))
                        .pulse((!playing).then_some(ACCENT))
                        .quiet(),
                )
                .clicked()
            {
                response.play_toggled = true;
            }

            ui.add_space(TRANSPORT_GAP);

            // CUE — CDJ semantics on press/release edges; an amber label
            // that lights the face amber while held.
            let cue_resp = ui.add(
                Key::new("CUE")
                    .size([TRANSPORT_W, theme::SIZE_KEY])
                    .label_color(ACCENT)
                    .lit_when_held(Lit::Accent)
                    .quiet(),
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
            theme::tray_divider(ui);

            // Pitch bend: momentary ±4% while held.
            let [bend_minus, bend_plus] = key::segmented(
                ui,
                [
                    Key::new("−").width(NUDGE_W).quiet(),
                    Key::new("+").width(NUDGE_W).quiet(),
                ],
            )
            .map(|r| r.is_pointer_button_down_on());
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
            theme::tray_divider(ui);

            // Loops: manual in/out, quantized autoloop at the shown
            // length, halve/double between 1/16 and 16 beats (gapless
            // session loop transaction), all one segmented strip. The
            // length segment is as wide as "1/16 BEATS" so halving
            // never shifts the row, and lights while a loop plays. The
            // strip fills the room up to GATE's divider, so GATE stays
            // under the tempo rail at any deck width: on a narrow deck
            // every segment scales down, and the length segment takes
            // what's left over.
            let has_loop = loop_region.is_some();
            let grid_ok = deck_ui.marks.is_usable();
            let room = (ui.available_width() - theme::TRAY_DIVIDER_W - GATE_W).max(0.0);
            let scale = (room / LOOP_WIDTHS.iter().sum::<f32>()).min(1.0);
            let mut widths = LOOP_WIDTHS.map(|w| (w * scale).floor());
            widths[LOOP_LENGTH] += room - widths.iter().sum::<f32>();
            let [w_in, w_out, w_halve, w_chip, w_double, w_exit] = widths;
            let length = format!("{} BEATS", format_beats(deck_ui.loop_beats));
            let [loop_in, loop_out, halve, chip, double, exit] = key::segmented(
                ui,
                [
                    Key::new("IN").width(w_in).quiet(),
                    Key::new("OUT").width(w_out).quiet(),
                    Key::new("÷2").width(w_halve).enabled(has_loop).quiet(),
                    Key::new(length)
                        .width(w_chip)
                        .lit(has_loop.then_some(Lit::Accent))
                        .enabled(grid_ok && has_track)
                        .quiet(),
                    Key::new("×2").width(w_double).enabled(has_loop).quiet(),
                    Key::new("EXIT").width(w_exit).enabled(has_loop).quiet(),
                ],
            );

            if loop_in.clicked() && has_track {
                deck_ui.loop_in_staged =
                    Some(quantize_frame(&deck_ui.marks, deck_ui.quantize, playhead));
            }
            if loop_out.clicked()
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

            if halve.clicked()
                && let Some((start, _)) = loop_region
            {
                deck_ui.loop_beats = (deck_ui.loop_beats / 2.0).max(0.0625);
                let end = loop_end_for(&deck_ui.marks, start, deck_ui.loop_beats);
                shared.set_loop(Some((start, end.max(start + 1))));
            }

            if chip.on_hover_text("Autoloop at this length").clicked() {
                deck_ui.autoloop(deck_ui.loop_beats);
            }

            if double.clicked()
                && let Some((start, _)) = loop_region
            {
                deck_ui.loop_beats = (deck_ui.loop_beats * 2.0).min(16.0);
                let end = loop_end_for(&deck_ui.marks, start, deck_ui.loop_beats);
                shared.set_loop(Some((start, end.max(start + 1))));
            }
            if exit.clicked() {
                shared.set_loop(None);
            }

            // GATE, right-aligned under the tempo rail.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        outlined_key(deck_ui.gated, "GATE")
                            .size([GATE_W, theme::SIZE_KEY])
                            .quiet(),
                    )
                    .on_hover_text("Gated hot cues: play while held, stop on release")
                    .clicked()
                {
                    deck_ui.gated = !deck_ui.gated;
                }
                theme::tray_divider(ui);
            });
        });
    });

    // Drag-and-drop target: the whole deck column accepts library tracks.
    if egui::DragAndDrop::has_payload_of_type::<DragTrack>(ui.ctx()) {
        let rect = ui.min_rect();
        if ui.rect_contains_pointer(rect) {
            ui.painter().rect_stroke(
                rect.expand(2.0),
                theme::RADIUS_PANEL,
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

// ---------------------------------------------------------------- deck (F · Quiet)

/// Padding inside a deck panel, and between its rows and columns.
const DECK_PAD: f32 = theme::SPACE_6;
const DECK_GAP: f32 = theme::SPACE_4;
/// Above and below the hot cues: between them and the waveform well,
/// and between them and the key row.
const DECK_CONTROLS_GAP: f32 = theme::SPACE_12;
/// The header tray and its fixed cells; the title takes the rest.
const DECK_HEADER_H: f32 = 52.0;
const HEADER_ART_W: f32 = DECK_HEADER_H;
const HEADER_TOGGLES_W: f32 = 58.0;
/// The A.CUE and Q latches: inset in their cell, and the gap between them.
const HEADER_LATCH_INSET: egui::Vec2 = egui::vec2(6.0, 6.0);
const HEADER_LATCH_GAP: f32 = 4.0;
const HEADER_TIME_W: f32 = 118.0;
const HEADER_BPM_W: f32 = 88.0;
const HEADER_KEY_W: f32 = 44.0;
/// PITCH ends the header, over the tempo rail and as wide as it.
const HEADER_PITCH_W: f32 = TEMPO_RAIL_W;
/// A header reading: its caption on a 14-high line over its value on a
/// 25-high line, the pair centred in the header's height.
const HEADER_CAPTION_H: f32 = 14.0;
const HEADER_VALUE_H: f32 = 25.0;
const HEADER_CAPTION_TOP: f32 = (DECK_HEADER_H - HEADER_CAPTION_H - HEADER_VALUE_H) / 2.0;
/// Inside the time, BPM and PITCH cells, and the narrower KEY cell.
const HEADER_CELL_PAD: f32 = 10.0;
const HEADER_KEY_PAD: f32 = 8.0;
/// Inside the title cell, left and right.
const HEADER_TITLE_PAD_L: f32 = 10.0;
const HEADER_TITLE_PAD_R: f32 = 8.0;
/// Between the title and the artist under it.
const HEADER_ARTIST_GAP: f32 = theme::SPACE_2;
/// The note glyph in an empty artwork cell.
const HEADER_ART_GLYPH: f32 = 20.0;
/// The zoom row under the zoomed waveform, the space above its line
/// (clear of the waveform's beat ticks), its − / + targets, and the
/// space before the BAR counter at its left (in line with the lane
/// captions under it).
const ZOOM_ROW_H: f32 = 26.0;
const ZOOM_ROW_TOP: f32 = 6.0;
const ZOOM_KEY_W: f32 = 22.0;
const ZOOM_COUNTER_PAD: f32 = 8.0;
/// The blank row between the waveform and the show lanes, and between
/// the lanes and the overview.
const WAVE_STACK_GAP: f32 = 12.0;
/// Waveform, zoom row, show lanes and overview, stacked in one well.
const WAVE_STACK_H: f32 = STACKED_ZOOMED_H
    + ZOOM_ROW_H
    + WAVE_STACK_GAP
    + STRIP_HEIGHT
    + WAVE_STACK_GAP
    + STACKED_OVERVIEW_H;
/// The playhead's flag at the top of the stack.
const PLAYHEAD_FLAG: egui::Vec2 = egui::vec2(10.0, 6.0);
/// The tempo rail: SYNC, the tempo fader, RANGE.
const TEMPO_RAIL_W: f32 = 84.0;
const TEMPO_FADER_W: f32 = 38.0;
const TEMPO_FADER_SLOT: f32 = 6.0;
const TEMPO_FADER_CAP: egui::Vec2 = egui::vec2(36.0, 15.0);
/// Gaps between the tempo fader's notches: five notches.
const TEMPO_FADER_NOTCH_GAPS: u32 = 4;
/// The keylock radio left of the fader: an 18px well, an 8px amber dot,
/// in a 22-wide target with 4 above and below.
const KEYLOCK_RADIO: f32 = 18.0;
const KEYLOCK_DOT: f32 = 8.0;
const KEYLOCK_HIT: egui::Vec2 = egui::vec2(22.0, KEYLOCK_RADIO + 8.0);
/// The hot cue strip and the key row under it, each in a tray.
const HOT_CUE_H: f32 = theme::SIZE_PAD;
/// PLAY and CUE, and the gap between them.
const TRANSPORT_W: f32 = 64.0;
const TRANSPORT_GAP: f32 = theme::SPACE_4;
const NUDGE_W: f32 = 30.0;
/// GATE ends at the tray's padding, so its left edge lines up with the
/// tempo rail's.
const GATE_W: f32 = TEMPO_RAIL_W - theme::TRAY_PAD;
/// The LOOP strip's segments at full width: IN, OUT, ÷2, length, ×2, EXIT.
const LOOP_WIDTHS: [f32; 6] = [52.0, 54.0, 46.0, 94.0, 46.0, 68.0];
/// The length segment, which takes the strip's leftover width.
const LOOP_LENGTH: usize = 3;

/// What the deck's header reads, measured once per frame by `deck_panel`.
struct DeckReadings {
    has_track: bool,
    is_master: bool,
    playhead: usize,
    total: usize,
    sample_rate: u32,
    /// Displayed tempo as a multiple of the track's own BPM.
    tempo_rate: f64,
}

/// The deck's header: one well split by hairlines into artwork | title
/// over artist | A.CUE over Q | time | BPM | KEY | PITCH.
fn deck_header(ui: &mut egui::Ui, deck_ui: &mut DeckUi, r: &DeckReadings) {
    let ctx = ui.ctx().clone();
    let (tray, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), DECK_HEADER_H),
        egui::Sense::hover(),
    );
    theme::paint_well(ui.painter(), tray, theme::RADIUS_CONTROL);

    let fixed = HEADER_ART_W
        + HEADER_TOGGLES_W
        + HEADER_TIME_W
        + HEADER_BPM_W
        + HEADER_KEY_W
        + HEADER_PITCH_W;
    let mut x = tray.left();
    let mut cell = |w: f32| {
        let rect =
            egui::Rect::from_min_size(egui::pos2(x, tray.top()), egui::vec2(w, tray.height()));
        x += w;
        rect
    };
    let art = cell(HEADER_ART_W);
    let title = cell((tray.width() - fixed).max(0.0));
    let toggles = cell(HEADER_TOGGLES_W);
    let time = cell(HEADER_TIME_W);
    let bpm = cell(HEADER_BPM_W);
    let key = cell(HEADER_KEY_W);
    let pitch = cell(HEADER_PITCH_W);
    for c in [title, toggles, time, bpm, key, pitch] {
        ui.painter().vline(
            c.left() + 0.5,
            c.y_range(),
            egui::Stroke::new(1.0, theme::QUIET_HAIRLINE),
        );
    }

    // Artwork, or the note glyph.
    let r_art = theme::RADIUS_CONTROL;
    match &deck_ui.artwork {
        Some(tex) => {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().add(
                egui::epaint::RectShape::filled(
                    art,
                    egui::CornerRadius {
                        nw: r_art,
                        sw: r_art,
                        ne: 0,
                        se: 0,
                    },
                    egui::Color32::WHITE,
                )
                .with_texture(tex.id(), uv),
            );
        }
        None => {
            ui.painter().text(
                art.center(),
                egui::Align2::CENTER_CENTER,
                "♪",
                egui::FontId::proportional(HEADER_ART_GLYPH),
                theme::TEXT_DISABLED,
            );
        }
    }

    // The title, with the artist under it when the tags have one, centred
    // as a block in the tray.
    let inner = egui::Rect::from_min_max(
        egui::pos2(title.left() + HEADER_TITLE_PAD_L, title.top()),
        egui::pos2(title.right() - HEADER_TITLE_PAD_R, title.bottom()),
    );
    let mut text = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    text.set_clip_rect(inner.intersect(ui.clip_rect()));
    text.spacing_mut().item_spacing.y = HEADER_ARTIST_GAP;
    if r.has_track {
        let artist = deck_ui
            .artist
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty());
        let title_h = theme::TRACK_TITLE
            .galley(&ctx, "Ag", theme::TEXT_STRONG)
            .size()
            .y;
        let block_h = match artist {
            Some(_) => {
                let artist_h = theme::BODY_TEXT
                    .galley(&ctx, "Ag", theme::TEXT_MUTED)
                    .size()
                    .y;
                title_h + HEADER_ARTIST_GAP + artist_h
            }
            None => title_h,
        };
        text.add_space(((DECK_HEADER_H - block_h) / 2.0).max(0.0));
        text.add(
            egui::Label::new(
                theme::TRACK_TITLE
                    .rich(&deck_ui.title)
                    .color(theme::TEXT_STRONG),
            )
            .truncate(),
        );
        if let Some(artist) = artist {
            text.add(
                egui::Label::new(theme::BODY_TEXT.rich(artist).color(theme::TEXT_MUTED)).truncate(),
            );
        }
    } else {
        let body_h = theme::BODY_TEXT
            .galley(&ctx, "Ag", theme::TEXT_MUTED)
            .size()
            .y;
        text.add_space((DECK_HEADER_H - body_h) / 2.0);
        text.label(
            theme::BODY_TEXT
                .rich("No track loaded")
                .color(theme::TEXT_MUTED),
        );
    }

    // A.CUE over Q, two latches filling the cell inside its inset.
    let inner = toggles.shrink2(HEADER_LATCH_INSET);
    let latch_size = egui::vec2(inner.width(), (inner.height() - HEADER_LATCH_GAP) / 2.0);
    let mut latches = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    latches.spacing_mut().item_spacing.y = HEADER_LATCH_GAP;
    latches.add_enabled_ui(r.has_track, |ui| {
        key::latch(ui, &mut deck_ui.auto_cue, "A.CUE", latch_size).on_hover_text(
            "Auto cue: on load, set the cue to the first detected beat and park the deck there",
        );
        key::latch(ui, &mut deck_ui.quantize, "Q", latch_size)
            .on_hover_text("Quantize hot cues and loops to the beat grid");
    });

    // Time: clicking it switches elapsed and remaining.
    let time_resp = ui
        .interact(time, ui.id().with("deck-time"), egui::Sense::click())
        .on_hover_text(if deck_ui.show_remaining {
            "Show elapsed time"
        } else {
            "Show remaining time"
        });
    if time_resp.clicked() {
        deck_ui.show_remaining = !deck_ui.show_remaining;
    }
    let (caption, value) = if deck_ui.show_remaining {
        (
            "REMAINING",
            format!(
                "−{}",
                format_time(r.total.saturating_sub(r.playhead), r.sample_rate)
            ),
        )
    } else {
        ("ELAPSED", format_time(r.playhead, r.sample_rate))
    };
    let caption_color = if time_resp.hovered() {
        theme::TEXT
    } else {
        theme::TEXT_MUTED
    };
    let painter = ui.painter();
    header_reading(
        painter,
        time,
        HEADER_CELL_PAD,
        (caption, caption_color),
        (&value, theme::QUIET_NUMERALS, theme::TEXT_STRONG),
    );

    // BPM: amber while this deck is the tempo master.
    let bpm_text = if deck_ui.bpm > 0.0 && r.has_track {
        format!("{:.1}", deck_ui.bpm * r.tempo_rate)
    } else {
        "—".to_string()
    };
    let bpm_color = if r.is_master {
        ACCENT
    } else {
        theme::TEXT_STRONG
    };
    header_reading(
        painter,
        bpm,
        HEADER_CELL_PAD,
        ("BPM", theme::TEXT_MUTED),
        (&bpm_text, theme::QUIET_NUMERALS, bpm_color),
    );

    let key_text = match (&deck_ui.key, r.has_track) {
        (Some(k), true) => k.as_str(),
        _ => "—",
    };
    header_reading(
        painter,
        key,
        HEADER_KEY_PAD,
        ("KEY", theme::TEXT_MUTED),
        (key_text, theme::NUMERALS_MD, theme::MUSICAL_KEY),
    );

    let pitch_text = format!("{:+.1}%", deck_ui.pitch_percent).replace('-', "−");
    header_reading(
        painter,
        pitch,
        HEADER_CELL_PAD,
        ("PITCH", theme::TEXT_MUTED),
        (&pitch_text, theme::NUMERALS_MD, theme::TEXT),
    );
}

/// One header reading in `cell`: its caption over its value, both at the
/// left, `pad` in from it.
fn header_reading(
    painter: &egui::Painter,
    cell: egui::Rect,
    pad: f32,
    (caption, caption_color): (&str, egui::Color32),
    (value, style, color): (&str, theme::TypeStyle, egui::Color32),
) {
    let x = cell.left() + pad;
    let caption_y = cell.top() + HEADER_CAPTION_TOP + HEADER_CAPTION_H / 2.0;
    let value_y = cell.top() + HEADER_CAPTION_TOP + HEADER_CAPTION_H + HEADER_VALUE_H / 2.0;
    theme::paint_text(
        painter,
        egui::pos2(x, caption_y),
        egui::Align2::LEFT_CENTER,
        caption,
        theme::QUIET_CAPTION,
        caption_color,
    );
    theme::paint_text(
        painter,
        egui::pos2(x, value_y),
        egui::Align2::LEFT_CENTER,
        value,
        style,
        color,
    );
}

/// A [`WAVE_STACK_GAP`] across the waveform stack, blank on the
/// waveform's ground.
fn wave_stack_gap(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), WAVE_STACK_GAP),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, theme::WAVE_GROUND);
}

/// The zoom row under the zoomed waveform: the BAR counter at the left,
/// "− 4 BARS +" in quiet muted captions at the right, each sign a click
/// target.
fn zoom_row(ui: &mut egui::Ui, deck_ui: &mut DeckUi, idx: usize, display_pos: f64) {
    let (row, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ZOOM_ROW_H),
        egui::Sense::hover(),
    );
    let row = row.with_min_y(row.top() + ZOOM_ROW_TOP);

    let mut counter = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(row.with_min_x(row.left() + ZOOM_COUNTER_PAD))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    counter.spacing_mut().item_spacing.x = theme::SPACE_8;
    paint_beat_counter(
        &mut counter,
        &deck_ui.marks,
        display_pos,
        theme::QUIET_CAPTION,
        false,
    );
    if deck_ui.marks.low_confidence() {
        ui.interact(
            counter.min_rect(),
            egui::Id::new(("deck-counter", idx)),
            egui::Sense::hover(),
        )
        .on_hover_text("Beat grid: low confidence");
    }

    let label = deck_ui.zoom.label(deck_ui.marks.is_usable());
    let label_w = theme::QUIET_CAPTION.width(ui.ctx(), &label);
    let span = |right: f32, w: f32| {
        egui::Rect::from_min_max(
            egui::pos2(right - w, row.top()),
            egui::pos2(right, row.bottom()),
        )
    };
    let plus = span(row.right(), ZOOM_KEY_W);
    let text = span(plus.left(), label_w);
    let minus = span(text.left(), ZOOM_KEY_W);
    theme::paint_text(
        ui.painter(),
        text.center(),
        egui::Align2::CENTER_CENTER,
        &label,
        theme::QUIET_CAPTION,
        theme::TEXT_MUTED,
    );
    for (rect, sign, hint, zoom_in) in [
        (minus, "−", "Zoom out", false),
        (plus, "+", "Zoom in", true),
    ] {
        let resp = ui
            .interact(
                rect,
                egui::Id::new(("deck-zoom", idx, zoom_in)),
                egui::Sense::click(),
            )
            .on_hover_text(hint);
        let color = if resp.hovered() {
            theme::TEXT
        } else {
            theme::TEXT_MUTED
        };
        theme::paint_text(
            ui.painter(),
            rect.center(),
            egui::Align2::CENTER_CENTER,
            sign,
            theme::QUIET_CAPTION,
            color,
        );
        if resp.clicked() {
            if zoom_in {
                deck_ui.zoom.zoom_in();
            } else {
                deck_ui.zoom.zoom_out();
            }
        }
    }
}

/// The tempo rail beside the waveform well: SYNC, the tempo fader with the
/// keylock radio level with its middle, and the RANGE selector, spanning
/// the well's height.
fn deck_tempo_rail(
    ui: &mut egui::Ui,
    deck_ui: &mut DeckUi,
    deck_idx: usize,
    is_master: bool,
    has_track: bool,
) {
    ui.add_enabled_ui(has_track, |ui| {
        ui.spacing_mut().item_spacing.y = DECK_GAP;
        // A master leads rather than follows. Retain its stored Sync intent
        // for a later handoff, but show only the currently active role.
        let following = deck_ui.synced && !is_master;
        let sync = ui
            .add(
                outlined_key(following, "SYNC")
                    .size([TEMPO_RAIL_W, theme::SIZE_KEY])
                    .enabled(!is_master)
                    .quiet(),
            )
            .on_hover_text(
                "Match the master BPM; Play keeps the parked position and waits for the next \
                 matching beat phase",
            );
        if sync.clicked() {
            deck_ui.synced = !deck_ui.synced;
            deck_ui
                .deck
                .shared
                .sync_enabled
                .store(deck_ui.synced, Ordering::Release);
        }

        let fader_h = WAVE_STACK_H - 2.0 * (theme::SIZE_KEY + DECK_GAP);
        let (area, _) =
            ui.allocate_exact_size(egui::vec2(TEMPO_RAIL_W, fader_h), egui::Sense::hover());

        // Keylock: a radio left of the fader, level with its middle.
        let radio = egui::Rect::from_min_size(
            egui::pos2(area.left(), area.center().y - KEYLOCK_HIT.y / 2.0),
            KEYLOCK_HIT,
        );
        let keylock = ui
            .interact(radio, ui.id().with("keylock"), egui::Sense::click())
            .on_hover_text("Keylock: keep pitch constant while tempo changes");
        if keylock.clicked() {
            deck_ui.keylock = !deck_ui.keylock;
        }
        paint_radio(
            ui.painter(),
            radio.center(),
            deck_ui.keylock,
            keylock.enabled(),
        );

        // Absolute-position fader: `.changed()` only fires on a real
        // grab, so touching it hands control back and drops sync
        // (matching the old slider). While synced, update_tempo keeps
        // writing pitch_percent and the fader just displays it.
        let range = deck_ui.pitch_range;
        let mut pct = deck_ui.pitch_percent;
        let fader_rect =
            egui::Rect::from_center_size(area.center(), egui::vec2(TEMPO_FADER_W, fader_h));
        let fader = ui
            .new_child(egui::UiBuilder::new().max_rect(fader_rect))
            .add(
                Fader::new(&mut pct, -range.min(100.0)..=range, ACCENT)
                    .vertical(true)
                    .size([TEMPO_FADER_W, fader_h])
                    .groove_width(TEMPO_FADER_SLOT)
                    .cap_size(TEMPO_FADER_CAP.x, TEMPO_FADER_CAP.y)
                    .notches(Notches::Even(TEMPO_FADER_NOTCH_GAPS))
                    .default_value(0.0),
            );
        if fader.changed() {
            deck_ui.pitch_percent =
                ((legal_tempo(1.0 + pct as f64 / 100.0, deck_ui.keylock) - 1.0) * 100.0) as f32;
            deck_ui.synced = false;
            deck_ui
                .deck
                .shared
                .sync_enabled
                .store(false, Ordering::Release);
        }

        // RANGE: a key opening the range menu.
        let prev_range = deck_ui.pitch_range;
        let prev_wide = deck_ui.wide;
        let selected = if deck_ui.wide {
            "WIDE".to_string()
        } else {
            format!("±{prev_range:.0}%")
        };
        let range_key = ui
            .add(
                Key::new(format!("{selected} ▼"))
                    .size([TEMPO_RAIL_W, theme::SIZE_KEY])
                    .quiet(),
            )
            .on_hover_text("Pitch range");
        let popup = ui.make_persistent_id(("pitch-range", deck_idx));
        if range_key.clicked() {
            ui.memory_mut(|m| m.toggle_popup(popup));
        }
        egui::popup::popup_below_widget(
            ui,
            popup,
            &range_key,
            egui::PopupCloseBehavior::CloseOnClick,
            |ui| {
                ui.set_min_width(TEMPO_RAIL_W);
                for r in [8.0_f32, 16.0, 50.0] {
                    if ui
                        .selectable_label(
                            !deck_ui.wide && deck_ui.pitch_range == r,
                            format!("±{r:.0}%"),
                        )
                        .clicked()
                    {
                        deck_ui.wide = false;
                        deck_ui.pitch_range = r;
                    }
                }
                if ui
                    .selectable_label(deck_ui.wide, "WIDE")
                    .on_hover_text(
                        "Full tempo range: down to 1/32 speed with keylock, \
                         1/4 with linked pitch, up to 4x. Bottom detent stops \
                         playback; raise the fader to resume.",
                    )
                    .clicked()
                {
                    deck_ui.wide = true;
                    deck_ui.pitch_range = 300.0;
                }
            },
        );
        if deck_ui.pitch_range != prev_range || deck_ui.wide != prev_wide {
            deck_ui.pitch_percent = deck_ui
                .pitch_percent
                .clamp(-deck_ui.pitch_range.min(100.0), deck_ui.pitch_range);
        }
    });
}

/// A radio: an 18px round well (edge ring, a lip of sheen under it) holding
/// an 8px amber dot with a glow when `on`.
fn paint_radio(painter: &egui::Painter, center: egui::Pos2, on: bool, enabled: bool) {
    let r = KEYLOCK_RADIO / 2.0;
    painter.circle_filled(center + egui::vec2(0.0, 1.0), r + 1.0, theme::SHEEN);
    painter.circle_filled(center, r + 1.0, theme::EDGE);
    painter.circle_filled(center, r, theme::WELL);
    // The well's inner shade along its top.
    painter.circle_stroke(
        center + egui::vec2(0.0, 0.5),
        r - 0.5,
        egui::Stroke::new(1.0, theme::SHADE),
    );
    painter.circle_filled(center, r - 1.0, theme::WELL);
    if on {
        let dot = egui::Rect::from_center_size(center, egui::Vec2::splat(KEYLOCK_DOT));
        let fade = if enabled { 1.0 } else { 0.4 };
        theme::paint_glow(
            painter,
            dot,
            KEYLOCK_DOT / 2.0,
            KEYLOCK_DOT,
            theme::ACCENT_GLOW.gamma_multiply(fade),
        );
        painter.circle_filled(center, KEYLOCK_DOT / 2.0, ACCENT.gamma_multiply(fade));
    }
}

/// One deck's knob strip (Trim/Hi/Mid/Low/Filter) with the volume fader
/// centered beneath it, in a fixed-width column so the knobs land directly
/// over the fader. Returns the column rect (used to size the level meter).
fn deck_channel_strip(ui: &mut egui::Ui, shared: &crate::state::DeckShared) -> egui::Rect {
    // Each knob sits `space-2` under its caption, `space-8` from the next.
    let caption = |ui: &mut egui::Ui, text: &str| {
        ui.label(theme::CAPTION.rich(text).color(theme::TEXT_MUTED));
        ui.add_space(theme::SPACE_2);
    };
    ui.scope(|ui| {
        ui.set_width(MIXER_STRIP_W);
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.vertical_centered(|ui| {
            // Trim + isolator EQ: all bipolar knobs centered at unity (1.0),
            // range 0..2 (kill hard left, +6 dB hard right).
            for (cap, label, atom) in [
                ("TRIM", "Trim", &shared.trim),
                ("HI", "Hi", &shared.eq_high),
                ("MID", "Mid", &shared.eq_mid),
                ("LOW", "Low", &shared.eq_low),
            ] {
                caption(ui, cap);
                let mut v = atom.load();
                if ui
                    .add(
                        Knob::new(&mut v, 0.0..=2.0, ACCENT)
                            .arc(KnobArc::Bipolar { center: 1.0 })
                            .default_value(1.0)
                            .diameter(MIXER_KNOB),
                    )
                    .on_hover_text(format!("{label}: {:+.1} dB", 20.0 * v.max(1e-4).log10()))
                    .changed()
                {
                    atom.store(v);
                }
                ui.add_space(theme::SPACE_8);
            }

            // Filter: one bipolar knob. Center (12 o'clock) = off; twist left =
            // low-pass sweeping closed, right = high-pass sweeping closed. Rides
            // a synthetic position t in [-1, 1] mapped to (mode, cutoff).
            caption(ui, "FILTER");
            let mut t = filter_pos(shared.filter_mode_u8(), shared.filter_cutoff.load());
            if ui
                .add(
                    Knob::new(&mut t, -1.0..=1.0, ACCENT)
                        .arc(KnobArc::Bipolar { center: 0.0 })
                        .default_value(0.0)
                        .diameter(MIXER_KNOB),
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

            ui.add_space(theme::SPACE_16);
            let mut fader = shared.fader.load();
            if ui
                .add(
                    Fader::new(&mut fader, 0.0..=1.0, ACCENT)
                        .vertical(true)
                        .size([CHANNEL_FADER_W, 120.0])
                        .cap_size(CHANNEL_FADER_CAP.x, CHANNEL_FADER_CAP.y)
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

/// The mixer's channel knobs: well under the system's `size-knob`, so the
/// mixer column stays nearer the decks' height.
const MIXER_KNOB: f32 = 43.0;
/// Width of one mixer channel strip: a knob across.
const MIXER_STRIP_W: f32 = MIXER_KNOB;
/// The channel volume faders, and their caps: span across the travel
/// by thickness along it.
const CHANNEL_FADER_W: f32 = 40.0;
const CHANNEL_FADER_CAP: egui::Vec2 = egui::vec2(36.0, 12.0);
/// Both strips with the LUFS ladders between them:
/// strip + 8 + 12 + 3 + 12 + 8 + strip.
const MIXER_W: f32 = MIXER_STRIP_W + 8.0 + 12.0 + 3.0 + 12.0 + 8.0 + MIXER_STRIP_W;

fn mixer_panel(ui: &mut egui::Ui, decks: &[DeckUi; 2]) {
    // Two channel strips packed toward the center, with the pair of level
    // meters between them (classic DJ-mixer layout). `vertical_centered` won't
    // center a multi-widget horizontal row (egui seeds it at full width), so
    // pad-center it to the panel mid-line. No crossfader: the volume faders
    // set each deck's level, and the crossfader stays centred.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(((ui.available_width() - MIXER_W) * 0.5).max(0.0));
        let a = deck_channel_strip(ui, &decks[0].deck.shared);
        ui.add_space(8.0);
        for (i, gap) in [(0, 3.0), (1, 8.0)] {
            let shared = &decks[i].deck.shared;
            let artifact = decks[i].deck.pre_analysis.as_ref();
            deck_lufs_meter(
                ui,
                shared.meter_lufs.load(),
                shared.meter.load(),
                artifact
                    .and_then(|a| a.loudness)
                    .map(|l| l.integrated_lufs as f32),
                a.height(),
            );
            ui.add_space(gap);
        }
        deck_channel_strip(ui, &decks[1].deck.shared);
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

/// Vertical channel meter on a LUFS scale (−40..0 mapped linearly to the
/// bar — LUFS is already logarithmic). A segmented LED ladder rises to the
/// momentary (400 ms) loudness, each segment colored by the zone its own
/// position occupies; a light marker sits at the track's analyzed
/// integrated loudness (gain-match decks by aligning bars to markers), and
/// a red strip at the top flags peaks at the ceiling. All inputs are
/// pre-fader / post-trim, so the meter shows the track's level regardless
/// of fader position.
fn deck_lufs_meter(
    ui: &mut egui::Ui,
    momentary_lufs: f32,
    peak_linear: f32,
    integrated_lufs: Option<f32>,
    height: f32,
) {
    /// LUFS at the bar bottom (top is 0).
    const RANGE_LO: f32 = -40.0;
    /// Healthy up to here (common normalization target)…
    const GREEN_TO: f32 = -14.0;
    /// …amber to here (hot but normal for club masters at unity trim),
    /// red above: trim too high, headed into the limiter.
    const AMBER_TO: f32 = -9.0;
    let norm = |lufs: f32| ((lufs - RANGE_LO) / -RANGE_LO).clamp(0.0, 1.0);

    let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, height), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    response.on_hover_text(match integrated_lufs {
        Some(i) => format!("Momentary {momentary_lufs:.1} LUFS\nTrack {i:.1} LUFS integrated"),
        None => format!("Momentary {momentary_lufs:.1} LUFS\nTrack: not analyzed"),
    });

    // A well with a 1px edge; every 3 px step is drawn, unlit ones in
    // `segment-off`, so the ladder reads before it lights.
    let painter = ui.painter();
    theme::paint_well_edged(painter, rect.shrink(0.5), 2);
    let inner = height - 4.0;
    let lit = norm(momentary_lufs) * inner;
    let mut y = 0.0;
    while y + 3.0 <= inner {
        let lufs_here = RANGE_LO * (1.0 - (y + 1.5) / inner);
        let color = if y + 1.5 > lit {
            theme::SEGMENT_OFF
        } else if lufs_here < GREEN_TO {
            theme::METER_GREEN
        } else if lufs_here < AMBER_TO {
            theme::ACCENT
        } else {
            theme::METER_RED
        };
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(rect.left() + 2.0, rect.bottom() - 2.0 - y - 3.0),
                egui::vec2(8.0, 3.0),
            ),
            0.0,
            color,
        );
        y += 4.0;
    }

    if let Some(i) = integrated_lufs {
        let marker_y = rect.bottom() - 2.0 - norm(i) * inner;
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(rect.left() + 1.0, marker_y - 1.0),
                egui::vec2(10.0, 2.0),
            ),
            0.0,
            theme::TEXT_STRONG,
        );
    }

    if peak_linear >= 0.99 {
        painter.rect_filled(
            egui::Rect::from_min_size(rect.left_top() + egui::vec2(2.0, 2.0), egui::vec2(8.0, 2.0)),
            0.0,
            theme::METER_RED,
        );
    }
}

/// Height of the toolbar's control row: the small master knob.
const TOOLBAR_ROW_H: f32 = theme::SIZE_KNOB_SMALL;

/// Toolbar master output meter as LED steps: the summed
/// stream level measured post-master / post-limiter, so pulling the master
/// knob down visibly limits it. Red = pinned near the ceiling (limiting).
fn master_level_meter(ui: &mut egui::Ui, level: f32) {
    let (rect, response) = ui.allocate_exact_size(theme::STEPS_SIZE, egui::Sense::hover());
    response.on_hover_text(format!("Output: {:.0}%", level.clamp(0.0, 1.0) * 100.0));
    theme::paint_steps(ui.painter(), rect, level);
}

/// The master tempo (MasterBpm): a 144×30 well, the tempo in `numerals-md`
/// centred between A and B at either end, the source deck lit in
/// `lane-lighting` (the rig follows the same deck).
fn paint_master_bpm(ui: &egui::Ui, center: egui::Pos2, bpm: f64, source: usize) {
    let ctx = ui.ctx();
    let painter = ui.painter();
    let well = egui::Rect::from_center_size(center, egui::vec2(144.0, theme::SIZE_KEY));
    theme::paint_well(painter, well, theme::RADIUS_CONTROL);

    // A and B at the ends, 10 in.
    let mut ends = [0.0_f32; 2];
    for (i, letter) in DECK_NAMES.iter().enumerate() {
        let lit = i == source;
        let color = if lit { ACCENT } else { theme::TEXT_MUTED };
        let g = theme::KEY_LABEL.galley(ctx, letter, color);
        let w = g.size().x - theme::KEY_LABEL.spacing;
        let x = if i == 0 {
            well.left() + 10.0
        } else {
            well.right() - 10.0 - w
        };
        ends[i] = if i == 0 { x + w } else { x };
        let rect = egui::Rect::from_min_size(
            egui::pos2(x, well.center().y - g.size().y / 2.0),
            egui::vec2(w, g.size().y),
        );
        painter.galley(rect.min, g, color);
    }

    // The tempo, centred between the letters.
    let text = if bpm > 0.0 {
        format!("{bpm:.1}")
    } else {
        "—".to_string()
    };
    let value = theme::NUMERALS_MD.galley(ctx, &text, theme::TEXT_STRONG);
    let value_w = value.size().x - theme::NUMERALS_MD.spacing;
    let left = ((ends[0] + ends[1]) - value_w) / 2.0;
    let value_top = well.center().y - value.size().y / 2.0;
    painter.galley(
        egui::pos2(left, value_top).round(),
        value,
        theme::TEXT_STRONG,
    );
}

/// Toolbar readout: DSP = audio-callback load (render time ÷ buffer time,
/// the number that matters for dropouts), zone-coloured; CPU = whole-process
/// usage. Both are latched once a second so the digits are legible.
fn cpu_meter(ui: &mut egui::Ui, dsp_latched: f32, process_pct: f32) {
    let dsp_pct = (dsp_latched * 100.0).clamp(0.0, 999.0);
    let zone = if dsp_pct < 50.0 {
        theme::METER_GREEN
    } else if dsp_pct < 80.0 {
        theme::ACCENT
    } else {
        theme::METER_RED
    };
    // Padded with figure spaces (as wide as a tabular digit) so the digits
    // never shift.
    let pad = |pct: f32| format!("{pct:>4.1}").replace(' ', "\u{2007}");
    let ctx = ui.ctx().clone();
    let dsp = theme::NUMERALS_SM.galley(&ctx, &format!("DSP {}%", pad(dsp_pct)), zone);
    let cpu = theme::NUMERALS_SM.galley(
        &ctx,
        &format!("CPU {}%", pad(process_pct)),
        theme::TEXT_MUTED,
    );
    // 8 | DSP | 8 | groove | 8 | CPU | 8
    let well_size = egui::vec2(
        dsp.size().x + cpu.size().x + 2.0 + 4.0 * theme::SPACE_8,
        22.0,
    );
    let (well, response) = ui.allocate_exact_size(well_size, egui::Sense::hover());
    response.on_hover_text("Busiest audio processing thread: stretching or output delivery.");
    let painter = ui.painter();
    theme::paint_well(painter, well, theme::RADIUS_CONTROL);
    let y = well.center().y - dsp.size().y / 2.0;
    let mut x = well.left() + theme::SPACE_8;
    let dsp_w = dsp.size().x;
    painter.galley(egui::pos2(x, y), dsp, zone);
    x += dsp_w + theme::SPACE_8;
    let groove = egui::Rangef::new(well.center().y - 6.0, well.center().y + 6.0);
    painter.vline(x + 0.5, groove, egui::Stroke::new(1.0, theme::EDGE));
    painter.vline(x + 1.5, groove, egui::Stroke::new(1.0, theme::LIP));
    x += 2.0 + theme::SPACE_8;
    painter.galley(egui::pos2(x, y), cpu, theme::TEXT_MUTED);
}

/// Cumulative user+system CPU time of this process, in seconds.
fn process_cpu_secs() -> f64 {
    use nix::sys::resource::{UsageWho, getrusage};
    let secs = |tv: nix::sys::time::TimeVal| tv.tv_sec() as f64 + tv.tv_usec() as f64 * 1e-6;
    getrusage(UsageWho::RUSAGE_SELF)
        .map(|u| secs(u.user_time()) + secs(u.system_time()))
        .unwrap_or(0.0)
}

/// Spaced uppercase caption over a control group ("TRANSPORT", …):
/// `caption` in `text-muted`, dimmed to `text-disabled` when the enclosing
/// scope is disabled (no track loaded).
fn section_caption(ui: &mut egui::Ui, text: &str) {
    let color = if ui.is_enabled() {
        theme::TEXT_MUTED
    } else {
        theme::TEXT_DISABLED
    };
    ui.label(theme::CAPTION.rich(text).color(color));
}

/// Caption over a horizontal control row, `space-6` under the caption and
/// `space-8` between controls; returns the row's inner value.
fn control_group<R>(
    ui: &mut egui::Ui,
    caption: &str,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        section_caption(ui, caption);
        ui.add_space(theme::SPACE_6);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_8;
            content(ui)
        })
        .inner
    })
    .inner
}

/// Width `group_divider` occupies: 3 before the groove, 2 for it, 4 after.
const GROUP_DIVIDER_W: f32 = 9.0;

/// The groove between captioned control groups: a 1px `edge` line with a
/// 1px `lip` of light beside it.
fn group_divider(ui: &mut egui::Ui, height: f32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(GROUP_DIVIDER_W, height), egui::Sense::hover());
    let x = rect.left() + 3.5;
    let painter = ui.painter();
    painter.vline(x, rect.y_range(), egui::Stroke::new(1.0, theme::EDGE));
    painter.vline(x + 1.0, rect.y_range(), egui::Stroke::new(1.0, theme::LIP));
}

/// Latching key with an LED before its label that lights amber when active
/// (SNAP, and the deck's SYNC and GATE). Off, the LED is dark and the
/// label `text-unlit`. `width: None` sizes it to the label.
fn outlined_toggle(
    ui: &mut egui::Ui,
    active: bool,
    label: &str,
    width: Option<f32>,
) -> egui::Response {
    let mut key = outlined_key(active, label);
    if let Some(width) = width {
        key = key.width(width);
    }
    ui.add(key)
}

/// The key [`outlined_toggle`] adds, for callers that size or skin it.
fn outlined_key(active: bool, label: &str) -> Key {
    Key::new(label).led(active, ACCENT).label_color(if active {
        theme::TEXT_STRONG
    } else {
        theme::TEXT_UNLIT
    })
}

/// Musical-key badge under the deck title: the raw tag string in
/// `musical-key` blue, in a small well 20 high with 7 either side.
fn key_badge(ui: &mut egui::Ui, key: &str) {
    let galley = theme::KEY_LABEL.galley(ui.ctx(), key, theme::MUSICAL_KEY);
    let text_w = galley.size().x - theme::KEY_LABEL.spacing;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(text_w + 14.0, 20.0), egui::Sense::hover());
    theme::paint_well(ui.painter(), rect, theme::RADIUS_CONTROL);
    let pos = egui::pos2(rect.left() + 7.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, theme::MUSICAL_KEY);
}

/// An 18px look swatch in Prepare's palette: the look's colour with a 1px
/// edge; the armed look carries a `text-strong` ring.
fn look_swatch(ui: &mut egui::Ui, color: egui::Color32, armed: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, theme::RADIUS_CONTROL, color.gamma_multiply(0.85));
    theme::paint_edge(painter, rect, theme::RADIUS_CONTROL, theme::EDGE);
    if armed {
        painter.rect_stroke(
            rect.expand(1.0),
            theme::RADIUS_CONTROL,
            egui::Stroke::new(2.0, theme::TEXT_STRONG),
            egui::StrokeKind::Outside,
        );
    }
    resp
}

fn apply_theme(ctx: &egui::Context) {
    // Pin to dark regardless of the OS appearance setting; set_visuals only
    // styles the active theme, so following the system would fall back to
    // egui's stock light visuals.
    ctx.set_theme(egui::ThemePreference::Dark);
    theme::install(ctx);

    let mut visuals = egui::Visuals::dark();
    // Panels add their own face on top of panel-deep; wells are the
    // deepest surface.
    visuals.panel_fill = theme::PANEL_DEEP;
    visuals.window_fill = theme::PANEL_DEEP;
    visuals.window_stroke = egui::Stroke::new(1.0, theme::EDGE_PANEL);
    visuals.window_corner_radius = theme::RADIUS_WINDOW.into();
    visuals.menu_corner_radius = theme::RADIUS_PANEL.into();
    visuals.extreme_bg_color = theme::WELL;
    visuals.code_bg_color = theme::WELL;
    // Table stripes sit in wells: one step up from the well.
    visuals.faint_bg_color = theme::PANEL_DEEP;
    visuals.warn_fg_color = theme::WARN;
    visuals.error_fg_color = theme::WARN;
    visuals.hyperlink_color = theme::ACCENT;
    visuals.selection.bg_fill = theme::SELECTION;
    visuals.selection.stroke = egui::Stroke::new(1.0, theme::TEXT_STRONG);
    visuals.text_cursor.stroke = egui::Stroke::new(2.0, theme::ACCENT);
    visuals.slider_trailing_fill = true;

    // Stock widgets (tabs, combo boxes, scroll bars, menus) take the key's
    // colours and the dark edge. Nothing grows on hover.
    let edge = egui::Stroke::new(1.0, theme::EDGE);
    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = theme::PANEL_DEEP;
    // egui derives "weak" text by tinting halfway towards this colour;
    // cap-hi lands it at text-muted against the panels.
    w.noninteractive.weak_bg_fill = theme::CAP_HI;
    w.noninteractive.bg_stroke = edge;
    w.noninteractive.fg_stroke = egui::Stroke::new(1.0, theme::TEXT);
    w.inactive.bg_fill = theme::KEY;
    w.inactive.weak_bg_fill = theme::KEY;
    w.inactive.bg_stroke = edge;
    w.inactive.fg_stroke = egui::Stroke::new(1.0, theme::TEXT);
    w.hovered.bg_fill = theme::KEY_HI;
    w.hovered.weak_bg_fill = theme::KEY_HI;
    w.hovered.bg_stroke = edge;
    w.hovered.fg_stroke = egui::Stroke::new(1.0, theme::TEXT_STRONG);
    w.active.bg_fill = theme::KEY_DOWN;
    w.active.weak_bg_fill = theme::KEY_DOWN;
    w.active.bg_stroke = edge;
    w.active.fg_stroke = egui::Stroke::new(1.0, theme::TEXT_STRONG);
    w.open.bg_fill = theme::KEY_DOWN;
    w.open.weak_bg_fill = theme::KEY_DOWN;
    w.open.bg_stroke = edge;
    w.open.fg_stroke = egui::Stroke::new(1.0, theme::TEXT_STRONG);
    for v in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        v.expansion = 0.0;
        v.corner_radius = theme::RADIUS_CONTROL.into();
    }
    ctx.set_visuals(visuals);

    ctx.style_mut(|style| {
        use egui::TextStyle;
        style.text_styles = [
            (TextStyle::Heading, theme::TRACK_TITLE.font_id()),
            (TextStyle::Body, theme::BODY_TEXT.font_id()),
            (TextStyle::Button, theme::BODY_TEXT.font_id()),
            (TextStyle::Small, theme::NOTE_TEXT.font_id()),
            (TextStyle::Monospace, theme::MONO_TEXT.font_id()),
        ]
        .into();
        // Stock buttons and combo boxes are small keys.
        style.spacing.interact_size.y = theme::SIZE_KEY_SMALL;
        style.spacing.button_padding = egui::vec2(theme::SPACE_8, 2.0);
    });
}

/// Seek, exiting any active loop the target falls outside of (otherwise
/// the session rejects seeks beyond an active loop end).
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

/// Supported nonzero rates plus the full-stop fader detent. Keep the displayed
/// rate in the same domain as the DSP, including when keylock is switched off.
fn legal_tempo(rate: f64, keylock: bool) -> f64 {
    if rate <= 0.001 {
        return 0.0;
    }
    crate::deck::playback_parameters(rate, keylock).map_or(1.0, |p| p.tempo.get())
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
    fn cached_analysis_auto_cue_keeps_first_beat_across_device_rates() {
        // A confident cached grid can label the second kick as a bar start.
        // Loading it at a different device rate must still cue the first kick.
        let first = 220.5;
        let period = 60.0 * 44_100.0 / 124.0;
        let beats: Vec<_> = (0..8).map(|i| first + i as f64 * period).collect();
        let artifact = PreAnalysisArtifact {
            sample_rate: 44_100,
            bpm: 124.0,
            confidence: 0.9,
            beat_positions: beats.iter().map(|b| b.round() as usize).collect(),
            beat_positions_fractional: beats,
            downbeat_beat_indices: vec![1, 5],
            ..Default::default()
        };
        for rate in [44_100, 48_000, 96_000] {
            let grid = grid_from_artifact(&artifact.resample_to(rate));
            let marks = GridMarks::from_grid(&grid);
            let cue = marks.auto_cue_frame().unwrap().ceil() as usize;
            // Fractional rescaling can cross an integer boundary by one ULP;
            // ceil may then park one sample later, but never one beat later.
            let expected = (first * rate as f64 / 44_100.0).ceil() as usize;
            assert!(cue.abs_diff(expected) <= 1);
            assert_eq!(marks.bar_start(grid.beats[1]), Some(grid.beats[1]));
        }
    }

    #[test]
    fn device_reload_keeps_non_library_sources_and_pending_decodes() {
        let mut deck = DeckUi::new();
        let first = PathBuf::from("unregistered.wav");
        deck.source_path = Some(first.clone());
        deck.deck
            .shared
            .total_frames
            .store(96_000, Ordering::Relaxed);
        deck.deck.shared.playhead.store(24_000, Ordering::Relaxed);
        deck.deck.shared.set_transport(Transport::Playing);
        assert_eq!(deck.prepare_device_reload(), Some((first, None)));
        assert_eq!(deck.pending_seek_frac, Some(0.25));
        assert_eq!(deck.deck.shared.transport(), Transport::Stopped);

        let next = PathBuf::from("next.wav");
        let (_tx, receiver) = mpsc::channel();
        deck.decode_rx = Some(PendingDecode {
            path: next.clone(),
            track_id: Some(42),
            receiver,
        });
        // A new track has no inherited seek. A second device change must keep
        // its request, rather than reload the previously installed track.
        deck.pending_seek_frac = None;
        assert_eq!(deck.prepare_device_reload(), Some((next.clone(), Some(42))));
        assert_eq!(deck.pending_seek_frac, None);
        // Repeated device changes during a position-restoring reload keep it.
        deck.pending_seek_frac = Some(0.75);
        assert_eq!(deck.prepare_device_reload(), Some((next, Some(42))));
        assert_eq!(deck.pending_seek_frac, Some(0.75));
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
    fn tempo_limits_follow_pitch_intent_and_preserve_stop() {
        assert_eq!(legal_tempo(0.0, true), 0.0);
        assert_eq!(legal_tempo(0.0, false), 0.0);
        assert_eq!(legal_tempo(0.01, true), 1.0 / 32.0);
        assert_eq!(legal_tempo(0.1, false), 0.25);
        assert_eq!(legal_tempo(2.5, true), 2.5);
        assert_eq!(legal_tempo(5.0, true), 4.0);
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
