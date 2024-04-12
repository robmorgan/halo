use rusty_link::{AblLink, SessionState};
use std::io::{stdout, Write};
use std::thread::sleep;
use std::time::{Duration, Instant};

//const CHARS: [&str; 4] = ["|", "/", "-", "\\"];

fn main() {
    println!("Starting Ableton Link Session...");

    // let mut source_address = "0.0.0.0:0".to_string();
    // let mut target_address = "127.0.0.1:6669".to_string();

    let link = AblLink::new(120.);
    link.enable(false);

    let mut state = SessionState::new();
    link.capture_app_session_state(&mut state);
    link.enable(true);

    // Due to Windows timers having a default resolution 0f 15.6ms, we need to use a "too high"
    // value to achieve ~60Hz
    let period = Duration::from_micros(1000000 / 120);
    let mut last_instant = Instant::now();

    let mut stdout = stdout();

    loop {
        let delta = Instant::now() - last_instant; // Is this timer accurate enough?
        last_instant = Instant::now();

        //let last_tempo = state.tempo();

        link.set_tempo_callback(|tempo| println!("Tempo: {}", tempo));

        print!(
            "\rRunning Frq: {: >3}Hz    Peers:{}   BPM: {}    ",
            1000000 / (delta.as_micros().max(1)),
            link.num_peers(),
            state.tempo()
        );
        stdout.flush().unwrap();

        sleep(period);
    }
}
