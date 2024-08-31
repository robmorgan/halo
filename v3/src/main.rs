use artnet_protocol::{ArtCommand, Output, PortAddress};
use rusty_link::{AblLink, SessionState};
use std::error::Error;
use std::f64::consts::PI;
use std::io::{self, stdout, Read, Write};
use std::net::SocketAddr;
use std::net::{ToSocketAddrs, UdpSocket};
use std::time::Duration;

const FIXTURES: usize = 4;
const CHANNELS_PER_FIXTURE: usize = 8; // SHEHDS PAR Fixtures
const TOTAL_CHANNELS: usize = FIXTURES * CHANNELS_PER_FIXTURE;

struct Cue {
    duration: f64,
    effect: fn(f64, usize) -> u8,
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

    let cues = vec![
        Cue {
            duration: 8.0,
            effect: sine_wave_effect,
        },
        Cue {
            duration: 8.0,
            effect: square_wave_effect,
        },
        Cue {
            duration: 8.0,
            effect: sawtooth_wave_effect,
        },
    ];

    let mut current_cue = 0;
    let mut cue_start_time = 0.0;
    let mut bpm = 0.0;
    let mut frames_sent = 0;

    loop {
        link.capture_app_session_state(&mut state);
        let beat_time = state.beat_at_time(link.clock_micros(), 0.0);

        let dmx_data = generate_effect(beat_time, cue_start_time, &cues[current_cue]);

        let dmx_vec: Vec<u8> = dmx_data.into_iter().map(|value| value as u8).collect();

        send_dmx_data(&socket, broadcast_addr, dmx_vec)?;
        frames_sent += 1;

        if beat_time - cue_start_time >= cues[current_cue].duration {
            cue_start_time = beat_time;
        }

        // Display status information
        bpm = state.tempo();
        display_status(&link, bpm, frames_sent, current_cue + 1);

        // TODO - make sure cues keep looping until a key is pressed.
        // if key_pressed()? {
        //     current_cue = (current_cue + 1) % cues.len();
        //     cue_start_time = beat_time;
        //     println!("Advanced to cue {}", current_cue + 1);
        // }

        std::thread::sleep(Duration::from_millis(33)); // ~30 fps
    }
}

fn generate_effect(beat_time: f64, cue_start_time: f64, cue: &Cue) -> Vec<u8> {
    //let mut dmx_data = [0u8; TOTAL_CHANNELS];
    let mut dmx_data = vec![0; 512]; // Initialize all channels to 0
    let cue_time = beat_time - cue_start_time;

    // for fixture in 0..FIXTURES {
    //     let base_index = fixture * CHANNELS_PER_FIXTURE;
    //     let phase_offset = (fixture as f64) * PI / 2.0; // Different phase for each fixture

    //     // TODO - only update intensity channels
    //     for channel in 0..CHANNELS_PER_FIXTURE {
    //         dmx_data[base_index + channel] = (cue.effect)(cue_time + phase_offset, channel);
    //     }
    // }

    //for (fixture_index, fixture) in cue.fixtures.iter().enumerate() {
    for fixture in 0..FIXTURES {
        //let start_channel = fixture_index * 8; // Assuming 8 channels per fixture
        let start_channel = fixture * CHANNELS_PER_FIXTURE;

        // Set the effect value on the first channel
        let effect_value = calculate_effect_value(beat_time, cue_start_time);
        dmx_data[start_channel] = effect_value;

        // Set hard-coded values for the rest of the channels
        dmx_data[start_channel + 1] = 255; // Example: Full intensity
        dmx_data[start_channel + 2] = 0; // Example: Mid-range for some parameter
        dmx_data[start_channel + 3] = 0; // Example: Off for another parameter
                                         // ... Set values for other channels as needed
    }

    dmx_data
}

fn calculate_effect_value(beat_time: f64, cue_start_time: f64) -> u8 {
    let elapsed_time = beat_time - cue_start_time;
    let normalized_value = (elapsed_time.sin() + 1.0) / 2.0;
    (normalized_value * 255.0) as u8
}

fn sine_wave_effect(time: f64, _channel: usize) -> u8 {
    ((time.sin() * 0.5 + 0.5) * 255.0) as u8
}

fn square_wave_effect(time: f64, _channel: usize) -> u8 {
    if time.sin() > 0.0 {
        255
    } else {
        0
    }
}

fn sawtooth_wave_effect(time: f64, _channel: usize) -> u8 {
    ((time % 1.0) * 255.0) as u8
}

fn send_dmx_data(
    //socket: &std::net::UdpSocket,
    socket: &UdpSocket,
    //target_addr: &str,
    broadcast_addr: SocketAddr,
    //dmx_data: &[u8],
    dmx_data: Vec<u8>,
) -> Result<(), Box<dyn Error>> {
    // let output = Output {
    //     port_address: PortAddress::try_from(0).unwrap(),
    //     data: dmx_data.to_vec(),
    //     length: dmx_data.len() as u16,
    // };

    // let packet = Packet::Output(output);
    // let buffer = packet.write_to_buffer()?;

    //let buffer = output.write_to_buffer()?;

    let command = ArtCommand::Output(Output {
        // length: dmx.len() as u16,
        //data: dmx.into(),
        //port_address: PortAddress::try_from(0).unwrap(),
        data: dmx_data.into(),
        ..Output::default()
    });

    let bytes = command.write_to_buffer().unwrap();

    //    self.socket.send_to(&bytes, self.broadcast_addr).unwrap();

    socket.send_to(&bytes, broadcast_addr)?;
    Ok(())
}

fn key_pressed() -> io::Result<bool> {
    let mut buffer = [0; 1];
    Ok(io::stdin().read(&mut buffer)? > 0)
}

fn display_status(link: &AblLink, bpm: f64, frames_sent: u64, current_cue: usize) {
    //let bpm = state.tempo();
    let num_peers = link.num_peers();

    print!("\r"); // Move cursor to the beginning of the line
    print!(
        "Frames: {:8} | BPM: {:6.2} | Peers: {:3} | Current Cue: {:3}",
        frames_sent, bpm, num_peers, current_cue
    );
    stdout().flush().unwrap();
}
