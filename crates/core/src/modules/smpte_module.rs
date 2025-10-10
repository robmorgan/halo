use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration, Instant};

use super::traits::{AsyncModule, ModuleEvent, ModuleId, ModuleMessage};
use crate::timecode::timecode::TimeCode;

pub struct SmpteModule {
    internal_timecode: TimeCode,
    external_timecode: Option<TimeCode>,
    frame_rate: u8,
    is_internal_source: bool,
    is_running: bool,
    last_update: Instant,
    status: HashMap<String, String>,
}

impl SmpteModule {
    pub fn new(frame_rate: u8) -> Self {
        Self {
            internal_timecode: TimeCode::default(),
            external_timecode: None,
            frame_rate,
            is_internal_source: true,
            is_running: false,
            last_update: Instant::now(),
            status: HashMap::new(),
        }
    }

    pub fn set_frame_rate(&mut self, frame_rate: u8) {
        self.frame_rate = frame_rate;
        self.internal_timecode.set_frame_rate(frame_rate);
    }

    pub fn use_external_source(&mut self, use_external: bool) {
        self.is_internal_source = !use_external;
        self.status.insert(
            "source".to_string(),
            if self.is_internal_source {
                "internal"
            } else {
                "external"
            }
            .to_string(),
        );
    }

    pub fn get_current_timecode(&self) -> TimeCode {
        if self.is_internal_source {
            self.internal_timecode
        } else {
            self.external_timecode.unwrap_or(self.internal_timecode)
        }
    }

    async fn update_internal_timecode(&mut self) {
        if self.is_internal_source && self.is_running {
            let now = Instant::now();
            let elapsed = now.duration_since(self.last_update);

            // Update at the configured frame rate
            let frame_duration = Duration::from_millis(1000 / self.frame_rate as u64);
            if elapsed >= frame_duration {
                self.internal_timecode.update();
                self.last_update = now;

                // Update status
                self.status
                    .insert("timecode".to_string(), self.internal_timecode.to_string());
            }
        }
    }

    pub fn start(&mut self) {
        self.is_running = true;
        self.last_update = Instant::now();
        self.status
            .insert("playback_state".to_string(), "running".to_string());
    }

    pub fn stop(&mut self) {
        self.is_running = false;
        self.status
            .insert("playback_state".to_string(), "stopped".to_string());
    }

    pub fn reset(&mut self) {
        self.internal_timecode.reset();
        self.last_update = Instant::now();
        self.status
            .insert("timecode".to_string(), self.internal_timecode.to_string());
    }
}

#[async_trait]
impl AsyncModule for SmpteModule {
    fn id(&self) -> ModuleId {
        ModuleId::Smpte
    }

    async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Initializing SMPTE module at {}fps", self.frame_rate);

        self.internal_timecode.set_frame_rate(self.frame_rate);

        self.status
            .insert("frame_rate".to_string(), self.frame_rate.to_string());
        self.status.insert(
            "source".to_string(),
            if self.is_internal_source {
                "internal"
            } else {
                "external"
            }
            .to_string(),
        );
        self.status
            .insert("status".to_string(), "initialized".to_string());
        self.status
            .insert("playback_state".to_string(), "stopped".to_string());
        self.status
            .insert("timecode".to_string(), self.internal_timecode.to_string());

        Ok(())
    }

    async fn run(
        &mut self,
        mut rx: mpsc::Receiver<ModuleEvent>,
        tx: mpsc::Sender<ModuleMessage>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!("SMPTE module started at {}fps", self.frame_rate);

        let _ = tx
            .send(ModuleMessage::Status("SMPTE module running".to_string()))
            .await;

        // Create interval for internal timecode updates
        let frame_duration = Duration::from_millis(1000 / self.frame_rate as u64);
        let mut update_interval = interval(frame_duration);

        // Status reporting interval (every second)
        let mut status_interval = interval(Duration::from_secs(1));

        let mut shutdown = false;

        while !shutdown {
            tokio::select! {
                // Handle incoming events
                Some(event) = rx.recv() => {
                    match event {
                        ModuleEvent::SmpteSync { timecode } => {
                            if !self.is_internal_source {
                                self.external_timecode = Some(timecode);
                                self.status.insert("timecode".to_string(), timecode.to_string());
                            }
                        }
                        ModuleEvent::Shutdown => {
                            log::info!("SMPTE module received shutdown signal");
                            shutdown = true;
                            break;
                        }
                        _ => {
                            // SMPTE module only handles sync events
                        }
                    }
                }

                // Update internal timecode at frame rate
                _ = update_interval.tick() => {
                    self.update_internal_timecode().await;
                }

                // Send periodic status updates
                _ = status_interval.tick() => {
                    let current_tc = self.get_current_timecode();
                    self.status.insert("timecode".to_string(), current_tc.to_string());

                    let _ = tx.send(ModuleMessage::Status(format!(
                        "SMPTE: {} ({}fps, {} source)",
                        current_tc.to_string(),
                        self.frame_rate,
                        if self.is_internal_source { "internal" } else { "external" }
                    ))).await;
                }
            }
        }

        log::info!("SMPTE module shutting down");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.stop();
        self.status
            .insert("status".to_string(), "shutdown".to_string());
        log::info!("SMPTE module shutdown complete");
        Ok(())
    }

    fn status(&self) -> HashMap<String, String> {
        self.status.clone()
    }
}
