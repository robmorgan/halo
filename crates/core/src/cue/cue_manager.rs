use std::time::{Duration, Instant, SystemTime};

use crate::{Cue, CueList, EffectMapping, StaticValue};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Holding,
}

pub struct CueManager {
    cue_lists: Vec<CueList>,
    current_cue_list: usize,
    current_cue: usize,
    playback_state: PlaybackState,
    current_cue_start_time: Option<Instant>,
    progress: f32,
}

impl CueManager {
    pub fn new(cue_lists: Vec<CueList>) -> Self {
        CueManager {
            cue_lists,
            current_cue_list: 0,
            current_cue: 0,
            playback_state: PlaybackState::Stopped,
            current_cue_start_time: None,
            progress: 0.0,
        }
    }

    pub fn update(&mut self, current_time: Duration) {
        if self.playback_state == PlaybackState::Playing {
            if let Some(current_cue) = self.get_current_cue() {
                if let Some(start_time) = self.current_cue_start_time {
                    let elapsed_cue_time = current_time - start_time.elapsed();
                    if elapsed_cue_time < current_cue.fade_time {
                        self.progress =
                            elapsed_cue_time.as_secs_f32() / current_cue.fade_time.as_secs_f32();
                    } else {
                        self.progress = 1.0;
                    }
                }
            }
        }
    }

    pub fn set_cue_lists(&mut self, cue_lists: Vec<CueList>) {
        self.cue_lists = cue_lists;
    }

    pub fn add_cue_list(&mut self, cue_list: CueList) -> usize {
        self.cue_lists.push(cue_list);
        self.cue_lists.len() - 1 // Return the index of the new cue list
    }

    pub fn get_cue_lists(&self) -> Vec<CueList> {
        self.cue_lists.clone()
    }

    pub fn get_cue_list(&self, index: usize) -> Option<&CueList> {
        self.cue_lists.get(index)
    }

    pub fn get_cue_list_mut(&mut self, index: usize) -> Option<&mut CueList> {
        self.cue_lists.get_mut(index)
    }

    pub fn get_current_cue_list(&self) -> Option<&CueList> {
        self.cue_lists.get(self.current_cue_list)
    }

    pub fn get_current_cue_list_idx(&self) -> usize {
        self.current_cue_list
    }

    pub fn remove_cue_list(&mut self, index: usize) -> Result<CueList, String> {
        if index < self.cue_lists.len() {
            Ok(self.cue_lists.remove(index))
        } else {
            Err("Cue list index out of bounds".to_string())
        }
    }

    pub fn set_audio_file(&mut self, cue_list_idx: usize, path: String) -> Result<(), String> {
        if let Some(cue_list) = self.cue_lists.get_mut(cue_list_idx) {
            cue_list.audio_file = Some(path);
            Ok(())
        } else {
            Err("Invalid cue list index".to_string())
        }
    }

    // Cue Management
    pub fn add_cue(&mut self, cue_list_idx: usize, cue: Cue) -> Result<usize, String> {
        if cue_list_idx >= self.cue_lists.len() {
            return Err("Invalid cue list index".to_string());
        }

        self.cue_lists[cue_list_idx].cues.push(cue);
        let cue_idx = self.cue_lists[cue_list_idx].cues.len() - 1;

        Ok(cue_idx)
    }

    pub fn get_cue(&self, cue_idx: usize) -> Option<&Cue> {
        self.cue_lists[self.current_cue_list].cues.get(cue_idx)
    }

    pub fn get_current_cue_idx(&self) -> Option<usize> {
        if self.current_cue_list >= self.cue_lists.len() {
            return None;
        }

        Some(self.current_cue)
    }

    pub fn is_cue_active(&self, cue_id: usize) -> bool {
        self.cue_lists[self.current_cue_list].cues[self.current_cue].id == cue_id
    }

    pub fn get_current_cue_progress(&self) -> f32 {
        self.progress
    }

    pub fn get_cue_mut(&mut self, cue_idx: usize) -> Option<&mut Cue> {
        self.cue_lists[self.current_cue_list].cues.get_mut(cue_idx)
    }

    pub fn remove_cue(&mut self, cue_list_idx: usize, cue_idx: usize) -> Result<(), String> {
        if cue_list_idx >= self.cue_lists.len() {
            return Err("Invalid cue list index".to_string());
        }

        // Remove the cue index from the cue list
        let cue_list = &mut self.cue_lists[cue_list_idx];
        if cue_idx < cue_list.cues.len() {
            cue_list.cues.remove(cue_idx);
            Ok(())
        } else {
            Err("Invalid cue index".to_string())
        }
    }

    // Playback Control

    /// Selects the previous cue list if available
    pub fn select_previous_cue_list(&mut self) -> Result<(), String> {
        if self.current_cue_list > 0 {
            self.current_cue_list = self.current_cue_list - 1;
            Ok(())
        } else if !self.cue_lists.is_empty() {
            // Wrap around to the last cue list
            self.current_cue_list = self.cue_lists.len() - 1;
            Ok(())
        } else {
            Err("No cue lists available".to_string())
        }
    }

