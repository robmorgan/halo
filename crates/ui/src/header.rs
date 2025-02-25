use eframe::egui;
use std::sync::{Arc, Mutex};

use crate::ActiveTab;
use halo_core::LightingConsole;

pub fn render(ui: &mut eframe::egui::Ui, active_tab: &mut ActiveTab) {
    ui.menu_button("File", |ui| {
        if ui.button("New Show").clicked() {
            // TODO: Implement new show functionality
            // self.selected_fixture_index = None;
            // self.selected_cue_index = None;
            // self.selected_chase_index = None;
            // self.selected_step_index = None;

            // // Reset visualizer
            // self.visualizer_state = VisualizerState::new();
        }
        if ui.button("Save Show").clicked() {
            // TODO: Implement save functionality
        }
        if ui.button("Load Show").clicked() {
            // TODO: Implement load functionality
        }
    });
    ui.menu_button("View", |ui| {
        if ui.button("Visualizer Window").clicked() {
            // TODO: enable or remove the visualizer
            //self.show_visualizer_window = !self.show_visualizer_window;
        }
        if ui.button("Patch Panel").clicked() {
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
            .selectable_label(matches!(active_tab, ActiveTab::PatchPanel), "Patch Panel")
            .clicked()
        {
            *active_tab = ActiveTab::PatchPanel;
        }
        if ui
            .selectable_label(matches!(active_tab, ActiveTab::Visualizer), "Visualizer")
            .clicked()
        {
            *active_tab = ActiveTab::Visualizer;
        }
        if ui
            .selectable_label(matches!(active_tab, ActiveTab::CueEditor), "Cue Editor")
            .clicked()
        {
            *active_tab = ActiveTab::CueEditor;
        }
        if ui
            .selectable_label(matches!(active_tab, ActiveTab::Dashboard), "Dashboard")
            .clicked()
        {
            *active_tab = ActiveTab::Dashboard;
        }
    });
}
