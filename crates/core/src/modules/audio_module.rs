use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::thread;

use async_trait::async_trait;
use rodio::{Decoder, OutputStream, Sink};
use tokio::sync::{mpsc, oneshot};

use super::traits::{AsyncModule, ModuleEvent, ModuleId, ModuleMessage};

/// Commands that can be sent to the audio thread
#[derive(Debug)]
enum AudioCommand {
    /// Load and play an audio file
    Play {
        file_path: PathBuf,
        response: oneshot::Sender<Result<(), String>>,
    },
    /// Stop playback
    Stop {
        response: oneshot::Sender<Result<(), String>>,
    },
    /// Pause playback
    Pause {
        response: oneshot::Sender<Result<(), String>>,
    },
    /// Resume playback
    Resume {
        response: oneshot::Sender<Result<(), String>>,
    },
    /// Set volume (0.0 to 1.0)
    SetVolume {
        volume: f32,
        response: oneshot::Sender<()>,
    },
    /// Query playback status
    #[allow(dead_code)]
    GetStatus {
        response: oneshot::Sender<AudioStatus>,
    },
    /// Shutdown the audio thread
    Shutdown,
}

/// Current status of the audio player
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct AudioStatus {
    current_file: Option<String>,
    is_playing: bool,
    is_paused: bool,
    volume: f32,
}

pub struct AudioModule {
    /// Channel to send commands to the audio thread
    command_tx: Option<mpsc::Sender<AudioCommand>>,
    /// Handle to the audio thread
    thread_handle: Option<thread::JoinHandle<()>>,
    /// Cached status for quick access
    cached_status: HashMap<String, String>,
}

impl AudioModule {
    pub fn new() -> Self {
        Self {
            command_tx: None,
            thread_handle: None,
            cached_status: HashMap::new(),
        }
    }

    /// Send a command to the audio thread and wait for response
    async fn send_command(&self, command: AudioCommand) -> Result<(), String> {
        if let Some(tx) = &self.command_tx {
            tx.send(command)
                .await
                .map_err(|_| "Audio thread has stopped".to_string())?;
            Ok(())
        } else {
            Err("Audio module not initialized".to_string())
        }
    }

    /// Play an audio file
    async fn play_file(&mut self, file_path: PathBuf) -> Result<(), String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.send_command(AudioCommand::Play {
            file_path,
            response: response_tx,
        })
        .await?;

        response_rx
            .await
            .map_err(|_| "Audio thread did not respond".to_string())?
    }

    /// Stop playback
    async fn stop(&mut self) -> Result<(), String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.send_command(AudioCommand::Stop {
            response: response_tx,
        })
        .await?;

        response_rx
            .await
            .map_err(|_| "Audio thread did not respond".to_string())?
    }

    /// Pause playback
    async fn pause(&mut self) -> Result<(), String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.send_command(AudioCommand::Pause {
            response: response_tx,
        })
        .await?;

        response_rx
            .await
            .map_err(|_| "Audio thread did not respond".to_string())?
    }

    /// Resume playback
    async fn resume(&mut self) -> Result<(), String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.send_command(AudioCommand::Resume {
            response: response_tx,
        })
        .await?;

        response_rx
            .await
            .map_err(|_| "Audio thread did not respond".to_string())?
    }

    /// Set volume (0.0 to 1.0)
    async fn set_volume(&mut self, volume: f32) -> Result<(), String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.send_command(AudioCommand::SetVolume {
            volume,
            response: response_tx,
        })
        .await?;

        response_rx
            .await
            .map_err(|_| "Audio thread did not respond".to_string())?;
        Ok(())
    }

    /// Get current status from the audio thread
    #[allow(dead_code)]
    async fn get_status(&self) -> Result<AudioStatus, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.send_command(AudioCommand::GetStatus {
            response: response_tx,
        })
        .await?;

        response_rx
            .await
            .map_err(|_| "Audio thread did not respond".to_string())
    }
}

