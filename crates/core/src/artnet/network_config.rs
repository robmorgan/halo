use std::net::{IpAddr, SocketAddr};

use super::artnet::ArtNetMode;

#[derive(Clone)]
pub struct NetworkConfig {
    pub mode: ArtNetMode,
    pub port: u16,
}

impl NetworkConfig {
    pub fn new(
        source_ip: IpAddr,
        dest_ip: Option<IpAddr>,
        artnet_port: u16,
        broadcast: bool,
    ) -> Self {
        let mode = if broadcast {
            ArtNetMode::Broadcast
        } else {
            match dest_ip {
                Some(ip) => ArtNetMode::Unicast(
                    SocketAddr::new(source_ip, artnet_port),
                    SocketAddr::new(ip, artnet_port),
                ),
                None => ArtNetMode::Broadcast,
            }
        };

        NetworkConfig {
            mode,
            port: artnet_port,
        }
    }

    pub fn get_destination(&self) -> String {
        match &self.mode {
            ArtNetMode::Unicast(src, destination) => {
                format!("{}:{} -> {}:{}", src, self.port, destination, self.port)
            }
            ArtNetMode::Broadcast => format!("255.255.255.255:{}", self.port),
        }
    }

    pub fn get_mode_string(&self) -> &str {
        match &self.mode {
            ArtNetMode::Unicast(_, _) => "unicast",
            ArtNetMode::Broadcast => "broadcast",
        }
    }
}
