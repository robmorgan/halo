use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Ok;
use clap::Parser;
use halo_core::{
    sawtooth_effect, sine_effect, square_effect, Cue, CueList, Effect, EffectParams, Interval,
    LightingConsole, MidiAction, MidiOverride, NetworkConfig, StaticValue,
};
use parking_lot::Mutex;

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
}

fn parse_ip(s: &str) -> Result<IpAddr, String> {
    s.parse().map_err(|e| format!("Invalid IP address: {}", e))
}

fn main() -> Result<(), anyhow::Error> {
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

    // Create the console
    let mut console = LightingConsole::new(80., network_config.clone()).unwrap();
    console.load_fixture_library();

    // patch fixtures
    let _ = console.patch_fixture("Left PAR", "shehds-rgbw-par", 1, 1);
    let _ = console.patch_fixture("Right PAR", "shehds-rgbw-par", 1, 9);
    let _ = console.patch_fixture("Left Spot", "shehds-led-spot-60w", 1, 18);
    let _ = console.patch_fixture("Right Spot", "shehds-led-spot-60w", 1, 28);
    let _ = console.patch_fixture("Left Wash", "shehds-led-wash-7x18w-rgbwa-uv", 1, 38);
    let _ = console.patch_fixture("Right Wash", "shehds-led-wash-7x18w-rgbwa-uv", 1, 48);
    let _ = console.patch_fixture(
        "Smoke #1",
        "dl-geyser-1000-led-smoke-machine-1000w-3x9w-rgb",
        1,
        69,
    );
    let _ = console.patch_fixture("Pinspot", "shehds-mini-led-pinspot-10w", 1, 80);

    // store the cues in a default cue list
    let cue_lists = vec![CueList {
        name: "Default".to_string(),
        cues: vec![
            Cue {
                id: 0,
                name: "Arm".to_string(),
                fade_time: Duration::from_secs(3),
                is_blocking: false,
                timecode: Some("00:00:00:00".to_string()),
                static_values: vec![],
                effects: vec![],
            },
            Cue {
                id: 1,
                name: "Cue 1".to_string(),
                fade_time: Duration::from_secs(3),
                is_blocking: false,
                timecode: Some("00:00:01:00".to_string()),
                static_values: vec![],
                effects: vec![],
            },
            Cue {
                id: 2,
                name: "Cue 2".to_string(),
                fade_time: Duration::from_secs(5),
                is_blocking: false,
                timecode: Some("00:00:10:00".to_string()),
                static_values: vec![],
                effects: vec![],
            },
            Cue {
                id: 3,
                name: "Cue 3".to_string(),
                fade_time: Duration::from_secs(5),
                is_blocking: false,
                timecode: Some("00:00:15:00".to_string()),
                static_values: vec![],
                effects: vec![],
            },
        ],
        audio_file: None,
    }];

    // load cue lists
    console.set_cue_lists(cue_lists);

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

    // Cue 5: Pinspot Purple
    console.add_midi_override(
        62,
        MidiOverride {
            action: MidiAction::TriggerCue("Pinspot Purple".to_string()),
        },
    );

    // Cue 6: Pinspot Gradient
    console.add_midi_override(
        64,
        MidiOverride {
            action: MidiAction::TriggerCue("Pinspot Gradient".to_string()),
        },
    );

    // Check if MIDI support is enabled
    if args.enable_midi {
        console.init_mpk49_midi()?;
    }

    // Launch the UI in the main thread
    let _ = halo_ui::run_ui(Arc::new(Mutex::new(console)));
    Ok(())
}

#[macro_export]
macro_rules! static_values {
    ($(($fixture:expr, $channel:expr, $value:expr)),* $(,)?) => {
        vec![
            $(
                StaticValue {
                    fixture_name: $fixture.to_string(),
                    channel_name: $channel.to_string(),
                    value: $value,
                },
            )*
        ]
    };
}
