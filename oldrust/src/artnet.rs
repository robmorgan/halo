use artnet_protocol::{ArtCommand, Output};
use std::{
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    time::SystemTime,
};

// The IP of the device running this SW
const DEVICE_IP: &str = "0.0.0.0";

const ART_NET_CONTROLLER_IP: &str = "255.255.255.255";

const CHANNELS_PER_UNIVERSE: u16 = 512;

pub struct ArtNet {
    socket: UdpSocket,
    destination: SocketAddr,
    channels: Vec<u8>,
    last_sent: Option<SystemTime>,
    mode: ArtNetMode,
}

#[derive(Clone, Debug)]
pub enum ArtNetMode {
    Broadcast,
    /// Specify from (interface) + to (destination) addresses
    Unicast(SocketAddr, SocketAddr),
}

impl ArtNet {
    pub fn new(mode: ArtNetMode) -> Result<Self, anyhow::Error> {
        let channels = Vec::with_capacity(CHANNELS_PER_UNIVERSE as usize);

        match mode {
            ArtNetMode::Broadcast => {
                let socket = UdpSocket::bind((String::from("0.0.0.0"), 6455))?;
                let broadcast_addr = (ART_NET_CONTROLLER_IP, 6454)
                    .to_socket_addrs()?
                    .next()
                    .unwrap();
                socket.set_broadcast(true).unwrap();
                //debug!("Broadcast mode set up OK");
                Ok(ArtNet {
                    socket,
                    destination: broadcast_addr,
                    channels,
                    last_sent: None,
                    mode: mode.clone(),
                })
            }

            ArtNetMode::Unicast(src, destination) => {
                // debug!(
                //     "Will connect from interface {} to destination {}",
                //     &src, &destination
                // );
                let socket = UdpSocket::bind(src)?;

                socket.set_broadcast(false)?;
                Ok(ArtNet {
                    socket,
                    destination,
                    channels,
                    last_sent: None,
                    mode: mode.clone(),
                })
            }
        }
    }

    pub fn send_data(&self, universe: u8, dmx: Vec<u8>) {
        let command = ArtCommand::Output(Output {
            // length: dmx.len() as u16,
            port_address: universe.into(),
            data: dmx.into(),
            ..Output::default()
        });

        let bytes = command.write_to_buffer().unwrap();
        self.socket.send_to(&bytes, self.destination).unwrap();
    }
}
