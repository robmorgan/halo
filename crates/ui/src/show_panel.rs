use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use halo_core::SyncLightingConsole as LightingConsole;
use parking_lot::Mutex;

pub struct ShowPanel {
    show_dialog_open: bool,
    load_dialog_open: bool,
    new_show_name: String,
    selected_show_path: Option<PathBuf>,
    available_shows: Vec<PathBuf>,
    show_names: Vec<String>,
    error_message: Option<String>,
}

impl Default for ShowPanel {
    fn default() -> Self {
        Self {
            show_dialog_open: false,
            load_dialog_open: false,
            new_show_name: String::new(),
            selected_show_path: None,
            available_shows: Vec::new(),
            show_names: Vec::new(),
            error_message: None,
        }
    }
}

impl ShowPanel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show(&mut self, ui: &mut egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        ui.heading("Show Management");

        ui.horizontal(|ui| {
            if ui.button("New Show").clicked() {
                self.show_dialog_open = true;
                self.new_show_name = String::new();
            }

            if ui.button("Save Show").clicked() {
                let mut console_lock = console.lock();
                if let Err(err) = console_lock.save_show() {
                    self.error_message = Some(format!("Error saving show: {}", err));
                }
            }

            if ui.button("Save As...").clicked() {
                // Use native file dialog
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Halo Show", &["json"])
                    .set_directory("~")
                    .save_file()
                {
                    let mut console_lock = console.lock();
                    if let Err(err) = console_lock.show_manager().load_show(&path) {
                        self.error_message = Some(format!("Error saving show: {}", err));
                    }
                }
            }

            if ui.button("Load Show").clicked() {
                self.load_dialog_open = true;
                // Refresh available shows
                let console_lock = console.lock();
                match console_lock.show_manager().list_shows() {
                    Ok(shows) => {
                        self.available_shows = shows;
                        self.show_names = self
                            .available_shows
                            .iter()
                            .map(|path| {
                                path.file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string()
                            })
                            .collect();
                    }
                    Err(err) => {
                        self.error_message = Some(format!("Error listing shows: {}", err));
                    }
                }
            }
        });

        // Show error if any
        if let Some(err) = &self.error_message {
            ui.label(egui::RichText::new(err).color(egui::Color32::RED));
            if ui.button("Dismiss").clicked() {
                self.error_message = None;
            }
        }

        // New show dialog
        if self.show_dialog_open {
            egui::Window::new("New Show")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Show Name:");
                        ui.text_edit_singleline(&mut self.new_show_name);
                    });

                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            let mut console_lock = console.lock();
                            if let Err(err) = console_lock.new_show(self.new_show_name.clone()) {
                                self.error_message = Some(format!("Error creating show: {}", err));
                            } else {
                                self.show_dialog_open = false;
                            }
                        }

                        if ui.button("Cancel").clicked() {
                            self.show_dialog_open = false;
                        }
                    });
                });
        }

        // Load show dialog
        if self.load_dialog_open {
            egui::Window::new("Load Show")
                .collapsible(false)
                .resizable(true)
                .show(ui.ctx(), |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (idx, name) in self.show_names.iter().enumerate() {
                            if ui
                                .selectable_label(
                                    self.selected_show_path.as_ref()
                                        == Some(&self.available_shows[idx]),
                                    name,
                                )
                                .clicked()
                            {
                                self.selected_show_path = Some(self.available_shows[idx].clone());
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        let load_enabled = self.selected_show_path.is_some();
                        if ui
                            .add_enabled(load_enabled, egui::Button::new("Load"))
                            .clicked()
                        {
                            if let Some(path) = &self.selected_show_path {
                                let mut console_lock = console.lock();
                                if let Err(err) = console_lock.show_manager().load_show(path) {
                                    self.error_message =
                                        Some(format!("Error loading show: {}", err));
                                } else {
                                    self.load_dialog_open = false;
                                }
                            }
                        }

                        if ui.button("Browse...").clicked() {
                            // Use native file dialog for more options
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Halo Show", &["json"])
                                .set_directory("~")
                                .pick_file()
                            {
                                self.selected_show_path = Some(path);
                            }
                        }

                        if ui.button("Cancel").clicked() {
                            self.load_dialog_open = false;
                        }
                    });
                });
        }
    }
}
