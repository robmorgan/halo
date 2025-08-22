use rusty_link::{AblLink, SessionState};
use std::sync::Arc;
use tokio::sync::Mutex;

/// A thread-safe wrapper for Ableton Link using tokio::sync::Mutex
///
/// This manager provides async-safe access to the Ableton Link functionality,
/// allowing multiple async tasks to safely interact with the Link session.
/// The underlying AblLink instance is wrapped in an Arc<Mutex<AblLink>> for
/// thread-safe concurrent access.
pub struct AbletonLinkManager {
    link: Option<Arc<Mutex<AblLink>>>,
    session_state: SessionState,
    is_enabled: bool,
    num_peers: u64,
}

impl AbletonLinkManager {
    pub fn new() -> Self {
        Self {
            link: None,
            session_state: SessionState::new(),
            is_enabled: false,
            num_peers: 0,
        }
    }

    pub async fn enable(&mut self) -> Result<(), String> {
        if self.is_enabled {
            return Ok(());
        }

        // AblLink::new() doesn't return a Result, it just takes a BPM parameter
        let link = AblLink::new(120.0);
        let link_arc = Arc::new(Mutex::new(link));
        self.link = Some(link_arc);
        self.is_enabled = true;
        log::info!("Ableton Link enabled");
        Ok(())
    }

    pub fn disable(&mut self) {
        self.link = None;
        self.is_enabled = false;
        self.num_peers = 0;
        log::info!("Ableton Link disabled");
    }

    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    pub fn num_peers(&self) -> u64 {
        self.num_peers
    }

    pub async fn update(&mut self) -> Option<(f64, f64)> {
        if !self.is_enabled {
            return None;
        }

        if let Some(link_arc) = &self.link {
            let link = link_arc.lock().await;

            // Update the session state
            link.capture_app_session_state(&mut self.session_state);

            // Get the number of peers
            self.num_peers = link.num_peers() as u64;

            // Get tempo and beat time
            let tempo = self.session_state.tempo();
            let clock_micros = link.clock_micros();
            let beat_time = self.session_state.beat_at_time(clock_micros, 4.0); // 4/4 time signature

            // Update the session state with our current state
            link.commit_app_session_state(&self.session_state);

            Some((tempo, beat_time))
        } else {
            None
        }
    }

    pub async fn set_tempo(&mut self, tempo: f64) -> Result<(), String> {
        if !self.is_enabled {
            return Err("Ableton Link is not enabled".to_string());
        }

        if let Some(link_arc) = &self.link {
            let link = link_arc.lock().await;
            let clock_micros = link.clock_micros();
            self.session_state.set_tempo(tempo, clock_micros);
            link.commit_app_session_state(&self.session_state);
            log::info!("Set Ableton Link tempo to {} BPM", tempo);
            Ok(())
        } else {
            Err("Link not initialized".to_string())
        }
    }

    pub async fn enable_start_stop_sync(&mut self, enable: bool) -> Result<(), String> {
        if !self.is_enabled {
            return Err("Ableton Link is not enabled".to_string());
        }

        if let Some(link_arc) = &self.link {
            let link = link_arc.lock().await;
            link.enable_start_stop_sync(enable);
            log::info!(
                "Ableton Link start/stop sync {}",
                if enable { "enabled" } else { "disabled" }
            );
            Ok(())
        } else {
            Err("Link not initialized".to_string())
        }
    }

    pub async fn is_playing(&self) -> bool {
        if let Some(link_arc) = &self.link {
            let link = link_arc.lock().await;
            link.is_enabled() && self.session_state.is_playing()
        } else {
            false
        }
    }
}

impl Default for AbletonLinkManager {
    fn default() -> Self {
        Self::new()
    }
}
