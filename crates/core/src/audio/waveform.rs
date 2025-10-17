use std::path::Path;

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Debug, Clone)]
pub struct WaveformData {
    pub samples: Vec<f32>,
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub bpm: Option<f64>,
}

impl WaveformData {
    pub fn new(
        samples: Vec<f32>,
        duration_seconds: f64,
        sample_rate: u32,
        bpm: Option<f64>,
    ) -> Self {
        Self {
            samples,
            duration_seconds,
            sample_rate,
            bpm,
        }
    }
}

pub fn analyze_audio_file<P: AsRef<Path>>(path: P) -> Result<WaveformData, String> {
    let path = path.as_ref();

    // Create a media source from the file
    let file = std::fs::File::open(path).map_err(|e| format!("Failed to open audio file: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    // Create a probe hint using the file's extension
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(extension);
    }

    // Use the default options for metadata and format readers
    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();

    // Probe the media source
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .map_err(|e| format!("Failed to probe audio file: {e}"))?;

    let mut format = probed.format;
    let _metadata = probed.metadata;

    // Get the default track
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or("No supported audio tracks found")?;

    // Create a decoder for the track
    let decoder_opts: DecoderOptions = Default::default();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &decoder_opts)
        .map_err(|e| format!("Failed to create decoder: {e}"))?;

    // Get track info
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let duration = track
        .codec_params
        .n_frames
        .map(|frames| frames as f64 / sample_rate as f64);

    // Extract BPM from metadata if available
    let bpm = None;
    // Skip BPM extraction for now - metadata API is complex
    // TODO: Implement proper BPM extraction from metadata

    // Read and decode audio data
    let mut samples = Vec::new();
    let mut decoded_samples = 0;
    let target_samples = 2000; // Target number of samples for visualization

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::ResetRequired) => {
                // The track list has changed and the user must select a new track
                return Err("Track list changed during decoding".to_string());
            }
            Err(symphonia::core::errors::Error::IoError(_)) => {
                // The packet is likely corrupted
                continue;
            }
            Err(_) => break, // End of stream or other error
        };

        // If the packet does not belong to the selected track, skip it
        if packet.track_id() != track_id {
            continue;
        }

        // Decode the packet
        let audio_buf = match decoder.decode(&packet) {
            Ok(audio_buf) => audio_buf,
            Err(symphonia::core::errors::Error::IoError(_)) => {
                // The packet is likely corrupted
                continue;
            }
            Err(_) => break,
        };

        // Convert the decoded audio buffer to a vector of samples
        let audio_samples = match audio_buf {
            AudioBufferRef::F32(buf) => {
                // For stereo, mix down to mono by averaging channels
                if buf.spec().channels.count() > 1 {
                    buf.chan(0)
                        .iter()
                        .zip(buf.chan(1).iter())
                        .map(|(l, r)| (l + r) / 2.0)
                        .collect::<Vec<f32>>()
                } else {
                    buf.chan(0).to_vec()
                }
            }
            AudioBufferRef::U8(buf) => {
                // Convert u8 to f32
                if buf.spec().channels.count() > 1 {
                    buf.chan(0)
                        .iter()
                        .zip(buf.chan(1).iter())
                        .map(|(l, r)| ((*l as f32 + *r as f32) / 2.0 - 128.0) / 128.0)
                        .collect::<Vec<f32>>()
                } else {
                    buf.chan(0)
                        .iter()
                        .map(|&s| (s as f32 - 128.0) / 128.0)
                        .collect::<Vec<f32>>()
                }
            }
            AudioBufferRef::U16(buf) => {
                // Convert u16 to f32
                if buf.spec().channels.count() > 1 {
                    buf.chan(0)
                        .iter()
                        .zip(buf.chan(1).iter())
                        .map(|(l, r)| ((*l as f32 + *r as f32) / 2.0 - 32768.0) / 32768.0)
                        .collect::<Vec<f32>>()
                } else {
                    buf.chan(0)
                        .iter()
                        .map(|&s| (s as f32 - 32768.0) / 32768.0)
                        .collect::<Vec<f32>>()
                }
            }
            AudioBufferRef::U24(_buf) => {
                // Skip u24 for now - complex conversion
                Vec::new()
            }
            AudioBufferRef::U32(buf) => {
                // Convert u32 to f32
                if buf.spec().channels.count() > 1 {
                    buf.chan(0)
                        .iter()
                        .zip(buf.chan(1).iter())
                        .map(|(l, r)| {
                            let l_f32 = (*l as f32 - 2147483648.0) / 2147483648.0;
                            let r_f32 = (*r as f32 - 2147483648.0) / 2147483648.0;
                            (l_f32 + r_f32) / 2.0
                        })
                        .collect::<Vec<f32>>()
                } else {
                    buf.chan(0)
                        .iter()
                        .map(|&s| (s as f32 - 2147483648.0) / 2147483648.0)
                        .collect::<Vec<f32>>()
                }
            }
            AudioBufferRef::S8(buf) => {
                // Convert s8 to f32
                if buf.spec().channels.count() > 1 {
                    buf.chan(0)
                        .iter()
                        .zip(buf.chan(1).iter())
                        .map(|(l, r)| (*l as f32 + *r as f32) / 2.0 / 128.0)
                        .collect::<Vec<f32>>()
                } else {
                    buf.chan(0)
                        .iter()
                        .map(|&s| s as f32 / 128.0)
                        .collect::<Vec<f32>>()
                }
            }
            AudioBufferRef::S16(buf) => {
                // Convert s16 to f32
                if buf.spec().channels.count() > 1 {
                    buf.chan(0)
                        .iter()
                        .zip(buf.chan(1).iter())
                        .map(|(l, r)| (*l as f32 + *r as f32) / 2.0 / 32768.0)
                        .collect::<Vec<f32>>()
                } else {
                    buf.chan(0)
                        .iter()
                        .map(|&s| s as f32 / 32768.0)
                        .collect::<Vec<f32>>()
                }
            }
            AudioBufferRef::S24(_buf) => {
                // Skip s24 for now - complex conversion
                Vec::new()
            }
            AudioBufferRef::S32(buf) => {
                // Convert s32 to f32
                if buf.spec().channels.count() > 1 {
                    buf.chan(0)
                        .iter()
                        .zip(buf.chan(1).iter())
                        .map(|(l, r)| {
                            let l_f32 = *l as f32 / 2147483648.0;
                            let r_f32 = *r as f32 / 2147483648.0;
                            (l_f32 + r_f32) / 2.0
                        })
                        .collect::<Vec<f32>>()
                } else {
                    buf.chan(0)
                        .iter()
                        .map(|&s| s as f32 / 2147483648.0)
                        .collect::<Vec<f32>>()
                }
            }
            AudioBufferRef::F64(buf) => {
                // Convert f64 to f32
                if buf.spec().channels.count() > 1 {
                    buf.chan(0)
                        .iter()
                        .zip(buf.chan(1).iter())
                        .map(|(l, r)| (l + r) as f32 / 2.0)
                        .collect::<Vec<f32>>()
                } else {
                    buf.chan(0).iter().map(|&s| s as f32).collect::<Vec<f32>>()
                }
            }
        };

        let sample_count = audio_samples.len();
        samples.extend(audio_samples);
        decoded_samples += sample_count;

        // If we have enough samples, break
        if decoded_samples >= target_samples * 10 {
            break;
        }
    }

    // Downsample to target number of samples
    let downsampled = if samples.len() > target_samples {
        let step = samples.len() / target_samples;
        samples
            .chunks(step)
            .map(|chunk| {
                // Calculate RMS (root mean square) for each chunk
                let sum_squares: f32 = chunk.iter().map(|&s| s * s).sum();
                (sum_squares / chunk.len() as f32).sqrt()
            })
            .collect()
    } else {
        samples
    };

    let duration_seconds = duration.unwrap_or(0.0);

    Ok(WaveformData::new(
        downsampled,
        duration_seconds,
        sample_rate,
        bpm,
    ))
}
