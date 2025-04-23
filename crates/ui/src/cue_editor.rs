use eframe::egui::{self, RichText};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

use halo_core::{Cue, CueList, LightingConsole};

pub struct CueEditor {
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
                    let mut console_lock = console.lock();
                    console_lock.cue_manager.add_cue_list(CueList {
                        name: self.new_cue_list_name.clone(),
                        cues: Vec::new(),
                        audio_file: None,
                    });
                    drop(console_lock);
                    self.new_cue_list_name.clear();
                }
            });

            ui.separator();

            // List of cue lists
            egui::ScrollArea::vertical().show(ui, |ui| {
                let console_lock = console.lock();
                let cue_lists = console_lock.cue_manager.get_cue_lists();

                for (idx, cue_list) in cue_lists.iter().enumerate() {
                    let is_selected = self.selected_cue_list_index == Some(idx);
                    if ui.selectable_label(is_selected, &cue_list.name).clicked() {
                        self.selected_cue_list_index = Some(idx);
                    }
                }
            });
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
                        let cue_idx = console_lock.cue_manager.get_next_cue_idx();

                        // Create new cue and add to the current cue list
                        if let Some(cue_list_idx) = self.selected_cue_list_index {
                            console_lock.cue_manager.add_cue(
                                cue_list_idx,
                                Cue {
                                    name: self.new_cue_name.clone(),
                                    fade_time: self.new_fade_time,
                                    timecode: self.new_timecode.clone(),
                                    duration: Duration::from_secs_f64(self.new_fade_time),
                                    ..Default::default()
                                },
                            );

                            // Clear inputs
                            self.new_cue_name.clear();
                            self.new_timecode.clear();
                        }
                    }
                });
            } else {
                ui.label("Select a cue list first");
            }

            ui.separator();

            // Table of cues
            if let Some(cue_list_idx) = self.selected_cue_list_index {
                let console_lock = console.lock();
                let cue_list = console_lock.cue_manager.get_cue_list(cue_list_idx);

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
                        if let Some(cue_list) = cue_list {
                            for (idx, cue) in cue_list.cues.iter().enumerate() {
                                let is_selected = self.selected_cue_index == Some(idx);
                                let id_text = RichText::new(format!("{}", idx + 1)).strong();

                                if ui.selectable_label(is_selected, id_text).clicked() {
                                    self.selected_cue_index = Some(idx);
                                }

                                ui.label(&cue.name);
                                ui.label(format!("{:.1} s", cue.fade_time));
                                ui.label(&cue.timecode);
                                ui.end_row();
                            }
                        }
                    });

                ui.separator();
                ui.heading("Audio File");

                ui.horizontal(|ui| {
                    let console_lock = console.lock();
                    let cue_lists = console_lock.cue_manager.get_cue_lists();

                    let audio_file = cue_lists[cue_list_idx]
                        .audio_file
                        .as_deref()
                        .unwrap_or("None");
                    ui.label(format!("Current: {}", audio_file));
                });

                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.audio_file_path);
                    if ui.button("Load Audio").clicked() && !self.audio_file_path.is_empty() {
                        self.set_audio_file(cue_list_idx, self.audio_file_path.clone(), console);
                    }
                });

                // Cue details if selected
                if let Some(cue_idx) = self.selected_cue_index {
                    let console_lock = console.lock();
                    let cue = console_lock.cue_manager.get_cue(cue_idx);

                    if let Some(cue) = cue {
                        ui.separator();
                        ui.heading(format!("Cue {} Details", cue_idx + 1));

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
        });
    }

    fn set_audio_file(
        &mut self,
        cue_list_idx: usize,
        audio_file: String,
        console: &Arc<Mutex<LightingConsole>>,
    ) {
        let mut console_lock = console.lock();
        console_lock
            .cue_manager
            .set_audio_file(cue_list_idx, audio_file);
        self.audio_file_path.clear();
    }
}
