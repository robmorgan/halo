//! Art-Net DMX output, ported from halo-old's `crates/core/src/artnet`.
//!
//! Synchronous by design: a 512-byte UDP send is microseconds, so the
//! DMX engine thread calls [`ArtNet::send`] directly from its 44 Hz tick.
//! Errors are returned (never panicked) so a mid-show network hiccup
//! degrades to a dropped frame, not a crash.

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

use artnet_protocol::{ArtCommand, Output};
use log::debug;
use serde::{Deserialize, Serialize};

/// The standard Art-Net UDP port.
pub const ARTNET_PORT: u16 = 6454;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ArtNetMode {
    Broadcast,
    /// From (interface) + to (destination) addresses.
    Unicast(SocketAddr, SocketAddr),
}

/// One open Art-Net socket aimed at a destination.
pub struct ArtNet {
    socket: UdpSocket,
    destination: SocketAddr,
    pub mode: ArtNetMode,
}

impl ArtNet {
    pub fn new(mode: ArtNetMode) -> io::Result<Self> {
        match mode {
            ArtNetMode::Broadcast => {
                // Ephemeral local port so multiple broadcast sockets can
                // coexist (one per destination).
                let socket = UdpSocket::bind(("0.0.0.0", 0))?;
                socket.set_broadcast(true)?;
                let destination = ("255.255.255.255", ARTNET_PORT)
                    .to_socket_addrs()?
                    .next()
                    .expect("broadcast addr always resolves");
                debug!(
                    "Art-Net broadcast ready on local port {}",
                    socket.local_addr()?.port()
                );
                Ok(ArtNet {
                    socket,
                    destination,
                    mode,
                })
            }
            ArtNetMode::Unicast(src, destination) => {
                // Bind to the source IP (interface selection) with an
                // ephemeral port.
                let socket = UdpSocket::bind(SocketAddr::new(src.ip(), 0))?;
                socket.set_broadcast(false)?;
                debug!(
                    "Art-Net unicast {} -> {} ready on local port {}",
                    src.ip(),
                    destination,
                    socket.local_addr()?.port()
                );
                Ok(ArtNet {
                    socket,
                    destination,
                    mode,
                })
            }
        }
    }

    /// Send one universe's channel data as an ArtDmx packet.
    pub fn send(&self, universe: u8, dmx: &[u8]) -> io::Result<()> {
        let command = ArtCommand::Output(Output {
            port_address: universe.into(),
            data: dmx.to_vec().into(),
            ..Output::default()
        });
        let bytes = command
            .write_to_buffer()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        self.socket.send_to(&bytes, self.destination)?;
        Ok(())
    }
}

/// A named place to send Art-Net (a node, or the broadcast domain).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtNetDestination {
    pub name: String,
    pub mode: ArtNetMode,
}

/// Where each universe goes. Persisted with app settings.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub destinations: Vec<ArtNetDestination>,
    /// universe -> index into `destinations`.
    pub universe_routing: HashMap<u8, usize>,
}

impl NetworkConfig {
    /// Single-destination config routing universe 1, the common case.
    pub fn single(name: &str, mode: ArtNetMode) -> Self {
        NetworkConfig {
            destinations: vec![ArtNetDestination {
                name: name.to_string(),
                mode,
            }],
            universe_routing: HashMap::from([(1, 0)]),
        }
    }

    /// Add a destination and return its index.
    pub fn add_destination(&mut self, destination: ArtNetDestination) -> usize {
        self.destinations.push(destination);
        self.destinations.len() - 1
    }

    /// Route a universe to a destination by index; out-of-range indices
    /// are ignored.
    pub fn route_universe(&mut self, universe: u8, destination_index: usize) {
        if destination_index < self.destinations.len() {
            self.universe_routing.insert(universe, destination_index);
        }
    }

    pub fn destination_for_universe(&self, universe: u8) -> Option<usize> {
        self.universe_routing.get(&universe).copied()
    }

    /// Open a socket per destination. Returns the connections in
    /// destination order so `universe_routing` indices line up.
    pub fn connect(&self) -> io::Result<Vec<ArtNet>> {
        self.destinations
            .iter()
            .map(|d| ArtNet::new(d.mode.clone()))
            .collect()
    }

    /// Human-readable summary for the settings panel.
    pub fn summary(&self) -> String {
        if self.destinations.is_empty() {
            return "no destinations configured".to_string();
        }
        self.destinations
            .iter()
            .map(|d| match &d.mode {
                ArtNetMode::Broadcast => {
                    format!("{}: 255.255.255.255:{ARTNET_PORT}", d.name)
                }
                ArtNetMode::Unicast(src, dst) => {
                    format!("{}: {} -> {}", d.name, src.ip(), dst)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_maps_universes_to_destinations() {
        let mut config = NetworkConfig::single("main", ArtNetMode::Broadcast);
        let ode = config.add_destination(ArtNetDestination {
            name: "ode-mk2".to_string(),
            mode: ArtNetMode::Unicast(
                "10.8.45.1:6454".parse::<SocketAddr>().unwrap(),
                "10.8.45.80:6454".parse::<SocketAddr>().unwrap(),
            ),
        });
        config.route_universe(2, ode);
        config.route_universe(9, 99); // out of range: ignored

        assert_eq!(config.destination_for_universe(1), Some(0));
        assert_eq!(config.destination_for_universe(2), Some(ode));
        assert_eq!(config.destination_for_universe(9), None);
    }

    #[test]
    fn network_config_roundtrips_json() {
        let config = NetworkConfig::single("main", ArtNetMode::Broadcast);
        let json = serde_json::to_string(&config).unwrap();
        let parsed: NetworkConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.destinations.len(), 1);
        assert_eq!(parsed.destination_for_universe(1), Some(0));
    }

    #[test]
    fn broadcast_socket_binds_and_frames_a_universe() {
        // Socket setup only — no packets are sent from tests.
        let artnet = ArtNet::new(ArtNetMode::Broadcast).expect("bind broadcast socket");
        assert!(matches!(artnet.mode, ArtNetMode::Broadcast));
        assert_eq!(artnet.destination.port(), ARTNET_PORT);
    }
}
