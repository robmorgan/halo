use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration, Instant};

use super::traits::{AsyncModule, ModuleEvent, ModuleId, ModuleMessage};
use crate::artnet::artnet::ArtNet;
use crate::artnet::network_config::NetworkConfig;

pub struct DmxModule {
    artnet_connections: Vec<Option<ArtNet>>, // Multiple ArtNet instances
    network_config: NetworkConfig,
    last_frame_time: Option<Instant>,
    frames_sent: u64,
    target_fps: f64,
    status: HashMap<String, String>,
}

impl DmxModule {
    pub fn new(network_config: NetworkConfig) -> Self {
        let num_destinations = network_config.destinations.len();
        let mut artnet_connections = Vec::new();
        for _ in 0..num_destinations {
            artnet_connections.push(None);
        }
        
        Self {
            artnet_connections,
            network_config,
            last_frame_time: None,
            frames_sent: 0,
            target_fps: 44.0, // DMX standard 44Hz
            status: HashMap::new(),
        }
    }

    pub fn set_target_fps(&mut self, fps: f64) {
        self.target_fps = fps;
    }
}

#[async_trait]
impl AsyncModule for DmxModule {
    fn id(&self) -> ModuleId {
        ModuleId::Dmx
    }

    async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!(
            "Initializing DMX module with {} destinations",
            self.network_config.destinations.len()
        );

        // Initialize ArtNet connections for each destination
        for (i, destination) in self.network_config.destinations.iter().enumerate() {
            log::info!(
                "Setting up ArtNet connection {} for destination: {}",
                i, destination.name
            );
            
            let artnet = ArtNet::new(destination.mode.clone())?;
            self.artnet_connections[i] = Some(artnet);
        }

        self.status.insert(
            "mode".to_string(),
            self.network_config.get_mode_string().to_string(),
        );
        self.status.insert(
            "destinations".to_string(),
            format!("{}", self.network_config.destinations.len()),
        );
        self.status.insert(
            "destination_info".to_string(),
            self.network_config.get_destination(),
        );
        self.status
            .insert("status".to_string(), "initialized".to_string());

        Ok(())
    }

    async fn run(
        &mut self,
        mut rx: mpsc::Receiver<ModuleEvent>,
        tx: mpsc::Sender<ModuleMessage>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Validate all ArtNet connections are initialized
        for (i, conn) in self.artnet_connections.iter().enumerate() {
            if conn.is_none() {
                return Err(format!("ArtNet connection {} not initialized", i).into());
            }
        }

        // Create interval for DMX output timing
        let frame_duration = Duration::from_secs_f64(1.0 / self.target_fps);
        let mut frame_interval = interval(frame_duration);

        let mut last_dmx_data: HashMap<u8, Vec<u8>> = HashMap::new();
        let mut shutdown = false;

        log::info!("DMX module started with {} destinations, running at {}Hz", 
                  self.artnet_connections.len(), self.target_fps);

        // Send initial status
        let _ = tx
            .send(ModuleMessage::Status(format!(
                "DMX module running at {}Hz with {} destinations",
                self.target_fps, self.artnet_connections.len()
            )))
            .await;

        while !shutdown {
            tokio::select! {
                // Handle incoming events
                Some(event) = rx.recv() => {
                    match event {
                        ModuleEvent::DmxOutput(universe, data) => {
                            last_dmx_data.insert(universe, data);
                        }
                        ModuleEvent::Shutdown => {
                            log::info!("DMX module received shutdown signal");
                            shutdown = true;
                            break;
                        }
                        _ => {
                            // DMX module only handles DMX output events
                        }
                    }
                }

                // Send DMX data at regular intervals
                _ = frame_interval.tick() => {
                    let now = Instant::now();

                    // Send each universe to its routed destination
                    for (universe, data) in &last_dmx_data {
                        if let Some(dest_index) = self.network_config.get_destination_for_universe(*universe) {
                            if let Some(Some(artnet)) = self.artnet_connections.get(dest_index) {
                                artnet.send_data(*universe, data.clone());
                            } else {
                                log::warn!("No ArtNet connection found for destination index {}", dest_index);
                            }
                        } else {
                            log::warn!("No destination routing configured for universe {}", universe);
                        }
                    }

                    self.frames_sent += 1;
                    self.last_frame_time = Some(now);

                    // Update status periodically
                    if self.frames_sent % (self.target_fps as u64 * 5) == 0 { // Every 5 seconds
                        self.status.insert("frames_sent".to_string(), self.frames_sent.to_string());
                        self.status.insert("fps".to_string(), format!("{:.1}", self.target_fps));
                        self.status.insert("universes".to_string(), last_dmx_data.len().to_string());

                        let _ = tx.send(ModuleMessage::Status(format!(
                            "DMX: {} frames sent, {} universes active across {} destinations",
                            self.frames_sent,
                            last_dmx_data.len(),
                            self.artnet_connections.len()
                        ))).await;
                    }
                }
            }
        }

        log::info!(
            "DMX module shutting down after sending {} frames",
            self.frames_sent
        );
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.status
            .insert("status".to_string(), "shutdown".to_string());
        log::info!("DMX module shutdown complete");
        Ok(())
    }

    fn status(&self) -> HashMap<String, String> {
        self.status.clone()
    }
}
