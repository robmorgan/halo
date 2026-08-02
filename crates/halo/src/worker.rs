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
    /// A folder import finished with this many audio files seen.
    Imported(usize),
}

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
                Ok(None) => match wake_rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
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
    lib.store_analysis(id, &artifact)
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
