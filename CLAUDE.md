# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Commands

### Building and Running
- **Build**: `cargo build --release`
- **Run with Art-Net broadcast**: `cargo run --release -- --source-ip <SOURCE_IP>`
- **Run with unicast and MIDI**: `cargo run --release -- --source-ip 192.168.1.100 --dest-ip 192.168.1.200 --enable-midi`
- **Load a show file**: `cargo run --release -- --source-ip <SOURCE_IP> --show-file shows/Guys40th.json`

### Development Tools
- **Check compilation**: `cargo check --workspace --all-targets`
- **Format code**: `cargo +nightly fmt --all` (requires nightly toolchain for unstable formatting options)
- **Lint**: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- **Test**: `cargo test --workspace`
- **Install nightly toolchain**: `rustup toolchain install nightly` (required for formatting only)

### Toolchain Requirements
- **Stable Rust**: Used for building, testing, and linting (MSRV: 1.76.0)
- **Nightly Rust**: Required only for formatting due to unstable options in `rustfmt.toml`

## Architecture Overview

Halo is a real-time lighting console built with Rust, designed for solo performers. It uses a multi-crate workspace architecture:

### Core Crates
- **`halo-core`**: Core lighting engine with DMX output, cue system, effects engine, and MIDI integration
- **`halo-fixtures`**: Fixture library and management system
- **`halo-ui`**: egui-based UI components and interface
- **`halo`**: CLI application and main entry point

### Key Systems

#### Lighting Console (`halo-core/src/console.rs`)
- Central `LightingConsole` struct manages all lighting operations
- Spawns background `EventLoop` thread for real-time DMX output at 44fps
- Handles fixture patching, cue management, and MIDI integration
- Uses Art-Net protocol for DMX over network

#### Cue System (`halo-core/src/cue/`)
- `CueManager` handles playback state and timecode synchronization
- Supports both internal and external SMPTE timecode
- `CueList` contains sequences of `Cue` objects with static values and effects
- Audio playback synchronization with Ableton Link

#### Effect Engine (`halo-core/src/effect/`)
- Mathematical effect generators: sine, sawtooth, square waves
- Beat-synchronized effects using rhythm detection
- Effect distribution across multiple fixtures with customizable parameters

#### MIDI Integration (`halo-core/src/midi/`)
- MPK49 controller support for live performance
- `MidiOverride` system for real-time control during shows
- Actions include static values and cue triggering

#### UI Architecture (`halo-ui/src/`)
- `HaloApp` is the main egui application with tabbed interface
- Separate panels: Dashboard, Programmer, Cue Editor, Patch Panel, Show Manager
- Real-time fixture grid visualization and control
- Timeline view for cue sequencing

### Network Configuration
- Art-Net output on port 6454 (configurable)
- Supports both broadcast and unicast modes
- Source and destination IP configuration via CLI

### Show File Format
- JSON-based show files in `shows/` directory
- Contains cue lists with timing, effects, and fixture assignments
- Loadable at runtime via `--show-file` parameter

## Development Notes

### macOS Platform Requirement
- Development and execution require macOS due to system-specific audio and MIDI dependencies
- Uses Core Audio frameworks through `rodio` and `midir` crates

### Fixture Patching
- Fixtures are defined in the fixture library with channel layouts
- Default fixture setup in `main.rs` includes PARs, spots, wash lights, and smoke machine
- DMX addressing starts from specified universe and channel

### Performance Considerations
- Event loop runs at 44fps for smooth lighting output
- UI updates independently at 60fps using egui's repaint system
- Uses `parking_lot::Mutex` for thread-safe console access between UI and lighting threads