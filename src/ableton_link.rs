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

    pub fn capture_app_state(&mut self) {
        self.link.capture_app_session_state(&mut self.session_state);
    }

    pub fn commit_app_state(&mut self) {
        self.link.commit_app_session_state(&self.session_state);
    }
}
