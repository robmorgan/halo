use std::net::IpAddr;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use halo_core::{
    ConfigManager, ConsoleCommand, ConsoleEvent, CueList, LightingConsole, NetworkConfig, Settings,
};
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
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Load configuration before initializing anything else
    println!("Loading configuration...");
    let mut config_manager = ConfigManager::new(None);
    let settings = match config_manager.load() {
        Ok(settings) => {
            println!(
                "Configuration loaded successfully from: {:?}",
                config_manager.config_path()
            );
            settings
        }
        Err(e) => {
            println!(
                "Warning: Failed to load configuration: {}. Using defaults.",
                e
            );
            Settings::default()
        }
    };

    // Apply CLI overrides to settings if provided
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
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<ConsoleEvent>();

    // Convert tokio receiver to std receiver for UI
    let (ui_event_tx, ui_event_rx) = std::sync::mpsc::channel::<ConsoleEvent>();

    // Spawn a task to forward events from tokio to std channel
    let event_forwarder = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Err(e) = ui_event_tx.send(event) {
                log::error!("Failed to forward event to UI: {}", e);
                break;
            }
        }
        log::info!("Event forwarder task completed");
    });

    // Create the async console with loaded settings
    let console =
        LightingConsole::new_with_settings(80., network_config.clone(), settings.clone()).unwrap();

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

    println!("Starting lighting console...");
    println!("MIDI support: {}", args.enable_midi);
    println!("Show file: {:?}", args.show_file);

    // Create a command sender for the initialization task
    let init_command_tx = command_tx.clone();

    // Spawn the console task with channel communication
    let console_task = tokio::spawn(async move {
        // Run the console with channels
        if let Err(e) = console.run_with_channels(command_rx, event_tx).await {
            println!("Console error: {}", e);
        }
    });

    // Store the show file path for later loading after UI starts
    let show_file_path = args.show_file.clone();

    // Spawn an initialization task to send all the setup commands
    let init_task = tokio::spawn(async move {
        println!("Starting initialization task...");

        // Send initialization commands
        println!("Sending Initialize command...");
        init_command_tx
            .send(ConsoleCommand::Initialize)
            .map_err(|e| anyhow::anyhow!("Failed to send Initialize command: {}", e))?;

        // Allow time for initialization
        println!("Waiting for initialization...");
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Patch fixtures via commands
        println!("Patching fixtures...");
        init_command_tx
            .send(ConsoleCommand::PatchFixture {
                name: "Left PAR".to_string(),
                profile_name: "shehds-rgbw-par".to_string(),
                universe: 1,
                address: 1,
            })
            .map_err(|e| anyhow::anyhow!("Failed to send PatchFixture command: {}", e))?;
        init_command_tx
            .send(ConsoleCommand::PatchFixture {
                name: "Right PAR".to_string(),
                profile_name: "shehds-rgbw-par".to_string(),
                universe: 1,
                address: 9,
            })
            .map_err(|e| anyhow::anyhow!("Failed to send PatchFixture command: {}", e))?;
        init_command_tx
            .send(ConsoleCommand::PatchFixture {
                name: "Left Spot".to_string(),
                profile_name: "shehds-led-spot-60w".to_string(),
                universe: 1,
                address: 18,
            })
            .map_err(|e| anyhow::anyhow!("Failed to send PatchFixture command: {}", e))?;
        init_command_tx
            .send(ConsoleCommand::PatchFixture {
                name: "Right Spot".to_string(),
                profile_name: "shehds-led-spot-60w".to_string(),
                universe: 1,
                address: 28,
            })
            .map_err(|e| anyhow::anyhow!("Failed to send PatchFixture command: {}", e))?;
        init_command_tx
            .send(ConsoleCommand::PatchFixture {
                name: "Left Wash".to_string(),
                profile_name: "shehds-led-wash-7x18w-rgbwa-uv".to_string(),
                universe: 1,
                address: 38,
            })
            .map_err(|e| anyhow::anyhow!("Failed to send PatchFixture command: {}", e))?;
        init_command_tx
            .send(ConsoleCommand::PatchFixture {
                name: "Right Wash".to_string(),
                profile_name: "shehds-led-wash-7x18w-rgbwa-uv".to_string(),
                universe: 1,
                address: 48,
            })
            .map_err(|e| anyhow::anyhow!("Failed to send PatchFixture command: {}", e))?;
        init_command_tx
            .send(ConsoleCommand::PatchFixture {
                name: "Smoke #1".to_string(),
                profile_name: "dl-geyser-1000-led-smoke-machine-1000w-3x9w-rgb".to_string(),
                universe: 1,
                address: 69,
            })
            .map_err(|e| anyhow::anyhow!("Failed to send PatchFixture command: {}", e))?;
        init_command_tx
            .send(ConsoleCommand::PatchFixture {
                name: "Pinspot".to_string(),
                profile_name: "shehds-mini-led-pinspot-10w".to_string(),
                universe: 1,
                address: 80,
            })
            .map_err(|e| anyhow::anyhow!("Failed to send PatchFixture command: {}", e))?;

        // Set up cue lists
        println!("Setting up cue lists...");
        let cue_lists = vec![CueList {
            name: "Default".to_string(),
            cues: vec![],
            audio_file: None,
        }];
        init_command_tx
            .send(ConsoleCommand::SetCueLists { cue_lists })
            .map_err(|e| anyhow::anyhow!("Failed to send SetCueLists command: {}", e))?;

        println!("Initialization task completed successfully");
        anyhow::Ok(())
    });

    // Wait for initialization to complete
    log::info!("Waiting for initialization to complete...");
    let init_result = init_task.await;
    if let Err(e) = init_result {
        log::error!("Initialization task join error: {}", e);
        return Err(anyhow::anyhow!("Initialization task failed: {}", e));
    }
    if let Err(e) = init_result.unwrap() {
        log::error!("Initialization task error: {}", e);
        return Err(e);
    }
    log::info!("Initialization completed successfully");

    // Run the UI with the channels (this will block until UI closes)
    log::info!("Starting UI...");
    let show_path = show_file_path.map(std::path::PathBuf::from);
    let ui_result = halo_ui::run_ui(command_tx.clone(), ui_event_rx, show_path, config_manager);
    log::info!("UI completed");

    // Send shutdown command
    log::info!("Sending shutdown command...");
    command_tx
        .send(ConsoleCommand::Shutdown)
        .map_err(|e| anyhow::anyhow!("Failed to send Shutdown command: {}", e))?;

    // Wait for console task to finish
    log::info!("Waiting for console task to finish...");
    let _ = console_task.await;

    // Wait for event forwarder task to finish
    log::info!("Waiting for event forwarder task to finish...");
    let _ = event_forwarder.await;

    // Check UI result
    if let Err(e) = ui_result {
        log::error!("UI error: {}", e);
    }

    log::info!("Application shutting down");
    anyhow::Ok(())
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
