use crate::TimeCode;

pub struct TimeCodeManager {
    timecode: TimeCode,
    is_running: bool,
}

impl TimeCodeManager {
    pub fn new() -> Self {
        Self {
            timecode: TimeCode::default(),
            is_running: false,
        }
    }

    pub fn start_timecode(&mut self) {
        self.is_running = true;
    }

    pub fn stop_timecode(&mut self) {
        self.is_running = false;
    }

    pub fn reset_timecode(&mut self) {
        self.timecode.reset();
    }

    pub fn get_timecode(&self) -> TimeCode {
        self.timecode
    }

    pub fn set_timecode_frame_rate(&mut self, frame_rate: u8) {
        self.timecode.set_frame_rate(frame_rate);
    }

    pub fn update(&mut self) {
        if self.is_running {
            self.timecode.update();
        }
    }
}
