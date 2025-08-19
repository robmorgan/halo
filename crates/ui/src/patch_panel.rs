use std::sync::Arc;

use eframe::egui;
use halo_core::SyncLightingConsole as LightingConsole;
use halo_fixtures::{Fixture, FixtureLibrary};
use parking_lot::Mutex;

pub struct PatchPanel {
    dmx_addresses: Vec<u16>,
    selected_universe: u8,
    search_text: String,
    profile_filter: String,
    available_profiles: Vec<String>,
    new_fixture_data: NewFixtureData,
    is_adding_fixture: bool,
}

/// Data structure for creating a new fixture
struct NewFixtureData {
    name: String,
    profile_name: String,
    universe: u8,
    start_address: u16,
}

impl PatchPanel {
    pub fn new() -> Self {
        Self {
            dmx_addresses: Vec::new(),
            selected_universe: 1,
            search_text: String::new(),
            profile_filter: String::new(),
            available_profiles: Vec::new(),
            new_fixture_data: NewFixtureData {
                name: String::new(),
                profile_name: String::new(),
                universe: 1,
                start_address: 1,
            },
            is_adding_fixture: false,
        }
    }

    /// Initialize or update available fixture profiles
    pub fn update_available_profiles(&mut self) {
        let fixture_library = FixtureLibrary::new();
        self.available_profiles = fixture_library.profiles.keys().map(|k| k.clone()).collect();
    }

    /// Process changes in the DMX addresses
    fn handle_dmx_address_change(&mut self, fixture_idx: usize, new_address: u16) {
        if fixture_idx < self.dmx_addresses.len() {
            self.dmx_addresses[fixture_idx] = new_address;
        }
    }

    /// Apply patches to the console
    fn apply_patch(&mut self, console: &Arc<Mutex<LightingConsole>>) {
        let mut console_lock = console.lock();

        // Update DMX addresses for existing fixtures
        for (idx, address) in self.dmx_addresses.iter().enumerate() {
            if idx < console_lock.fixtures.len() {
                let fixture = &mut console_lock.fixtures[idx];
                fixture.start_address = *address;
                fixture.universe = self.selected_universe;
            }
        }
    }

    /// Add a new fixture to the console
    fn add_new_fixture(&mut self, console: &Arc<Mutex<LightingConsole>>) -> Result<(), String> {
        let mut console_lock = console.lock();

        console_lock.patch_fixture(
            &self.new_fixture_data.name,
            &self.new_fixture_data.profile_name,
            self.new_fixture_data.universe,
            self.new_fixture_data.start_address,
        )?;

        // Reset the new fixture form
        self.new_fixture_data.name = String::new();
        self.new_fixture_data.start_address =
            self.find_next_available_address(&console_lock.fixtures);

        // Update local DMX addresses
        if self.dmx_addresses.len() < console_lock.fixtures.len() {
            self.dmx_addresses.resize(console_lock.fixtures.len(), 1);
        }

        drop(console_lock);

        Ok(())
    }

    /// Find the next available DMX address
    fn find_next_available_address(&self, fixtures: &[Fixture]) -> u16 {
        let mut used_addresses = Vec::new();

        // Collect all used addresses for the selected universe
        for fixture in fixtures {
            if fixture.universe == self.selected_universe {
                let start = fixture.start_address;
                let end = start + fixture.channels.len() as u16 - 1;
                for addr in start..=end {
                    used_addresses.push(addr);
                }
            }
        }

        // Find the first available address
        for addr in 1..=512 {
            if !used_addresses.contains(&addr) {
                return addr;
            }
        }

        // If everything is used (unlikely), return 1
        1
    }

    /// Draw the patch panel UI
    pub fn render(&mut self, ctx: &egui::Context, console: &Arc<Mutex<LightingConsole>>) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("DMX Patch Panel");