/// The audio thread worker that handles all rodio operations
fn audio_thread_worker(mut command_rx: mpsc::Receiver<AudioCommand>) {
    log::info!("Audio thread starting");

    // Create the OutputStream - this must live for the entire thread lifetime
    let (_stream, stream_handle) = match OutputStream::try_default() {
        Ok((stream, handle)) => {
            log::info!("Successfully created audio output stream");
            (stream, handle)
        }
        Err(e) => {
            log::error!("Failed to create audio output stream: {e}");
            match e {
                rodio::StreamError::NoDevice => {
                    log::error!("No audio device available. This is a common issue on macOS.");
                    log::error!("Try: sudo killall coreaudiod");
                }
                _ => {
                    log::error!("Audio stream creation failed: {e}");
                }
            }
            return;
        }
    };

    // Audio state
    let mut sink: Option<Sink> = None;
    let mut current_file: Option<String> = None;
    let mut volume: f32 = 1.0;

    // Process commands
    while let Some(command) = command_rx.blocking_recv() {
        match command {
            AudioCommand::Play {
                file_path,
                response,
            } => {
                log::info!("Audio thread: Loading file: {file_path:?}");

                let result = (|| -> Result<(), String> {
                    // Create a new sink
                    let new_sink = Sink::try_new(&stream_handle)
                        .map_err(|e| format!("Failed to create audio sink: {e}"))?;

                    // Open and decode the audio file
                    let file = File::open(&file_path)
                        .map_err(|e| format!("Failed to open audio file: {e}"))?;
                    let reader = BufReader::new(file);
                    let source = Decoder::new(reader)
                        .map_err(|e| format!("Failed to decode audio file: {e}"))?;

                    // Add source to sink and configure
                    new_sink.append(source);
                    new_sink.set_volume(volume);
                    new_sink.play(); // Start playing immediately

                    // Update state
                    sink = Some(new_sink);
                    current_file = Some(file_path.to_string_lossy().to_string());

                    log::info!("Audio thread: File loaded and playing");
                    Ok(())
                })();

                let _ = response.send(result);
            }

            AudioCommand::Stop { response } => {
                let result = if let Some(s) = &sink {
                    s.stop();
                    sink = None;
                    current_file = None;
                    log::info!("Audio thread: Stopped playback");
                    Ok(())
                } else {
                    Err("No audio file loaded".to_string())
                };
                let _ = response.send(result);
            }

            AudioCommand::Pause { response } => {
                let result = if let Some(s) = &sink {
                    s.pause();
                    log::info!("Audio thread: Paused playback");
                    Ok(())
                } else {
                    Err("No audio file loaded".to_string())
                };
                let _ = response.send(result);
            }

            AudioCommand::Resume { response } => {
                let result = if let Some(s) = &sink {
                    s.play();
                    log::info!("Audio thread: Resumed playback");
                    Ok(())
                } else {
                    Err("No audio file loaded".to_string())
                };
                let _ = response.send(result);
            }

            AudioCommand::SetVolume {
                volume: vol,
                response,
            } => {
                volume = vol.clamp(0.0, 1.0);
                if let Some(s) = &sink {
                    s.set_volume(volume);
                }
                log::info!("Audio thread: Set volume to {volume}");
                let _ = response.send(());
            }

            AudioCommand::GetStatus { response } => {
                let status = AudioStatus {
                    current_file: current_file.clone(),
                    is_playing: sink
                        .as_ref()
                        .map(|s| !s.is_paused() && !s.empty())
                        .unwrap_or(false),
                    is_paused: sink.as_ref().map(|s| s.is_paused()).unwrap_or(false),
                    volume,
                };
                let _ = response.send(status);
            }

            AudioCommand::Shutdown => {
                log::info!("Audio thread: Received shutdown command");
                if let Some(s) = sink.take() {
                    s.stop();
                }
                break;
            }
        }
    }

    log::info!("Audio thread shutting down");
}

#[async_trait]
impl AsyncModule for AudioModule {
    fn id(&self) -> ModuleId {
        ModuleId::Audio
    }

    async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Initializing Audio module");
        println!("Initializing Audio module");

        // Create channel for communication with the audio thread
        let (command_tx, command_rx) = mpsc::channel::<AudioCommand>(32);

