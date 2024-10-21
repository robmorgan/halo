use rusty_link::{AblLink, SessionState};
use std::io::{stdout, Write};
use std::thread::sleep;
use std::time::{Duration, Instant};

use artnet_protocol::*;
use std::net::{ToSocketAddrs, UdpSocket};
mod artnet;

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
    //let period = Duration::from_micros(1000000 / 120);
    let period = Duration::from_millis(500);
    let mut last_instant = Instant::now();

    let mut stdout = stdout();

    let art_net_controller = artnet::ArtNet::new(artnet::ArtNetMode::Broadcast).unwrap();

    // let socket = UdpSocket::bind(("0.0.0.0", 6455)).unwrap();
    // let broadcast_addr = ("255.255.255.255", 6454)
    //     .to_socket_addrs()
    //     .unwrap()
    //     .next()
    //     .unwrap();
    // socket.set_broadcast(true).unwrap();
    // let buff = ArtCommand::Poll(Poll::default()).write_to_buffer().unwrap();
    // socket.send_to(&buff, &broadcast_addr).unwrap();

    loop {
        let delta = Instant::now() - last_instant; // Is this timer accurate enough?
        last_instant = Instant::now();

        //let last_tempo = state.tempo();

        link.set_tempo_callback(|tempo| println!("Tempo: {}", tempo));

        //art_net_controller.send_data(vec![0xff; 512]);

        println!("Calling socket.recv_from");
        let mut buffer = [0u8; 1024];
        //let (length, addr) = socket.recv_from(&mut buffer).unwrap();
        //let command = ArtCommand::from_buffer(&buffer[..length]).unwrap();

        //println!("Received {:?}", command);
        // match command {
        //     ArtCommand::Poll(poll) => {
        //         // This will most likely be our own poll request, as this is broadcast to all devices on the network
        //         println!("Recv poll {:?}", poll);
        //     }
        //     ArtCommand::PollReply(reply) => {
        //         println!("Reply {:?}", reply);

        //         // This is an ArtNet node on the network. We can send commands to it like this:
        //         art_net_controller.send_data(1.into(), vec![0xff; 512]);

        //         let command = ArtCommand::Output(Output {
        //             // length: dmx.len() as u16,
        //             //data: dmx.into(),
        //             port_address: 1.into(),
        //             data: vec![0xff; 512].into(),
        //             ..Output::default()
        //         });

        //         let bytes = command.write_to_buffer().unwrap();
        //         socket.send_to(&bytes, &addr).unwrap();
        //     }
        //     _ => {}
        // }

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
