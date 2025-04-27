use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, RichText};
use halo_core::{Cue, CueList, LightingConsole};
use parking_lot::Mutex;
use rfd::FileDialog;

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
            self.render_cues_panel(ui, console);
        });
    }

    fn render_cue_lists_panel(&mut self, ui: &mut egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        ui.vertical(|ui| {
            ui.heading("Cue Lists");

            // Add new cue list
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut self.new_cue_list_name);

                let name_valid = !self.new_cue_list_name.is_empty();
                if ui
                    .add_enabled(name_valid, egui::Button::new("Add Cue List"))
                    .clicked()
                {
                    let mut console_lock = console.lock();
                    console_lock.cue_manager.add_cue_list(CueList {
                        name: std::mem::take(&mut self.new_cue_list_name),
                        cues: Vec::new(),
                        audio_file: None,
                    });
                    drop(console_lock);
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
                        self.selected_cue_index = None; // Reset cue selection when changing lists
                    }
                }
                drop(console_lock);
            });
        });
    }

    fn render_cues_panel(&mut self, ui: &mut egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        ui.vertical(|ui| {
            ui.heading("Cues");

            // Add new cue section
            self.render_add_cue_section(ui, console);

            ui.separator();

            // Table of cues
            if let Some(cue_list_idx) = self.selected_cue_list_index {
                self.render_cues_table(ui, console, cue_list_idx);
                self.render_audio_section(ui, cue_list_idx, console);
                self.render_cue_details(ui, console);
            }
        });
    }

    fn render_add_cue_section(&mut self, ui: &mut egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
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

                let name_valid = !self.new_cue_name.is_empty();
                if ui
                    .add_enabled(name_valid, egui::Button::new("Add Cue"))
                    .clicked()
                {
                    // Need to get a mutable lock for modifying the console
                    let mut console_lock = console.lock();
                    let _ = console_lock.cue_manager.add_cue(
                        cue_list_idx,
                        Cue {
                            name: std::mem::take(&mut self.new_cue_name),
                            fade_time: self.new_fade_time,
                            timecode: std::mem::take(&mut self.new_timecode),
                            duration: Duration::from_secs_f64(self.new_fade_time),
                            ..Default::default()
                        },
                    );
                    drop(console_lock);
                }
            });
        } else {
            ui.label("Select a cue list first");
        }
    }

    fn render_cues_table(
        &mut self,
        ui: &mut egui::Ui,
        console: &Arc<Mutex<LightingConsole>>,
        cue_list_idx: usize,
    ) {
        egui::Grid::new("cues_grid")
            .striped(true)
            .num_columns(4)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                let console_lock = console.lock();
                let cue_list = console_lock.cue_manager.get_cue_list(cue_list_idx);

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
                drop(console_lock);
            });
    }

    fn render_audio_section(
        &mut self,
        ui: &mut egui::Ui,
        cue_list_idx: usize,
        console: &Arc<Mutex<LightingConsole>>,
    ) {
        let console_lock = console.lock();
        ui.separator();
        ui.heading("Audio File");

        ui.horizontal(|ui| {
            let cue_lists = console_lock.cue_manager.get_cue_lists();
            if cue_list_idx < cue_lists.len() {
                let audio_file = cue_lists[cue_list_idx]
                    .audio_file
                    .as_deref()
                    .unwrap_or("None");
                ui.label(format!("Current: {}", audio_file));
            }
        });
        drop(console_lock);

        let mut console_lock = console.lock();
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.audio_file_path);
            //let path_valid = !self.audio_file_path.is_empty();
            if ui.button("Browse Audio").clicked() {
                self.audio_file_path.clear();
                if let Some(cue_id) = &self.selected_cue_list_index {
                    if let Some(path) = FileDialog::new()
                        .add_filter("Audio", &["mp3", "wav", "ogg", "flac"])
                        .set_title("Select Audio File")
                        .pick_file()
                    {
                        // if let Ok(mut audio_manager) = self.audio_manager.lock() {
                        //     let _ = audio_manager.add_track(path, cue_id.clone());
                        // }
                        self.audio_file_path = path.to_string_lossy().to_string();
                        let _ = console_lock
                            .cue_manager
                            .set_audio_file(cue_list_idx, self.audio_file_path.clone());
                    }
                } else {
                    // Show error or notification that no cue is selected
                    // TODO - show message modal
                    ui.label("Please select a cue first");
                }
            }
        });
        drop(console_lock);
    }

    fn render_cue_details(&mut self, ui: &mut egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        let console_lock = console.lock();
        // Cue details if selected
        if let Some(cue_idx) = self.selected_cue_index {
            let cue = console_lock.cue_manager.get_cue(cue_idx);

            if let Some(cue) = cue {
                ui.separator();
                ui.heading(format!("Cue {} Details", cue_idx + 1));

                // Create a collapsing region for this section
                egui::CollapsingHeader::new("Cue Properties")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label(format!("Static Values: {}", cue.static_values.len()));
                        ui.label(format!("Effects: {}", cue.effects.len()));

                        if ui.button("Edit Cue").clicked() {
                            // This would open the detailed cue editor
                            // The actual implementation would depend on how you want to handle
                            // navigation
                        }
                    });
            }
        }
        drop(console_lock);
    }
}
