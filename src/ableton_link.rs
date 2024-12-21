use rusty_link::{AblLink, SessionState};

pub struct State {
    pub link: AblLink,
    pub session_state: SessionState,
    pub running: bool,
    pub quantum: f64,
}

impl State {
    pub fn new(bpm: f64) -> Self {
        Self {
            link: AblLink::new(bpm),
            session_state: SessionState::new(),
            running: true,
            quantum: 4.,
        }
    }

    pub fn set_tempo(&mut self, new_tempo: f64) {
        self.capture_app_state();
        self.session_state
            .set_tempo(new_tempo, self.link.clock_micros());
        self.commit_app_state();
    }

    pub fn capture_app_state(&mut self) {
        self.link.capture_app_session_state(&mut self.session_state);
    }

    pub fn commit_app_state(&mut self) {
        self.link.commit_app_session_state(&self.session_state);
    }

    pub fn get_clock_state(&mut self) -> ClockState {
        self.capture_app_state();
        let time = self.link.clock_micros();
        let enabled = match self.link.is_enabled() {
            true => "yes",
            false => "no ",
        }
        .to_string();
        let num_peers = self.link.num_peers();
        let start_stop = match self.link.is_start_stop_sync_enabled() {
            true => "yes",
            false => "no ",
        }
        .to_string();
        let playing = match self.session_state.is_playing() {
            true => "[playing]",
            false => "[stopped]",
        }
        .to_string();
        let tempo = self.session_state.tempo();
        let beats = self.session_state.beat_at_time(time, self.quantum);
        let phase = self.session_state.phase_at_time(time, self.quantum);
        let mut metro = String::with_capacity(self.quantum as usize);
        for i in 0..self.quantum as usize {
            if i > phase as usize {
                metro.push('O');
            } else {
                metro.push('X');
            }
        }

        ClockState {
            enabled,
            num_peers,
            start_stop,
            playing,
            tempo,
            beats,
            phase,
            metro,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClockState {
    pub enabled: String,
    pub num_peers: u64,
    pub start_stop: String,
    pub playing: String,
    pub tempo: f64,
    pub beats: f64,
    pub phase: f64,
    pub metro: String,
}
