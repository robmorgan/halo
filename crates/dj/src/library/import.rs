//! Audio file import and metadata extraction.

use std::fs::{self, File};
use std::path::Path;

use symphonia::core::codecs::CODEC_TYPE_NULL;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::analysis::{analyze_file, AnalysisConfig, AnalysisResult};
use super::database::LibraryDatabase;
use super::types::{AudioFormat, Track, TrackId};

/// Result of importing and analyzing a track.
#[derive(Debug)]
pub struct ImportResult {
    /// The imported track with database ID.
    pub track: Track,
    /// Analysis result (if analysis was performed).
    pub analysis: Option<AnalysisResult>,
}

/// Import a file, add it to the database, and optionally analyze it.
///
/// This is the primary function for adding new tracks to the library.
/// It handles:
/// 1. Extracting metadata from the audio file
/// 2. Inserting the track into the database
/// 3. Running BPM/beat-grid analysis
/// 4. Storing analysis results (beat grid, waveform)
/// 5. Updating the track's BPM from analysis
pub fn import_and_analyze_file<P: AsRef<Path>>(
    path: P,
    db: &LibraryDatabase,
    run_analysis: bool,
) -> Result<ImportResult, anyhow::Error> {
    let path = path.as_ref();

    // Check if track already exists in database
    let path_str = path.to_string_lossy().to_string();
    if let Some(existing) = db.get_track_by_path(&path_str)? {
        log::info!("Track already in library: {:?}", path);
        // Get existing analysis if available
        let beat_grid = db.get_beat_grid(existing.id)?;
        let waveform = db.get_waveform(existing.id)?;
        let analysis = beat_grid.map(|bg| AnalysisResult {
            beat_grid: bg,
            waveform: waveform.unwrap_or_else(|| super::types::TrackWaveform {
                track_id: existing.id,
                samples: vec![],
                sample_count: 0,
                duration_seconds: existing.duration_seconds,
            }),
        });
        return Ok(ImportResult {
            track: existing,
            analysis,
        });
    }

    // Import file metadata
    let track = import_file(path)?;

    // Insert into database
    let track_id = db.insert_track(&track)?;
    log::info!("Inserted track with ID: {}", track_id);

    // Get the track back with the correct ID
    let mut track = db.get_track(track_id)?.ok_or_else(|| {
        anyhow::anyhow!("Failed to retrieve inserted track")
    })?;

    // Run analysis if requested
    let analysis = if run_analysis {
        log::info!("Running analysis on track: {}", track.title);
        let config = AnalysisConfig::default();

        match analyze_file(path, track_id, &config) {
            Ok(result) => {
                // Save beat grid to database
                if let Err(e) = db.save_beat_grid(&result.beat_grid) {
                    log::warn!("Failed to save beat grid: {}", e);
                }

                // Save waveform to database
                if let Err(e) = db.save_waveform(&result.waveform) {
                    log::warn!("Failed to save waveform: {}", e);
                }

                // Update track BPM from analysis
                if let Err(e) = db.update_track_bpm(track_id, result.beat_grid.bpm) {
                    log::warn!("Failed to update track BPM: {}", e);
                } else {
                    track.bpm = Some(result.beat_grid.bpm);
                }

                log::info!(
                    "Analysis complete: BPM={:.1} (confidence={:.2})",
                    result.beat_grid.bpm,
                    result.beat_grid.confidence
                );

                Some(result)
            }
            Err(e) => {
                log::warn!("Analysis failed for {}: {}", track.title, e);
                None
            }
        }
    } else {
        None
    };

    Ok(ImportResult { track, analysis })
}

/// Import and analyze all audio files from a directory.
pub fn import_and_analyze_directory<P: AsRef<Path>>(
    path: P,
    db: &LibraryDatabase,
    run_analysis: bool,
    recursive: bool,
) -> Vec<Result<ImportResult, anyhow::Error>> {
    let path = path.as_ref();
    log::info!(
        "Importing directory: {:?} (recursive: {}, analyze: {})",
        path,
        recursive,
        run_analysis
    );

    let mut results = Vec::new();

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();

            if entry_path.is_dir() {
                if recursive {
                    results.extend(import_and_analyze_directory(
                        &entry_path,
                        db,
                        run_analysis,
                        true,
                    ));
                }
            } else if is_supported_audio_file(&entry_path) {
                results.push(import_and_analyze_file(&entry_path, db, run_analysis));
            }
        }
    }

    let success_count = results.iter().filter(|r| r.is_ok()).count();
    log::info!(
        "Imported {} of {} files from {:?}",
        success_count,
        results.len(),
        path
    );
    results
}

