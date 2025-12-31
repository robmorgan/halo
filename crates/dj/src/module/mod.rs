//! DJ module implementation.

mod audio_engine;
mod deck_player;
mod time_stretcher;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
pub use audio_engine::{
    default_device_name, list_audio_devices, AudioDeviceInfo, AudioEngineConfig, DjAudioEngine,
};
pub use deck_player::{BeatEvent, DeckPlayer, PlayerState};
use halo_core::{AsyncModule, MidiMessage, ModuleEvent, ModuleId, ModuleMessage};
use parking_lot::RwLock;
use tokio::sync::mpsc;

use crate::deck::{Deck, DeckId, DeckState};
use crate::library::database::LibraryDatabase;
use crate::library::{
    analyze_file_streaming, AnalysisConfig, BeatGrid, HotCue, MasterTempoMode, TempoRange, Track,
    TrackId, TrackWaveform,
};
use crate::midi::z1_mapping::Z1Mapping;

/// Commands for the DJ module.
#[derive(Debug, Clone)]
pub enum DjCommand {
    // Library commands
    /// Import all audio files from a folder.
    ImportFolder { path: PathBuf },
    /// Analyze a track for BPM/beat grid.
    AnalyzeTrack { track_id: TrackId },
    /// Search the library.
    SearchLibrary { query: String },
    /// Get all tracks in the library.
    GetAllTracks,

    // Deck loading commands
    /// Load a track onto a deck.
    LoadTrack { deck: DeckId, track_id: TrackId },
    /// Eject the track from a deck.
    EjectTrack { deck: DeckId },

    // Track navigation commands
    /// Go to previous track (or start of current track if not at start).
    PreviousTrack { deck: DeckId },
    /// Go to next track in the library.
    NextTrack { deck: DeckId },

    // Playback commands
    /// Start playback.
    Play { deck: DeckId },
    /// Pause playback.
    Pause { deck: DeckId },
    /// Toggle play/pause.
    PlayPause { deck: DeckId },
    /// Stop playback (return to start).
    Stop { deck: DeckId },

    // Cueing commands
    /// Set the cue point at current position.
    SetCue { deck: DeckId },
    /// Jump to cue point and start playing.
    CuePlay { deck: DeckId },
    /// Preview from cue point while button is held.
    CuePreview { deck: DeckId, pressed: bool },

    // Hot cue commands
    /// Set a hot cue at current position.
    SetHotCue { deck: DeckId, slot: u8 },
    /// Jump to a hot cue.
    JumpToHotCue { deck: DeckId, slot: u8 },
    /// Clear a hot cue.
    ClearHotCue { deck: DeckId, slot: u8 },

    // Tempo commands
    /// Set the pitch fader position (-1.0 to 1.0).
    SetPitch { deck: DeckId, percent: f64 },
    /// Set the tempo range.
    SetTempoRange { deck: DeckId, range: TempoRange },
    /// Nudge tempo temporarily.
    NudgeTempo {
        deck: DeckId,
        direction: NudgeDirection,
    },

    // Sync commands
    /// Set this deck as the tempo master.
    SetMaster { deck: DeckId },
    /// Toggle sync mode for this deck.
    ToggleSync { deck: DeckId },
    /// Sync this deck to the other deck.
    SyncToDeck { deck: DeckId },

    // Seek commands
    /// Seek to a position in seconds.
    Seek { deck: DeckId, position_seconds: f64 },
    /// Seek by a number of beats.
    SeekBeats { deck: DeckId, beats: i32 },

    // Master Tempo commands
    /// Toggle Master Tempo (key lock) mode.
    ToggleMasterTempo { deck: DeckId },

    // Configuration commands
    /// Set the output channels for a deck.
    SetOutputChannels { deck: DeckId, channels: (u16, u16) },
    /// Set the audio device.
    SetAudioDevice { device_name: String },
}

/// Direction for tempo nudge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NudgeDirection {
    Forward,
    Backward,
}

/// Events emitted by the DJ module.
#[derive(Debug, Clone)]
pub enum DjEvent {
    // State updates
    /// Deck state has changed.
    DeckStateChanged { deck: DeckId, state: Deck },
    /// A track was loaded onto a deck.
    TrackLoaded { deck: DeckId, track: Track },
    /// A track was ejected from a deck.
    TrackEjected { deck: DeckId },

    // Playback updates
    /// Playback position updated.
    PositionUpdated {
        deck: DeckId,
        position_seconds: f64,
        position_beats: f64,
    },
    /// A beat was triggered.
    BeatTriggered {
        deck: DeckId,
        beat_number: u64,
        is_downbeat: bool,
    },

    // Tempo updates
    /// Tempo changed on a deck.
    TempoChanged { deck: DeckId, bpm: f64 },
    /// Master deck changed.
    MasterChanged { deck: Option<DeckId> },

    // Library updates
    /// Library was updated.
    LibraryUpdated { track_count: usize },
    /// Track analysis progress.
    AnalysisProgress { track_id: TrackId, progress: f32 },
    /// Track analysis completed.
    AnalysisComplete {
        track_id: TrackId,
        beat_grid: BeatGrid,
    },
    /// Search results ready.
    SearchResults { tracks: Vec<Track> },
    /// All tracks retrieved.
    AllTracks { tracks: Vec<Track> },

    // Waveform data
    /// Waveform data available for a track.
    WaveformReady {
        track_id: TrackId,
        waveform: TrackWaveform,
    },

    // Hot cues
    /// Hot cue was set.
    HotCueSet { deck: DeckId, slot: u8, cue: HotCue },
    /// Hot cue was cleared.
    HotCueCleared { deck: DeckId, slot: u8 },

    // Errors
    /// An error occurred.
    Error { message: String },
}

/// DJ module state and audio engine.
pub struct DjModule {
    /// Deck A state.
    deck_a: Arc<RwLock<Deck>>,
    /// Deck B state.
    deck_b: Arc<RwLock<Deck>>,
    /// Current master deck.
    master_deck: Option<DeckId>,
    /// Library database path.
    library_path: PathBuf,
    /// Audio engine configuration.
    audio_config: AudioEngineConfig,
    /// Audio engine (created during initialization).
    audio_engine: Option<DjAudioEngine>,
    /// Library database (created during initialization, wrapped for thread safety).
    database: Option<Arc<Mutex<LibraryDatabase>>>,
}

impl DjModule {
    /// Create a new DJ module.
    pub fn new() -> Self {
        // Default library path: ~/.halo/library.db
        let library_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".halo")
            .join("library.db");

