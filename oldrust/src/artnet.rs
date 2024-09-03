use artnet_protocol::{ArtCommand, Output};
use log::{debug, trace};
use serde::{Deserialize, Serialize};
use std::{
    io::Error,
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    time::{Duration, SystemTime},
};

// The IP of the device running this SW
const DEVICE_IP: &str = "0.0.0.0";

const ART_NET_CONTROLLER_IP: &str = "255.255.255.255";

const CHANNELS_PER_UNIVERSE: u16 = 512;

pub struct ArtNet {
    socket: UdpSocket,
    destination: SocketAddr,
    channels: Vec<u8>,
    update_interval: Duration,
    last_sent: Option<SystemTime>,
    mode: ArtNetMode,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum ArtNetMode {
    Broadcast,
    /// Specify from (interface) + to (destination) addresses
    Unicast(SocketAddr, SocketAddr),
}

impl ArtNet {
    pub fn new(mode: ArtNetMode, update_frequency: u64) -> Result<Self, anyhow::Error> {
        let channels = Vec::with_capacity(CHANNELS_PER_UNIVERSE as usize);

        let update_interval = Duration::from_secs_f32(1.0 / update_frequency as f32);

        match mode {
            ArtNetMode::Broadcast => {
                let socket = UdpSocket::bind((String::from("0.0.0.0"), 6455))?;
                let broadcast_addr = ("255.255.255.255", 6454).to_socket_addrs()?.next().unwrap();
                socket.set_broadcast(true).unwrap();
                debug!("Broadcast mode set up OK");
                Ok(ArtNet {
                    socket,
                    destination: broadcast_addr,
                    channels,
                    update_interval,
                    last_sent: None,
                    mode: mode.clone(),
                })
            }

            ArtNetMode::Unicast(src, destination) => {
                debug!(
                    "Will connect from interface {} to destination {}",
                    &src, &destination
                );
                let socket = UdpSocket::bind(src)?;

                socket.set_broadcast(false)?;
                Ok(ArtNet {
                    socket,
                    destination,
                    channels,
                    update_interval,
                    last_sent: None,
                    mode: mode.clone(),
                })
            }
        }

        let socket = UdpSocket::bind((DEVICE_IP, 6454))?;

        let destination = (ART_NET_CONTROLLER_IP, 6454)
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();
        socket.set_broadcast(true).unwrap();

        Ok(Self {
            socket: socket,
            destination: destination,
        })
    }

    pub fn send_data(&self, dmx: Vec<u8>) {
        //ArtCommand::Output::BigEndian::from(dmx.clone())

        // let length = ArtCommand::Output::BigEndianLength((dmx.len()));

        let command = ArtCommand::Output(Output {
            // length: dmx.len() as u16,
            //data: dmx.into(),
            data: dmx.into(),
            ..Output::default()
        });

        // let command = ArtCommand::Output(Output {
        //     //length: dmx.len() as u16,
        //     length: 5,
        //     data: dmx.clone().into(),
        //     ..Output::default() //..Output::default()
        // });

        // command
        //     .write_to_buffer()
        //     .map_err(|err| ArtnetError::Art(err))?;

        // self.socket.send_to(&bytes, self.broadcast_addr).unwrap();

        let bytes = command.write_to_buffer().unwrap();
        self.socket.send_to(&bytes, self.broadcast_addr).unwrap();

        //let bytes = command.into_buffer().unwrap();
        //self.socket.send_to(&bytes, self.broadcast_addr).unwrap();
    }
}
