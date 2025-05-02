use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};
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
    audio_file_path: Option<String>,
    editing_timecode_cue_index: Option<usize>,
    editing_timecode_value: String,
}

impl Default for CueEditor {
    fn default() -> Self {
        Self {
            selected_cue_list_index: Some(0),
            selected_cue_index: None,
            new_cue_list_name: String::new(),
            new_cue_name: String::new(),
            new_fade_time: 3.0,
            new_timecode: "00:00:00:00".to_string(),
            audio_file_path: None,
            editing_timecode_cue_index: None,
            editing_timecode_value: String::new(),
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
                            fade_time: Duration::from_secs_f64(self.new_fade_time),
                            timecode: if self.new_timecode.is_empty() {
                                None
                            } else {
                                Some(self.new_timecode.clone())
                            },
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
                let cue_list = {
                    let console_lock = console.lock();
                    console_lock.cue_manager.get_cue_list(cue_list_idx).cloned()
                };

                // Header
                ui.strong("ID");
                ui.strong("Name");
                ui.strong("Fade Time");
                ui.strong("Timecode");
                ui.strong("Static Values");
                ui.strong("Effects");
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
                        ui.label(format!("{:.1} s", cue.fade_time.as_secs_f64()));

                        // Check if this cue's timecode is being edited
                        if self.editing_timecode_cue_index == Some(idx) {
                            // Show text edit for editing the timecode
                            let response =
                                ui.text_edit_singleline(&mut self.editing_timecode_value);

                            // If user presses Enter or clicks elsewhere, save the changes
                            if response.lost_focus()
                                || ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                let mut console_lock = console.lock();
                                if let Some(cue_list) =
                                    console_lock.cue_manager.get_cue_list_mut(cue_list_idx)
                                {
                                    if idx < cue_list.cues.len() {
                                        // Update the timecode
                                        if self.editing_timecode_value.is_empty() {
                                            cue_list.cues[idx].timecode = None;
                                        } else {
                                            cue_list.cues[idx].timecode =
                                                Some(self.editing_timecode_value.clone());
                                        }
                                    }
                                }
                                drop(console_lock);

                                self.editing_timecode_cue_index = None;
                            }
                        } else {
                            // Display the timecode as a clickable label
                            let timecode_text = if let Some(tc) = &cue.timecode {
                                RichText::new(tc).color(Color32::from_rgb(0, 150, 255))
                            } else {
                                RichText::new("None").color(Color32::from_gray(120))
                            };

                            if ui
                                .add(egui::Label::new(timecode_text).sense(egui::Sense::click()))
                                .clicked()
                            {
                                // Start editing this timecode
                                self.editing_timecode_cue_index = Some(idx);
                                self.editing_timecode_value =
                                    cue.timecode.clone().unwrap_or_default();
                            }
                        }

                        ui.label(format!("{}", cue.static_values.len()));
                        ui.label(format!("{}", cue.effects.len()));
                        ui.end_row();
                    }
                }
            });
    }

    fn render_audio_section(
        &mut self,
        ui: &mut egui::Ui,
        cue_list_idx: usize,
        console: &Arc<Mutex<LightingConsole>>,
    ) {
        ui.separator();
        ui.heading("Audio File");

        let mut console_lock = console.lock();
        ui.horizontal(|ui| {
            if let Some(audio_file_path) = &self.audio_file_path {
                ui.label("Audio File:");
                ui.monospace(audio_file_path);
            }

            if ui.button("Browse Audio").clicked() {
                if let Some(cue_id) = &self.selected_cue_list_index {
                    if let Some(path) = FileDialog::new()
                        .add_filter("Audio", &["mp3", "wav", "ogg", "flac"])
                        .set_title("Select Audio File")
                        .pick_file()
                    {
                        // if let Ok(mut audio_manager) = self.audio_manager.lock() {
                        //     let _ = audio_manager.add_track(path, cue_id.clone());
                        // }
                        let audio_file_path = path.display().to_string();
                        self.audio_file_path = Some(audio_file_path.clone());
                        let _ = console_lock
                            .cue_manager
                            .set_audio_file(cue_list_idx, audio_file_path);
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