/// Import a single audio file and extract metadata.
pub fn import_file<P: AsRef<Path>>(path: P) -> Result<Track, anyhow::Error> {
    let path = path.as_ref();
    log::info!("Importing file: {:?}", path);

    // Get file metadata
    let metadata = fs::metadata(path)?;
    let file_size = metadata.len();

    // Determine format from extension
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let format = AudioFormat::from_extension(extension);

    if format == AudioFormat::Unknown {
        return Err(anyhow::anyhow!("Unsupported audio format: {}", extension));
    }

    // Get title from filename
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    // Open and probe the file
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    hint.with_extension(extension);

    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;

    let mut format_reader = probed.format;

    // Find the first audio track
    let track_info = format_reader
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow::anyhow!("No audio track found"))?;

    let codec_params = &track_info.codec_params;

    // Extract audio parameters
    let sample_rate = codec_params.sample_rate.unwrap_or(44100);
    let channels = codec_params.channels.map(|c| c.count()).unwrap_or(2) as u8;
    let bit_depth = codec_params.bits_per_sample.unwrap_or(16) as u16;
    let n_frames = codec_params.n_frames.unwrap_or(0);

    // Calculate duration
    let duration_seconds = n_frames as f64 / sample_rate as f64;

    // Extract metadata (title, artist, album)
    let mut title = file_name;
    let mut artist = None;
    let mut album = None;

    // Check for metadata in the format
    if let Some(metadata) = format_reader.metadata().current() {
        for tag in metadata.tags() {
            match tag.std_key {
                Some(symphonia::core::meta::StandardTagKey::TrackTitle) => {
                    title = tag.value.to_string();
                }
                Some(symphonia::core::meta::StandardTagKey::Artist) => {
                    artist = Some(tag.value.to_string());
                }
                Some(symphonia::core::meta::StandardTagKey::Album) => {
                    album = Some(tag.value.to_string());
                }
                _ => {}
            }
        }
    }

    let mut track = Track::new(
        TrackId(0), // Will be set by database
        path.to_string_lossy().to_string(),
        title,
        duration_seconds,
        format,
        sample_rate,
    );

    track.artist = artist;
    track.album = album;
    track.bit_depth = bit_depth;
    track.channels = channels;
    track.file_size_bytes = file_size;

    log::info!(
        "Imported: {} by {:?} ({:.1}s, {} Hz)",
        track.title,
        track.artist,
        track.duration_seconds,
        track.sample_rate
    );

    Ok(track)
}

/// Import all audio files from a directory.
pub fn import_directory<P: AsRef<Path>>(
    path: P,
    recursive: bool,
) -> Vec<Result<Track, anyhow::Error>> {
    let path = path.as_ref();
    log::info!("Importing directory: {:?} (recursive: {})", path, recursive);

    let mut results = Vec::new();

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();

            if entry_path.is_dir() {
                if recursive {
                    results.extend(import_directory(&entry_path, true));
                }
            } else if is_supported_audio_file(&entry_path) {
                results.push(import_file(&entry_path));
            }
        }
    }

    log::info!("Imported {} files from {:?}", results.len(), path);
    results
}

/// Check if a file is a supported audio format.
pub fn is_supported_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            matches!(
                ext.to_lowercase().as_str(),
                "mp3" | "wav" | "aiff" | "aif" | "flac" | "m4a" | "aac" | "ogg"
            )
        })
        .unwrap_or(false)
}

/// Supported audio file extensions.
pub fn supported_extensions() -> &'static [&'static str] {
    &["mp3", "wav", "aiff", "aif", "flac", "m4a", "aac", "ogg"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported_audio_file() {
        assert!(is_supported_audio_file(Path::new("/path/to/song.mp3")));
        assert!(is_supported_audio_file(Path::new("/path/to/song.MP3")));
        assert!(is_supported_audio_file(Path::new("/path/to/song.wav")));
        assert!(is_supported_audio_file(Path::new("/path/to/song.aiff")));
        assert!(is_supported_audio_file(Path::new("/path/to/song.aif")));
        assert!(!is_supported_audio_file(Path::new("/path/to/song.txt")));
        assert!(!is_supported_audio_file(Path::new("/path/to/song")));
    }

    #[test]
    fn test_audio_format_from_extension() {
        assert_eq!(AudioFormat::from_extension("mp3"), AudioFormat::Mp3);
        assert_eq!(AudioFormat::from_extension("wav"), AudioFormat::Wav);
        assert_eq!(AudioFormat::from_extension("aiff"), AudioFormat::Aiff);
        assert_eq!(AudioFormat::from_extension("xyz"), AudioFormat::Unknown);
    }
}
