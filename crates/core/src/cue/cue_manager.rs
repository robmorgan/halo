use crate::Cue;
use crate::CueList;

pub struct CueManager {
    cue_lists: Vec<CueList>,
    current_cue_list: usize,
    current_cue: usize,
    is_playing: bool,
}

impl CueManager {
    pub fn new(cue_lists: Vec<CueList>) -> Self {
        CueManager {
            cue_lists,
            current_cue_list: 0,
            current_cue: 0,
            is_playing: false,
        }
    }

    pub fn set_cue_lists(&mut self, cue_lists: Vec<CueList>) {
        self.cue_lists = cue_lists;
    }

    pub fn add_cue_list(&mut self, cue_list: CueList) -> usize {
        self.cue_lists.push(cue_list);
        self.cue_lists.len() - 1 // Return the index of the new cue list
    }

    pub fn get_cue_lists(&self) -> &[CueList] {
        &self.cue_lists
    }

    pub fn get_cue_list(&self, index: usize) -> Option<&CueList> {
        self.cue_lists.get(index)
    }

    pub fn get_cue_list_mut(&mut self, index: usize) -> Option<&mut CueList> {
        self.cue_lists.get_mut(index)
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
    pub fn go_to_next_cue(&mut self) -> Result<&Cue, String> {
        if self.current_cue_list >= self.cue_lists.len() {
            return Err("Invalid cue list index".to_string());
        }

        let cue_list = &self.cue_lists[self.current_cue_list];
        if self.current_cue + 1 >= cue_list.cues.len() {
            return Err("No next cue".to_string());
        }

        self.current_cue += 1;
        self.is_playing = true;
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
            self.is_playing = true;
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
        self.is_playing = true;

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

    pub fn get_next_cue_idx(&self) -> Option<usize> {
        if self.current_cue_list >= self.cue_lists.len() {
            return None;
        }

        let cue_list = &self.cue_lists[self.current_cue_list];
        if self.current_cue >= cue_list.cues.len() {
            return None;
        }

        Some((self.current_cue + 1) % cue_list.cues.len())
    }

    pub fn stop_playback(&mut self) {
        self.is_playing = false;
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }
}
