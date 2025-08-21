use std::sync::Arc;
use halo_core::{ConsoleEvent, CueList, PlaybackState, RhythmState, Show, TimeCode};
use halo_fixtures::Fixture;

pub struct ConsoleState {
    pub fixtures: Arc<Vec<Fixture>>,
    pub cue_lists: Arc<Vec<CueList>>,
    pub playback_state: PlaybackState,
    pub rhythm_state: RhythmState,
    pub timecode: Option<TimeCode>,
    pub bpm: f64,
    pub show: Option<Arc<Show>>,
    pub link_enabled: bool,
    pub link_peers: u64,
    pub dirty_flags: DirtyFlags,
}

#[derive(Default)]
pub struct DirtyFlags {
    pub fixtures_changed: bool,
    pub cues_changed: bool,
    pub show_changed: bool,
}

impl Default for ConsoleState {
    fn default() -> Self {
        Self {
            fixtures: Arc::new(Vec::new()),
            cue_lists: Arc::new(Vec::new()),
            playback_state: PlaybackState::Stopped,
            rhythm_state: RhythmState {
                beat_phase: 0.0,
                bar_phase: 0.0,
                phrase_phase: 0.0,
                beats_per_bar: 4,
                bars_per_phrase: 4,
                last_tap_time: None,
                tap_count: 0,
            },
            timecode: None,
            bpm: 120.0,
            show: None,
            link_enabled: false,
            link_peers: 0,
            dirty_flags: DirtyFlags::default(),
        }
    }
}

impl ConsoleState {
    pub fn update(&mut self, update: ConsoleEvent) {
        match update {
            ConsoleEvent::FixturesUpdated { fixtures } => self.fixtures = Arc::new(fixtures),
            ConsoleEvent::CueListsUpdated { cue_lists } => self.cue_lists = Arc::new(cue_lists),
            ConsoleEvent::PlaybackStateChanged { state } => {
                self.playback_state = state
            }
            ConsoleEvent::RhythmStateUpdated { state } => self.rhythm_state = state,
            ConsoleEvent::TimecodeUpdated { timecode } => self.timecode = Some(timecode),
            ConsoleEvent::BpmChanged { bpm } => self.bpm = bpm,
            ConsoleEvent::ShowLoaded { show } => self.show = Some(Arc::new(show)),
            ConsoleEvent::LinkStateChanged { enabled, num_peers } => {
                self.link_enabled = enabled;
                self.link_peers = num_peers;
            }
            _ => {} // Ignore other events for now
        }
    }
}
