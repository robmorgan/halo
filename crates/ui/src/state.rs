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
            rhythm_state: RhythmState::default(),
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
            ConsoleEvent::FixturesUpdated(fixtures) => self.fixtures = fixtures,
            ConsoleEvent::CueListsUpdated(cue_lists) => self.cue_lists = cue_lists,
            ConsoleEvent::PlaybackStateChanged(playback_state) => {
                self.playback_state = playback_state
            }
            ConsoleEvent::RhythmStateUpdated(rhythm_state) => self.rhythm_state = rhythm_state,
            ConsoleEvent::TimecodeUpdated(timecode) => self.timecode = timecode,
            ConsoleEvent::TempoUpdated(bpm) => self.bpm = bpm,
            ConsoleEvent::ShowLoaded(show) => self.show = Some(Arc::new(show)),
            ConsoleEvent::LinkStateChanged(link_enabled, link_peers) => {
                self.link_enabled = link_enabled;
                self.link_peers = link_peers;
            }
        }
    }
}
