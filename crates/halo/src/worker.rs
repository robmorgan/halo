//! Background library workers: the analysis queue (one track at a time,
//! decoded and analyzed at the file's native rate) and one-shot folder
//! imports. Each worker opens its own SQLite connection.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::decoder::decode_file;
use crate::library::Library;

pub enum WorkerEvent {
    /// A track's analysis landed in the DB.
    Analyzed(i64),
    /// A track's browser waveform thumbnail landed in the DB (backfill;
    /// fresh analysis is covered by [`WorkerEvent::Analyzed`]).
    WaveformReady(i64),
    /// A folder import finished with this many audio files seen.
    Imported(usize),
}

/// Columns per band in the browser thumbnail blob: covers the widest the
/// ~92pt cell gets at 2x pixel density, with margin.
pub const THUMB_COLS: usize = 256;

/// Long-lived analysis worker: drains the unanalyzed queue, then idles until
/// woken (or polls every few seconds as a fallback). Exits when the wake
/// channel disconnects.
pub fn spawn_analysis_worker(
    db_path: PathBuf,
    wake_rx: mpsc::Receiver<()>,
    event_tx: mpsc::Sender<WorkerEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let lib = match Library::open(&db_path) {
            Ok(l) => l,
            Err(e) => {
                log::error!("analysis worker: {e}");
                return;
            }
        };
        loop {
            match lib.next_unanalyzed() {
                Ok(Some((id, path))) => {
                    match analyze_one(&lib, id, &path) {
                        Ok(()) => {
                            if event_tx.send(WorkerEvent::Analyzed(id)).is_err() {
                                return;
                            }
                        }
                        Err(e) => {
                            log::warn!("analysis of {}: {e}", path.display());
                            // Park a failure marker so the queue can't spin
                            // on an undecodable file.
                            let _ = lib.store_analysis_failure(id);
                        }
                    }
                }
                // Analysis drained: backfill thumbnails for tracks analyzed
                // before the thumbnail table existed (or a version bump).
                Ok(None) => match lib.next_missing_waveform() {
                    Ok(Some((id, path))) => {
                        match backfill_waveform(&lib, id, &path) {
                            Ok(()) => {
                                if event_tx.send(WorkerEvent::WaveformReady(id)).is_err() {
                                    return;
                                }
                            }
                            Err(e) => {
                                log::warn!("waveform for {}: {e}", path.display());
                                // Park the failure marker so the queue can't
                                // spin on an undecodable file.
                                let _ = lib.store_waveform(id, &[]);
                            }
                        }
                    }
                    Ok(None) => match wake_rx.recv_timeout(Duration::from_secs(5)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    },
                    Err(e) => {
                        log::error!("waveform queue: {e}");
                        return;
                    }
                },
                Err(e) => {
                    log::error!("analysis queue: {e}");
                    return;
                }
            }
        }
    })
}

fn analyze_one(lib: &Library, id: i64, path: &std::path::Path) -> Result<(), String> {
    let decoded = decode_file(path)?;
    let signal = timestretch::downmix_to_mid(&decoded.samples, 2);
    let start = std::time::Instant::now();
    let mut artifact = timestretch::analyze_for_dj(&signal, decoded.sample_rate);
    // BS.1770 sums per-channel energies, so loudness is measured on the
    // original interleaved signal — the mono analysis downmix would read
    // up to ~3 dB low. `analyze_for_dj` deliberately leaves this None.
    artifact.loudness = timestretch::measure_loudness(
        &decoded.samples,
        decoded.channels as usize,
        decoded.sample_rate,
    );
    let lufs = artifact
        .loudness
        .map_or("n/a".to_string(), |l| format!("{:.1}", l.integrated_lufs));
    log::info!(
        "Analyzed {}: {:.1} BPM, confidence {:.2}, {lufs} LUFS ({:.2}s)",
        path.display(),
        artifact.bpm,
        artifact.confidence,
        start.elapsed().as_secs_f64()
    );
    lib.store_analysis(id, &artifact)?;
    let peaks = timestretch::BandPeaks::compute(
        &decoded.samples,
        decoded.channels as usize,
        decoded.sample_rate,
    );
    lib.store_waveform(id, &thumbnail_blob(peaks.coarsest(), THUMB_COLS))
}

