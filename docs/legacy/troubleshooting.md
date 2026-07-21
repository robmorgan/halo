# Troubleshooting Multi-Destination Art-Net

Common issues and solutions when using Halo's multi-destination Art-Net feature.

## Network Connectivity Issues

### Controllers Not Receiving Data

**Symptoms:**
- Controllers show no Art-Net activity
- Fixtures not responding to Halo commands
- Network indicators on controllers inactive

**Diagnosis Steps:**

1. **Verify Network Connectivity**
   ```bash
   ping 192.168.1.200  # Test lighting controller
   ping 192.168.1.201  # Test pixel controller
   ```

2. **Check Art-Net Traffic**
   ```bash
   # Monitor outbound Art-Net packets
   tcpdump -i en0 udp port 6454
   
   # Monitor specific destination
   tcpdump -i en0 host 192.168.1.200 and udp port 6454
   ```

3. **Verify Interface Configuration**
   ```bash
   # macOS
   ifconfig en0
   
   # Linux  
   ip addr show eth0
   ```

**Common Solutions:**

- **Wrong source IP**: Ensure `--source-ip` matches your network interface
- **Network interface down**: Activate the network interface used for Art-Net
- **Firewall blocking**: Disable firewall or allow UDP port 6454
- **Wrong subnet**: Ensure all devices on same network segment

### "Can't assign requested address" Error

**Error Message:**
```
DmxModule error: Can't assign requested address (os error 49)
```

**Causes:**
- Source IP doesn't exist on any network interface
- Network interface is not active
- IP address conflict

**Solutions:**

1. **List available interfaces:**
   ```bash
   # macOS
   ifconfig
   
   # Linux
   ip addr show
   ```

2. **Configure correct IP:**
   ```bash
   # macOS
   sudo ifconfig en0 192.168.1.100 netmask 255.255.255.0
   
   # Linux
   sudo ip addr add 192.168.1.100/24 dev eth0
   ```

3. **Use available interface IP:**
   ```bash
   # Find your current IP and use it
   halo --source-ip $(ifconfig en0 | grep "inet " | awk '{print $2}') ...
   ```

## Configuration Issues

### Wrong Universe Assignment

**Symptoms:**
- Some fixtures respond, others don't
- Pixel fixtures showing on lighting controller
- Lighting fixtures not responding

**Diagnosis:**
- Check controller universe configuration
- Verify fixture patching in Halo console
- Monitor which universes are being sent

**Solution:**
```bash
# Adjust universe assignments to match controller config
halo --source-ip 192.168.1.100 \
     --lighting-dest-ip 192.168.1.200 \
     --pixel-dest-ip 192.168.1.201 \
     --lighting-universe 1 \      # Match lighting controller
     --pixel-start-universe 5     # Match pixel controller
```

### Legacy vs Multi-Destination Confusion

**Error:** Trying to use both legacy and new parameters

**Invalid Command:**
```bash
# ❌ This won't work
halo --source-ip 192.168.1.100 \
     --dest-ip 192.168.1.200 \
     --lighting-dest-ip 192.168.1.201
```

**Valid Commands:**
```bash
# ✅ Legacy single destination
halo --source-ip 192.168.1.100 --dest-ip 192.168.1.200

# ✅ Multi-destination
halo --source-ip 192.168.1.100 \
     --lighting-dest-ip 192.168.1.200 \
     --pixel-dest-ip 192.168.1.201
```

## Controller-Specific Issues

### Enttec Ode MK2

**Common Issues:**
- Incorrect IP configuration
- Universe/subnet settings
- Port forwarding disabled

**Configuration:**
1. Access web interface: `http://192.168.1.200`
2. Set IP address and subnet mask
3. Configure Art-Net universe (typically 0 or 1)
4. Enable Art-Net input
5. Verify port settings (6454)

**Debug Commands:**
```bash
# Test specific universe to Ode
halo --source-ip 192.168.1.100 \
     --lighting-dest-ip 192.168.1.200 \
     --lighting-universe 1
```

### Enttec Octo MK2  

**Common Issues:**
- Multiple universe configuration
- Pixel output mapping
- Power supply requirements

**Configuration:**
1. Set base universe (e.g., 2)
2. Configure output mapping for each port
3. Set pixel protocol (WS2812B, etc.)
4. Verify power requirements for LED count

**Debug Commands:**
```bash
# Send only to pixel controller
halo --source-ip 192.168.1.100 \
     --pixel-dest-ip 192.168.1.201 \
     --pixel-start-universe 2
```

### Generic Art-Net Nodes

**Configuration Checklist:**
- IP address in correct subnet
- Art-Net enabled (not sACN/E1.31)
- Correct universe numbers
- Input direction enabled
- Port 6454 accessible

## Performance Issues

### High Latency or Dropped Frames

**Symptoms:**
- Jerky movement on moving lights
- Delayed response to console changes
- Intermittent fixture behavior

**Diagnosis:**
```bash
# Monitor frame rate and packet loss
RUST_LOG=info halo --source-ip 192.168.1.100 \
                   --lighting-dest-ip 192.168.1.200 \
                   --pixel-dest-ip 192.168.1.201
```

