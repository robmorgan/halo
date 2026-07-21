# Halo Documentation

This directory contains comprehensive documentation for the Halo lighting console.

## Table of Contents

- [Multi-Destination Art-Net Setup](multi-destination-artnet.md) - Configure separate outputs for lighting and pixel fixtures
- [CLI Reference](cli-reference.md) - Complete command-line interface documentation
- [Architecture](architecture.md) - Technical architecture and implementation details
- [Examples](examples.md) - Common usage scenarios and configurations
- [Troubleshooting](troubleshooting.md) - Common issues and solutions

## Quick Start

For the most common use case of separating lighting and pixel outputs:

```bash
cargo run --release -- \
  --source-ip 192.168.1.100 \
  --lighting-dest-ip 192.168.1.200 \
  --pixel-dest-ip 192.168.1.201
```

This sends:
- **Lighting fixtures** (universe 1) → `192.168.1.200` (e.g., Enttec Ode MK2)
- **Pixel fixtures** (universes 2+) → `192.168.1.201` (e.g., Enttec Octo MK2)

## Getting Help

- Run `halo --help` for CLI usage
- Check [troubleshooting.md](troubleshooting.md) for common issues
- See [CLAUDE.md](../CLAUDE.md) for development commands and architecture overview