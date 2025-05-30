use std::sync::Arc;

use eframe::egui;
use halo_core::LightingConsole;
use parking_lot::Mutex;

use crate::ActiveTab;

pub fn render(
    ui: &mut eframe::egui::Ui,
    active_tab: &mut ActiveTab,
    console: &Arc<Mutex<LightingConsole>>,
) {
    ui.menu_button("File", |ui| {
        if ui.button("New Show").clicked() {
            if let Some(path) = rfd::FileDialog::new().set_title("New Show").save_file() {
                let mut console_lock = console.lock();
                let name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let _ = console_lock.new_show(name);
                drop(console_lock);
            }
            ui.close_menu();
        }

        if ui.button("Open Show...").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Halo Show", &["json"])
                .set_title("Open Show")
                .pick_file()
            {
                let mut console_lock = console.lock();
                let _ = console_lock.load_show(&path);
                drop(console_lock);
            }
            ui.close_menu();
        }

        if ui.button("Reload Show").clicked() {
            let mut console_lock = console.lock();
            let _ = console_lock.reload_show();
            drop(console_lock);
        }

        if ui.button("Save Show").clicked() {
            let mut console_lock = console.lock();
            let _ = console_lock.save_show();
            drop(console_lock);
            ui.close_menu();
        }

        if ui.button("Save Show As...").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Halo Show", &["json"])
                .set_title("Save Show As")
                .save_file()
            {
                let mut console_lock = console.lock();
                let name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let _ = console_lock.save_show_as(name, path);
                drop(console_lock);
            }
            ui.close_menu();
        }

        ui.separator();

        if ui.button("Show Manager").clicked() {
            *active_tab = ActiveTab::ShowManager;
            ui.close_menu();
        }

        ui.separator();

        if ui.button("Quit").clicked() {
            // TODO - add are you sure? modal.
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    });
    ui.menu_button("View", |ui| {
        if ui.button("Patch").clicked() {
            *active_tab = ActiveTab::PatchPanel;
        }
    });
    ui.menu_button("Tools", |ui| {
        if ui.button("Ableton Link").clicked() {
            // TODO: Toggle Ableton Link
        }
        if ui.button("MIDI Settings").clicked() {
            // TODO: Open MIDI settings
        }
        if ui.button("DMX Settings").clicked() {
            // TODO: Open DMX settings
        }
    });
    // Tab selector
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui
            .selectable_label(matches!(active_tab, ActiveTab::ShowManager), "Shows")
            .clicked()
        {
            *active_tab = ActiveTab::ShowManager;
        }
        if ui
            .selectable_label(matches!(active_tab, ActiveTab::PatchPanel), "Patch")
            .clicked()
        {
            *active_tab = ActiveTab::PatchPanel;
        }
        if ui
            .selectable_label(matches!(active_tab, ActiveTab::CueEditor), "Cue Editor")
            .clicked()
        {
            *active_tab = ActiveTab::CueEditor;
        }
        if ui
            .selectable_label(matches!(active_tab, ActiveTab::Programmer), "Programmer")
            .clicked()
        {
            *active_tab = ActiveTab::Programmer;
        }
        if ui
            .selectable_label(matches!(active_tab, ActiveTab::Dashboard), "Dashboard")
            .clicked()
        {
            *active_tab = ActiveTab::Dashboard;
        }
    });
}
