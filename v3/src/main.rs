use artnet_protocol::{ArtCommand, Output, PortAddress};
use rusty_link::{AblLink, SessionState};
use std::collections::HashMap;
use std::error::Error;
use std::io::{self, stdout, Read, Write};
use std::net::SocketAddr;
use std::net::{ToSocketAddrs, UdpSocket};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

const FIXTURES: usize = 4;
const CHANNELS_PER_FIXTURE: usize = 8; // SHEHDS PAR Fixtures
const TOTAL_CHANNELS: usize = FIXTURES * CHANNELS_PER_FIXTURE;
const TARGET_FREQUENCY: f64 = 44.0; // 44Hz
const TARGET_DELTA: f64 = 1.0 / TARGET_FREQUENCY;

struct Fixture {
    name: String,
    channels: Vec<Channel>,
    start_address: u16,
}

struct Channel {
    name: String,
    value: u8,
}

struct Cue {
    name: String,
    duration: f64,
    effect_mappings: Vec<EffectMapping>,
}

#[derive(Clone)]
struct Effect {
    name: String,
    apply: fn(&mut Channel, f64, f64, f64) -> u8,
}

// TODO - one day we'll make this apply to multiple fixtures and channels
struct EffectMapping {
    effect: Effect,
    fixture_name: String,
    channel_pattern: String,
}

impl Fixture {
    fn new(name: &str, num_lights: usize, has_tilt: bool, start_address: u16) -> Self {
        let mut channels = Vec::new();
        for i in 0..num_lights {
            channels.push(Channel {
                name: format!("Dimmer {}", i + 1),
                value: 0,
            });
        }
        if has_tilt {
            channels.push(Channel {
                name: "Tilt".to_string(),
                value: 128,
            }); // Center position
        }
        Fixture {
            name: name.to_string(),
            channels,
            start_address,
        }
    }

    fn set_channel_value(&mut self, channel_name: &str, value: u8) {
        if let Some(channel) = self.channels.iter_mut().find(|c| c.name == channel_name) {
            channel.value = value;
        }
    }

    fn get_dmx_values(&self) -> Vec<u8> {
        self.channels.iter().map(|c| c.value).collect()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let link = AblLink::new(120.0);
    link.enable(false);

    let mut state = SessionState::new();
    link.capture_app_session_state(&mut state);
    link.enable(true);

    //let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
    let socket = UdpSocket::bind((String::from("0.0.0.0"), 6455))?;
    //let target_addr = "192.168.1.100:6454"; // Replace with your Art-Net node's IP and port

    let broadcast_addr = ("255.255.255.255", 6454)
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();
    socket.set_broadcast(true).unwrap();

    let mut fixtures = vec![
        Fixture::new("Complex Fixture 1", 8, true, 1),
        Fixture::new("Complex Fixture 2", 8, true, 9),
    ];

    let effects = vec![
        Effect {
            name: "Sine Wave".to_string(),
            apply: sine_wave_effect,
        },
        Effect {
            name: "Square Wave".to_string(),
            apply: square_wave_effect,
        },
        Effect {
            name: "Sawtooth Wave".to_string(),
            apply: sawtooth_wave_effect,
        },
    ];

    let cues = vec![
        Cue {
            name: "All Dimmers Sine".to_string(),
            duration: 10.0,
            effect_mappings: vec![
                EffectMapping {
                    effect: effects[0].clone(),
                    fixture_name: "Complex Fixture 1".to_string(),
                    channel_pattern: "Dimmer".to_string(),
                },
                EffectMapping {
                    effect: effects[0].clone(),
                    fixture_name: "Complex Fixture 2".to_string(),
                    channel_pattern: "Dimmer".to_string(),
                },
            ],
        },
        Cue {
            name: "Mixed Effects".to_string(),
            duration: 15.0,
            effect_mappings: vec![
                EffectMapping {
                    effect: effects[1].clone(),
                    fixture_name: "Complex Fixture 1".to_string(),
                    channel_pattern: "Dimmer".to_string(),
                },
                EffectMapping {
                    effect: effects[2].clone(),
                    fixture_name: "Complex Fixture 2".to_string(),
                    channel_pattern: "Dimmer".to_string(),
                },
                EffectMapping {
                    effect: effects[0].clone(),
                    fixture_name: "Complex Fixture 1".to_string(),
                    channel_pattern: "Tilt".to_string(),
                },
                EffectMapping {
                    effect: effects[0].clone(),
                    fixture_name: "Complex Fixture 2".to_string(),
                    channel_pattern: "Tilt".to_string(),
                },
            ],
        },
    ];

    // Keyboard input handling
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || loop {
        let mut buffer = [0; 1];
        if io::stdin().read_exact(&mut buffer).is_ok() {
            if buffer[0] == b'G' || buffer[0] == b'g' {
                tx.send(()).unwrap();
            }
        }
    });

    let mut current_cue = 0;
    let mut frames_sent = 0;
    let mut accumulated_time = 0.0;
    let mut cue_time = 0.0;
    let mut last_update = Instant::now();