        // Spawn the dedicated audio thread (NOT a Tokio task)
        let thread_handle = thread::Builder::new()
            .name("audio-worker".to_string())
            .spawn(move || {
                audio_thread_worker(command_rx);
            })
            .map_err(|e| format!("Failed to spawn audio thread: {e}"))?;

        self.command_tx = Some(command_tx);
        self.thread_handle = Some(thread_handle);

        // Initialize cached status
        self.cached_status
            .insert("status".to_string(), "initialized".to_string());
        self.cached_status
            .insert("playback_state".to_string(), "idle".to_string());
        self.cached_status
            .insert("volume".to_string(), "1.00".to_string());

        log::info!("Audio module initialized successfully with dedicated thread");
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
                    log::info!("Audio module received AudioPlay event for file: {file_path}");

                    if file_path.is_empty() {
                        log::warn!("AudioPlay received with empty file path");
                        let _ = tx
                            .send(ModuleMessage::Error("Empty file path provided".to_string()))
                            .await;
                        continue;
                    }

                    log::info!("Loading and playing audio file: {file_path}");
                    match self.play_file(PathBuf::from(&file_path)).await {
                        Ok(_) => {
                            log::info!("Audio file loaded and playing successfully");
                            let _ = tx
                                .send(ModuleMessage::Status(format!("Playing audio: {file_path}")))
                                .await;
                        }
                        Err(e) => {
                            let error_msg = format!("Failed to play audio file: {e}");
                            log::error!("{error_msg}");
                            let _ = tx.send(ModuleMessage::Error(error_msg)).await;
                        }
                    }
                }

                ModuleEvent::AudioPause => {
                    if let Err(e) = self.pause().await {
                        let error_msg = format!("Failed to pause audio: {e}");
                        log::error!("{error_msg}");
                        let _ = tx.send(ModuleMessage::Error(error_msg)).await;
                    } else {
                        let _ = tx
                            .send(ModuleMessage::Status("Audio paused".to_string()))
                            .await;
                    }
                }

                ModuleEvent::AudioResume => {
                    if let Err(e) = self.resume().await {
                        let error_msg = format!("Failed to resume audio: {e}");
                        log::error!("{error_msg}");
                        let _ = tx.send(ModuleMessage::Error(error_msg)).await;
                    } else {
                        let _ = tx
                            .send(ModuleMessage::Status("Audio resumed".to_string()))
                            .await;
                    }
                }

                ModuleEvent::AudioStop => {
                    if let Err(e) = self.stop().await {
                        let error_msg = format!("Failed to stop audio: {e}");
                        log::error!("{error_msg}");
                        let _ = tx.send(ModuleMessage::Error(error_msg)).await;
                    } else {
                        let _ = tx
                            .send(ModuleMessage::Status("Audio stopped".to_string()))
                            .await;
                    }
                }

                ModuleEvent::AudioSetVolume(volume) => {
                    if let Err(e) = self.set_volume(volume).await {
                        log::error!("Failed to set volume: {e}");
                    } else {
                        let _ = tx
                            .send(ModuleMessage::Status(format!("Volume set to {volume:.2}")))
                            .await;
                    }
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
        log::info!("Audio module shutting down");

        // Send shutdown command to the audio thread
        if let Some(tx) = self.command_tx.take() {
            let _ = tx.send(AudioCommand::Shutdown).await;
            // Give the thread a moment to process the shutdown
            drop(tx);
        }

        // Wait for the audio thread to finish
        if let Some(handle) = self.thread_handle.take() {
            // Join with a reasonable timeout using tokio::task::spawn_blocking
            tokio::task::spawn_blocking(move || {
                if let Err(e) = handle.join() {
                    log::error!("Audio thread panicked during shutdown: {e:?}");
                }
            })
            .await?;
        }

        self.cached_status
            .insert("status".to_string(), "shutdown".to_string());
        log::info!("Audio module shutdown complete");
        Ok(())
    }

    fn status(&self) -> HashMap<String, String> {
        // Return cached status - querying the thread would be async
        // For real-time status, call get_status() from async context
        self.cached_status.clone()
    }
}
