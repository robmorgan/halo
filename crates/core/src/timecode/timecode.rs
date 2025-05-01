use std::time::{Duration, Instant};

#[derive(Clone, Debug, Copy)]
pub struct TimeCode {
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
    pub frames: u8,
    pub frame_rate: u8,
    last_update: Instant,
}

impl Default for TimeCode {
    fn default() -> Self {
        Self {
            hours: 0,
            minutes: 0,
            seconds: 0,
            frames: 0,
            frame_rate: 30, // Default to 30fps
            last_update: Instant::now(),
        }
    }
}

impl TimeCode {
    pub fn update(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);

        // Only update at the configured frame rate
        if elapsed > Duration::from_millis(1000 / self.frame_rate as u64) {
            self.last_update = now;

            // Update timecode
            self.frames += 1;
            if self.frames >= self.frame_rate {
                self.frames = 0;
                self.seconds += 1;
            }
            if self.seconds >= 60 {
                self.seconds = 0;
                self.minutes += 1;
            }
            if self.minutes >= 60 {
                self.minutes = 0;
                self.hours += 1;
            }
        }
    }

    pub fn reset(&mut self) {
        self.hours = 0;
        self.minutes = 0;
        self.seconds = 0;
        self.frames = 0;
        self.last_update = Instant::now();
    }

    pub fn set_frame_rate(&mut self, frame_rate: u8) {
        self.frame_rate = frame_rate;
    }

    /// Create a timecode from seconds
    pub fn from_seconds(total_seconds: f64, frame_rate: u8) -> Self {
        let hours = (total_seconds / 3600.0) as u8;
        let minutes = ((total_seconds % 3600.0) / 60.0) as u8;
        let seconds = (total_seconds % 60.0) as u8;
        let frames = ((total_seconds % 1.0) * frame_rate as f64) as u8;

        Self {
            hours,
            minutes,
            seconds,
            frames,
            frame_rate,
            last_update: Instant::now(),
        }
    }

    pub fn to_seconds(&self) -> f64 {
        self.hours as f64 * 3600.0
            + self.minutes as f64 * 60.0
            + self.seconds as f64
            + self.frames as f64 / self.frame_rate as f64
    }

    pub fn from_string(&mut self, timecode: &str) -> Result<(), String> {
        let parts: Vec<&str> = timecode.split(':').collect();
        if parts.len() < 4 {
            return Err("Invalid timecode format. Expected HH:MM:SS:FF".to_string());
        }

        self.hours = parts[0].parse().map_err(|_| "Invalid hours")?;
        self.minutes = parts[1].parse().map_err(|_| "Invalid minutes")?;
        self.seconds = parts[2].parse().map_err(|_| "Invalid seconds")?;
        self.frames = parts[3].parse().map_err(|_| "Invalid frames")?;

        Ok(())
    }

    pub fn to_string(&self) -> String {
        format!(
            "{:02}:{:02}:{:02}:{:02}",
            self.hours, self.minutes, self.seconds, self.frames
        )
    }
}
