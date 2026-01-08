//! Beat events example for DJ playback with rhythm tracking.
//!
//! This example demonstrates:
//! - Loading a track with beat grid analysis
//! - Tracking beat and bar phases during playback
//! - Detecting beat triggers (useful for lighting integration)
//!
//! Usage: cargo run --package halo-dj --example beat_events <audio_file>

use std::time::Duration;
use std::{env, thread};

use halo_dj::deck::DeckId;
use halo_dj::library::{AnalysisConfig, AnalysisResult, TrackId};
use halo_dj::module::{AudioEngineConfig, DjAudioEngine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Get audio file from command line
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <audio_file>", args[0]);
        eprintln!("\nExample: cargo run --package halo-dj --example beat_events path/to/song.mp3");
        std::process::exit(1);
    }
    let audio_file = &args[1];

    println!("DJ Beat Events Example");
    println!("======================");
    println!("File: {}\n", audio_file);

    // First, analyze the track for BPM and beat grid
    println!("Analyzing track for BPM...");
    let config = AnalysisConfig::default();
    let result: AnalysisResult =
        halo_dj::library::analysis::analyze_file(audio_file, TrackId(0), &config)?;
    let beat_grid = result.beat_grid;
    let bpm = result.bpm;
    println!(
        "Detected BPM: {:.2} (confidence: {:.1}%)",
        bpm,
        beat_grid.confidence * 100.0
    );
    println!(
        "First beat at: {:.3}s",
        beat_grid.first_beat_offset_ms / 1000.0
    );
    println!();

    // Create audio engine
    let config = AudioEngineConfig::default();
    let mut engine = DjAudioEngine::new(config);

    // Start the audio engine
    println!("Starting audio engine...");
    engine.start()?;

    // Load the audio file onto Deck A with beat grid
    println!("Loading audio file...");
    {
        let mut player = engine.deck_player(DeckId::A).write();
        player.load(audio_file)?;
        player.set_beat_grid(beat_grid.clone(), bpm);
        println!(
            "Loaded: {:.2}s @ {:.2} BPM\n",
            player.duration_seconds(),
            bpm
        );
    }

    // Start playback
    println!("Starting playback with beat tracking...\n");
    engine.deck_player(DeckId::A).write().play();

    // Track beats and display rhythm info
    let mut last_beat: Option<u64> = None;
    let beats_per_bar = 4;

    println!("Beat | Bar  | Beat Phase | Bar Phase | Phrase Phase | BPM");
    println!("-----|------|------------|-----------|--------------|------");

    loop {
        let (position, duration, state, beat_phase, bar_phase, phrase_phase, beat_num, bpm) = {
            let player = engine.deck_player(DeckId::A).read();
            (
                player.position_seconds(),
                player.duration_seconds(),
                player.state(),
                player.beat_phase(),
                player.bar_phase(),
                player.phrase_phase(),
                player.current_beat_number(),
                player.effective_bpm(),
            )
        };

        // Check for new beat
        if let Some(beat) = beat_num {
            if last_beat.map_or(true, |last| beat > last) {
                // New beat detected!
                let beat_in_bar = (beat % beats_per_bar as u64) + 1;
                let bar_number = (beat / beats_per_bar as u64) + 1;
                let is_downbeat = beat_in_bar == 1;

                // Print beat info
                println!(
                    "{:4} | {:4} |   {:5.3}    |   {:5.3}   |    {:5.3}     | {:6.2} {}",
                    beat,
                    bar_number,
                    beat_phase.unwrap_or(0.0),
                    bar_phase.unwrap_or(0.0),
                    phrase_phase.unwrap_or(0.0),
                    bpm.unwrap_or(0.0),
                    if is_downbeat { "**DOWNBEAT**" } else { "" }
                );

                last_beat = Some(beat);
            }
        }

        // Check if playback finished
        if position >= duration - 0.1 {
            println!("\nPlayback finished.");
            break;
        }

        // Check if stopped
        if state != halo_dj::module::PlayerState::Playing {
            println!("\nPlayback stopped.");
            break;
        }

        // Sleep briefly to not spam the output
        thread::sleep(Duration::from_millis(20));
    }

    // Stop the engine
    engine.stop();
    println!("Audio engine stopped.");

    Ok(())
}
