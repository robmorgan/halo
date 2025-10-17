use eframe::egui;
use halo_core::{ConsoleCommand, CueList};
use tokio::sync::mpsc;

use crate::state::ConsoleState;

pub struct CueEditor {
    selected_cue_list_index: Option<usize>,
    selected_cue_index: Option<usize>,
    new_cue_list_name: String,
    new_cue_name: String,
    new_fade_time: f64,
    new_timecode: String,

    // Confirmation dialog state
    show_delete_cue_dialog: bool,
    show_delete_cue_list_dialog: bool,
    cue_to_delete: Option<(usize, usize)>, // (list_index, cue_index)
    cue_list_to_delete: Option<usize>,
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
            show_delete_cue_dialog: false,
            show_delete_cue_list_dialog: false,
            cue_to_delete: None,
            cue_list_to_delete: None,
        }
    }
}

impl CueEditor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn render(
        &mut self,
        ctx: &egui::Context,
        state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        egui::SidePanel::right("right_panel").show(ctx, |ui| {
            self.render_cue_lists_panel(ui, state, console_tx);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_cues_panel(ui, state, console_tx);
        });

        // Render confirmation dialogs
        self.render_confirmation_dialogs(ctx, state, console_tx);
    }

    fn render_confirmation_dialogs(
        &mut self,
        ctx: &egui::Context,
        state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        // Delete cue confirmation dialog
        if self.show_delete_cue_dialog {
            if let Some((list_idx, cue_idx)) = self.cue_to_delete {
                if let Some(cue_list) = state.cue_lists.get(list_idx) {
                    if let Some(cue) = cue_list.cues.get(cue_idx) {
                        egui::Window::new("Delete Cue")
                            .collapsible(false)
                            .resizable(false)
                            .show(ctx, |ui| {
                                ui.label(format!(
                                    "Are you sure you want to delete cue \"{}\"?",
                                    cue.name
                                ));
                                ui.label("This action cannot be undone.");

                                ui.horizontal(|ui| {
                                    if ui.button("Cancel").clicked() {
                                        self.show_delete_cue_dialog = false;
                                        self.cue_to_delete = None;
                                    }

                                    if ui.button("Delete").clicked() {
                                        let _ = console_tx.send(ConsoleCommand::DeleteCue {
                                            list_index: list_idx,
                                            cue_index: cue_idx,
                                        });
                                        self.show_delete_cue_dialog = false;
                                        self.cue_to_delete = None;
                                    }
                                });
                            });
                    }
                }
            }
        }

        // Delete cue list confirmation dialog
        if self.show_delete_cue_list_dialog {
            if let Some(list_idx) = self.cue_list_to_delete {
                if let Some(cue_list) = state.cue_lists.get(list_idx) {
                    egui::Window::new("Delete Cue List")
                        .collapsible(false)
                        .resizable(false)
                        .show(ctx, |ui| {
                            ui.label(format!(
                                "Are you sure you want to delete cue list \"{}\"?",
                                cue_list.name
                            ));
                            ui.label(format!(
                                "This will also delete all {} cues in this list.",
                                cue_list.cues.len()
                            ));
                            ui.label("This action cannot be undone.");

                            ui.horizontal(|ui| {
                                if ui.button("Cancel").clicked() {
                                    self.show_delete_cue_list_dialog = false;
                                    self.cue_list_to_delete = None;
                                }

                                if ui.button("Delete").clicked() {
                                    let _ = console_tx.send(ConsoleCommand::DeleteCueList {
                                        list_index: list_idx,
                                    });
                                    self.show_delete_cue_list_dialog = false;
                                    self.cue_list_to_delete = None;

                                    // Reset selection if we deleted the selected cue list
                                    if self.selected_cue_list_index == Some(list_idx) {
                                        self.selected_cue_list_index = None;
                                        self.selected_cue_index = None;
                                    }
                                }
                            });
                        });
                }
            }
        }
    }

    fn render_cue_lists_panel(
        &mut self,
        ui: &mut egui::Ui,
        state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
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
                    let _ = console_tx.send(ConsoleCommand::SetCueLists {
                        cue_lists: vec![CueList {
                            name: std::mem::take(&mut self.new_cue_list_name),
                            cues: Vec::new(),
                            audio_file: None,
                        }],
                    });
                }
            });

            ui.separator();

            // List of cue lists
            egui::ScrollArea::vertical().show(ui, |ui| {
                let cue_lists = &state.cue_lists;

                for (idx, cue_list) in cue_lists.iter().enumerate() {
                    let is_selected = self.selected_cue_list_index == Some(idx);

                    ui.horizontal(|ui| {
                        // Fixed width for cue list name
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(ui.available_width() - 30.0, 0.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                if ui.selectable_label(is_selected, &cue_list.name).clicked() {
                                    self.selected_cue_list_index = Some(idx);
                                    self.selected_cue_index = None; // Reset cue selection when
                                                                    // changing lists
                                }
                            },
                        );

                        // Fixed width for delete button
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(25.0, 0.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                if ui.button("🗑").clicked() {
                                    self.cue_list_to_delete = Some(idx);
                                    self.show_delete_cue_list_dialog = true;
                                }
                            },
                        );
                    });
                }
            });

            // Audio file section for selected cue list
            if let Some(cue_list_idx) = self.selected_cue_list_index {
                if let Some(cue_list) = state.cue_lists.get(cue_list_idx) {
                    ui.separator();
                    ui.heading("Audio File");

                    ui.horizontal(|ui| {
                        if let Some(audio_file) = &cue_list.audio_file {
                            // Extract filename from path
                            let filename = std::path::Path::new(audio_file)
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or(audio_file);

                            let label = ui.label(format!("📁 {}", filename));
                            label.on_hover_text(audio_file);
                        } else {
                            ui.label("No audio file selected");
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui.button("Browse").clicked() {
                            // TODO: Implement file picker
                            ui.label("File picker not yet implemented");
                        }

                        if ui.button("Clear").clicked() {
                            let _ = console_tx.send(ConsoleCommand::SetCueListAudioFile {
                                list_index: cue_list_idx,
                                audio_file: None,
                            });
                        }
                    });
                }
            }
        });
    }

    fn render_cues_panel(
        &mut self,
        ui: &mut egui::Ui,
        state: &ConsoleState,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        ui.vertical(|ui| {
            ui.heading("Cues");

            if let Some(cue_list_idx) = self.selected_cue_list_index {
                if let Some(cue_list) = state.cue_lists.get(cue_list_idx) {
                    // Add new cue
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.new_cue_name);
                        ui.label("Fade Time:");
                        ui.add(egui::DragValue::new(&mut self.new_fade_time).speed(0.1));
                        ui.label("Timecode:");
                        ui.text_edit_singleline(&mut self.new_timecode);

                        let name_valid = !self.new_cue_name.is_empty();
                        if ui
                            .add_enabled(name_valid, egui::Button::new("Add Cue"))
                            .clicked()
                        {
                            let _ = console_tx.send(ConsoleCommand::AddCue {
                                list_index: cue_list_idx,
                                name: std::mem::take(&mut self.new_cue_name),
                                fade_time: self.new_fade_time,
                                timecode: if self.new_timecode.is_empty() {
                                    None
                                } else {
                                    Some(std::mem::take(&mut self.new_timecode))
                                },
                                is_blocking: false,
                            });
                            // Reset the timecode field
                            self.new_timecode = "00:00:00:00".to_string();
                        }
                    });

                    ui.separator();

                    // Cue table
                    self.render_cue_table(ui, cue_list, cue_list_idx, console_tx);
                }
            } else {
                ui.label("Please select a cue list from the right panel");
            }
        });
    }

    fn render_cue_table(
        &mut self,
        ui: &mut egui::Ui,
        cue_list: &CueList,
        cue_list_idx: usize,
        console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
    ) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("cue_table")
                .num_columns(5)
                .spacing([20.0, 4.0])
                .show(ui, |ui| {
                    // Header row with fixed widths
                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(300.0, 0.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| ui.label("Name"),
                    );
                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(100.0, 0.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| ui.label("Fade Time (s)"),
                    );
                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(120.0, 0.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| ui.label("Timecode"),
                    );
                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(80.0, 0.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| ui.label("Blocking"),
                    );
                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(60.0, 0.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| ui.label("Actions"),
                    );
                    ui.end_row();

                    // Cue rows
                    for (idx, cue) in cue_list.cues.iter().enumerate() {
                        let mut cue_name = cue.name.clone();
                        let mut fade_time = cue.fade_time.as_secs_f64();
                        let mut timecode = cue.timecode.clone().unwrap_or_default();
                        let mut is_blocking = cue.is_blocking;

                        // Name column - lots of space
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(300.0, 0.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                if ui.text_edit_singleline(&mut cue_name).lost_focus() {
                                    if cue_name != cue.name {
                                        let _ = console_tx.send(ConsoleCommand::UpdateCue {
                                            list_index: cue_list_idx,
                                            cue_index: idx,
                                            name: cue_name.clone(),
                                            fade_time,
                                            timecode: if timecode.is_empty() {
                                                None
                                            } else {
                                                Some(timecode.clone())
                                            },
                                            is_blocking,
                                        });
                                    }
                                }
                            },
                        );

                        // Fade time column
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(100.0, 0.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                if ui
                                    .add(egui::DragValue::new(&mut fade_time).speed(0.1))
                                    .changed()
                                {
                                    let _ = console_tx.send(ConsoleCommand::UpdateCue {
                                        list_index: cue_list_idx,
                                        cue_index: idx,
                                        name: cue_name.clone(),
                                        fade_time,
                                        timecode: if timecode.is_empty() {
                                            None
                                        } else {
                                            Some(timecode.clone())
                                        },
                                        is_blocking,
                                    });
                                }
                            },
                        );

                        // Timecode column
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(120.0, 0.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                if ui.text_edit_singleline(&mut timecode).lost_focus() {
                                    let _ = console_tx.send(ConsoleCommand::UpdateCue {
                                        list_index: cue_list_idx,
                                        cue_index: idx,
                                        name: cue_name.clone(),
                                        fade_time,
                                        timecode: if timecode.is_empty() {
                                            None
                                        } else {
                                            Some(timecode.clone())
                                        },
                                        is_blocking,
                                    });
                                }
                            },
                        );

                        // Blocking column
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(80.0, 0.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                if ui.checkbox(&mut is_blocking, "").changed() {
                                    let _ = console_tx.send(ConsoleCommand::UpdateCue {
                                        list_index: cue_list_idx,
                                        cue_index: idx,
                                        name: cue_name.clone(),
                                        fade_time,
                                        timecode: if timecode.is_empty() {
                                            None
                                        } else {
                                            Some(timecode.clone())
                                        },
                                        is_blocking,
                                    });
                                }
                            },
                        );

                        // Actions column
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(60.0, 0.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                if ui.button("🗑").clicked() {
                                    self.cue_to_delete = Some((cue_list_idx, idx));
                                    self.show_delete_cue_dialog = true;
                                }
                            },
                        );

                        ui.end_row();
                    }
                });
        });
    }
}
