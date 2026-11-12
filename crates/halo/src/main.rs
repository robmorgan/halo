mod app;
mod audio;
mod crossover;
mod deck;
mod decoder;
mod delivery;
mod dmx;
mod dsp;
mod fader;
mod footer;
mod key;
mod knob;
mod library;
mod midi;
mod options;
mod output_queue;
mod patch_view;
mod programmer_ui;
mod render_worker;
mod routing;
mod scrub;
mod show;
mod show_preview;
mod state;
mod sync;
mod theme;
mod waveform;
mod worker;
mod worker_priority;
mod z1;

/// Halo's icon, 512 px: the Dock's and the toolbar's logo.
const APP_ICON_PNG: &[u8] = include_bytes!("../icons/halo-512.png");

/// The Dock icon while Halo runs, `cargo run` included. A bundled Halo.app
/// also has it from `icons/Halo.icns` before it launches.
fn app_icon() -> eframe::egui::IconData {
    eframe::icon_data::from_png_bytes(APP_ICON_PNG).unwrap_or_else(|e| {
        log::warn!("app icon: {e}");
        Default::default()
    })
}

fn main() -> eframe::Result<()> {
    env_logger::init();

    // Optional audio file to load on startup (skips the file dialog).
    let initial_file = std::env::args().nth(1).map(std::path::PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 700.0])
            .with_min_inner_size([900.0, 550.0])
            .with_maximized(true)
            .with_title("Halo")
            .with_icon(app_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Halo",
        options,
        Box::new(|cc| Ok(Box::new(app::HaloApp::new(cc, initial_file)))),
    )
}