Look for messages like:
```
DMX: 2200 frames sent, 3 universes active across 2 destinations
```

**Solutions:**
- **Use wired network** instead of Wi-Fi
- **Reduce network hops** between computer and controllers
- **Check for network congestion** with other Art-Net devices
- **Upgrade network hardware** to gigabit switches

### CPU/Memory Usage

**Symptoms:**
- High CPU usage
- Memory leaks over time
- System responsiveness issues

**Solutions:**
- Close unnecessary applications
- Monitor with system tools:
  ```bash
  # macOS
  top -pid $(pgrep halo)
  
  # Linux  
  htop -p $(pgrep halo)
  ```

## Debug Commands and Logging

### Enable Debug Logging

```bash
# Full debug output
RUST_LOG=debug halo --source-ip 192.168.1.100 \
                    --lighting-dest-ip 192.168.1.200 \
                    --pixel-dest-ip 192.168.1.201

# DMX module only
RUST_LOG=halo_core::modules::dmx_module=debug halo ...

# Network-related only  
RUST_LOG=halo_core::artnet=debug halo ...
```

### Key Log Messages

**Successful Initialization:**
```
Initializing DMX module with 2 destinations
Setting up ArtNet connection 0 for destination: lighting
Setting up ArtNet connection 1 for destination: pixel  
DMX module started with 2 destinations, running at 44Hz
```

**Warning Messages:**
```
No destination routing configured for universe 3
No ArtNet connection found for destination index 2
```

**Error Messages:**
```
ArtNet connection 0 not initialized
DmxModule error: Can't assign requested address
```

## Network Debugging Tools

### Monitor Art-Net Traffic

**tcpdump Examples:**
```bash
# All Art-Net traffic
sudo tcpdump -i en0 udp port 6454

# Specific destination
sudo tcpdump -i en0 host 192.168.1.200 and udp port 6454

# Show packet contents  
sudo tcpdump -i en0 -X udp port 6454

# Save to file for analysis
sudo tcpdump -i en0 -w artnet.pcap udp port 6454
```

**Wireshark Analysis:**
1. Capture on Art-Net interface
2. Filter: `udp.port == 6454`
3. Analyze packet destinations and universe numbers
4. Check for missing or duplicate packets

### Network Connectivity Tests

```bash
# Basic connectivity
ping 192.168.1.200

# Art-Net port accessibility  
nc -u -v 192.168.1.200 6454

# Network route verification
traceroute 192.168.1.200

# Interface statistics
netstat -i en0
```

## Art-Net Protocol Issues

### Universe Number Confusion

Art-Net uses different universe numbering than some controllers:

**Art-Net Protocol:**
- Universe range: 0-32767
- Net: 0-127, Subnet: 0-15, Universe: 0-15
- 15-bit universe address

**Controller Configuration:**
- May display as 1-based (Universe 1 = Art-Net Universe 0)
- Check controller documentation for addressing scheme

### Packet Size Limits

Standard Art-Net DMX packet:
- Header: 18 bytes
- Data: 512 bytes maximum
- Total: 530 bytes per universe

**Large Pixel Setups:**
- Multiple universes required for large LED installations
- Each universe = 170 RGB pixels (512 ÷ 3) maximum
- Plan universe allocation accordingly

## Common Error Messages

### "Module initialization failed"

**Full Error:**
```
Console error: Module initialization failed: DmxModule error: ...
```

**Solutions:**
1. Check network configuration
2. Verify source IP is valid
3. Ensure no other Art-Net software is using the interface
4. Try broadcast mode: `--broadcast`

### "No destination routing configured"

**Error:**
```
No destination routing configured for universe 5
```

**Cause:** Universe being sent but no destination assigned

**Solutions:**
1. Adjust `--pixel-start-universe` to include universe 5
2. Add custom routing for universe 5
3. Use broadcast mode if complex routing needed

### "Failed to load configuration"

**Error:**
```
Warning: Failed to load configuration: missing field 'pixel_engine_enabled'
```

**Cause:** Config file format has changed

**Solutions:**
1. Delete `config.json` to use defaults
2. Update config file with missing fields
3. Use `config.template.json` as reference

## Getting Additional Help

### Log Collection for Support

```bash
# Collect comprehensive logs
RUST_LOG=debug halo --source-ip 192.168.1.100 \
                    --lighting-dest-ip 192.168.1.200 \
                    --pixel-dest-ip 192.168.1.201 \
                    2>&1 | tee halo-debug.log
```

### System Information

```bash
# Network configuration
ifconfig -a

# System info
uname -a

# Halo version
halo --version  # (if version flag implemented)

# Cargo/Rust info
cargo --version
rustc --version
```

### Testing Minimal Configuration

When troubleshooting, start with minimal configuration:

```bash
# Single destination, broadcast mode
halo --source-ip 192.168.1.100 --broadcast

# Single lighting destination
halo --source-ip 192.168.1.100 --lighting-dest-ip 192.168.1.200

# Single pixel destination  
halo --source-ip 192.168.1.100 --pixel-dest-ip 192.168.1.201
```

Once basic connectivity works, add complexity incrementally.