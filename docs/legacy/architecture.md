# Multi-Destination Art-Net Architecture

Technical documentation for the multi-destination Art-Net implementation in Halo.

## Overview

The multi-destination system allows Halo to route different DMX universes to different Art-Net destinations simultaneously. This enables scenarios like sending lighting fixtures to one controller and pixel fixtures to another.

## Architecture Components

### 1. NetworkConfig (`artnet/network_config.rs`)

Central configuration structure that manages multiple Art-Net destinations.

#### Key Structures

```rust
pub struct NetworkConfig {
    pub destinations: Vec<ArtNetDestination>,
    pub universe_routing: HashMap<u8, usize>, // universe -> destination index
    pub port: u16,
}

pub struct ArtNetDestination {
    pub name: String,
    pub mode: ArtNetMode, // Broadcast or Unicast
}
```

#### Key Methods

- `new_multi_destination()` - Create config with multiple destinations
- `add_destination()` - Add a new destination and return its index
- `route_universe()` - Route a universe to a specific destination
- `get_destination_for_universe()` - Get destination index for a universe

### 2. DmxModule (`modules/dmx_module.rs`)

Handles the actual DMX output with support for multiple Art-Net connections.

#### Key Changes

```rust
pub struct DmxModule {
    artnet_connections: Vec<Option<ArtNet>>, // Multiple ArtNet instances
    network_config: NetworkConfig,
    // ... other fields
}
```

#### Routing Logic

```rust
// Send each universe to its routed destination
for (universe, data) in &last_dmx_data {
    if let Some(dest_index) = self.network_config.get_destination_for_universe(*universe) {
        if let Some(Some(artnet)) = self.artnet_connections.get(dest_index) {
            artnet.send_data(*universe, data.clone());
        }
    }
}
```

### 3. Console Integration (`console.rs`)

The lighting console sends DMX data through the module system without knowing about routing details.

#### Current Universe Assignment

- **Universe 1**: Regular lighting fixtures (`send_dmx_data:315`)
- **Universe 2+**: Pixel fixtures from `PixelEngine` (`send_dmx_data:325`)

### 4. CLI Interface (`main.rs`)

Command-line parsing and network configuration setup.

#### Configuration Logic

```rust
let network_config = if args.lighting_dest_ip.is_some() || args.pixel_dest_ip.is_some() {
    // Multi-destination setup
    let mut destinations = Vec::new();
    let mut universe_routing = HashMap::new();
    
    // Add lighting destination
    if let Some(lighting_ip) = args.lighting_dest_ip {
        let lighting_index = destinations.len();
        destinations.push(lighting_dest);
        universe_routing.insert(args.lighting_universe, lighting_index);
    }
    
    // Add pixel destination  
    if let Some(pixel_ip) = args.pixel_dest_ip {
        let pixel_index = destinations.len();
        destinations.push(pixel_dest);
        for universe in args.pixel_start_universe..=16 {
            universe_routing.insert(universe, pixel_index);
        }
    }
    
    NetworkConfig::new_multi_destination(destinations, universe_routing, args.artnet_port)
} else {
    // Legacy single destination
    NetworkConfig::new(args.source_ip, args.dest_ip, args.artnet_port, args.broadcast)
};
```

## Data Flow

### 1. Console Renders DMX Data

```
Console::send_dmx_data()
├── Render regular fixtures → Universe 1 (512 channels)
└── PixelEngine::render() → Multiple universes (2-16)
```

### 2. Module Manager Routes Events  

```
ModuleEvent::DmxOutput(universe, data)
│
└── DmxModule receives events
```

### 3. DmxModule Routes to Destinations

```
DmxModule::run()
├── Receive DmxOutput events
├── Store data by universe: HashMap<u8, Vec<u8>>
└── On timer tick:
    ├── For each (universe, data):
    │   ├── Look up destination: universe_routing[universe] → dest_index
    │   └── Send: artnet_connections[dest_index].send_data(universe, data)
    └── Log status
```

### 4. ArtNet Sends UDP Packets

```
ArtNet::send_data(universe, data)
├── Create Art-Net packet with universe and DMX data
└── Send UDP packet to configured destination
```

## Routing Examples

### Example 1: Lighting + Pixel Separation

**Configuration:**
```bash
--lighting-dest-ip 192.168.1.200 --pixel-dest-ip 192.168.1.201
```

**Routing Map:**
```rust
universe_routing = {
    1: 0,     // Universe 1 → Destination 0 (lighting: 192.168.1.200)
    2: 1,     // Universe 2 → Destination 1 (pixel: 192.168.1.201)
    3: 1,     // Universe 3 → Destination 1 (pixel: 192.168.1.201)
    // ... up to universe 16
}
```

**Data Flow:**
- Regular fixtures on Universe 1 → `192.168.1.200:6454`
- Pixel Bar 1 on Universe 2 → `192.168.1.201:6454`
- Pixel Bar 2 on Universe 3 → `192.168.1.201:6454`

### Example 2: Custom Universe Assignment

**Configuration:**
```bash
--lighting-universe 5 --pixel-start-universe 10
```

**Routing Map:**
```rust
universe_routing = {
    5: 0,     // Universe 5 → Lighting destination
    10: 1,    // Universe 10 → Pixel destination  
    11: 1,    // Universe 11 → Pixel destination
    // ... up to universe 16
}
```

## Backward Compatibility

### Legacy Mode

When using `--dest-ip`, the system creates a single-destination configuration:

```rust
let destination = ArtNetDestination {
    name: "default".to_string(),
    mode: /* unicast or broadcast */,
};

let mut universe_routing = HashMap::new();
universe_routing.insert(1, 0); // Route universe 1 to destination 0

NetworkConfig {
    destinations: vec![destination],
    universe_routing,
    port: artnet_port,
}
```

## Performance Considerations

### Memory Usage

- Each `ArtNet` instance maintains its own UDP socket
- `universe_routing` HashMap has O(1) lookup time
- DMX data is stored per universe: `HashMap<u8, Vec<u8>>`

### Network Efficiency

- Each destination receives only its assigned universes
- No unnecessary network traffic to controllers that don't need specific universes
- Maintains 44Hz update rate per the DMX standard

### Thread Safety

- `NetworkConfig` is passed to `DmxModule` during initialization
- No runtime changes to routing configuration (restart required)
- DMX data flows through async message passing (`ModuleEvent`)

## Extension Points

### Adding More Destinations

To support more than lighting + pixel destinations:

1. **Extend CLI parsing** to accept additional destination IPs
2. **Modify universe routing logic** to assign universe ranges  
3. **Update `NetworkConfig` creation** to include new destinations

### Custom Universe Routing

For complex routing scenarios:

1. **Add configuration file support** for detailed universe mapping
2. **Implement runtime routing changes** via console commands
3. **Add UI controls** for destination management

### Protocol Extensions

The architecture supports extending beyond Art-Net:

1. **Abstract the transport layer** with trait objects
2. **Implement additional protocols** (sACN/E1.31, KiNet, etc.)
3. **Add protocol-specific destination types**

## Debugging and Monitoring

### Status Information

The `DmxModule` provides status updates:
- Number of active destinations  
- Universes being transmitted
- Frame rate and packet counts
- Destination connection status

### Logging

Key log messages for debugging:
- `DMX module started with N destinations`
- `Setting up ArtNet connection N for destination: name`
- `No destination routing configured for universe N`
- `No ArtNet connection found for destination index N`

### Validation

The system validates:
- All destinations are properly initialized
- Universe routing references valid destination indices
- IP addresses and ports are valid
- No conflicting configuration (legacy + multi-destination)