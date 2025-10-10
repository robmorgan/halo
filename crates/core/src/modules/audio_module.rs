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
        println!("Audio module: Attempting to load file: {}", path_str);

        if let Some(stream_handle) = &self.stream_handle {
            println!("Audio module: Stream handle available, creating sink");
            // Create a new sink
            // let sink = Sink::try_new(stream_handle).map_err(|e| {
            //     let error_msg = format!("Failed to create audio sink: {}", e);
            //     println!("ERROR: {}", error_msg);

            //     // Provide more specific error information
            //     match e {
            //         rodio::PlayError::NoDevice => {
            //             println!("ERROR: No audio device available. Please check your audio system and permissions.");
            //             println!("Try running: sudo killall coreaudiod");
            //         }
            //         _ => {
            //             println!("ERROR: Audio sink creation failed with error: {}", e);
            //         }
            //     }

            //     error_msg
            // })?;

            let sink = Sink::try_new(stream_handle)
                .map_err(|e| format!("Failed to create audio sink: {}", e))?;

            println!("Audio module: Sink created, opening file");
            // Open the audio file
            let file = File::open(&path).map_err(|e| {
                let error_msg = format!("Failed to open audio file: {}", e);
                println!("ERROR: {}", error_msg);
                error_msg
            })?;
            let reader = BufReader::new(file);

            println!("Audio module: File opened, decoding audio");
            // Decode the audio file
            let source = Decoder::new(reader).map_err(|e| {
                let error_msg = format!("Failed to decode audio file: {}", e);
                println!("ERROR: {}", error_msg);
                error_msg
            })?;

            println!("Audio module: Audio decoded, adding to sink");
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

            println!("Audio module: File loaded successfully");
            Ok(())
        } else {
            let error_msg = "Audio module not initialized - no stream handle available";
            println!("ERROR: {}", error_msg);
            Err(error_msg.to_string())
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

    async fn resume(&mut self) -> Result<(), String> {
        if let Some(sink) = &self.sink {
            sink.play();
            self.status
                .insert("playback_state".to_string(), "playing".to_string());
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
        log::info!("Setting volume to {}", volume);
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(sink) = &self.sink {
            sink.set_volume(self.volume);
            log::info!("Volume set to {}", self.volume);
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
        println!("Initializing Audio module");
        log::info!("Initializing Audio module");

        // Try to create output stream with better error handling
        let (stream, stream_handle) = match OutputStream::try_default() {
            Ok((stream, handle)) => {
                println!("Successfully created default audio output stream");
                (stream, handle)
            }
            Err(e) => {
                println!("Failed to create default audio stream: {}", e);

                // Provide helpful error message for common issues
                match e {
                    rodio::StreamError::NoDevice => {
                        println!(
                            "ERROR: No audio device available. This is a common issue on macOS."
                        );
                        println!("Try the following solutions:");
                        println!("1. Check System Preferences > Sound > Output");
                        println!("2. Restart the audio system: sudo killall coreaudiod");
                        println!("3. Check if another application is using the audio device");
                        println!("4. Try plugging in headphones or external speakers");
                    }
                    _ => {
                        println!("ERROR: Audio stream creation failed: {}", e);
                    }
                }

                let error_msg = format!("Failed to open audio output stream: {}", e);
                log::error!("{}", error_msg);
                return Err(error_msg.into());
            }
        };

        // Drop the stream immediately to avoid Send issues
        drop(stream);
        self.stream_handle = Some(stream_handle);

        self.status
            .insert("volume".to_string(), format!("{:.2}", self.volume));
        self.status
            .insert("status".to_string(), "initialized".to_string());
        self.status
            .insert("playback_state".to_string(), "idle".to_string());

        log::info!("Audio module initialized successfully");
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
                    log::info!(
                        "Audio module received AudioPlay event for file: {}",
                        file_path
                    );

                    if file_path.is_empty() {
                        log::warn!("AudioPlay received with empty file path");
                        let _ = tx
                            .send(ModuleMessage::Error("Empty file path provided".to_string()))
                            .await;
                        continue;
                    }

                    log::info!("Loading and playing audio file: {}", file_path);
                    match self.load_file(&file_path).await {
                        Ok(_) => {
                            log::info!("Audio file loaded successfully, starting playback");
                            if let Err(e) = self.play().await {
                                let error_msg = format!("Failed to play audio: {}", e);
                                log::error!("{}", error_msg);
                                let _ = tx.send(ModuleMessage::Error(error_msg)).await;
                            } else {
                                log::info!("Audio playback started successfully");
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

                ModuleEvent::AudioResume => {
                    if let Err(e) = self.resume().await {
                        let error_msg = format!("Failed to resume audio: {}", e);
                        log::error!("{}", error_msg);
                        let _ = tx.send(ModuleMessage::Error(error_msg)).await;
                    } else {
                        let _ = tx
                            .send(ModuleMessage::Status("Audio resumed".to_string()))
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
