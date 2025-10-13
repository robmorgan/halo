use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use super::artnet::ArtNetMode;

#[derive(Clone)]
pub struct NetworkConfig {
    pub destinations: Vec<ArtNetDestination>,
    pub universe_routing: HashMap<u8, usize>, // universe -> destination index
    pub port: u16,
}

#[derive(Clone, Debug)]
pub struct ArtNetDestination {
    pub name: String,
    pub mode: ArtNetMode,
}

impl NetworkConfig {
    // Legacy constructor for backward compatibility
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

        let destination = ArtNetDestination {
            name: "default".to_string(),
            mode,
        };

        // Default: route universe 1 to the single destination
        let mut universe_routing = HashMap::new();
        universe_routing.insert(1, 0);

        NetworkConfig {
            destinations: vec![destination],
            universe_routing,
            port: artnet_port,
        }
    }

    // New constructor for multi-destination setup
    pub fn new_multi_destination(
        destinations: Vec<ArtNetDestination>,
        universe_routing: HashMap<u8, usize>,
        artnet_port: u16,
    ) -> Self {
        NetworkConfig {
            destinations,
            universe_routing,
            port: artnet_port,
        }
    }

    // Add a destination and return its index
    pub fn add_destination(&mut self, destination: ArtNetDestination) -> usize {
        self.destinations.push(destination);
        self.destinations.len() - 1
    }

    // Route a universe to a specific destination
    pub fn route_universe(&mut self, universe: u8, destination_index: usize) {
        if destination_index < self.destinations.len() {
            self.universe_routing.insert(universe, destination_index);
        }
    }

    // Get destination index for a universe (returns None if not routed)
    pub fn get_destination_for_universe(&self, universe: u8) -> Option<usize> {
        self.universe_routing.get(&universe).copied()
    }

    // Legacy compatibility methods
    pub fn get_destination(&self) -> String {
        if self.destinations.is_empty() {
            return "No destinations configured".to_string();
        }
        
        let mut result = String::new();
        for (i, dest) in self.destinations.iter().enumerate() {
            if i > 0 {
                result.push_str(", ");
            }
            result.push_str(&format!("{}: {}", dest.name, self.get_destination_string(&dest.mode)));
        }
        result
    }

    pub fn get_mode_string(&self) -> &str {
        if self.destinations.is_empty() {
            return "none";
        }
        // Return the mode of the first destination for backward compatibility
        match &self.destinations[0].mode {
            ArtNetMode::Unicast(_, _) => "multi-unicast",
            ArtNetMode::Broadcast => "multi-broadcast",
        }
    }

    fn get_destination_string(&self, mode: &ArtNetMode) -> String {
        match mode {
            ArtNetMode::Unicast(src, destination) => {
                format!("{}:{} -> {}:{}", src.ip(), self.port, destination.ip(), self.port)
            }
            ArtNetMode::Broadcast => format!("255.255.255.255:{}", self.port),
        }
    }
}
