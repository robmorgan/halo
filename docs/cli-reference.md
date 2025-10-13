# CLI Reference

Complete reference for Halo's command-line interface options.

## Usage

```bash
halo [OPTIONS] --source-ip <SOURCE_IP>
```

## Required Arguments

### `--source-ip <IP_ADDRESS>`

**Required.** The IP address of your computer's network interface for Art-Net communication.

```bash
--source-ip 192.168.1.100
```

**Notes:**
- Must be a valid IPv4 address
- Should match your computer's Art-Net network interface
- Used as the source address in outbound Art-Net packets

## Network Configuration

### Single Destination (Legacy)

#### `--dest-ip <IP_ADDRESS>`

*Optional.* Art-Net destination IP address for backward compatibility.

```bash
--dest-ip 192.168.1.200
```

**Notes:**
- If not provided, broadcast mode is used (`255.255.255.255`)
- Cannot be combined with `--lighting-dest-ip` or `--pixel-dest-ip`
- All universes are sent to this single destination

### Multi-Destination Setup

#### `--lighting-dest-ip <IP_ADDRESS>`

*Optional.* Destination IP address for lighting fixtures (e.g., Enttec Ode MK2).

```bash
--lighting-dest-ip 192.168.1.200
```

#### `--pixel-dest-ip <IP_ADDRESS>`

*Optional.* Destination IP address for pixel fixtures (e.g., Enttec Octo MK2).

```bash
--pixel-dest-ip 192.168.1.201
```

**Notes:**
- At least one of `--lighting-dest-ip` or `--pixel-dest-ip` must be specified for multi-destination mode
- Cannot be combined with legacy `--dest-ip`
- Each destination receives only its assigned universes

### Universe Assignment

#### `--lighting-universe <NUMBER>`

*Optional.* Universe number for lighting fixtures.

```bash
--lighting-universe 1
```

**Default:** `1`  
**Range:** `1-255`

#### `--pixel-start-universe <NUMBER>`

*Optional.* Starting universe number for pixel fixtures.

```bash
--pixel-start-universe 2
```

**Default:** `2`  
**Range:** `1-255`  
**Notes:** Pixel fixtures use this universe and higher (up to universe 16)

### Network Options

#### `--artnet-port <PORT>`

*Optional.* Art-Net port number.

```bash
--artnet-port 6454
```

**Default:** `6454`  
**Range:** `1-65535`  
**Standard:** Art-Net standard port is 6454

#### `--broadcast`

*Optional.* Force broadcast mode even if destination IPs are provided.

```bash
--broadcast
```

**Notes:**
- Sends to `255.255.255.255` regardless of destination IP settings
- All controllers receive all universe data
- Controllers filter for their configured universes

## Application Options

### `--enable-midi` / `-e`

*Optional.* Enable MIDI controller support.

```bash
--enable-midi
# or
-e
```

**Notes:**
- Enables MIDI input for live performance control
- Requires MIDI hardware to be connected
- See MIDI configuration documentation for setup

### `--show-file <PATH>`

*Optional.* Path to a show JSON file to load on startup.

```bash
--show-file shows/MyShow.json
```

**Notes:**
- Must be a valid path to a JSON show file
- Show files contain cue lists, fixture patches, and automation
- Can be absolute or relative path

## Help and Information

### `--help` / `-h`

Display help information and exit.

```bash
halo --help
```

## Examples

### Basic Single Destination

```bash
# Broadcast to all devices on network
halo --source-ip 192.168.1.100

# Send to specific controller
halo --source-ip 192.168.1.100 --dest-ip 192.168.1.200
```

### Multi-Destination Lighting + Pixels

```bash
# Separate lighting and pixel controllers
halo --source-ip 192.168.1.100 \
     --lighting-dest-ip 192.168.1.200 \
     --pixel-dest-ip 192.168.1.201

# Custom universe assignment
halo --source-ip 192.168.1.100 \
     --lighting-dest-ip 192.168.1.200 \
     --pixel-dest-ip 192.168.1.201 \
     --lighting-universe 3 \
     --pixel-start-universe 10
```

### Performance Setup

```bash
# Full live performance setup
halo --source-ip 192.168.1.100 \
     --lighting-dest-ip 192.168.1.200 \
     --pixel-dest-ip 192.168.1.201 \
     --enable-midi \
     --show-file shows/LiveSet.json
```

### Custom Port and Broadcast

```bash
# Custom port with broadcast mode
halo --source-ip 192.168.1.100 \
     --lighting-dest-ip 192.168.1.200 \
     --pixel-dest-ip 192.168.1.201 \
     --artnet-port 6455 \
     --broadcast
```

## Configuration Validation

### Valid Combinations

✅ **Legacy single destination:**
```bash
--source-ip 192.168.1.100 --dest-ip 192.168.1.200
```

✅ **Multi-destination:**
```bash
--source-ip 192.168.1.100 --lighting-dest-ip 192.168.1.200 --pixel-dest-ip 192.168.1.201
```

✅ **Lighting only:**
```bash
--source-ip 192.168.1.100 --lighting-dest-ip 192.168.1.200
```

✅ **Pixels only:**
```bash
--source-ip 192.168.1.100 --pixel-dest-ip 192.168.1.201
```

### Invalid Combinations

❌ **Cannot mix legacy and multi-destination:**
```bash
--source-ip 192.168.1.100 --dest-ip 192.168.1.200 --lighting-dest-ip 192.168.1.201
```

❌ **Source IP is always required:**
```bash
halo --lighting-dest-ip 192.168.1.200
```

## Environment and Config Files

While CLI arguments are the primary configuration method, some settings may be loaded from:

- **Config file:** `config.json` in the working directory
- **Environment variables:** None currently supported
- **Show files:** Loaded via `--show-file` parameter

CLI arguments always override config file settings for network configuration.