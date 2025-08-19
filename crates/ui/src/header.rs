use std::sync::Arc;

use crate::console_adapter::ConsoleAdapter;
use crate::ActiveTab;
use eframe::egui;

pub fn render(
    ui: &mut eframe::egui::Ui,
    active_tab: &mut ActiveTab,
    console: &Arc<ConsoleAdapter>,
) {
    ui.menu_button("File", |ui| {
        if ui.button("New Show").clicked() {
            if let Some(path) = rfd::FileDialog::new().set_title("New Show").save_file() {
                let name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let _ = console.new_show(name);
            }
            ui.close();
        }

        if ui.button("Open Show...").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Halo Show", &["json"])
                .set_title("Open Show")
                .pick_file()
            {
                let _ = console.load_show(path);
            }
            ui.close();
        }

        if ui.button("Reload Show").clicked() {
            let _ = console.reload_show();
        }

        if ui.button("Save Show").clicked() {
            let _ = console.save_show();
            ui.close();
        }

        if ui.button("Save Show As...").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Halo Show", &["json"])
                .set_title("Save Show As")
                .save_file()
            {
                let name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let _ = console.save_show_as(name, path);
            }
            ui.close();
        }

        ui.separator();

        if ui.button("Show Manager").clicked() {
            *active_tab = ActiveTab::ShowManager;
            ui.close();
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
