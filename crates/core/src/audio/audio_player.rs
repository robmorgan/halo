use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct AudioPlayer {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    sink: Option<Sink>,
    current_file: Option<String>,
    volume: f32,
}

impl AudioPlayer {
    pub fn new() -> Result<Self, String> {
        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| format!("Failed to open audio output stream: {}", e))?;

        Ok(AudioPlayer {
            _stream: stream,
            stream_handle,
            sink: None,
            current_file: None,
            volume: 1.0,
        })
    }

    pub fn load_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), String> {
        let path_str = path.as_ref().to_string_lossy().to_string();

        // Create a new sink
        let sink = Sink::try_new(&self.stream_handle)
            .map_err(|e| format!("Failed to create audio sink: {}", e))?;

        // Open the audio file
        let file = File::open(&path).map_err(|e| format!("Failed to open audio file: {}", e))?;
        let reader = BufReader::new(file);

        // Decode the audio file
        let source =
            Decoder::new(reader).map_err(|e| format!("Failed to decode audio file: {}", e))?;

        // Add the source to the sink
        sink.append(source);
        sink.set_volume(self.volume);

        // Store the sink and current file
        self.sink = Some(sink);
        self.current_file = Some(path_str);

        Ok(())
    }

    pub fn play(&self) -> Result<(), String> {
        if let Some(sink) = &self.sink {
            sink.play();
            Ok(())
        } else {
            Err("No audio file loaded".to_string())
        }
    }

    pub fn pause(&self) -> Result<(), String> {
        if let Some(sink) = &self.sink {
            sink.pause();
            Ok(())
        } else {
            Err("No audio file loaded".to_string())
        }
    }

    pub fn stop(&self) -> Result<(), String> {
        if let Some(sink) = &self.sink {
            sink.stop();
            Ok(())
        } else {
            Err("No audio file loaded".to_string())
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(sink) = &self.sink {
            sink.set_volume(self.volume);
        }
    }

    pub fn is_playing(&self) -> bool {
        if let Some(sink) = &self.sink {
            !sink.is_paused() && !sink.empty()
        } else {
            false
        }
    }

    pub fn seek(&self, position: Duration) -> Result<(), String> {
        if let Some(sink) = &self.sink {
            // Rodio doesn't support seeking directly, so we'd need to
            // implement a custom solution for this, potentially by
            // reloading the file and skipping to position
            Err("Seeking not implemented yet".to_string())
        } else {
            Err("No audio file loaded".to_string())
        }
    }

    pub fn get_current_file(&self) -> Option<&String> {
        self.current_file.as_ref()
    }
}
