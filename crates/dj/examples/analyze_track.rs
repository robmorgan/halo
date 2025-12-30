//! Track analysis example for testing BPM detection.
//!
//! This example demonstrates importing a track into the library
//! and running BPM/beat-grid analysis.
//!
//! Usage:
//!   cargo run --package halo-dj --example analyze_track <audio_file>
//!   cargo run --package halo-dj --example analyze_track <directory> --dir

use std::env;
use std::path::Path;

use halo_dj::library::{
    import_and_analyze_directory, import_and_analyze_file, is_supported_audio_file,
    LibraryDatabase,
};

fn print_usage(program: &str) {
    eprintln!("Track Analysis Example");
    eprintln!("======================");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  {} <audio_file>              Analyze a single file", program);
    eprintln!("  {} <directory> --dir         Analyze all files in directory", program);
    eprintln!("  {} <directory> --dir -r      Analyze recursively", program);
    eprintln!();
    eprintln!("The library database is stored at: ~/.halo/library.db");
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
    let path = &args[1];
    let is_dir = args.iter().any(|a| a == "--dir" || a == "-d");
    let recursive = args.iter().any(|a| a == "-r" || a == "--recursive");

    if path == "--help" || path == "-h" {
        print_usage(program);
        return Ok(());
    }

    println!("Track Analysis Example");
    println!("======================");

    // Get or create library path
    let library_path = dirs::home_dir()
        .map(|h| h.join(".halo").join("library.db"))
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

    // Create library directory if needed
    if let Some(parent) = library_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    println!("\nLibrary: {}", library_path.display());

    // Open database
    let db = LibraryDatabase::open(&library_path)?;
    println!("Database opened successfully");

    let path = Path::new(path);

    if is_dir {
        // Analyze directory
        if !path.is_dir() {
            eprintln!("Error: {} is not a directory", path.display());
            std::process::exit(1);
        }

        println!("\nAnalyzing directory: {}", path.display());
        println!("Recursive: {}", recursive);
        println!();

        let results = import_and_analyze_directory(path, &db, true, recursive);

        let mut success_count = 0;
        let mut fail_count = 0;

        for result in results {
            match result {
                Ok(import_result) => {
                    success_count += 1;
                    let track = &import_result.track;
                    let bpm_str = track.bpm.map_or("N/A".to_string(), |b| format!("{:.1}", b));
                    println!(
                        "  [OK] {} - {} ({:.1}s, BPM: {})",
                        track.artist.as_deref().unwrap_or("Unknown"),
                        track.title,
                        track.duration_seconds,
                        bpm_str
                    );
                }
                Err(e) => {
                    fail_count += 1;
                    eprintln!("  [FAIL] {}", e);
                }
            }
        }

        println!();
        println!("Summary: {} successful, {} failed", success_count, fail_count);
    } else {
        // Analyze single file
        if !path.exists() {
            eprintln!("Error: File not found: {}", path.display());
            std::process::exit(1);
        }

        if !is_supported_audio_file(path) {
            eprintln!("Error: Unsupported audio format");
            std::process::exit(1);
        }

        println!("\nAnalyzing: {}", path.display());
        println!();

        let start_time = std::time::Instant::now();
        let result = import_and_analyze_file(path, &db, true)?;
        let elapsed = start_time.elapsed();

        let track = &result.track;
        println!("Track Information:");
        println!("  Title:       {}", track.title);
        println!(
            "  Artist:      {}",
            track.artist.as_deref().unwrap_or("Unknown")
        );
        println!(
            "  Album:       {}",
            track.album.as_deref().unwrap_or("Unknown")
        );
        println!("  Duration:    {:.2}s", track.duration_seconds);
        println!("  Sample Rate: {} Hz", track.sample_rate);
        println!("  Channels:    {}", track.channels);
        println!("  Format:      {}", track.format.as_str());
        println!("  File Size:   {} bytes", track.file_size_bytes);

        if let Some(analysis) = &result.analysis {
            println!();
            println!("Analysis Results:");
            println!("  BPM:           {:.2}", analysis.beat_grid.bpm);
            println!("  Confidence:    {:.2}%", analysis.beat_grid.confidence * 100.0);
            println!(
                "  First Beat:    {:.2}ms",
                analysis.beat_grid.first_beat_offset_ms
            );
            println!("  Beat Count:    {}", analysis.beat_grid.beat_positions.len());
            println!(
                "  Waveform:      {} samples",
                analysis.waveform.sample_count
            );

            // Print first few beat positions
            if !analysis.beat_grid.beat_positions.is_empty() {
                println!();
                println!("First 8 beat positions (seconds):");
                for (i, &pos) in analysis.beat_grid.beat_positions.iter().take(8).enumerate() {
                    println!("  Beat {}: {:.3}s", i + 1, pos);
                }
            }
        } else {
            println!();
            println!("Analysis: FAILED");
        }

        println!();
        println!("Analysis completed in {:.2}s", elapsed.as_secs_f64());
    }

    // Print library statistics
    println!();
    println!("Library Statistics:");
    println!("  Total Tracks: {}", db.track_count()?);

    Ok(())
}
