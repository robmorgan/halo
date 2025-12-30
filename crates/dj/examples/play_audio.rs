//! Simple audio playback example for testing the DJ module.
//!
//! Usage: cargo run --package halo-dj --example play_audio <audio_file>

use std::env;
use std::thread;
use std::time::Duration;

use halo_dj::deck::DeckId;
use halo_dj::module::{AudioEngineConfig, DjAudioEngine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Get audio file from command line
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <audio_file>", args[0]);
        eprintln!("\nExample: cargo run --package halo-dj --example play_audio path/to/song.mp3");
        std::process::exit(1);
    }
    let audio_file = &args[1];

    println!("DJ Audio Playback Test");
    println!("======================");
    println!("File: {}", audio_file);

    // Create audio engine with default config (4-channel output)
    let config = AudioEngineConfig::default();
    println!(
        "\nAudio Config:\n  Sample Rate: {} Hz\n  Deck A: channels {}-{}\n  Deck B: channels {}-{}",
        config.sample_rate,
        config.deck_a_channels.0,
        config.deck_a_channels.1,
        config.deck_b_channels.0,
        config.deck_b_channels.1
    );

    let mut engine = DjAudioEngine::new(config);

    // Start the audio engine
    println!("\nStarting audio engine...");
    engine.start()?;
    println!("Audio engine started with {} output channels", engine.output_channels());

    // Load the audio file onto Deck A
    println!("\nLoading audio file onto Deck A...");
    {
        let mut player = engine.deck_player(DeckId::A).write();
        player.load(audio_file)?;
        println!(
            "Loaded: {} Hz, {} channels, {:.2}s duration",
            player.sample_rate(),
            player.channels(),
            player.duration_seconds()
        );
    }

    // Start playback
    println!("\nStarting playback...");
    engine.deck_player(DeckId::A).write().play();

    // Play for a while, showing position updates
    println!("\nPlaying (press Ctrl+C to stop)...\n");
    loop {
        let (position, duration, state) = {
            let player = engine.deck_player(DeckId::A).read();
            (player.position_seconds(), player.duration_seconds(), player.state())
        };

        print!(
            "\r  Position: {:02}:{:05.2} / {:02}:{:05.2}  [{:?}]  ",
            (position / 60.0) as u32,
            position % 60.0,
            (duration / 60.0) as u32,
            duration % 60.0,
            state
        );

        // Check if playback finished
        if position >= duration - 0.1 {
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