        Self {
            deck_a: Arc::new(RwLock::new(Deck::new(DeckId::A))),
            deck_b: Arc::new(RwLock::new(Deck::new(DeckId::B))),
            master_deck: None,
            library_path,
            audio_config: AudioEngineConfig::default(),
            audio_engine: None,
            database: None,
        }
    }

    /// Create a new DJ module with a specific library path.
    pub fn with_library_path(library_path: PathBuf) -> Self {
        Self {
            deck_a: Arc::new(RwLock::new(Deck::new(DeckId::A))),
            deck_b: Arc::new(RwLock::new(Deck::new(DeckId::B))),
            master_deck: None,
            library_path,
            audio_config: AudioEngineConfig::default(),
            audio_engine: None,
            database: None,
        }
    }

    /// Set the audio device name.
    pub fn with_audio_device(mut self, device_name: String) -> Self {
        self.audio_config.device_name = device_name;
        self
    }

    /// Set the audio engine configuration.
    pub fn with_audio_config(mut self, config: AudioEngineConfig) -> Self {
        self.audio_config = config;
        self
    }

    /// Get the audio engine (if initialized).
    pub fn audio_engine(&self) -> Option<&DjAudioEngine> {
        self.audio_engine.as_ref()
    }

    /// Get the audio engine mutably (if initialized).
    pub fn audio_engine_mut(&mut self) -> Option<&mut DjAudioEngine> {
        self.audio_engine.as_mut()
    }

    /// Get the database (if initialized).
    pub fn database(&self) -> Option<Arc<Mutex<LibraryDatabase>>> {
        self.database.clone()
    }

    /// Get a reference to a deck.
    pub fn deck(&self, id: DeckId) -> &Arc<RwLock<Deck>> {
        match id {
            DeckId::A => &self.deck_a,
            DeckId::B => &self.deck_b,
        }
    }

    /// Get the current master deck.
    pub fn master_deck(&self) -> Option<DeckId> {
        self.master_deck
    }

    /// Set the master deck.
    pub fn set_master_deck(&mut self, deck: Option<DeckId>) {
        // Clear master flag on old deck
        if let Some(old_master) = self.master_deck {
            self.deck(old_master).write().is_master = false;
        }

        // Set master flag on new deck
        if let Some(new_master) = deck {
            self.deck(new_master).write().is_master = true;
        }

        self.master_deck = deck;
    }

    /// Translate a console command to an internal DJ command.
    fn translate_console_command(&self, cmd: halo_core::ConsoleCommand) -> Option<DjCommand> {
        use halo_core::ConsoleCommand;
        match cmd {
            ConsoleCommand::DjImportFolder { path } => Some(DjCommand::ImportFolder { path }),
            ConsoleCommand::DjLoadTrack { deck, track_id } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::LoadTrack {
                    deck: deck_id,
                    track_id: TrackId(track_id),
                })
            }
            ConsoleCommand::DjPlay { deck } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::Play { deck: deck_id })
            }
            ConsoleCommand::DjPause { deck } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::Pause { deck: deck_id })
            }
            ConsoleCommand::DjStop { deck } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::Stop { deck: deck_id })
            }
            ConsoleCommand::DjSetCue { deck } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::SetCue { deck: deck_id })
            }
            ConsoleCommand::DjJumpToCue { deck } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::CuePlay { deck: deck_id })
            }
            ConsoleCommand::DjCuePreview { deck, pressed } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::CuePreview {
                    deck: deck_id,
                    pressed,
                })
            }
            ConsoleCommand::DjSetHotCue { deck, slot } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::SetHotCue {
                    deck: deck_id,
                    slot,
                })
            }
            ConsoleCommand::DjJumpToHotCue { deck, slot } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::JumpToHotCue {
                    deck: deck_id,
                    slot,
                })
            }
            ConsoleCommand::DjSetPitch { deck, percent } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::SetPitch {
                    deck: deck_id,
                    percent,
                })
            }
            ConsoleCommand::DjToggleSync { deck } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::ToggleSync { deck: deck_id })
            }
            ConsoleCommand::DjSetMaster { deck } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::SetMaster { deck: deck_id })
            }
            ConsoleCommand::DjSeek {
                deck,
                position_seconds,
            } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::Seek {
                    deck: deck_id,
                    position_seconds,
                })
            }
            ConsoleCommand::DjPreviousTrack { deck } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::PreviousTrack { deck: deck_id })
            }
            ConsoleCommand::DjNextTrack { deck } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::NextTrack { deck: deck_id })
            }
            ConsoleCommand::DjQueryLibrary => Some(DjCommand::GetAllTracks),
            ConsoleCommand::DjToggleMasterTempo { deck } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                Some(DjCommand::ToggleMasterTempo { deck: deck_id })
            }
            ConsoleCommand::DjSetTempoRange { deck, range } => {
                let deck_id = if deck == 0 { DeckId::A } else { DeckId::B };
                let tempo_range = match range {
                    0 => TempoRange::Range6,
                    1 => TempoRange::Range10,
                    2 => TempoRange::Range16,
                    3 => TempoRange::Range25,
                    _ => TempoRange::Wide,
                };
                Some(DjCommand::SetTempoRange {
                    deck: deck_id,
                    range: tempo_range,
                })
            }
            _ => None,
        }
    }

    /// Handle a DJ command.
    fn handle_command(&mut self, command: DjCommand) {
        match command {
            DjCommand::Play { deck } => {
                // Update deck state
                {
                    let mut d = self.deck(deck).write();
                    if d.state.has_track() {
                        d.state = DeckState::Playing;
                    }
                }
                // Control audio player
                if let Some(engine) = &self.audio_engine {
                    engine.deck_player(deck).write().play();
                }
                log::info!("Deck {} playing", deck);
            }
            DjCommand::Pause { deck } => {
                // Update deck state
                {
                    let mut d = self.deck(deck).write();
                    if d.state == DeckState::Playing {
                        d.state = DeckState::Paused;
                    }
                }
                // Control audio player
                if let Some(engine) = &self.audio_engine {
                    engine.deck_player(deck).write().pause();
                }
                log::info!("Deck {} paused", deck);
            }
            DjCommand::PlayPause { deck } => {
                let should_play = {
                    let d = self.deck(deck).read();
                    matches!(d.state, DeckState::Paused | DeckState::Stopped)
                };

                if should_play {
                    self.handle_command(DjCommand::Play { deck });
                } else {
                    self.handle_command(DjCommand::Pause { deck });
                }
            }
            DjCommand::Stop { deck } => {
                // Update deck state
                {
                    let mut d = self.deck(deck).write();
                    if d.state.has_track() {
                        d.state = DeckState::Stopped;
                        d.position_seconds = 0.0;
                        d.position_beats = 0.0;
                    }
                }
                // Control audio player
                if let Some(engine) = &self.audio_engine {
                    engine.deck_player(deck).write().stop();
                }
                log::info!("Deck {} stopped", deck);
            }
            DjCommand::SetCue { deck } => {
                let position = if let Some(engine) = &self.audio_engine {
                    let player = engine.deck_player(deck).read();
                    player.position_seconds()
                } else {
                    self.deck(deck).read().position_seconds
                };

                // Update deck state
                self.deck(deck).write().cue_point = Some(position);

                // Set cue in player
                if let Some(engine) = &self.audio_engine {
                    engine.deck_player(deck).write().set_cue_at(position);
                }
                log::info!("Deck {} cue set at {:.2}s", deck, position);
            }
            DjCommand::CuePreview { deck, pressed } => {
                let mut d = self.deck(deck).write();
                if pressed {
                    if let Some(cue_point) = d.cue_point {
                        d.cue_preview_start = Some(d.position_seconds);
                        d.position_seconds = cue_point;
                        d.state = DeckState::Cueing;

                        // Seek to cue and start playing
                        if let Some(engine) = &self.audio_engine {
                            let mut player = engine.deck_player(deck).write();
                            player.seek(cue_point);
                            player.play();
                        }
                        log::info!("Deck {} cue preview started", deck);
                    }
                } else if d.state == DeckState::Cueing {
                    if let Some(cue_point) = d.cue_point {
                        d.position_seconds = cue_point;

                        // Pause and seek back to cue
                        if let Some(engine) = &self.audio_engine {
                            let mut player = engine.deck_player(deck).write();
                            player.pause();
                            player.seek(cue_point);
                        }
                    }
                    d.state = DeckState::Paused;
                    d.cue_preview_start = None;
                    log::info!("Deck {} cue preview ended", deck);
                }
            }
            DjCommand::SetPitch { deck, percent } => {
                let adjusted_bpm = {
                    let mut d = self.deck(deck).write();
                    d.pitch_percent = percent.clamp(-1.0, 1.0);
                    d.update_adjusted_bpm();
                    d.adjusted_bpm
                };

                // Update playback rate based on pitch
                if let Some(engine) = &self.audio_engine {
                    let playback_rate = adjusted_bpm / self.deck(deck).read().original_bpm;
                    engine
                        .deck_player(deck)
                        .write()
                        .set_playback_rate(playback_rate);
                }
                log::debug!(
                    "Deck {} pitch set to {:.1}% (BPM: {:.2})",
                    deck,
                    percent * 100.0,
                    adjusted_bpm
                );
            }
            DjCommand::SetTempoRange { deck, range } => {
                let mut d = self.deck(deck).write();
                d.tempo_range = range;
                d.update_adjusted_bpm();
                log::info!("Deck {} tempo range set to {:?}", deck, range);
            }
            DjCommand::SetMaster { deck } => {
                self.set_master_deck(Some(deck));
                log::info!("Deck {} set as master", deck);
            }
            DjCommand::ToggleSync { deck } => {
                let mut d = self.deck(deck).write();
                d.sync_enabled = !d.sync_enabled;
                log::info!(
                    "Deck {} sync {}",
                    deck,
                    if d.sync_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
            }
            DjCommand::SetHotCue { deck, slot } => {
                if slot < 4 {
                    let position = if let Some(engine) = &self.audio_engine {
                        engine.deck_player(deck).read().position_seconds()
                    } else {
                        self.deck(deck).read().position_seconds
                    };

                    self.deck(deck).write().set_hot_cue(slot, position);
                    log::info!("Deck {} hot cue {} set at {:.2}s", deck, slot, position);
                }
            }
            DjCommand::JumpToHotCue { deck, slot } => {
                if slot < 4 {
                    let position = {
                        let d = self.deck(deck).read();
                        d.hot_cues[slot as usize]
                            .as_ref()
                            .map(|cue| cue.position_seconds)
                    };

                    if let Some(pos) = position {
                        self.deck(deck).write().position_seconds = pos;
                        self.deck(deck).write().update_beat_position();

                        if let Some(engine) = &self.audio_engine {
                            engine.deck_player(deck).write().seek(pos);
                        }
                        log::info!("Deck {} jumped to hot cue {}", deck, slot);
                    }
                }
            }
            DjCommand::ClearHotCue { deck, slot } => {
                if slot < 4 {
                    self.deck(deck).write().clear_hot_cue(slot);
                    log::info!("Deck {} hot cue {} cleared", deck, slot);
                }
            }
            DjCommand::Seek {
                deck,
                position_seconds,
            } => {
                let clamped_position = {
                    let d = self.deck(deck).read();
                    if let Some(track) = &d.loaded_track {
                        position_seconds.clamp(0.0, track.duration_seconds)
                    } else {
                        return;
                    }
                };

                {
                    let mut d = self.deck(deck).write();
                    d.position_seconds = clamped_position;
                    d.update_beat_position();
                }

                if let Some(engine) = &self.audio_engine {
                    engine.deck_player(deck).write().seek(clamped_position);
                }
            }
            DjCommand::EjectTrack { deck } => {
                self.deck(deck).write().eject();
                if let Some(engine) = &self.audio_engine {
                    engine.deck_player(deck).write().eject();
                }
                log::info!("Deck {} ejected", deck);
            }
            DjCommand::LoadTrack { deck, track_id } => {
                log::info!(
                    "DJ Module: Processing LoadTrack command - deck={:?}, track_id={:?}",
                    deck,
                    track_id
                );
                self.load_track_to_deck(deck, track_id);
            }
            DjCommand::ImportFolder { path } => {
                self.import_folder(path);
            }
            DjCommand::GetAllTracks => {
                // Handled separately in run loop to send response
            }
            DjCommand::SearchLibrary { query } => {
                if let Some(db) = &self.database {
                    let db = db.lock().unwrap();
                    match db.search_tracks(&query) {
                        Ok(tracks) => {
                            log::info!("Found {} tracks matching '{}'", tracks.len(), query);
                        }
                        Err(e) => {
                            log::error!("Search failed: {}", e);
                        }
                    }
                }
            }
            DjCommand::AnalyzeTrack { track_id } => {
                log::info!("Track analysis not yet implemented for track {}", track_id);
            }
            // Handle remaining commands
            _ => {
                log::warn!("Unhandled DJ command: {:?}", command);
            }
        }
    }

    /// Load a track from the library onto a deck.
    fn load_track_to_deck(&mut self, deck: DeckId, track_id: TrackId) {
        log::info!(
            "DJ Module: load_track_to_deck called - deck={:?}, track_id={:?}",
            deck,
            track_id
        );

        let Some(db) = &self.database else {
            log::error!("Database not initialized");
            return;
        };

        // Get track and related data from database
        let (track, hot_cues, beat_grid) = {
            let db = db.lock().unwrap();

            let track = match db.get_track(track_id) {
                Ok(Some(track)) => track,
                Ok(None) => {
                    log::error!("Track {} not found in library", track_id);
                    return;
                }
                Err(e) => {
                    log::error!("Failed to load track {}: {}", track_id, e);
                    return;
                }
            };

            let hot_cues = db.get_hot_cues(track_id).unwrap_or_default();
            let beat_grid = db.get_beat_grid(track_id).ok().flatten();

            (track, hot_cues, beat_grid)
        };

        // Load track into deck state
        {
            let mut d = self.deck(deck).write();
            d.loaded_track = Some(track.clone());
            d.state = DeckState::Stopped;
            d.position_seconds = 0.0;
            d.position_beats = 0.0;
            d.original_bpm = track.bpm.unwrap_or(120.0);
            d.adjusted_bpm = d.original_bpm;

            // Load hot cues
            for cue in hot_cues {
                let slot = cue.slot;
                if slot < 4 {
                    d.hot_cues[slot as usize] = Some(cue);
                }
            }

            // Load beat grid
            d.beat_grid = beat_grid;
        }

        // Load audio file into player
        if let Some(engine) = &self.audio_engine {
            if let Err(e) = engine.deck_player(deck).write().load(&track.file_path) {
                log::error!("Failed to load audio file: {}", e);
                return;
            }
        }

        log::info!(
            "Deck {} loaded: {} - {}",
            deck,
            track.artist.as_deref().unwrap_or("Unknown"),
            track.title
        );
    }

    /// Get all tracks from the library for UI display.
    fn get_all_tracks_for_ui(&self) -> Option<Vec<halo_core::DjTrackInfo>> {
        let db = self.database.as_ref()?;
        let db = db.lock().unwrap();

        match db.get_all_tracks() {
            Ok(tracks) => {
                let track_infos: Vec<halo_core::DjTrackInfo> = tracks
                    .into_iter()
                    .map(|t| halo_core::DjTrackInfo {
                        id: t.id.0,
                        title: t.title,
                        artist: t.artist,
                        duration_seconds: t.duration_seconds,
                        bpm: t.bpm,
                    })
                    .collect();
                log::info!("Returning {} tracks to UI", track_infos.len());
                Some(track_infos)
            }
            Err(e) => {
                log::error!("Failed to get tracks: {}", e);
                None
            }
        }
    }

    /// Import all audio files from a folder into the library with BPM analysis.
    fn import_folder(&mut self, path: PathBuf) {
        use crate::library::import::import_and_analyze_directory;

        let Some(db) = &self.database else {
            log::error!("Database not initialized, cannot import folder");
            return;
        };

        log::info!("Importing and analyzing folder: {:?}", path);

        let db_guard = db.lock().unwrap();

        // Import all tracks from the directory (recursively) with analysis enabled
        let results = import_and_analyze_directory(&path, &db_guard, true, true);

        let mut imported_count = 0;
        let mut error_count = 0;

        for result in results {
            match result {
                Ok(import_result) => {
                    let bpm_info = import_result
                        .track
                        .bpm
                        .map(|b| format!(" (BPM: {:.1})", b))
                        .unwrap_or_default();
                    log::debug!(
                        "Imported: {} - {}{}",
                        import_result.track.artist.as_deref().unwrap_or("Unknown"),
                        import_result.track.title,
                        bpm_info
                    );
                    imported_count += 1;
                }
                Err(e) => {
                    log::warn!("Failed to import/analyze file: {}", e);
                    error_count += 1;
                }
            }
        }

        log::info!(
            "Import complete: {} imported with analysis, {} errors",
            imported_count,
            error_count
        );
    }
}

