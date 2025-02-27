use eframe::egui;

use crate::ActiveTab;

fn render(ui: &mut eframe::egui::Ui, theme: &mut eframe::Theme) {
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
        ui.horizontal(|ui| {
            ui.label("Halo v0.1");
            ui.separator();
            ui.label("Made with ❤️ by your name");
        });
    });

}

/// Delete everything below. I'm just leaving it here as a reference.
///
// fn draw_footer(&self, ui: &mut Ui, bg_color: Color32, text_dim: Color32) {
// pub fn render(ui: &mut egui::Ui, active_tab: &mut ActiveTab) {
//     ui.menu_button("Help", |ui| {
//         if ui.button("About").clicked() {
//             // TODO: Implement about functionality
//         }
//         if ui.button("Documentation").clicked() {
//             // TODO: Implement documentation functionality
//         }
//         if ui.button("Support").clicked() {
//             // TODO: Implement support functionality
//         }
//     });
//     ui.menu_button("Window", |ui| {
//         if ui.button("Dashboard").clicked() {
//             *active_tab = ActiveTab::Dashboard;
//         }
//         if ui.button("Cue Editor").clicked() {
//             *active_tab = ActiveTab::CueEditor;
//         }
//         if ui.button("Visualizer").clicked() {
//             *active_tab = ActiveTab::Visualizer;
//         }
//         if ui.button("Patch Panel").clicked() {
//             *active_tab = ActiveTab::PatchPanel;
//         }
//     });
// }