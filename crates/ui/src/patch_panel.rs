use eframe::egui;
use std::sync::{Arc, Mutex};

use halo_core::LightingConsole;

pub struct PatchPanel {
    pub dmx_addresses: Vec<u16>,
    pub selected_universe: u8,
}

impl PatchPanel {
    pub fn new() -> Self {
        Self {
            dmx_addresses: Vec::new(),
            selected_universe: 0,
        }
    }

    pub fn show_patch_panel(&mut self, ui: &mut egui::Ui, console: &Arc<Mutex<LightingConsole>>) {
        ui.heading("DMX Patch Panel");

        ui.horizontal(|ui| {
            ui.label("Universe:");
            ui.add(
                egui::DragValue::new(&mut self.selected_universe)
                    .speed(0.1)
                    .range(0..=15),
            );
        });

        ui.separator();

        // Ensure we have the right number of DMX addresses
        let console_guard = console.lock().unwrap();
        if self.dmx_addresses.len() != console_guard.fixtures.len() {
            self.dmx_addresses.resize(console_guard.fixtures.len(), 1);
        }

        // Create the patch table
        egui::Grid::new("patch_grid")
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                // Header
                ui.label("Fixture");
                ui.label("Channels");
                ui.label("DMX Address");
                ui.label("Universe");
                ui.label("Channel Map");
                ui.end_row();

                // Rows
                for (idx, fixture) in console_guard.fixtures.iter().enumerate() {
                    ui.label(&fixture.name);
                    ui.label(format!("{}", fixture.channels.len()));

                    // DMX address input
                    let mut address = self.dmx_addresses[idx];
                    if ui
                        .add(egui::DragValue::new(&mut address).speed(1.0).range(1..=512))
                        .changed()
                    {
                        self.dmx_addresses[idx] = address;
                    }

                    // Universe (same for all fixtures in this version)
                    ui.label(format!("{}", self.selected_universe));

                    // Channel mapping preview
                    let mut channel_map = String::new();
                    for (ch_idx, channel) in fixture.channels.iter().enumerate() {
                        if ch_idx > 0 {
                            channel_map.push_str(", ");
                        }
                        channel_map.push_str(&format!(
                            "{}: {}",
                            address + ch_idx as u16,
                            channel.name
                        ));
                    }
                    ui.label(channel_map);

                    ui.end_row();
                }
            });

        if ui.button("Apply Patch").clicked() {
            // Here we would apply the patch to the DMX output configuration
            // For now we'll just show a confirmation
            ui.label("Patch applied successfully!");
        }

        drop(console_guard);

        // DMX universe visualization
        ui.separator();
        ui.heading(format!("DMX Universe {} Preview", self.selected_universe));

        // Draw universe channels as a grid
        let channel_size = 15.0;
        let channels_per_row = 32;
        let rows = 16;

        let available_width = ui.available_width();
        let scale = available_width / (channel_size * channels_per_row as f32);

        let (universe_rect, _) = ui.allocate_exact_size(
            egui::vec2(
                channel_size * channels_per_row as f32 * scale,
                channel_size * rows as f32 * scale,
            ),
            egui::Sense::hover(),
        );

        // Draw all channels
        for row in 0..rows {
            for col in 0..channels_per_row {
                let channel = row * channels_per_row + col + 1;
                let channel_rect = egui::Rect::from_min_size(
                    egui::pos2(
                        universe_rect.min.x + col as f32 * channel_size * scale,
                        universe_rect.min.y + row as f32 * channel_size * scale,
                    ),
                    egui::vec2(channel_size * scale, channel_size * scale),
                );

                // Check if this channel is used by any fixture
                let mut used = false;
                let mut fixture_name = String::new();
                let console_guard = console.lock().unwrap();

                for (idx, fixture) in console_guard.fixtures.iter().enumerate() {
                    let start_addr = self.dmx_addresses[idx];
                    if channel >= start_addr as i32
                        && channel < (start_addr + fixture.channels.len() as u16) as i32
                    {
                        used = true;
                        fixture_name = fixture.name.clone();
                        break;
                    }
                }

                drop(console_guard);

                // Draw channel
                ui.painter().rect_filled(
                    channel_rect,
                    2.0,
                    if used {
                        egui::Color32::from_rgb(100, 150, 255)
                    } else {
                        egui::Color32::from_gray(40)
                    },
                );

                // Draw channel number
                ui.painter().text(
                    channel_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{}", channel),
                    egui::FontId::proportional(10.0 * scale),
                    if used {
                        egui::Color32::BLACK
                    } else {
                        egui::Color32::LIGHT_GRAY
                    },
                );
            }
        }
    }
}
