use halo_core::{ConsoleCommand, CueList, PlaybackState};
use halo_fixtures::Fixture;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct ConsoleState {
    pub fixtures: HashMap<String, Fixture>,
    pub cue_lists: Vec<CueList>,
    pub playback_state: PlaybackState,
    pub bpm: f64,
    pub current_time: SystemTime,
    pub link_peers: u32,
    pub link_quantum: f64,
    pub link_tempo: f64,
    pub link_start_stop_sync: bool,
}

impl Default for ConsoleState {
    fn default() -> Self {
        Self {
            fixtures: HashMap::new(),
            cue_lists: Vec::new(),
            playback_state: PlaybackState::Stopped,
            bpm: 120.0,
            current_time: SystemTime::now(),
            link_peers: 0,
            link_quantum: 4.0,
            link_tempo: 120.0,
            link_start_stop_sync: false,
        }
    }
}

impl ConsoleState {
    pub fn update(&mut self, event: halo_core::ConsoleEvent) {
        match event {
            halo_core::ConsoleEvent::FixturesUpdated { fixtures } => {
                self.fixtures.clear();
                for fixture in fixtures {
                    self.fixtures.insert(fixture.id.to_string(), fixture);
                }
            }
            halo_core::ConsoleEvent::CueListsUpdated { cue_lists } => {
                self.cue_lists = cue_lists;
            }
            halo_core::ConsoleEvent::PlaybackStateChanged { state } => {
                self.playback_state = state;
            }
            halo_core::ConsoleEvent::BpmChanged { bpm } => {
                self.bpm = bpm;
            }
            halo_core::ConsoleEvent::TimecodeUpdated { timecode } => {
                // Store timecode info if needed
            }
            halo_core::ConsoleEvent::LinkStateChanged { enabled, num_peers } => {
                self.link_peers = num_peers as u32;
                // Note: enabled state could be added to ConsoleState if needed
            }
            halo_core::ConsoleEvent::FixturePatched {
                fixture_id,
                fixture,
            } => {
                self.fixtures.insert(fixture_id.to_string(), fixture);
            }
            halo_core::ConsoleEvent::FixtureUnpatched { fixture_id } => {
                self.fixtures.remove(&fixture_id.to_string());
            }
            halo_core::ConsoleEvent::ShowLoaded { show } => {
                self.fixtures.clear();
                for fixture in show.fixtures {
                    self.fixtures.insert(fixture.id.to_string(), fixture);
                }
                self.cue_lists = show.cue_lists;
            }
            _ => {
                // Handle other events as needed
            }
        }
    }
}

/// Context struct that combines console state and command sender
/// This reduces parameter passing and provides a cleaner interface for UI components
pub struct ConsoleContext<'a> {
    pub state: &'a ConsoleState,
    pub console_tx: &'a mpsc::UnboundedSender<ConsoleCommand>,
}

impl<'a> ConsoleContext<'a> {
    pub fn new(
        state: &'a ConsoleState,
        console_tx: &'a mpsc::UnboundedSender<ConsoleCommand>,
    ) -> Self {
        Self { state, console_tx }
    }

    /// Convenience method to send a command
    pub fn send_command(&self, command: ConsoleCommand) {
        let _ = self.console_tx.send(command);
    }
}
