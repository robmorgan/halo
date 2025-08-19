use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Ok;
use clap::Parser;
use halo_core::{ConsoleCommand, ConsoleEvent, ConsoleHandle, LightingConsole, CueList, MidiAction, MidiOverride, NetworkConfig};
use tokio::sync::mpsc;

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

    // Create channels for communication
    let (command_tx, command_rx) = mpsc::unbounded_channel::<ConsoleCommand>();
    let (event_tx, event_rx) = mpsc::unbounded_channel::<ConsoleEvent>();

    // Create the console handle for sending commands
    let console_handle = ConsoleHandle::new(command_tx.clone());

    // Create the async console
    let console = LightingConsole::new(80., network_config.clone()).unwrap();

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

    println!("Starting lighting console with channel-based communication...");
    println!("MIDI support: {}", args.enable_midi);
    println!("Show file: {:?}", args.show_file);

    // Spawn the console task with channel communication
    let console_handle_for_task = console_handle.clone();
    let console_task = tokio::spawn(async move {
        // Run the console with channels
        if let Err(e) = console.run_with_channels(command_rx, event_tx).await {
            log::error!("Console error: {}", e);
        }
    });

    // Send initialization commands
    console_handle.send_command(ConsoleCommand::Initialize)?;
    
    // Allow time for initialization
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Patch fixtures via commands
    console_handle.send_command(ConsoleCommand::PatchFixture {
        name: "Left PAR".to_string(),
        profile_name: "shehds-rgbw-par".to_string(),
        universe: 1,
        address: 1,
    })?;
    console_handle.send_command(ConsoleCommand::PatchFixture {
        name: "Right PAR".to_string(),
        profile_name: "shehds-rgbw-par".to_string(),
        universe: 1,
        address: 9,
    })?;
    console_handle.send_command(ConsoleCommand::PatchFixture {
        name: "Left Spot".to_string(),
        profile_name: "shehds-led-spot-60w".to_string(),
        universe: 1,
        address: 18,
    })?;
    console_handle.send_command(ConsoleCommand::PatchFixture {
        name: "Right Spot".to_string(),
        profile_name: "shehds-led-spot-60w".to_string(),
        universe: 1,
        address: 28,
    })?;
    console_handle.send_command(ConsoleCommand::PatchFixture {
        name: "Left Wash".to_string(),
        profile_name: "shehds-led-wash-7x18w-rgbwa-uv".to_string(),
        universe: 1,
        address: 38,
    })?;
    console_handle.send_command(ConsoleCommand::PatchFixture {
        name: "Right Wash".to_string(),
        profile_name: "shehds-led-wash-7x18w-rgbwa-uv".to_string(),
        universe: 1,
        address: 48,
    })?;
    console_handle.send_command(ConsoleCommand::PatchFixture {
        name: "Smoke #1".to_string(),
        profile_name: "dl-geyser-1000-led-smoke-machine-1000w-3x9w-rgb".to_string(),
        universe: 1,
        address: 69,
    })?;
    console_handle.send_command(ConsoleCommand::PatchFixture {
        name: "Pinspot".to_string(),
        profile_name: "shehds-mini-led-pinspot-10w".to_string(),
        universe: 1,
        address: 80,
    })?;

    // Set up cue lists
    let cue_lists = vec![CueList {
        name: "Default".to_string(),
        cues: vec![],
        audio_file: None,
    }];
    console_handle.send_command(ConsoleCommand::SetCueLists { cue_lists })?;

    // Launch the UI in the main thread with the channel-based console adapter
    // Temporarily disabled due to UI compilation issues
    // let ui_console_adapter = Arc::new(halo_ui::console_adapter::ConsoleAdapter::new(
    //     console_handle.clone(),
    //     event_rx,
    // ));
    // let ui_result = tokio::task::spawn_blocking(move || {
    //     halo_ui::run_ui(ui_console_adapter)
    // }).await;

    // For now, just wait for a bit then shutdown
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Send shutdown command
    console_handle.send_command(ConsoleCommand::Shutdown)?;
    
    // Wait for console task to finish
    let _ = console_task.await;

    log::info!("Application shutting down");
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
