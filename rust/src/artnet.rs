use artnet_protocol::{ArtCommand, Output};
use std::io::Error;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

// The IP of the device running this SW
const DEVICE_IP: &str = "0.0.0.0";

const ART_NET_CONTROLLER_IP: &str = "255.255.255.255";

#[derive(Debug)]
pub struct ArtNet {
    socket: UdpSocket,
    broadcast_addr: SocketAddr,
}

impl ArtNet {
    pub fn new() -> Result<Self, Error> {
        let socket = UdpSocket::bind((DEVICE_IP, 6454))?;

        let broadcast_addr = (ART_NET_CONTROLLER_IP, 6454)
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();
        socket.set_broadcast(true).unwrap();

        Ok(Self {
            socket: socket,
            broadcast_addr: broadcast_addr,
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