impl Default for DjModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AsyncModule for DjModule {
    fn id(&self) -> ModuleId {
        ModuleId::Dj
    }

    async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Initializing DJ module");
        log::info!("Library path: {:?}", self.library_path);
        log::info!("Audio device: {}", self.audio_config.device_name);

        // Ensure library directory exists
        if let Some(parent) = self.library_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Initialize database
        let db = LibraryDatabase::open(&self.library_path)?;
        self.database = Some(Arc::new(Mutex::new(db)));
        log::info!("Library database opened");

        // Initialize audio engine
        let mut engine = DjAudioEngine::new(self.audio_config.clone());
        if let Err(e) = engine.start() {
            log::error!("Failed to start audio engine: {}", e);
            // Continue without audio engine - useful for testing
        } else {
            log::info!("Audio engine started");
        }
        self.audio_engine = Some(engine);

        log::info!("DJ module initialized");
        Ok(())
    }

    async fn run(
        &mut self,
        mut rx: mpsc::Receiver<ModuleEvent>,
        tx: mpsc::Sender<ModuleMessage>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!("DJ module running");

        // Rhythm sync update interval (roughly 30Hz for smooth phase tracking)
        let mut rhythm_interval = tokio::time::interval(std::time::Duration::from_millis(33));
        // Track last beat number to detect beat triggers
        let mut last_beat_a: Option<u64> = None;
        let mut last_beat_b: Option<u64> = None;

        loop {
            tokio::select! {
                Some(event) = rx.recv() => {
                    match event {
                        ModuleEvent::Shutdown => {
                            log::info!("DJ module received shutdown");
                            break;
                        }
                        // Handle MIDI input via Z1 mapping
                        ModuleEvent::MidiInput(midi_msg) => {
                            log::debug!("DJ module received MIDI: {:?}", midi_msg);

                            // Translate MIDI to DJ command via Z1 mapping
                            let command = match midi_msg {
                                MidiMessage::NoteOn(note, velocity) => {
                                    Z1Mapping::translate_note_on(note, velocity)
                                }
                                MidiMessage::NoteOff(note) => {
                                    Z1Mapping::translate_note_off(note)
                                }
                                MidiMessage::ControlChange(cc, value) => {
                                    Z1Mapping::translate_cc(cc, value)
                                }
                                MidiMessage::Clock => {
                                    // MIDI clock messages are handled by rhythm sync
                                    None
                                }
                            };

                            // Execute the command if one was generated
                            if let Some(cmd) = command {
                                log::debug!("Executing DJ command from MIDI: {:?}", cmd);
                                self.handle_command(cmd);
                            }
                        }
                        // Handle DJ commands from console
                        ModuleEvent::DjCommand(console_cmd) => {
                            eprintln!("DEBUG: DJ module received command: {:?}", console_cmd);
                            log::debug!("DJ module received command: {:?}", console_cmd);
                            // Translate ConsoleCommand to internal DjCommand
                            let translated = self.translate_console_command(console_cmd);
                            eprintln!("DEBUG: Translated command: {:?}", translated);
                            if let Some(cmd) = translated {
                                // Special handling for commands that need to send responses
                                match cmd {
                                    DjCommand::GetAllTracks => {
                                        if let Some(tracks) = self.get_all_tracks_for_ui() {
                                            let _ = tx.send(ModuleMessage::Event(
                                                ModuleEvent::DjLibraryTracks(tracks)
                                            )).await;
                                        }
                                    }
                                    DjCommand::LoadTrack { deck, track_id } => {
                                        let deck_num = if deck == DeckId::A { 0 } else { 1 };
                                        let tid = track_id.0;
                                        eprintln!("DEBUG: Calling handle_command for LoadTrack");
                                        self.handle_command(DjCommand::LoadTrack { deck, track_id });
                                        // Get track info (drop lock before await)
                                        let track_info = {
                                            let deck_state = self.deck(deck).read();
                                            deck_state.loaded_track.as_ref().map(|t| {
                                                (t.title.clone(), t.artist.clone(), t.duration_seconds, t.bpm)
                                            })
                                        };
                                        // Send deck loaded event
                                        if let Some((title, artist, duration_seconds, bpm)) = track_info.clone() {
                                            let _ = tx.send(ModuleMessage::Event(
                                                ModuleEvent::DjDeckLoaded {
                                                    deck: deck_num,
                                                    track_id: tid,
                                                    title,
                                                    artist,
                                                    duration_seconds,
                                                    bpm,
                                                }
                                            )).await;
                                            eprintln!("DEBUG: Sent DjDeckLoaded event for deck {}", deck_num);
                                        }
                                        // Check if waveform exists in database
                                        let existing_waveform = if let Some(db) = &self.database {
                                            if let Ok(db_guard) = db.lock() {
                                                db_guard.get_waveform(track_id).ok().flatten()
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        };

                                        if let Some(waveform) = existing_waveform {
                                            // Waveform exists - send immediately
                                            let sample_count = waveform.sample_count;
                                            let _ = tx.send(ModuleMessage::Event(
                                                ModuleEvent::DjWaveformLoaded {
                                                    deck: deck_num,
                                                    samples: waveform.samples,
                                                    frequency_bands: waveform.frequency_bands.map(|bands| {
                                                        bands.iter().map(|b| b.as_tuple()).collect()
                                                    }),
                                                    duration_seconds: waveform.duration_seconds,
                                                }
                                            )).await;
                                            eprintln!("DEBUG: Sent cached DjWaveformLoaded event for deck {} ({} samples)", deck_num, sample_count);

                                            // Load beat grid from database and auto-cue to first beat
                                            let beat_grid = if let Some(db) = &self.database {
                                                if let Ok(db_guard) = db.lock() {
                                                    db_guard.get_beat_grid(track_id).ok().flatten()
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            };

                                            if let Some(beat_grid) = beat_grid {
                                                // Store beat grid in deck state
                                                {
                                                    let mut deck_state = self.deck(deck).write();
                                                    deck_state.beat_grid = Some(beat_grid.clone());
                                                }

                                                // Auto-cue to first beat
                                                let first_beat_seconds = beat_grid.first_beat_offset_ms / 1000.0;
                                                {
                                                    let mut deck_state = self.deck(deck).write();
                                                    deck_state.cue_point = Some(first_beat_seconds);
                                                    deck_state.position_seconds = first_beat_seconds;
                                                }

                                                // Seek audio engine to first beat
                                                if let Some(engine) = &self.audio_engine {
                                                    engine.deck_player(deck).write().seek(first_beat_seconds);
                                                }

                                                // Send cue point event
                                                let _ = tx.send(ModuleMessage::Event(
                                                    ModuleEvent::DjCuePointSet {
                                                        deck: deck_num,
                                                        position_seconds: first_beat_seconds,
                                                    }
                                                )).await;

                                                // Send beat grid loaded event
                                                let _ = tx.send(ModuleMessage::Event(
                                                    ModuleEvent::DjBeatGridLoaded {
                                                        deck: deck_num,
                                                        beat_positions: beat_grid.beat_positions.clone(),
                                                        first_beat_offset: first_beat_seconds,
                                                        bpm: beat_grid.bpm,
                                                    }
                                                )).await;
                                                eprintln!("DEBUG: Sent DjBeatGridLoaded event for deck {} ({} beats)", deck_num, beat_grid.beat_positions.len());
                                            }
                                        } else {
                                            // No waveform - spawn background analysis task
                                            let file_path = {
                                                let deck_state = self.deck(deck).read();
                                                deck_state.loaded_track.as_ref().map(|t| t.file_path.clone())
                                            };

                                            if let Some(path) = file_path {
                                                eprintln!("DEBUG: Spawning background analysis for: {}", path);
                                                let tx_clone = tx.clone();
                                                let db_clone = self.database.clone();
                                                let deck_arc = self.deck(deck).clone();

                                                tokio::spawn(async move {
                                                    // Create channel for progress updates from blocking analysis
                                                    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<(Vec<f32>, f32)>();

                                                    // Spawn blocking analysis in a separate thread
                                                    let analysis_handle = {
                                                        let progress_tx = progress_tx.clone();
                                                        let path = path.clone();
                                                        tokio::task::spawn_blocking(move || {
                                                            let config = AnalysisConfig::default();
                                                            analyze_file_streaming(
                                                                &path,
                                                                track_id,
                                                                &config,
                                                                100, // Send progress every 100 samples (10 updates total)
                                                                |samples, progress| {
                                                                    let _ = progress_tx.send((samples, progress));
                                                                },
                                                            )
                                                        })
                                                    };

                                                    // Drop our copy of progress_tx so channel closes when analysis completes
                                                    drop(progress_tx);

                                                    // Forward progress updates as they arrive
                                                    while let Some((samples, progress)) = progress_rx.recv().await {
                                                        let _ = tx_clone.send(ModuleMessage::Event(
                                                            ModuleEvent::DjWaveformProgress {
                                                                deck: deck_num,
                                                                samples,
                                                                frequency_bands: None, // Legacy analysis without color data
                                                                progress,
                                                            }
                                                        )).await;
                                                    }

                                                    // Wait for analysis to complete
                                                    match analysis_handle.await {
                                                        Ok(Ok(result)) => {
                                                            // Save to database
                                                            if let Some(db) = db_clone {
                                                                if let Ok(db_guard) = db.lock() {
                                                                    let _ = db_guard.save_waveform(&result.waveform);
                                                                    let _ = db_guard.save_beat_grid(&result.beat_grid);
                                                                    eprintln!("DEBUG: Saved analysis results to database");
                                                                }
                                                            }

                                                            // Calculate first beat position
                                                            let first_beat_seconds = result.beat_grid.first_beat_offset_ms / 1000.0;
                                                            let beat_positions = result.beat_grid.beat_positions.clone();
                                                            let bpm = result.beat_grid.bpm;

                                                            // Update deck with beat grid and auto-cue to first beat
                                                            {
                                                                let mut deck_state = deck_arc.write();
                                                                deck_state.beat_grid = Some(result.beat_grid);
                                                                deck_state.cue_point = Some(first_beat_seconds);
                                                                deck_state.position_seconds = first_beat_seconds;
                                                            }

                                                            // Send cue point event
                                                            let _ = tx_clone.send(ModuleMessage::Event(
                                                                ModuleEvent::DjCuePointSet {
                                                                    deck: deck_num,
                                                                    position_seconds: first_beat_seconds,
                                                                }
                                                            )).await;

                                                            // Send beat grid loaded event
                                                            let _ = tx_clone.send(ModuleMessage::Event(
                                                                ModuleEvent::DjBeatGridLoaded {
                                                                    deck: deck_num,
                                                                    beat_positions,
                                                                    first_beat_offset: first_beat_seconds,
                                                                    bpm,
                                                                }
                                                            )).await;
                                                            eprintln!("DEBUG: Sent DjBeatGridLoaded event for deck {} after analysis", deck_num);

                                                            // Send final waveform
                                                            let _ = tx_clone.send(ModuleMessage::Event(
                                                                ModuleEvent::DjWaveformLoaded {
                                                                    deck: deck_num,
                                                                    samples: result.waveform.samples,
                                                                    frequency_bands: result.waveform.frequency_bands.map(|bands| {
                                                                        bands.iter().map(|b| b.as_tuple()).collect()
                                                                    }),
                                                                    duration_seconds: result.waveform.duration_seconds,
                                                                }
                                                            )).await;
                                                            eprintln!("DEBUG: Background analysis complete for deck {}", deck_num);
                                                        }
                                                        Ok(Err(e)) => {
                                                            eprintln!("DEBUG: Background analysis failed: {}", e);
                                                            log::error!("Background analysis failed: {}", e);
                                                        }
                                                        Err(e) => {
                                                            eprintln!("DEBUG: Analysis task panicked: {}", e);
                                                            log::error!("Analysis task panicked: {}", e);
                                                        }
                                                    }
                                                });
                                            }
                                        }
                                    }
                                    DjCommand::Play { deck } => {
                                        let deck_num = if deck == DeckId::A { 0 } else { 1 };
                                        self.handle_command(DjCommand::Play { deck });
                                        // Get current state after play command
                                        let position = {
                                            if let Some(engine) = &self.audio_engine {
                                                engine.deck_player(deck).read().position_seconds()
                                            } else {
                                                self.deck(deck).read().position_seconds
                                            }
                                        };
                                        let adjusted_bpm = self.deck(deck).read().adjusted_bpm;
                                        let _ = tx.send(ModuleMessage::Event(
                                            ModuleEvent::DjDeckStateChanged {
                                                deck: deck_num,
                                                is_playing: true,
                                                position_seconds: position,
                                                bpm: Some(adjusted_bpm),
                                            }
                                        )).await;
                                        eprintln!("DEBUG: Sent DjDeckStateChanged (playing) for deck {}", deck_num);
                                    }
                                    DjCommand::Pause { deck } => {
                                        let deck_num = if deck == DeckId::A { 0 } else { 1 };
                                        self.handle_command(DjCommand::Pause { deck });
                                        // Get current state after pause command
                                        let position = {
                                            if let Some(engine) = &self.audio_engine {
                                                engine.deck_player(deck).read().position_seconds()
                                            } else {
                                                self.deck(deck).read().position_seconds
                                            }
                                        };
                                        let adjusted_bpm = self.deck(deck).read().adjusted_bpm;
                                        let _ = tx.send(ModuleMessage::Event(
                                            ModuleEvent::DjDeckStateChanged {
                                                deck: deck_num,
                                                is_playing: false,
                                                position_seconds: position,
                                                bpm: Some(adjusted_bpm),
                                            }
                                        )).await;
                                        eprintln!("DEBUG: Sent DjDeckStateChanged (paused) for deck {}", deck_num);
                                    }
                                    DjCommand::Stop { deck } => {
                                        let deck_num = if deck == DeckId::A { 0 } else { 1 };
                                        self.handle_command(DjCommand::Stop { deck });
                                        let adjusted_bpm = self.deck(deck).read().adjusted_bpm;
                                        let _ = tx.send(ModuleMessage::Event(
                                            ModuleEvent::DjDeckStateChanged {
                                                deck: deck_num,
                                                is_playing: false,
                                                position_seconds: 0.0,
                                                bpm: Some(adjusted_bpm),
                                            }
                                        )).await;
                                    }
                                    DjCommand::SetCue { deck } => {
                                        let deck_num = if deck == DeckId::A { 0 } else { 1 };
                                        self.handle_command(DjCommand::SetCue { deck });
                                        // Get the cue position that was just set
                                        let cue_position = self.deck(deck).read().cue_point;
                                        if let Some(position_seconds) = cue_position {
                                            let _ = tx.send(ModuleMessage::Event(
                                                ModuleEvent::DjCuePointSet {
                                                    deck: deck_num,
                                                    position_seconds,
                                                }
                                            )).await;
                                            eprintln!("DEBUG: Sent DjCuePointSet for deck {} at {:.2}s", deck_num, position_seconds);
                                        }
                                    }
                                    DjCommand::Seek { deck, position_seconds } => {
                                        let deck_num = if deck == DeckId::A { 0 } else { 1 };
                                        self.handle_command(DjCommand::Seek { deck, position_seconds });
                                        // Get current state after seek
                                        let is_playing = {
                                            if let Some(engine) = &self.audio_engine {
                                                engine.deck_player(deck).read().state() == PlayerState::Playing
                                            } else {
                                                self.deck(deck).read().state == DeckState::Playing
                                            }
                                        };
                                        let adjusted_bpm = self.deck(deck).read().adjusted_bpm;
                                        let _ = tx.send(ModuleMessage::Event(
                                            ModuleEvent::DjDeckStateChanged {
                                                deck: deck_num,
                                                is_playing,
                                                position_seconds,
                                                bpm: Some(adjusted_bpm),
                                            }
                                        )).await;
                                    }
                                    DjCommand::CuePreview { deck, pressed } => {
                                        let deck_num = if deck == DeckId::A { 0 } else { 1 };
                                        self.handle_command(DjCommand::CuePreview { deck, pressed });
                                        // Get state after cue preview action
                                        // When releasing (pressed=false), use cue_point directly since seek may not have updated player yet
                                        let (is_playing, position) = {
                                            let d = self.deck(deck).read();
                                            if pressed {
                                                // Starting preview - playing from cue point
                                                (true, d.cue_point.unwrap_or(0.0))
                                            } else {
                                                // Ending preview - stopped at cue point
                                                (false, d.cue_point.unwrap_or(d.position_seconds))
                                            }
                                        };
                                        let adjusted_bpm = self.deck(deck).read().adjusted_bpm;
                                        let _ = tx.send(ModuleMessage::Event(
                                            ModuleEvent::DjDeckStateChanged {
                                                deck: deck_num,
                                                is_playing,
                                                position_seconds: position,
                                                bpm: Some(adjusted_bpm),
                                            }
                                        )).await;
                                    }
                                    DjCommand::PreviousTrack { deck } => {
                                        let deck_num = if deck == DeckId::A { 0 } else { 1 };

                                        // Stop playback first
                                        self.handle_command(DjCommand::Pause { deck });
                                        let (pause_position, adjusted_bpm) = if let Some(engine) = &self.audio_engine {
                                            let pos = engine.deck_player(deck).read().position_seconds();
                                            let bpm = self.deck(deck).read().adjusted_bpm;
                                            (pos, bpm)
                                        } else {
                                            let deck_state = self.deck(deck).read();
                                            (deck_state.position_seconds, deck_state.adjusted_bpm)
                                        };
                                        let _ = tx.send(ModuleMessage::Event(
                                            ModuleEvent::DjDeckStateChanged {
                                                deck: deck_num,
                                                is_playing: false,
                                                position_seconds: pause_position,
                                                bpm: Some(adjusted_bpm),
                                            }
                                        )).await;

                                        // Get current position and loaded track ID
                                        let (position, current_track_id) = {
                                            let deck_state = self.deck(deck).read();
                                            let pos = if let Some(engine) = &self.audio_engine {
                                                engine.deck_player(deck).read().position_seconds()
                                            } else {
                                                deck_state.position_seconds
                                            };
                                            let track_id = deck_state.loaded_track.as_ref().map(|t| t.id);
                                            (pos, track_id)
                                        };

                                        // Threshold: if position > 0.5s, seek to start
                                        if position > 0.5 {
                                            self.handle_command(DjCommand::Seek { deck, position_seconds: 0.0 });
                                            let (is_playing, seek_bpm) = {
                                                if let Some(engine) = &self.audio_engine {
                                                    let playing = engine.deck_player(deck).read().state() == PlayerState::Playing;
                                                    let bpm = self.deck(deck).read().adjusted_bpm;
                                                    (playing, bpm)
                                                } else {
                                                    let deck_state = self.deck(deck).read();
                                                    (deck_state.state == DeckState::Playing, deck_state.adjusted_bpm)
                                                }
                                            };
                                            let _ = tx.send(ModuleMessage::Event(
                                                ModuleEvent::DjDeckStateChanged {
                                                    deck: deck_num,
                                                    is_playing,
                                                    position_seconds: 0.0,
                                                    bpm: Some(seek_bpm),
                                                }
                                            )).await;
                                            eprintln!("DEBUG: PreviousTrack: Seeked to start of deck {}", deck_num);
                                        } else if let Some(track_id) = current_track_id {
                                            // Already at start, try to load previous track
                                            let prev_track = if let Some(db) = &self.database {
                                                if let Ok(db_guard) = db.lock() {
                                                    db_guard.get_adjacent_track(track_id, false).ok().flatten()
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            };

                                            if let Some(track) = prev_track {
                                                let new_track_id = track.id;
                                                eprintln!("DEBUG: PreviousTrack: Loading previous track: {}", track.title);

                                                // Load the track using handle_command
                                                self.handle_command(DjCommand::LoadTrack { deck, track_id: new_track_id });

                                                // Get track info and send loaded event
                                                let track_info = {
                                                    let deck_state = self.deck(deck).read();
                                                    deck_state.loaded_track.as_ref().map(|t| {
                                                        (t.title.clone(), t.artist.clone(), t.duration_seconds, t.bpm)
                                                    })
                                                };
                                                if let Some((title, artist, duration_seconds, bpm)) = track_info {
                                                    let _ = tx.send(ModuleMessage::Event(
                                                        ModuleEvent::DjDeckLoaded {
                                                            deck: deck_num,
                                                            track_id: new_track_id.0,
                                                            title,
                                                            artist,
                                                            duration_seconds,
                                                            bpm,
                                                        }
                                                    )).await;
                                                }

                                                // Check for cached waveform
                                                let existing_waveform = if let Some(db) = &self.database {
                                                    if let Ok(db_guard) = db.lock() {
                                                        db_guard.get_waveform(new_track_id).ok().flatten()
                                                    } else {
                                                        None
                                                    }
                                                } else {
                                                    None
                                                };

                                                if let Some(waveform) = existing_waveform {
                                                    let _ = tx.send(ModuleMessage::Event(
                                                        ModuleEvent::DjWaveformLoaded {
                                                            deck: deck_num,
                                                            samples: waveform.samples,
                                                            frequency_bands: waveform.frequency_bands.map(|bands| {
                                                                bands.iter().map(|b| b.as_tuple()).collect()
                                                            }),
                                                            duration_seconds: waveform.duration_seconds,
                                                        }
                                                    )).await;
                                                }
                                            } else {
                                                eprintln!("DEBUG: PreviousTrack: No previous track available");
                                            }
                                        }
                                    }
                                    DjCommand::NextTrack { deck } => {
                                        let deck_num = if deck == DeckId::A { 0 } else { 1 };

                                        // Stop playback first
                                        self.handle_command(DjCommand::Pause { deck });
                                        let (pause_position, next_track_bpm) = if let Some(engine) = &self.audio_engine {
                                            let pos = engine.deck_player(deck).read().position_seconds();
                                            let bpm = self.deck(deck).read().adjusted_bpm;
                                            (pos, bpm)
                                        } else {
                                            let deck_state = self.deck(deck).read();
                                            (deck_state.position_seconds, deck_state.adjusted_bpm)
                                        };
                                        let _ = tx.send(ModuleMessage::Event(
                                            ModuleEvent::DjDeckStateChanged {
                                                deck: deck_num,
                                                is_playing: false,
                                                position_seconds: pause_position,
                                                bpm: Some(next_track_bpm),
                                            }
                                        )).await;

                                        // Get loaded track ID
                                        let current_track_id = {
                                            let deck_state = self.deck(deck).read();
                                            deck_state.loaded_track.as_ref().map(|t| t.id)
                                        };

                                        if let Some(track_id) = current_track_id {
                                            // Load next track
                                            let next_track = if let Some(db) = &self.database {
                                                if let Ok(db_guard) = db.lock() {
                                                    db_guard.get_adjacent_track(track_id, true).ok().flatten()
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            };

                                            if let Some(track) = next_track {
                                                let new_track_id = track.id;
                                                eprintln!("DEBUG: NextTrack: Loading next track: {}", track.title);

                                                // Load the track using handle_command
                                                self.handle_command(DjCommand::LoadTrack { deck, track_id: new_track_id });

                                                // Get track info and send loaded event
                                                let track_info = {
                                                    let deck_state = self.deck(deck).read();
                                                    deck_state.loaded_track.as_ref().map(|t| {
                                                        (t.title.clone(), t.artist.clone(), t.duration_seconds, t.bpm)
                                                    })
                                                };
                                                if let Some((title, artist, duration_seconds, bpm)) = track_info {
                                                    let _ = tx.send(ModuleMessage::Event(
                                                        ModuleEvent::DjDeckLoaded {
                                                            deck: deck_num,
                                                            track_id: new_track_id.0,
                                                            title,
                                                            artist,
                                                            duration_seconds,
                                                            bpm,
                                                        }
                                                    )).await;
                                                }

                                                // Check for cached waveform
                                                let existing_waveform = if let Some(db) = &self.database {
                                                    if let Ok(db_guard) = db.lock() {
                                                        db_guard.get_waveform(new_track_id).ok().flatten()
                                                    } else {
                                                        None
                                                    }
                                                } else {
                                                    None
                                                };

                                                if let Some(waveform) = existing_waveform {
                                                    let _ = tx.send(ModuleMessage::Event(
                                                        ModuleEvent::DjWaveformLoaded {
                                                            deck: deck_num,
                                                            samples: waveform.samples,
                                                            frequency_bands: waveform.frequency_bands.map(|bands| {
                                                                bands.iter().map(|b| b.as_tuple()).collect()
                                                            }),
                                                            duration_seconds: waveform.duration_seconds,
                                                        }
                                                    )).await;
                                                }
                                            } else {
                                                eprintln!("DEBUG: NextTrack: No next track available");
                                            }
                                        }
                                    }
                                    DjCommand::ToggleMasterTempo { deck } => {
                                        let deck_num = if deck == DeckId::A { 0 } else { 1 };

                                        // Toggle master tempo on the deck state
                                        let new_mode = {
                                            let mut d = self.deck(deck).write();
                                            d.master_tempo = match d.master_tempo {
                                                MasterTempoMode::Off => MasterTempoMode::On,
                                                MasterTempoMode::On => MasterTempoMode::Off,
                                            };
                                            d.master_tempo
                                        };

                                        // Toggle on the audio player
                                        if let Some(engine) = &self.audio_engine {
                                            engine.deck_player(deck).write().toggle_master_tempo();
                                        }

                                        let enabled = new_mode == MasterTempoMode::On;
                                        let _ = tx.send(ModuleMessage::Event(
                                            ModuleEvent::DjMasterTempoChanged {
                                                deck: deck_num,
                                                enabled,
                                            }
                                        )).await;
                                        log::info!("Deck {} Master Tempo {}", deck, if enabled { "ON" } else { "OFF" });
                                    }
                                    DjCommand::SetTempoRange { deck, range } => {
                                        let deck_num = if deck == DeckId::A { 0 } else { 1 };

                                        // Update deck state
                                        {
                                            let mut d = self.deck(deck).write();
                                            d.tempo_range = range;
                                            d.update_adjusted_bpm();
                                        }

                                        // Update audio player
                                        if let Some(engine) = &self.audio_engine {
                                            engine.deck_player(deck).write().set_tempo_range(range);
                                        }

                                        let range_value = match range {
                                            TempoRange::Range6 => 0,
                                            TempoRange::Range10 => 1,
                                            TempoRange::Range16 => 2,
                                            TempoRange::Range25 => 3,
                                            TempoRange::Wide => 4,
                                        };
                                        let _ = tx.send(ModuleMessage::Event(
                                            ModuleEvent::DjTempoRangeChanged {
                                                deck: deck_num,
                                                range: range_value,
                                            }
                                        )).await;
                                        log::info!("Deck {} tempo range set to {:?}", deck, range);
                                    }
                                    other => {
                                        eprintln!("DEBUG: Calling handle_command for {:?}", other);
                                        self.handle_command(other);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }

                _ = rhythm_interval.tick() => {
                    // Collect events to send (without holding locks across await)
                    let mut events_to_send = Vec::new();

                    if let Some(engine) = &self.audio_engine {
                        // Get master deck rhythm sync info
                        if let Some(master) = engine.master_deck() {
                            let player = engine.deck_player(master).read();
                            if player.state() == PlayerState::Playing {
                                if let (Some(bpm), Some(beat_phase), Some(bar_phase), Some(phrase_phase)) = (
                                    player.effective_bpm(),
                                    player.beat_phase(),
                                    player.bar_phase(),
                                    player.phrase_phase(),
                                ) {
                                    events_to_send.push(ModuleEvent::DjRhythmSync {
                                        bpm,
                                        beat_phase,
                                        bar_phase,
                                        phrase_phase,
                                    });
                                }
                            }
                        }

                        // Send position updates and check for beat triggers on Deck A
                        {
                            let player = engine.deck_player(DeckId::A).read();
                            let is_playing = player.state() == PlayerState::Playing;
                            let position = player.position_seconds();
                            let adjusted_bpm = self.deck(DeckId::A).read().adjusted_bpm;

                            // Always send position updates when playing
                            if is_playing {
                                events_to_send.push(ModuleEvent::DjDeckStateChanged {
                                    deck: 0,
                                    is_playing: true,
                                    position_seconds: position,
                                    bpm: Some(adjusted_bpm),
                                });
                            }

                            // Check for beat triggers
                            if is_playing {
                                if let Some(beat_num) = player.current_beat_number() {
                                    if last_beat_a.map_or(true, |last| beat_num > last) {
                                        let is_downbeat = player.bar_phase().map_or(false, |phase| phase < 0.25);
                                        events_to_send.push(ModuleEvent::DjBeat {
                                            deck: 0,
                                            beat_number: beat_num,
                                            is_downbeat,
                                        });
                                        last_beat_a = Some(beat_num);
                                    }
                                }
                            }
                        }

                        // Send position updates and check for beat triggers on Deck B
                        {
                            let player = engine.deck_player(DeckId::B).read();
                            let is_playing = player.state() == PlayerState::Playing;
                            let position = player.position_seconds();
                            let adjusted_bpm = self.deck(DeckId::B).read().adjusted_bpm;

                            // Always send position updates when playing
                            if is_playing {
                                events_to_send.push(ModuleEvent::DjDeckStateChanged {
                                    deck: 1,
                                    is_playing: true,
                                    position_seconds: position,
                                    bpm: Some(adjusted_bpm),
                                });
                            }

                            // Check for beat triggers
                            if is_playing {
                                if let Some(beat_num) = player.current_beat_number() {
                                    if last_beat_b.map_or(true, |last| beat_num > last) {
                                        let is_downbeat = player.bar_phase().map_or(false, |phase| phase < 0.25);
                                        events_to_send.push(ModuleEvent::DjBeat {
                                            deck: 1,
                                            beat_number: beat_num,
                                            is_downbeat,
                                        });
                                        last_beat_b = Some(beat_num);
                                    }
                                }
                            }
                        }
                    }

                    // Now send events (locks are dropped, safe to await)
                    for event in events_to_send {
                        let _ = tx.send(ModuleMessage::Event(event)).await;
                    }
                }
            }
        }

        // Send status before shutdown
        let _ = tx
            .send(ModuleMessage::Status("DJ module stopped".to_string()))
            .await;

        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Shutting down DJ module");

        // Stop audio engine
        if let Some(engine) = &mut self.audio_engine {
            engine.stop();
            log::info!("Audio engine stopped");
        }

        // Close database (implicitly done when dropped)
        self.database = None;

        log::info!("DJ module shutdown complete");
        Ok(())
    }

    fn status(&self) -> HashMap<String, String> {
        let deck_a = self.deck_a.read();
        let deck_b = self.deck_b.read();

        let mut status = HashMap::new();
        status.insert("deck_a_state".to_string(), format!("{:?}", deck_a.state));
        status.insert("deck_b_state".to_string(), format!("{:?}", deck_b.state));
        status.insert(
            "deck_a_bpm".to_string(),
            format!("{:.2}", deck_a.adjusted_bpm),
        );
        status.insert(
            "deck_b_bpm".to_string(),
            format!("{:.2}", deck_b.adjusted_bpm),
        );
        status.insert(
            "master".to_string(),
            self.master_deck
                .map(|d| format!("{}", d))
                .unwrap_or_else(|| "none".to_string()),
        );
        status.insert(
            "library_path".to_string(),
            self.library_path.display().to_string(),
        );
        status
    }
}
