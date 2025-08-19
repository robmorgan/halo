use async_trait::async_trait;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use tokio::sync::mpsc;

use super::traits::{AsyncModule, ModuleEvent, ModuleId, ModuleMessage};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

pub struct AudioModule {
    stream_handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    current_file: Option<String>,
    volume: f32,
    status: HashMap<String, String>,
}

impl AudioModule {
    pub fn new() -> Self {
        Self {
            stream_handle: None,
            sink: None,
            current_file: None,
            volume: 1.0,
            status: HashMap::new(),
        }
    }

    async fn load_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), String> {
        let path_str = path.as_ref().to_string_lossy().to_string();

        if let Some(stream_handle) = &self.stream_handle {
            // Create a new sink
            let sink = Sink::try_new(stream_handle)
                .map_err(|e| format!("Failed to create audio sink: {}", e))?;

            // Open the audio file
            let file =
                File::open(&path).map_err(|e| format!("Failed to open audio file: {}", e))?;
            let reader = BufReader::new(file);

            // Decode the audio file
            let source =
                Decoder::new(reader).map_err(|e| format!("Failed to decode audio file: {}", e))?;

            // Add the source to the sink
            sink.append(source);
            sink.set_volume(self.volume);
            sink.pause();

            // Store the sink and current file
            self.sink = Some(sink);
            self.current_file = Some(path_str.clone());

            self.status.insert("current_file".to_string(), path_str);
            self.status
                .insert("status".to_string(), "loaded".to_string());

            Ok(())
        } else {
            Err("Audio module not initialized".to_string())
        }
    }

    async fn play(&mut self) -> Result<(), String> {
        if let Some(sink) = &self.sink {
            sink.play();
            self.status
                .insert("playback_state".to_string(), "playing".to_string());
            Ok(())
        } else {
            Err("No audio file loaded".to_string())
        }
    }

    async fn pause(&mut self) -> Result<(), String> {
        if let Some(sink) = &self.sink {
            sink.pause();
            self.status
                .insert("playback_state".to_string(), "paused".to_string());
            Ok(())
        } else {
            Err("No audio file loaded".to_string())
        }
    }

    async fn stop(&mut self) -> Result<(), String> {
        if let Some(sink) = &self.sink {
            sink.stop();
            self.status
                .insert("playback_state".to_string(), "stopped".to_string());
            Ok(())
        } else {
            Err("No audio file loaded".to_string())
        }
    }

    async fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(sink) = &self.sink {
            sink.set_volume(self.volume);
        }
        self.status
            .insert("volume".to_string(), format!("{:.2}", self.volume));
    }

    fn is_playing(&self) -> bool {
        if let Some(sink) = &self.sink {
            !sink.is_paused() && !sink.empty()
        } else {
            false
        }
    }
}

#[async_trait]
impl AsyncModule for AudioModule {
    fn id(&self) -> ModuleId {
        ModuleId::Audio
    }

    async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Initializing Audio module");

        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| format!("Failed to open audio output stream: {}", e))?;

        // Drop the stream immediately to avoid Send issues
        drop(stream);
        self.stream_handle = Some(stream_handle);

        self.status
            .insert("volume".to_string(), format!("{:.2}", self.volume));
        self.status
            .insert("status".to_string(), "initialized".to_string());
        self.status
            .insert("playback_state".to_string(), "idle".to_string());

        Ok(())
    }

    async fn run(
        &mut self,
        mut rx: mpsc::Receiver<ModuleEvent>,
        tx: mpsc::Sender<ModuleMessage>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Audio module started");

        let _ = tx
            .send(ModuleMessage::Status("Audio module running".to_string()))
            .await;

        while let Some(event) = rx.recv().await {
            match event {
                ModuleEvent::AudioPlay { file_path } => {
                    log::info!("Loading and playing audio file: {}", file_path);
                    match self.load_file(&file_path).await {
                        Ok(_) => {
                            if let Err(e) = self.play().await {
                                let error_msg = format!("Failed to play audio: {}", e);
                                log::error!("{}", error_msg);
                                let _ = tx.send(ModuleMessage::Error(error_msg)).await;
                            } else {
                                let _ = tx
                                    .send(ModuleMessage::Status(format!(
                                        "Playing audio: {}",
                                        file_path
                                    )))
                                    .await;
                            }
                        }
                        Err(e) => {
                            let error_msg = format!("Failed to load audio file: {}", e);
                            log::error!("{}", error_msg);
                            let _ = tx.send(ModuleMessage::Error(error_msg)).await;
                        }
                    }
                }

                ModuleEvent::AudioPause => {
                    if let Err(e) = self.pause().await {
                        let error_msg = format!("Failed to pause audio: {}", e);
                        log::error!("{}", error_msg);
                        let _ = tx.send(ModuleMessage::Error(error_msg)).await;
                    } else {
                        let _ = tx
                            .send(ModuleMessage::Status("Audio paused".to_string()))
                            .await;
                    }
                }

                ModuleEvent::AudioStop => {
                    if let Err(e) = self.stop().await {
                        let error_msg = format!("Failed to stop audio: {}", e);
                        log::error!("{}", error_msg);
                        let _ = tx.send(ModuleMessage::Error(error_msg)).await;
                    } else {
                        let _ = tx
                            .send(ModuleMessage::Status("Audio stopped".to_string()))
                            .await;
                    }
                }

                ModuleEvent::AudioSetVolume(volume) => {
                    self.set_volume(volume).await;
                    let _ = tx
                        .send(ModuleMessage::Status(format!(
                            "Volume set to {:.2}",
                            volume
                        )))
                        .await;
                }

                ModuleEvent::Shutdown => {
                    log::info!("Audio module received shutdown signal");
                    break;
                }

                _ => {
                    // Audio module ignores other events
                }
            }
        }

        log::info!("Audio module shutting down");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(sink) = &self.sink {
            sink.stop();
        }
        self.status
            .insert("status".to_string(), "shutdown".to_string());
        log::info!("Audio module shutdown complete");
        Ok(())
    }

    fn status(&self) -> HashMap<String, String> {
        let mut status = self.status.clone();
        status.insert("is_playing".to_string(), self.is_playing().to_string());
        status
    }
}
