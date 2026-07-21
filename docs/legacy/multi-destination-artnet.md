# Multi-Destination Art-Net Setup

Halo supports sending Art-Net DMX data to multiple destinations simultaneously, allowing you to route different types of fixtures to different hardware controllers.

## Overview

The multi-destination system allows you to:

- **Separate lighting and pixel outputs** to different Art-Net controllers
- **Route specific universes** to specific IP addresses
- **Maintain backward compatibility** with single-destination setups
- **Configure flexible universe assignments**

## Common Use Case: Lighting + Pixel Separation

The most common scenario is separating traditional lighting fixtures from pixel fixtures:

- **Lighting fixtures** → Enttec Ode MK2 (or similar controller)
- **Pixel fixtures** → Enttec Octo MK2 (or similar pixel controller)

### Basic Setup

```bash
cargo run --release -- \
  --source-ip 192.168.1.100 \
  --lighting-dest-ip 192.168.1.200 \
  --pixel-dest-ip 192.168.1.201
```

This configuration:
- Sends **universe 1** (lighting fixtures) to `192.168.1.200`
- Sends **universes 2-16** (pixel fixtures) to `192.168.1.201`
- Uses source IP `192.168.1.100` for outbound packets

### Custom Universe Assignment

You can customize which universes are used:

```bash
cargo run --release -- \
  --source-ip 192.168.1.100 \
  --lighting-dest-ip 192.168.1.200 \
  --pixel-dest-ip 192.168.1.201 \
  --lighting-universe 3 \
  --pixel-start-universe 10
```

This routes:
- **Universe 3** → `192.168.1.200` (lighting)
- **Universes 10-16** → `192.168.1.201` (pixels)

## Configuration Options

### Required Parameters

- `--source-ip <IP>` - Your computer's Art-Net interface IP address

### Multi-Destination Parameters

- `--lighting-dest-ip <IP>` - Destination for lighting fixtures
- `--pixel-dest-ip <IP>` - Destination for pixel fixtures
- `--lighting-universe <NUM>` - Universe number for lighting (default: 1)
- `--pixel-start-universe <NUM>` - Starting universe for pixels (default: 2)

### Network Parameters

- `--artnet-port <PORT>` - Art-Net port (default: 6454)
- `--broadcast` - Force broadcast mode instead of unicast

## Network Setup Requirements

### IP Address Planning

1. **Ensure all devices are on the same network segment**
   ```
   Computer:        192.168.1.100
   Lighting DMX:    192.168.1.200  (Enttec Ode MK2)
   Pixel DMX:       192.168.1.201  (Enttec Octo MK2)
   ```

2. **Configure your network interface**
   ```bash
   # macOS
   sudo ifconfig en0 192.168.1.100 netmask 255.255.255.0
   
   # Linux
   sudo ip addr add 192.168.1.100/24 dev eth0
   ```

3. **Verify connectivity**
   ```bash
   ping 192.168.1.200  # Test lighting controller
   ping 192.168.1.201  # Test pixel controller
   ```

### Art-Net Controller Configuration

#### Enttec Ode MK2 (Lighting)
- Set IP address to `192.168.1.200`
- Configure to receive Art-Net on universe 1
- Set subnet/universe as needed

#### Enttec Octo MK2 (Pixels)
- Set IP address to `192.168.1.201`
- Configure to receive Art-Net starting from universe 2
- Each output can handle one universe (up to 8 total)

## Fixture Configuration

### Lighting Fixtures (Universe 1)

Regular fixtures (PAR cans, moving lights, etc.) are automatically assigned to the lighting universe:

```rust
// These go to --lighting-dest-ip
console.patch_fixture("Front Wash", "PAR64", 1, 1).await?;
console.patch_fixture("Moving Head 1", "MH250", 1, 10).await?;
```

### Pixel Fixtures (Universe 2+)

Pixel bars and LED strips are assigned to pixel universes:

```rust
// These go to --pixel-dest-ip  
console.patch_fixture("Pixel Bar 1", "PixelBar60", 2, 1).await?;
console.patch_fixture("Pixel Bar 2", "PixelBar60", 3, 1).await?;
```

## Monitoring and Status

Halo provides status information about the multi-destination setup:

```
Configuring Halo with Art-Net settings:
Mode: multi-unicast
Destinations: 
  - lighting: 192.168.1.100:6454 -> 192.168.1.200:6454
  - pixel: 192.168.1.100:6454 -> 192.168.1.201:6454
```

DMX module logs show routing information:
```
DMX module started with 2 destinations, running at 44Hz
DMX: 1000 frames sent, 3 universes active across 2 destinations
```

## Migration from Single Destination

### Legacy Single Destination
```bash
# Old way - everything to one destination
cargo run --release -- \
  --source-ip 192.168.1.100 \
  --dest-ip 192.168.1.200
```

### New Multi-Destination
```bash
# New way - separate destinations
cargo run --release -- \
  --source-ip 192.168.1.100 \
  --lighting-dest-ip 192.168.1.200 \
  --pixel-dest-ip 192.168.1.201
```

The legacy `--dest-ip` parameter still works for backward compatibility but only sends to a single destination.

## Advanced Configuration

### Multiple Lighting Controllers

You can only specify one lighting and one pixel destination via CLI. For more complex setups with multiple controllers, you'll need to modify the console code to add additional destinations programmatically.

### Custom Universe Routing

The current implementation routes:
- **Lighting universe** → lighting destination  
- **Pixel universes (start+)** → pixel destination

For custom universe routing (e.g., universe 5 to a third controller), you'll need to extend the `NetworkConfig` setup in the console initialization.

### Broadcast Mode

Use broadcast mode if you want all controllers to receive all universe data:

```bash
cargo run --release -- \
  --source-ip 192.168.1.100 \
  --broadcast
```

This sends all universes to `255.255.255.255` and each controller filters for its configured universes.