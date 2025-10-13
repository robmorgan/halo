# Multi-Destination Art-Net Examples

Real-world examples and common configurations for multi-destination Art-Net setups.

## Example 1: Basic Club Setup

**Hardware:**
- Enttec Ode MK2 for traditional lighting
- Enttec Octo MK2 for LED pixel strips
- Network switch connecting all devices

**Network Configuration:**
```
Computer (Halo):     192.168.1.100
Ode MK2 (Lighting):  192.168.1.200
Octo MK2 (Pixels):   192.168.1.201
```

**Command:**
```bash
cargo run --release -- \
  --source-ip 192.168.1.100 \
  --lighting-dest-ip 192.168.1.200 \
  --pixel-dest-ip 192.168.1.201 \
  --enable-midi
```

**Fixtures:**
- Universe 1: PAR cans, moving heads → Ode MK2
- Universe 2: LED pixel strips → Octo MK2  
- Universe 3: More pixel strips → Octo MK2

## Example 2: Theater Production

**Scenario:** Theater with separate conventional and LED systems

**Hardware:**
- ETC Net3 Gateway for conventional dimmers
- Enttec Datagate MK2 for LED fixtures

**Network Configuration:**
```
Computer (Halo):     10.0.1.100
ETC Net3:            10.0.1.150
Enttec Datagate:     10.0.1.160
```

**Command:**
```bash
cargo run --release -- \
  --source-ip 10.0.1.100 \
  --lighting-dest-ip 10.0.1.150 \
  --pixel-dest-ip 10.0.1.160 \
  --lighting-universe 1 \
  --pixel-start-universe 5 \
  --show-file shows/TheaterShow.json
```

**Universe Assignment:**
- Universe 1: Conventional dimmers → ETC Net3
- Universes 5-8: LED color mixing fixtures → Datagate MK2

## Example 3: DJ/Live Performance

**Scenario:** Mobile DJ setup with lighting and pixel effects

**Hardware:**
- DMXking ultraDMX Micro for moving heads
- ENTTEC USB DMX PRO Mk2 for LED strips

**Network Configuration:**
```
Laptop (Halo):       192.168.0.100
DMXking ultraDMX:    192.168.0.200  
ENTTEC USB Pro:      192.168.0.201
```

**Command:**
```bash
cargo run --release -- \
  --source-ip 192.168.0.100 \
  --lighting-dest-ip 192.168.0.200 \
  --pixel-dest-ip 192.168.0.201 \
  --enable-midi \
  --show-file shows/DJSet.json
```

**Performance Features:**
- MIDI controller for live triggering
- Synchronized audio playback
- Beat-matched lighting effects

## Example 4: Festival Stage

**Scenario:** Large outdoor stage with multiple fixture types

**Hardware:**
- MA-Net2 node for moving lights and LEDs
- Madrix Luna 16 for pixel mapping

**Network Configuration:**
```
FOH Computer:        192.168.10.100
MA-Net2 Node:        192.168.10.150
Madrix Luna:         192.168.10.200
```

**Command:**
```bash
cargo run --release -- \
  --source-ip 192.168.10.100 \
  --lighting-dest-ip 192.168.10.150 \
  --pixel-dest-ip 192.168.10.200 \
  --lighting-universe 3 \
  --pixel-start-universe 10 \
  --artnet-port 6454
```

**Scale:**
- Universe 3: 200+ moving lights and wash fixtures
- Universes 10-16: Pixel-mapped LED panels and strips

## Example 5: Broadcast Mode Setup

**Scenario:** Multiple controllers on network, each filtering for specific universes

**Hardware:**
- Multiple Art-Net nodes on same network
- Each node configured for specific universe ranges

**Command:**
```bash
cargo run --release -- \
  --source-ip 192.168.1.100 \
  --broadcast
```

**Network Behavior:**
- All universes broadcast to 255.255.255.255
- Each controller filters for its configured universes
- Redundant but simple setup

## Example 6: Custom Universe Ranges

**Scenario:** Non-standard universe assignments

**Hardware:**
- Controller A handles universes 5-8
- Controller B handles universes 20-24

