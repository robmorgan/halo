mod app;
mod audio;
mod deck;
mod decoder;
mod dmx;
mod dsp;
mod fader;
mod knob;
mod library;
mod programmer_ui;
mod scrub;
mod show;
mod show_preview;
mod state;
mod waveform;
mod worker;

fn main() -> eframe::Result<()> {
    env_logger::init();

    // Optional audio file to load on startup (skips the file dialog).
    let initial_file = std::env::args().nth(1).map(std::path::PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 700.0])
            .with_min_inner_size([900.0, 550.0])
            .with_maximized(true)
            .with_title("Halo"),
        ..Default::default()
    };

    eframe::run_native(
        "Halo",
        options,
        Box::new(|cc| Ok(Box::new(app::HaloApp::new(cc, initial_file)))),
    )
}