    let mut cue_start_time = 0.0;
    let mut bpm = 0.0;
    let mut frames_sent = 0;
    let start_time = Instant::now();
    let mut accumulated_time = 0.0;
    let mut effect_time = 0.0;

    loop {
        let now = Instant::now();
        let delta = now.duration_since(last_update).as_secs_f64();
        last_update = now;

        accumulated_time += delta;
        cue_time += delta;

        link.capture_app_session_state(&mut state);
        let beat_time = state.beat_at_time(link.clock_micros(), 0.0);

        if cue_time >= cues[current_cue].duration {
            cue_time = 0.0; // Reset cue time but don't change the cue
        }

        if rx.try_recv().is_ok() {
            current_cue = (current_cue + 1) % cues.len();
            cue_time = 0.0;
            println!("Advanced to cue: {}", cues[current_cue].name);
        }

        apply_cue(
            &mut fixtures,
            &cues[current_cue],
            accumulated_time,
            cue_time,
            delta,
        );

        let dmx_data = generate_dmx_data(&fixtures);
        send_dmx_data(&socket, broadcast_addr, dmx_data)?;
        frames_sent += 1;

        if beat_time - cue_start_time >= cues[current_cue].duration {
            cue_start_time = beat_time;
        }

        // Display status information
        bpm = state.tempo();

        display_status(
            &link,
            bpm,
            frames_sent,
            &cues[current_cue].name,
            accumulated_time,
            cue_time,
        );

        let frame_time = now.elapsed().as_secs_f64();
        if frame_time < TARGET_DELTA {
            std::thread::sleep(Duration::from_secs_f64(TARGET_DELTA - frame_time));
        }
    }
}

fn apply_cue(fixtures: &mut [Fixture], cue: &Cue, total_time: f64, cue_time: f64, delta: f64) {
    for mapping in &cue.effect_mappings {
        if let Some(fixture) = fixtures.iter_mut().find(|f| f.name == mapping.fixture_name) {
            for channel in &mut fixture.channels {
                if channel.name.starts_with(&mapping.channel_pattern) {
                    channel.value = (mapping.effect.apply)(channel, total_time, cue_time, delta);
                }
            }
        }
    }
}

fn generate_dmx_data(fixtures: &[Fixture]) -> Vec<u8> {
    let mut dmx_data = vec![0; 512]; // Full DMX universe
    for fixture in fixtures {
        let start = (fixture.start_address - 1) as usize;
        let end = start + fixture.channels.len();
        dmx_data[start..end].copy_from_slice(&fixture.get_dmx_values());
    }
    dmx_data
}

fn sine_wave_effect(_channel: &mut Channel, time: f64, _cue_time: f64, _delta: f64) -> u8 {
    ((time.sin() * 0.5 + 0.5) * 255.0) as u8
}

fn square_wave_effect(_channel: &mut Channel, time: f64, _cue_time: f64, _delta: f64) -> u8 {
    if (time * TARGET_FREQUENCY).sin() > 0.0 {
        255
    } else {
        0
    }
}

fn sawtooth_wave_effect(_channel: &mut Channel, time: f64, _cue_time: f64, _delta: f64) -> u8 {
    ((time * TARGET_FREQUENCY % 1.0) * 255.0) as u8
}

fn calculate_effect_value(beat_time: f64, cue_start_time: f64) -> u8 {
    let elapsed_time = beat_time - cue_start_time;
    let normalized_value = (elapsed_time.sin() + 1.0) / 2.0;
    (normalized_value * 255.0) as u8
}

fn beat_intensity(beat_time: f64) -> f64 {
    let beat_fraction = beat_time.fract();
    (1.0 - beat_fraction * 2.0).abs() // Creates a triangle wave that peaks on each beat
}

fn send_dmx_data(
    //socket: &std::net::UdpSocket,
    socket: &UdpSocket,
    //target_addr: &str,
    broadcast_addr: SocketAddr,
    dmx_data: Vec<u8>,
) -> Result<(), Box<dyn Error>> {
    let command = ArtCommand::Output(Output {
        // length: dmx.len() as u16,
        //data: dmx.into(),
        //port_address: PortAddress::try_from(0).unwrap(),
        data: dmx_data.into(),
        ..Output::default()
    });

    let bytes = command.write_to_buffer().unwrap();
    socket.send_to(&bytes, broadcast_addr)?;
    Ok(())
}

fn key_pressed() -> io::Result<bool> {
    let mut buffer = [0; 1];
    Ok(io::stdin().read(&mut buffer)? > 0)
}

fn display_status(
    link: &AblLink,
    bpm: f64,
    frames_sent: u64,
    current_cue: &str,
    elapsed: f64,
    cue_time: f64,
) {
    //let bpm = state.tempo();
    let num_peers = link.num_peers();

    print!("\r"); // Move cursor to the beginning of the line
    print!(
        "Frames: {:8} | BPM: {:6.2} | Peers: {:3} | Current Cue: {:3} | Elapsed: {:6.2}s | Cue Time: {:6.2}s | FPS: {:5.2}",
        frames_sent, bpm, num_peers, current_cue, elapsed, cue_time, frames_sent as f64 / elapsed
    );
    stdout().flush().unwrap();
}