            // Top controls
            ui.horizontal(|ui| {
                ui.label("Universe:");
                ui.add(
                    egui::DragValue::new(&mut self.selected_universe)
                        .speed(0.1)
                        .range(1..=255),
                );

                // Add fixture button
                if ui.button("Add Fixture").clicked() {
                    self.is_adding_fixture = true;
                    self.update_available_profiles();

                    // Initialize new fixture data
                    let console_lock = console.lock();
                    self.new_fixture_data.universe = self.selected_universe;
                    self.new_fixture_data.start_address =
                        self.find_next_available_address(&console_lock.fixtures);
                    drop(console_lock);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Search box
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut self.search_text);
                });
            });

            ui.separator();

            // Ensure we have the right number of DMX addresses
            let mut console_lock = console.lock();
            if self.dmx_addresses.len() != console_lock.fixtures.len() {
                self.dmx_addresses.resize(console_lock.fixtures.len(), 1);
                for (i, fixture) in console_lock.fixtures.iter().enumerate() {
                    self.dmx_addresses[i] = fixture.start_address;
                }
            }
            drop(console_lock);

            // "Add Fixture" dialog
            if self.is_adding_fixture {
                self.show_add_fixture_dialog(ui, console);
            }

            // Create the patch table
            let table_height = ui.available_height() - 40.0; // Reserve space for bottom buttons
            egui::ScrollArea::vertical()
                .max_height(table_height)
                .show(ui, |ui| {
                    egui::Grid::new("patch_grid")
                        .striped(true)
                        .spacing([10.0, 4.0])
                        .min_col_width(80.0)
                        .show(ui, |ui| {
                            // Header
                            ui.strong("ID");
                            ui.strong("Fixture");
                            ui.strong("Profile");
                            ui.strong("Channels");
                            ui.strong("DMX Address");
                            ui.strong("Universe");
                            ui.strong("Channel Map");
                            ui.strong("Actions");
                            ui.end_row();

                            // Filter fixtures by search text
                            let search_lowercase = self.search_text.to_lowercase();

                            // Rows
                            let console_lock = console.lock();
                            for (idx, fixture) in console_lock.fixtures.iter().enumerate() {
                                // Skip if doesn't match search or isn't in current universe
                                if (!self.search_text.is_empty()
                                    && !fixture.name.to_lowercase().contains(&search_lowercase))
                                    || fixture.universe != self.selected_universe
                                {
                                    continue;
                                }

                                // Fixture ID
                                ui.label(format!("{}", idx));

                                // Fixture name
                                ui.label(&fixture.name);

                                // Profile name
                                ui.label(&fixture.profile.to_string());

                                // Channels count
                                ui.label(format!("{}", fixture.channels.len()));

                                // DMX address input
                                let mut address = self.dmx_addresses[idx];
                                let address_response = ui.add(
                                    egui::DragValue::new(&mut address).speed(1.0).range(1..=512),
                                );

                                if address_response.changed() {
                                    self.handle_dmx_address_change(idx, address);
                                }

                                // Universe (currently same for all fixtures in this view)
                                ui.label(format!("{}", self.selected_universe));

                                // Channel mapping preview
                                let end_address = address + fixture.channels.len() as u16 - 1;
                                let channel_map = format!(
                                    "{}-{} ({})",
                                    address,
                                    end_address,
                                    fixture.channels.len()
                                );
                                ui.label(channel_map);

                                // Actions
                                ui.horizontal(|ui| {
                                    if ui.button("Details").clicked() {
                                        // Show modal with channel details
                                        // This would be implemented in a real app
                                    }

                                    // Delete button with confirmation
                                    if ui.button("🗑").on_hover_text("Delete fixture").clicked() {
                                        // This would delete the fixture in a real app
                                        // You would need to handle this in the console
                                    }
                                });

                                ui.end_row();

                                // Optional: Show expanded channel details
                                // This could be toggled by the Details button
                            }
                            drop(console_lock);
                        });
                });

            ui.separator();

            // Bottom buttons
            ui.horizontal(|ui| {
                if ui.button("Apply Patch").clicked() {
                    self.apply_patch(console);
                    ui.label("Patch applied successfully!");
                }

                if ui.button("Auto-Address").clicked() {
                    // Auto-address would arrange fixtures sequentially
                    // without overlapping addresses
                    let console_lock = console.lock();
                    self.auto_address_fixtures(&console_lock.fixtures);
                    drop(console_lock);
                }
            });

            // DMX universe visualization
            ui.separator();
            ui.heading(format!("DMX Universe {} Overview", self.selected_universe));

            // Draw universe channels as a grid
            let console_lock = console.lock();
            self.draw_dmx_universe_visualization(ui, &console_lock.fixtures);
            drop(console_lock);
        });
    }

    fn show_add_fixture_dialog(
        &mut self,
        ui: &mut egui::Ui,
        console: &Arc<Mutex<LightingConsole>>,
    ) {
        egui::Window::new("Add New Fixture")
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Fixture Name:");
                    ui.text_edit_singleline(&mut self.new_fixture_data.name);
                });

                ui.horizontal(|ui| {
                    ui.label("Profile:");

                    // Filter for profiles
                    ui.text_edit_singleline(&mut self.profile_filter)
                        .on_hover_text("Type to filter profiles");
                });

                // Profile selector
                let filter_lower = self.profile_filter.to_lowercase();
                let filtered_profiles: Vec<_> = self
                    .available_profiles
                    .iter()
                    .filter(|p| filter_lower.is_empty() || p.to_lowercase().contains(&filter_lower))
                    .collect();

                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for profile in filtered_profiles {
                            if ui
                                .selectable_label(
                                    self.new_fixture_data.profile_name == *profile,
                                    profile,
                                )
                                .clicked()
                            {
                                self.new_fixture_data.profile_name = profile.clone();
                            }
                        }
                    });

                ui.horizontal(|ui| {
                    ui.label("Universe:");
                    ui.add(
                        egui::DragValue::new(&mut self.new_fixture_data.universe)
                            .speed(1.0)
                            .range(1..=255),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("DMX Address:");
                    ui.add(
                        egui::DragValue::new(&mut self.new_fixture_data.start_address)
                            .speed(1.0)
                            .range(1..=512),
                    );
                });

                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.is_adding_fixture = false;
                    }

                    if self.new_fixture_data.name.is_empty()
                        || self.new_fixture_data.profile_name.is_empty()
                    {
                        ui.add_enabled(false, egui::Button::new("Add Fixture"));
                    } else {
                        if ui.button("Add Fixture").clicked() {
                            match self.add_new_fixture(console) {
                                Ok(_) => {
                                    self.is_adding_fixture = false;
                                }
                                Err(e) => {
                                    // Show error somewhere in the UI
                                    println!("Error adding fixture: {}", e);
                                }
                            }
                        }
                    }
                });
            });
    }

    fn auto_address_fixtures(&mut self, fixtures: &[Fixture]) {
        // Sort fixtures by their current order in the universe
        let mut fixtures_in_universe: Vec<(usize, &Fixture)> = fixtures
            .iter()
            .enumerate()
            .filter(|(_, f)| f.universe == self.selected_universe)
            .collect();

        fixtures_in_universe.sort_by_key(|(_, f)| f.start_address);

        // Assign sequential addresses
        let mut next_address: u16 = 1;

        for (idx, _) in fixtures_in_universe {
            self.dmx_addresses[idx] = next_address;
            let channel_count = fixtures[idx].channels.len() as u16;
            next_address += channel_count;

            // If we would overflow to the next universe, wrap back
            if next_address > 512 {
                // In a real implementation, you might want to handle this differently
                next_address = 1;
            }
        }
    }

    fn draw_dmx_universe_visualization(&self, ui: &mut egui::Ui, fixtures: &[Fixture]) {
        // Draw universe channels as a grid
        let channel_size = 15.0;
        let channels_per_row = 32;
        let rows = 16;

        let available_width = ui.available_width();
        let scale = available_width / (channel_size * channels_per_row as f32);

        // Calculate used channels for coloring
        let mut used_channels = vec![None; 512];
        for (idx, fixture) in fixtures.iter().enumerate() {
            if fixture.universe != self.selected_universe {
                continue;
            }

            let start_addr = self.dmx_addresses[idx] as usize - 1; // 0-based indexing
            let end_addr = (start_addr + fixture.channels.len()).min(512);

            for i in start_addr..end_addr {
                used_channels[i] = Some((fixture.name.clone(), i - start_addr));
            }
        }

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
                let channel_idx = row * channels_per_row + col;
                let dmx_addr = channel_idx + 1; // DMX addresses are 1-based

                let channel_rect = egui::Rect::from_min_size(
                    egui::pos2(
                        universe_rect.min.x + col as f32 * channel_size * scale,
                        universe_rect.min.y + row as f32 * channel_size * scale,
                    ),
                    egui::vec2(channel_size * scale, channel_size * scale),
                );

                // Check if this channel is used by any fixture
                let (color, text_color) =
                    if let Some((fixture_name, channel_num)) = &used_channels[channel_idx] {
                        (
                            egui::Color32::from_rgb(100, 150, 255), // Blue for used channels
                            egui::Color32::BLACK,
                        )
                    } else {
                        (
                            egui::Color32::from_gray(40), // Dark gray for unused
                            egui::Color32::LIGHT_GRAY,
                        )
                    };

                // Draw channel
                ui.painter().rect_filled(channel_rect, 2.0, color);

                // Draw channel number
                ui.painter().text(
                    channel_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{}", dmx_addr),
                    egui::FontId::proportional(10.0 * scale),
                    text_color,
                );

                // Optional: Show tooltip on hover with fixture and channel info
                if let Some((fixture_name, channel_num)) = &used_channels[channel_idx] {
                    if ui.rect_contains_pointer(channel_rect) {
                        let fixture = fixtures.iter().find(|f| &f.name == fixture_name).unwrap();

                        let channel_info = if *channel_num < fixture.channels.len() {
                            let channel = &fixture.channels[*channel_num];
                            format!("{}: {}", channel.name, channel.value)
                        } else {
                            "Unknown channel".to_string()
                        };

                        egui::show_tooltip(
                            ui.ctx(),
                            ui.layer_id(),
                            egui::Id::new("dmx_tooltip"),
                            |ui| {
                                ui.label(format!("Fixture: {}", fixture_name));
                                ui.label(channel_info);
                            },
                        );
                    }
                }
            }
        }
    }
}
