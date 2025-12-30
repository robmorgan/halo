//! Multi-channel audio test for the DJ module.
//!
//! Tests 4-channel output with separate routing for each deck.
//! Designed for testing with the Motu M4 or similar multi-channel interface.
//!
//! Usage:
//!   cargo run --package halo-dj --example multichannel_test -- --list-devices
//!   cargo run --package halo-dj --example multichannel_test -- --device "MOTU M4" <file_a> [file_b]
//!   cargo run --package halo-dj --example multichannel_test -- <file_a> [file_b]

use std::env;
use std::thread;
use std::time::Duration;

use halo_dj::deck::DeckId;
use halo_dj::module::{list_audio_devices, AudioEngineConfig, DjAudioEngine, PlayerState};

fn print_usage(program: &str) {
    eprintln!("Multi-Channel DJ Audio Test");
    eprintln!("============================");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  {} --list-devices", program);
    eprintln!("  {} [--device <name>] <file_a> [file_b]", program);
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --list-devices    List available audio output devices");
    eprintln!("  --device <name>   Select audio device by name (partial match)");
    eprintln!();
    eprintln!("Channel Routing:");
    eprintln!("  Deck A -> Outputs 1-2 (channels 0-1)");
    eprintln!("  Deck B -> Outputs 3-4 (channels 2-3)");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  {} --list-devices", program);
    eprintln!("  {} song.mp3", program);
    eprintln!("  {} --device \"MOTU M4\" song_a.mp3 song_b.mp3", program);
}

fn list_devices() {
    println!("Available Audio Output Devices:");
    println!("================================");

    let devices = list_audio_devices();
    if devices.is_empty() {
        println!("  No audio output devices found!");
        return;
    }

    for (i, device) in devices.iter().enumerate() {
        let default_marker = if device.is_default { " (default)" } else { "" };
        println!(
            "  [{}] {} - {} channels{}",
            i, device.name, device.max_channels, default_marker
        );
    }

    println!();
    println!("Note: For 4-channel output (DJ mode), select a device with 4+ channels.");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = env::args().collect();
    let program = &args[0];

    if args.len() < 2 {
        print_usage(program);
        std::process::exit(1);
    }

    // Parse arguments
    let mut device_name = String::new();
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--list-devices" | "-l" => {
                list_devices();
                return Ok(());
            }
            "--device" | "-d" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --device requires a device name");
                    std::process::exit(1);
                }
                device_name = args[i].clone();
            }
            "--help" | "-h" => {
                print_usage(program);
                return Ok(());
            }
            arg if arg.starts_with('-') => {
                eprintln!("Unknown option: {}", arg);
                print_usage(program);
                std::process::exit(1);
            }
            _ => {
                files.push(args[i].clone());
            }
        }
        i += 1;
    }

    if files.is_empty() {
        eprintln!("Error: At least one audio file is required");
        print_usage(program);
        std::process::exit(1);
    }

    println!("Multi-Channel DJ Audio Test");
    println!("============================");

    // Create audio engine config
    let mut config = AudioEngineConfig::default();
    config.device_name = device_name.clone();

    println!("\nConfiguration:");
    println!("  Device: {}", if device_name.is_empty() { "default" } else { &device_name });
    println!("  Sample Rate: {} Hz", config.sample_rate);
    println!("  Deck A: channels {}-{} (outputs 1-2)", config.deck_a_channels.0, config.deck_a_channels.1);
    println!("  Deck B: channels {}-{} (outputs 3-4)", config.deck_b_channels.0, config.deck_b_channels.1);

    // Create and start the audio engine
    let mut engine = DjAudioEngine::new(config);

    println!("\nStarting audio engine...");
    engine.start()?;
    println!("Audio engine started with {} output channels", engine.output_channels());

    if engine.output_channels() < 4 {
        println!("\nWARNING: Device has only {} channels.", engine.output_channels());
        println!("         Deck B may not output correctly (needs channels 2-3).");
        println!("         Consider using a multi-channel audio interface like Motu M4.");
    }

    // Load Deck A
    println!("\n--- Deck A ---");
    println!("Loading: {}", files[0]);
    {
        let mut player = engine.deck_player(DeckId::A).write();
        player.load(&files[0])?;
        println!(
            "Loaded: {} Hz, {} ch, {:.2}s",
            player.sample_rate(),
            player.channels(),
            player.duration_seconds()
        );
    }

    // Load Deck B if a second file is provided
    let has_deck_b = files.len() > 1;
    if has_deck_b {
        println!("\n--- Deck B ---");
        println!("Loading: {}", files[1]);
        {
            let mut player = engine.deck_player(DeckId::B).write();
            player.load(&files[1])?;
            println!(
                "Loaded: {} Hz, {} ch, {:.2}s",
                player.sample_rate(),
                player.channels(),
                player.duration_seconds()
            );
        }
    }

    // Start playback on both decks
    println!("\n--- Starting Playback ---");
    engine.deck_player(DeckId::A).write().play();
    if has_deck_b {
        engine.deck_player(DeckId::B).write().play();
    }

    println!("Playing (press Ctrl+C to stop)...\n");

    // Display status loop
    loop {
        let (pos_a, dur_a, state_a) = {
            let player = engine.deck_player(DeckId::A).read();
            (player.position_seconds(), player.duration_seconds(), player.state())
        };

        let (pos_b, dur_b, state_b) = if has_deck_b {
            let player = engine.deck_player(DeckId::B).read();
            (player.position_seconds(), player.duration_seconds(), player.state())
        } else {
            (0.0, 0.0, PlayerState::Empty)
        };

        // Format time as MM:SS.ss
        let fmt_time = |secs: f64| -> String {
            format!("{:02}:{:05.2}", (secs / 60.0) as u32, secs % 60.0)
        };

        print!(
            "\r  A: {} / {} [{:?}]",
            fmt_time(pos_a),
            fmt_time(dur_a),
            state_a
        );

        if has_deck_b {
            print!(
                "  |  B: {} / {} [{:?}]",
                fmt_time(pos_b),
                fmt_time(dur_b),
                state_b
            );
        }
        print!("    ");

        // Check if all playback finished
        let a_done = state_a != PlayerState::Playing;
        let b_done = !has_deck_b || state_b != PlayerState::Playing;

        if a_done && b_done {
            println!("\n\nPlayback finished.");
            break;
        }

        thread::sleep(Duration::from_millis(100));
    }

    // Stop the engine
    engine.stop();
    println!("Audio engine stopped.");

    Ok(())
}