    /// Selects the next cue list if available
    pub fn select_next_cue_list(&mut self) -> Result<(), String> {
        if self.current_cue_list + 1 < self.cue_lists.len() {
            self.current_cue_list = self.current_cue_list + 1;
            Ok(())
        } else if !self.cue_lists.is_empty() {
            // Wrap around to the first cue list
            self.current_cue_list = 0;
            Ok(())
        } else {
            Err("No cue lists available".to_string())
        }
    }

    pub fn go(&mut self, elapsed: Duration) -> Result<&Cue, String> {
        self.playback_state = PlaybackState::Playing;
        self.go_to_next_cue(elapsed)
    }

    pub fn hold(&mut self) -> Result<&Cue, String> {
        self.playback_state = PlaybackState::Holding;
        self.get_current_cue()
            .ok_or_else(|| "No current cue".to_string())
    }

    pub fn stop(&mut self) -> Result<&Cue, String> {
        self.playback_state = PlaybackState::Stopped;
        self.get_current_cue()
            .ok_or_else(|| "No current cue".to_string())
    }

    pub fn go_to_next_cue(&mut self, elapsed: Duration) -> Result<&Cue, String> {
        if self.current_cue_list >= self.cue_lists.len() {
            return Err("Invalid cue list index".to_string());
        }

        let cue_list = &self.cue_lists[self.current_cue_list];
        if self.current_cue + 1 >= cue_list.cues.len() {
            return Err("No next cue".to_string());
        }

        self.current_cue += 1;
        self.playback_state = PlaybackState::Playing;
        self.current_cue_start_time = Some(Instant::now() - elapsed);
        self.get_current_cue()
            .ok_or_else(|| "No current cue".to_string())
    }

    pub fn go_to_previous_cue(&mut self) -> Result<&Cue, String> {
        if self.current_cue_list >= self.cue_lists.len() {
            return Err("Invalid cue list index".to_string());
        }

        let cue_list = &self.cue_lists[self.current_cue_list];
        if cue_list.cues.is_empty() {
            return Err("No previous cue".to_string());
        }

        if self.current_cue > 0 {
            self.current_cue -= 1;
            self.playback_state = PlaybackState::Playing;
            self.get_current_cue()
                .ok_or_else(|| "No current cue".to_string())
        } else {
            Err("Already at first cue".to_string())
        }
    }

    pub fn go_to_cue(&mut self, cue_list_idx: usize, cue_idx: usize) -> Result<&Cue, String> {
        if cue_list_idx >= self.cue_lists.len() {
            return Err("Invalid cue list index".to_string());
        }

        let cue_list = &self.cue_lists[cue_list_idx];
        if cue_idx >= cue_list.cues.len() {
            return Err("Invalid cue index".to_string());
        }

        self.current_cue_list = cue_list_idx;
        self.current_cue = cue_idx;
        self.playback_state = PlaybackState::Playing;

        self.get_current_cue()
            .ok_or_else(|| "No current cue".to_string())
    }

    pub fn get_current_cue(&self) -> Option<&Cue> {
        if self.current_cue_list >= self.cue_lists.len() {
            return None;
        }

        let cue_list = &self.cue_lists[self.current_cue_list];
        if self.current_cue >= cue_list.cues.len() {
            return None;
        }

        cue_list.cues.get(self.current_cue)
    }

    pub fn get_current_cues(&self) -> Vec<&Cue> {
        if self.current_cue_list >= self.cue_lists.len() {
            return vec![];
        }

        let cue_list = &self.cue_lists[self.current_cue_list];
        if self.current_cue >= cue_list.cues.len() {
            return vec![];
        }

        cue_list.cues.iter().collect()
    }

    pub fn get_next_cue_id(&self) -> Option<usize> {
        let cue_list = self.get_current_cue_list()?;

        if cue_list.cues.is_empty() {
            return Some(1);
        }

        // Find the maximum ID in the current cue list
        let max_id = cue_list.cues.iter().map(|cue| cue.id).max().unwrap_or(0);

        Some(max_id + 1)
    }

    pub fn stop_playback(&mut self) {
        self.playback_state = PlaybackState::Stopped;
    }

    pub fn get_playback_state(&self) -> PlaybackState {
        self.playback_state
    }

    pub fn record(
        &mut self,
        cue_name: String,
        cue_list_idx: usize,
        fade_time: f32,
        values: Vec<StaticValue>,
        effects: Vec<EffectMapping>,
    ) {
        if let Some(id) = self.get_next_cue_id() {
            self.cue_lists[cue_list_idx].cues.push(Cue {
                id,
                name: cue_name,
                fade_time: Duration::from_secs_f32(fade_time),
                static_values: values,
                effects,
                timecode: None,
                is_blocking: false,
            });
        }
    }
}