**Commands:**
```bash
# Option 1: Use lighting controller for universe 5
cargo run --release -- \
  --source-ip 192.168.1.100 \
  --lighting-dest-ip 192.168.1.200 \
  --pixel-dest-ip 192.168.1.201 \
  --lighting-universe 5 \
  --pixel-start-universe 20

# Option 2: Use broadcast with controller filtering
cargo run --release -- \
  --source-ip 192.168.1.100 \
  --broadcast
```

**Note:** For complex routing beyond lighting + pixel, consider broadcast mode or code modifications.

## Example 7: Development and Testing

**Scenario:** Testing multi-destination without hardware

**Using Art-Net Monitor Software:**
```bash
# Terminal 1: Run Art-Net monitor on port 6454
artnet-monitor --universe 1

# Terminal 2: Run another monitor on different port
artnet-monitor --universe 2 --port 6455

# Terminal 3: Run Halo
cargo run --release -- \
  --source-ip 127.0.0.1 \
  --lighting-dest-ip 127.0.0.1 \
  --pixel-dest-ip 127.0.0.1
```

**Using Wireshark:**
1. Capture on network interface
2. Filter: `udp.port == 6454`
3. Verify packets going to correct destinations

## Configuration Files

### Example Theater Show File

```json
{
  "name": "Theater Show",
  "fixtures": [
    {
      "name": "House Lights",
      "profile": "PAR64",
      "universe": 1,
      "address": 1
    },
    {
      "name": "LED Strip 1", 
      "profile": "PixelBar60",
      "universe": 5,
      "address": 1
    }
  ],
  "cues": [
    {
      "name": "Preset",
      "time": 0.0,
      "fixtures": {
        "House Lights": {"intensity": 0.8}
      }
    }
  ]
}
```

### Network Interface Setup Scripts

**macOS:**
```bash
#!/bin/bash
# setup-network.sh
sudo ifconfig en0 192.168.1.100 netmask 255.255.255.0
echo "Network configured for Art-Net"
```

**Linux:**
```bash
#!/bin/bash
# setup-network.sh
sudo ip addr flush dev eth0
sudo ip addr add 192.168.1.100/24 dev eth0
sudo ip link set eth0 up
echo "Network configured for Art-Net"
```

## Performance Monitoring

### Monitor Network Traffic

```bash
# Monitor Art-Net packets
tcpdump -i en0 udp port 6454

# Monitor specific destination
tcpdump -i en0 host 192.168.1.200 and udp port 6454
```

### Monitor Halo Output

```bash
# Run with debug logging
RUST_LOG=debug cargo run --release -- \
  --source-ip 192.168.1.100 \
  --lighting-dest-ip 192.168.1.200 \
  --pixel-dest-ip 192.168.1.201
```

Look for log messages:
- `DMX module started with 2 destinations`
- `DMX: N frames sent, M universes active across 2 destinations`

## Integration Examples

### With Ableton Live

```bash
# Enable Link synchronization
cargo run --release -- \
  --source-ip 192.168.1.100 \
  --lighting-dest-ip 192.168.1.200 \
  --pixel-dest-ip 192.168.1.201 \
  --enable-midi \
  --show-file shows/EDMSet.json
```

### With MIDI Controllers

**Akai MPK49 Configuration:**
- Map pads to cue triggers
- Map faders to fixture intensities  
- Map knobs to effect parameters

### With External Timecode

```bash
# Listen for external SMPTE timecode
cargo run --release -- \
  --source-ip 192.168.1.100 \
  --lighting-dest-ip 192.168.1.200 \
  --pixel-dest-ip 192.168.1.201 \
  --show-file shows/TimecodeShow.json
```

## Troubleshooting Common Setups

### Controller Not Receiving Data

1. **Check network connectivity:**
   ```bash
   ping 192.168.1.200
   ```

2. **Verify universe routing:**
   - Check controller's universe configuration
   - Verify Halo is sending to correct universe

3. **Monitor Art-Net traffic:**
   ```bash
   tcpdump -i en0 host 192.168.1.200
   ```

### Performance Issues

1. **Reduce update rate if needed** (code modification required)
2. **Use wired network** instead of Wi-Fi
3. **Minimize network hops** between Halo and controllers

### Universe Conflicts

1. **Verify universe assignments** don't overlap
2. **Check controller configurations** for universe filtering
3. **Use broadcast mode** if routing becomes complex