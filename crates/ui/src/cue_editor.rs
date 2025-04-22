use eframe::egui::{self, Align, Color32, Layout, RichText, Stroke};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

use halo_core::{Chase, ChaseStep, Cue, Effect, EffectMapping, EffectType, LightingConsole};
use halo_fixtures::ChannelType;

pub struct CueList {
    pub name: String,
    pub cues: Vec<usize>, // Indices of cues in the console's cue list
    pub audio_file: Option<String>,
}

pub struct CueEditor {
    cue_lists: Vec<CueList>,
    selected_cue_list_index: Option<usize>,
    selected_cue_index: Option<usize>,
    new_cue_list_name: String,
    new_cue_name: String,
    new_fade_time: f64,
    new_timecode: String,
    audio_file_path: String,
}

impl Default for CueEditor {
    fn default() -> Self {
        Self {
            cue_lists: vec![CueList {
                name: "Main".to_string(),
                cues: Vec::new(),
                audio_file: None,
            }],
            selected_cue_list_index: Some(0),
            selected_cue_index: None,
            new_cue_list_name: String::new(),
            new_cue_name: String::new(),
            new_fade_time: 3.0,
            new_timecode: String::new(),
            audio_file_path: String::new(),
        }
    }
}

impl CueEditor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn render(&mut self, ctx: &egui::Context, console: &Arc<Mutex<LightingConsole>>) {
        egui::SidePanel::right("right_panel").show(ctx, |ui| {
            self.render_cue_lists_panel(ui, console);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                self.render_cues_panel(ui, console);
            });
        });
    }

    fn render_cue_lists_panel(&mut self, ui: &mut egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        ui.vertical(|ui| {
            ui.heading("Cue Lists");

            // Add new cue list
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut self.new_cue_list_name);
                if ui.button("Add Cue List").clicked() && !self.new_cue_list_name.is_empty() {
                    self.cue_lists.push(CueList {
                        name: self.new_cue_list_name.clone(),
                        cues: Vec::new(),
                        audio_file: None,
                    });
                    self.new_cue_list_name.clear();
                }
            });

            ui.separator();

            // List of cue lists
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (idx, cue_list) in self.cue_lists.iter().enumerate() {
                    let is_selected = self.selected_cue_list_index == Some(idx);
                    if ui.selectable_label(is_selected, &cue_list.name).clicked() {
                        self.selected_cue_list_index = Some(idx);
                    }
                }
            });

            // Audio file for selected cue list
            if let Some(idx) = self.selected_cue_list_index {
                if idx < self.cue_lists.len() {
                    ui.separator();
                    ui.heading("Audio File");

                    ui.horizontal(|ui| {
                        let audio_file =
                            self.cue_lists[idx].audio_file.as_deref().unwrap_or("None");
                        ui.label(format!("Current: {}", audio_file));
                    });

                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.audio_file_path);
                        if ui.button("Load Audio").clicked() && !self.audio_file_path.is_empty() {
                            self.cue_lists[idx].audio_file = Some(self.audio_file_path.clone());
                            self.audio_file_path.clear();
                        }
                    });
                }
            }
        });
    }

    fn render_cues_panel(&mut self, ui: &mut egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        ui.vertical(|ui| {
            ui.heading("Cues");

            // Add new cue
            if let Some(cue_list_idx) = self.selected_cue_list_index {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.new_cue_name);

                    ui.label("Fade Time:");
                    ui.add(
                        egui::DragValue::new(&mut self.new_fade_time)
                            .speed(0.1)
                            .suffix(" s"),
                    );

                    ui.label("Timecode:");
                    ui.text_edit_singleline(&mut self.new_timecode);

                    if ui.button("Add Cue").clicked() && !self.new_cue_name.is_empty() {
                        let mut console_lock = console.lock();
                        let cue_idx = console_lock.cues.len();

                        // Create new cue
                        console_lock.cues.push(Cue {
                            name: self.new_cue_name.clone(),
                            fade_time: self.new_fade_time,
                            timecode: self.new_timecode.clone(),
                            duration: Duration::from_secs_f64(self.new_fade_time),
                            ..Default::default()
                        });

                        // Add to current cue list
                        self.cue_lists[cue_list_idx].cues.push(cue_idx);

                        // Clear inputs
                        self.new_cue_name.clear();
                        self.new_timecode.clear();
                    }
                });
            } else {
                ui.label("Select a cue list first");
            }

            ui.separator();

            // Table of cues
            if let Some(cue_list_idx) = self.selected_cue_list_index {
                if cue_list_idx < self.cue_lists.len() {
                    let cue_list = &self.cue_lists[cue_list_idx];
                    let console_lock = console.lock();

                    egui::Grid::new("cues_grid")
                        .striped(true)
                        .num_columns(4)
                        .spacing([10.0, 6.0])
                        .show(ui, |ui| {
                            // Header
                            ui.strong("ID");
                            ui.strong("Name");
                            ui.strong("Fade Time");
                            ui.strong("Timecode");
                            ui.end_row();

                            // Cues
                            for (list_idx, &cue_idx) in cue_list.cues.iter().enumerate() {
                                if let Some(cue) = console_lock.cues.get(cue_idx) {
                                    let is_selected = self.selected_cue_index == Some(list_idx);
                                    let id_text =
                                        RichText::new(format!("{}", cue_idx + 1)).strong();

                                    if ui.selectable_label(is_selected, id_text).clicked() {
                                        self.selected_cue_index = Some(list_idx);
                                    }

                                    ui.label(&cue.name);
                                    ui.label(format!("{:.1} s", cue.fade_time));
                                    ui.label(&cue.timecode);
                                    ui.end_row();
                                }
                            }
                        });

                    // Cue details if selected
                    if let Some(cue_idx) = self.selected_cue_index {
                        if cue_idx < cue_list.cues.len() {
                            let console_cue_idx = cue_list.cues[cue_idx];
                            if let Some(cue) = console_lock.cues.get(console_cue_idx) {
                                ui.separator();
                                ui.heading(format!("Cue {} Details", console_cue_idx + 1));

                                ui.label(format!("Static Values: {}", cue.static_values.len()));
                                ui.label(format!("Chases: {}", cue.chases.len()));

                                if ui.button("Edit Cue").clicked() {
                                    // This would open the detailed cue editor
                                    // For now, we'll just set the selected cue in the main app
                                    // The actual implementation would depend on how you want to handle navigation
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}
