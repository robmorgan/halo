use std::time::Duration;

use eframe::egui::{self, Color32, RichText};
use halo_core::{ConsoleCommand, Cue, CueList};
use tokio::sync::mpsc;

use crate::state::ConsoleState;

pub struct CueEditor {
    selected_cue_list_index: Option<usize>,
    selected_cue_index: Option<usize>,
    new_cue_list_name: String,
    new_cue_name: String,
    new_fade_time: f64,
    new_timecode: String,
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
                    if ui.selectable_label(is_selected, &cue_list.name).clicked() {
                        self.selected_cue_list_index = Some(idx);
                        self.selected_cue_index = None; // Reset cue selection when changing lists
                    }
                }
            });
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
                            // TODO: Implement cue creation via message passing
                            ui.label("Cue creation not yet implemented");
                        }
                    });

                    ui.separator();

                    // List of cues
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (idx, cue) in cue_list.cues.iter().enumerate() {
                            let is_selected = self.selected_cue_index == Some(idx);
                            if ui.selectable_label(is_selected, &cue.name).clicked() {
                                self.selected_cue_index = Some(idx);
                            }
                        }
                    });

                    // Cue details if selected
                    if let Some(cue_idx) = self.selected_cue_index {
                        if let Some(cue) = cue_list.cues.get(cue_idx) {
                            ui.separator();
                            ui.heading(format!("Cue {} Details", cue_idx + 1));

                            egui::CollapsingHeader::new("Cue Properties")
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.label(format!("Static Values: {}", cue.static_values.len()));
                                    ui.label(format!("Effects: {}", cue.effects.len()));
                                    ui.label(format!(
                                        "Fade Time: {:.1}s",
                                        cue.fade_time.as_secs_f64()
                                    ));

                                    if let Some(timecode) = &cue.timecode {
                                        ui.label(format!("Timecode: {}", timecode));
                                    }
                                });
                        }
                    }
                }
            } else {
                ui.label("Please select a cue list from the right panel");
            }
        });
    }
}

pub fn render(
    ui: &mut eframe::egui::Ui,
    state: &ConsoleState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    let mut cue_editor = CueEditor::default();
    cue_editor.render(ui.ctx(), state, console_tx);
}
