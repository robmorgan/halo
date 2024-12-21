/// Polls Keyboard Input, manipulates AblLink and sends messages to
/// the Audio Callback Thread to alter the SessionState there. The Link documentation
/// recommends to commit changes to the SessionState in the Audio thread, if there
/// are both App and Audio Threads.
pub fn poll_input(
    tx: mpsc::Sender<UpdateSessionState>,
    running: Arc<AtomicBool>,
    link: Arc<AblLink>,
    quantum: Arc<Mutex<f64>>,
) {
    let mut last_time = Instant::now();
}
