use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Ok;
use clap::Parser;
use halo_core::{LightingConsole, CueList, MidiAction, MidiOverride, NetworkConfig};
use parking_lot::Mutex;
use tokio::time::interval;

/// Lighting Console for live performances with precise automation and control.
#[derive(Parser, Debug)]
#[command(name = "halo")]
#[command(about = "Halo lighting console")]
struct Args {
    /// Art-Net Source IP address
    #[arg(long, value_parser = parse_ip)]
    source_ip: IpAddr,

    /// Art-Net Destination IP address (optional - if not provided, broadcast mode will be used)
    #[arg(long, value_parser = parse_ip)]
    dest_ip: Option<IpAddr>,

    /// Art-Net port (default: 6454)
    #[arg(long, default_value = "6454")]
    artnet_port: u16,

    /// Force broadcast mode even if destination IP is provided
    #[arg(long, default_value = "false")]
    broadcast: bool,

    /// Whether to enable MIDI support
    #[arg(short, long)]
    enable_midi: bool,

    /// Path to the show JSON file
    #[arg(long)]
    show_file: Option<String>,
}

fn parse_ip(s: &str) -> Result<IpAddr, String> {
    s.parse().map_err(|e| format!("Invalid IP address: {}", e))
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    let network_config = NetworkConfig::new(
        args.source_ip,
        args.dest_ip,
        args.artnet_port,
        args.broadcast,
    );

    println!("Configuring Halo with Art-Net settings:");
    //    println!("Source IP: {}", network_config.source_ip);
    println!("Mode: {}", network_config.get_mode_string());
    println!("Destination: {}", network_config.get_destination());
    println!("Port: {}", network_config.port);

    // Create the async console
    let mut console = LightingConsole::new(80., network_config.clone()).unwrap();
    console.load_fixture_library();

    // patch fixtures
    let _ = console.patch_fixture("Left PAR", "shehds-rgbw-par", 1, 1).await;
    let _ = console.patch_fixture("Right PAR", "shehds-rgbw-par", 1, 9).await;
    let _ = console.patch_fixture("Left Spot", "shehds-led-spot-60w", 1, 18).await;
    let _ = console.patch_fixture("Right Spot", "shehds-led-spot-60w", 1, 28).await;
    let _ = console.patch_fixture("Left Wash", "shehds-led-wash-7x18w-rgbwa-uv", 1, 38).await;
    let _ = console.patch_fixture("Right Wash", "shehds-led-wash-7x18w-rgbwa-uv", 1, 48).await;
    let _ = console.patch_fixture(
        "Smoke #1",
        "dl-geyser-1000-led-smoke-machine-1000w-3x9w-rgb",
        1,
        69,
    ).await;
    let _ = console.patch_fixture("Pinspot", "shehds-mini-led-pinspot-10w", 1, 80).await;

    // store the cues in a default cue list
    let cue_lists = vec![CueList {
        name: "Default".to_string(),
        cues: vec![],
        audio_file: None,
    }];

    // load cue lists
    console.set_cue_lists(cue_lists).await;

    // // Blue Strobe Fast
    // console.add_midi_override(
    //     76,
    //     MidiOverride {
    //         action: MidiAction::StaticValues(static_values![
    //             ("Smoke #1", "Blue", 255),
    //             ("Smoke #1", "Strobe", 255),
    //         ]),
    //     },
    // );

    // // Red Strobe Medium w/Half Smoke
    // console.add_midi_override(
    //     77,
    //     MidiOverride {
    //         action: MidiAction::StaticValues(static_values![
    //             ("Smoke #1", "Smoke", 100),
    //             ("Smoke #1", "Red", 255),
    //             ("Smoke #1", "Strobe", 220),
    //         ]),
    //     },
    // );

    // // Blue Strobe Fast w/Full Smoke
    // console.add_midi_override(
    //     78,
    //     MidiOverride {
    //         action: MidiAction::StaticValues(static_values![
    //             ("Smoke #1", "Smoke", 255),
    //             ("Smoke #1", "Blue", 255),
    //             ("Smoke #1", "Strobe", 255),
    //         ]),
    //     },
    // );

    // // Full Smoke
    // console.add_midi_override(
    //     71,
    //     MidiOverride {
    //         action: MidiAction::StaticValues(static_values![("Smoke #1", "Smoke", 255),]),
    //     },
    // );

    //// Cue Overrides

    // Initialize the async console and all modules
    console.initialize().await?;

    println!("Async lighting console initialized successfully!");
    println!("MIDI support: {}", args.enable_midi);
    println!("Show file: {:?}", args.show_file);

    // Create the main console update loop
    let console_arc = Arc::new(Mutex::new(console));
    let console_clone = Arc::clone(&console_arc);

    // Spawn the main lighting loop (replaces the old EventLoop)
    let lighting_handle = tokio::spawn(async move {
        let mut update_interval = interval(Duration::from_millis(23)); // ~44Hz
        
        loop {
            update_interval.tick().await;
            
            {
                let mut console = console_clone.lock();
                if !console.is_running() {
                    break;
                }
                
                if let Err(e) = console.update().await {
                    log::error!("Console update error: {}", e);
                }
            }
        }
        
        log::info!("Lighting loop shutting down");
    });

    // Launch the UI in the main thread with the async console
    // Temporarily disabled due to UI compilation issues
    // let ui_result = tokio::task::spawn_blocking(move || {
    //     halo_ui::run_ui(console_arc)
    // }).await;

    // Wait for lighting loop to finish
    lighting_handle.abort();
    let _ = lighting_handle.await;

    // Shutdown console
    // Note: This is simplified - in a real implementation you'd want proper shutdown handling
    log::info!("Application shutting down");

    // Temporarily return success since UI is disabled
    Ok(())
}

#[macro_export]
macro_rules! static_values {
    ($(($fixture:expr, $channel:expr, $value:expr)),* $(,)?) => {
        vec![
            $(
                StaticValue {
                    fixture_id: $fixture,
                    channel_type: $channel,
                    value: $value,
                },
            )*
        ]
    };
}