/// Decode-and-store just the browser thumbnail for an already-analyzed
/// track (the pre-thumbnail-table backfill path).
fn backfill_waveform(lib: &Library, id: i64, path: &std::path::Path) -> Result<(), String> {
    let decoded = decode_file(path)?;
    let peaks = timestretch::BandPeaks::compute(
        &decoded.samples,
        decoded.channels as usize,
        decoded.sample_rate,
    );
    lib.store_waveform(id, &thumbnail_blob(peaks.coarsest(), THUMB_COLS))
}

/// Resample a peak level to `n_cols` u8 amplitude columns per band
/// (band-major low/mid/high), max-pooled: these are peaks, so the
/// downsample must preserve transients — averaging would visibly dim
/// isolated kicks. With fewer source buckets than columns the window
/// formula degrades to nearest-neighbor replication.
pub(crate) fn thumbnail_blob(level: &timestretch::PeakLevel, n_cols: usize) -> Vec<u8> {
    let n = level.num_buckets();
    if n == 0 {
        return vec![0; 3 * n_cols];
    }
    let mut out = Vec::with_capacity(3 * n_cols);
    for band in 0..3 {
        for i in 0..n_cols {
            let lo = i * n / n_cols;
            let hi = ((i + 1) * n / n_cols).max(lo + 1);
            let amp = (lo..hi)
                .map(|j| level.pos[band][j].max(-level.neg[band][j]))
                .fold(0.0_f32, f32::max)
                .clamp(0.0, 1.0);
            out.push((amp * 255.0).round() as u8);
        }
    }
    out
}

/// One-shot folder import on its own thread; wakes the analysis worker when
/// done.
pub fn spawn_folder_import(
    db_path: PathBuf,
    dir: PathBuf,
    wake_tx: mpsc::Sender<()>,
    event_tx: mpsc::Sender<WorkerEvent>,
) {
    thread::spawn(move || {
        let lib = match Library::open(&db_path) {
            Ok(l) => l,
            Err(e) => {
                log::error!("import worker: {e}");
                return;
            }
        };
        match lib.import_folder(&dir) {
            Ok(n) => {
                let _ = event_tx.send(WorkerEvent::Imported(n));
                let _ = wake_tx.send(());
            }
            Err(e) => log::error!("import {}: {e}", dir.display()),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A level with `n` buckets, silent except where set by the caller.
    fn level(n: usize) -> timestretch::PeakLevel {
        timestretch::PeakLevel {
            buckets_per_sec: 1500.0,
            pos: std::array::from_fn(|_| vec![0.0; n]),
            neg: std::array::from_fn(|_| vec![0.0; n]),
        }
    }

    #[test]
    fn blob_is_always_three_bands_of_n_cols() {
        for n in [0, 1, 7, 256, 1024] {
            assert_eq!(thumbnail_blob(&level(n), 64).len(), 3 * 64);
        }
    }

    #[test]
    fn max_pooling_preserves_a_single_bucket_spike() {
        // 1024 -> 64 columns: a lone full-scale bucket must survive into
        // its window's column at full amplitude.
        let mut l = level(1024);
        l.pos[0][500] = 1.0;
        let blob = thumbnail_blob(&l, 64);
        assert_eq!(blob[500 * 64 / 1024], 255);
        assert_eq!(blob.iter().filter(|&&b| b != 0).count(), 1);
    }

    #[test]
    fn negative_peaks_count_via_magnitude() {
        let mut l = level(8);
        l.neg[1][3] = -0.5;
        let blob = thumbnail_blob(&l, 8);
        // Band-major: mid band is the second block of 8.
        assert_eq!(blob[8 + 3], 128);
    }

    #[test]
    fn fewer_buckets_than_cols_replicates() {
        let mut l = level(2);
        l.pos[0][0] = 1.0;
        l.pos[0][1] = 0.5;
        let blob = thumbnail_blob(&l, 8);
        // First half of the low band mirrors bucket 0, second half bucket 1.
        assert_eq!(&blob[0..4], &[255, 255, 255, 255]);
        assert_eq!(&blob[4..8], &[128, 128, 128, 128]);
    }

    #[test]
    fn out_of_range_peaks_clamp() {
        let mut l = level(1);
        l.pos[2][0] = 3.5;
        let blob = thumbnail_blob(&l, 4);
        assert!(blob[2 * 4..].iter().all(|&b| b == 255));
    }
}
