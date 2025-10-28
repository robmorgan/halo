# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Commands

### Building and Running
- **Build**: `cargo build --release`
- **Run with Art-Net broadcast**: `cargo run --release -- --source-ip <SOURCE_IP>`
- **Run with unicast and MIDI**: `cargo run --release -- --source-ip 192.168.1.100 --dest-ip 192.168.1.200 --enable-midi`
- **Run with multi-destination setup**: `cargo run --release -- --source-ip 192.168.1.100 --lighting-dest-ip 192.168.1.200 --pixel-dest-ip 192.168.1.201`
- **Load a show file**: `cargo run --release -- --source-ip <SOURCE_IP> --show-file shows/Jasons40th.json`

### CLI Arguments
- `--source-ip <IP>` - Art-Net source IP address (required)
- `--dest-ip <IP>` - Single destination IP (legacy, optional)
- `--lighting-dest-ip <IP>` - Lighting fixtures destination IP (multi-destination)
- `--pixel-dest-ip <IP>` - Pixel fixtures destination IP (multi-destination)
- `--lighting-universe <NUM>` - Universe for lighting fixtures (default: 1)
- `--pixel-start-universe <NUM>` - Starting universe for pixel fixtures (default: 2)
- `--artnet-port <PORT>` - Art-Net port (default: 6454)
- `--broadcast` - Force broadcast mode
- `--enable-midi` - Enable MIDI support
- `--show-file <PATH>` - Path to show JSON file

See `docs/multi-destination-artnet.md` for detailed multi-destination Art-Net setup.

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

Halo is a real-time lighting console built with Rust, designed for solo performers. It uses a multi-crate workspace architecture with async/tokio runtime:

### Core Crates
- **`halo-core`**: Core lighting engine with async module system, DMX output, cue system, effects engine, pixel engine, and MIDI integration
- **`halo-fixtures`**: Fixture library and management system
- **`halo-ui`**: egui-based UI components and interface
- **`halo`**: CLI application and main entry point (uses `#[tokio::main]` async runtime)

### Key Systems

#### Lighting Console (`halo-core/src/console.rs`)
- Central `LightingConsole` struct manages all lighting operations
- Uses async `ModuleManager` to coordinate separate modules (`DmxModule`, `AudioModule`, `MidiModule`, `SmpteModule`)
- Channel-based communication using `ConsoleCommand` and `ConsoleEvent` via tokio `mpsc`
- Handles fixture patching, cue management, and MIDI integration
- Supports multi-destination Art-Net routing for different fixture types

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

#### Module System (`halo-core/src/modules/`)
- `ModuleManager` coordinates all async modules in separate tokio tasks
- Each module implements `AsyncModule` trait with `initialize()`, `run()`, and `shutdown()` methods
- Inter-module communication via `ModuleEvent` (DMX output, audio commands, timecode sync, MIDI input)
- Status/error reporting via `ModuleMessage` back to manager
- **DmxModule**: Real-time DMX output at 44Hz with multi-destination Art-Net routing
- **AudioModule**: Audio file playback in dedicated OS thread (not tokio task) using `rodio` and `symphonia`
- **MidiModule**: MIDI input handling and event forwarding
- **SmpteModule**: SMPTE timecode synchronization for external timecode sources

#### Pixel Engine (`halo-core/src/pixel/`)
- `PixelEngine` manages all pixel bar fixtures with per-universe routing
- Pixel effects: `Chase`, `Wave`, `Strobe`, `ColorCycle`
- `PixelEffectScope` controls effect application: `Bar` (uniform) or `Individual` (per-pixel)
- Beat-synchronized effects using rhythm detection
- Supports effect distribution across multiple fixtures (All, Step, Wave)
- Renders RGB data per universe for pixel bar fixtures

#### Configuration System (`halo-core/src/config.rs`)
- `ConfigManager` handles persistent configuration in `config.json` (repository root)
- `Settings` structure stores audio device, MIDI device, DMX settings, and fixture library preferences
- Configuration loaded at startup; CLI arguments override saved settings
- Settings UI panel allows runtime configuration changes
- Version-aware configuration with migration support

#### UI Architecture (`halo-ui/src/`)
- `HaloApp` is the main egui application with tabbed interface
- Separate panels: Dashboard, Programmer, Cue Editor, Patch Panel, Show Manager
- Real-time fixture grid visualization and control
- Timeline view for cue sequencing

### Network Configuration
- Multi-destination Art-Net architecture via `NetworkConfig` (`artnet/network_config.rs`)
- Multiple `ArtNetDestination` entries with independent broadcast/unicast modes
- Universe routing via `HashMap<u8, usize>` maps universes to destination indices
- Common setup: Universe 1 for lighting fixtures, Universes 2+ for pixel fixtures
- CLI supports separate `--lighting-dest-ip` and `--pixel-dest-ip` for easy multi-destination setup
- Legacy `--dest-ip` still supported for single-destination backward compatibility
- Art-Net output on port 6454 (configurable via `--artnet-port`)

### Show File Format
- JSON-based show files in `shows/` directory
- Contains cue lists with timing, effects, pixel effects, and fixture assignments
- Includes audio file paths for synchronized playback
- Loadable at runtime via `--show-file` parameter
- Managed through UI Show Manager with save/load functionality

## Development Notes

### Repository Conventions

#### Rust Crates
- Crate names are prefixed with `halo-`. For example, the `core` folder's crate is named `halo-core`
- When using `format!` and you can inline variables into `{}`, always do that
- Never use `unsafe` blocks or functions in any code

#### Code Formatting
After making any changes to Rust code, always run:
```bash
cargo +nightly fmt --all
```

### macOS Platform Requirement
- Development and execution require macOS due to system-specific audio and MIDI dependencies
- Uses Core Audio frameworks through `rodio` and `midir` crates

### Async Architecture
- Built on tokio async runtime with async/await throughout the codebase
- Module system uses async tasks for concurrent operation
- Channel-based communication between UI and console (`tokio::sync::mpsc`)
- AudioModule uses dedicated OS thread (not async task) for real-time audio playback

### Configuration Management
- `config.json` file in repository root stores persistent settings
- Auto-created with defaults on first run if not present
- CLI arguments override configuration file settings
- Editable through Settings panel in UI

### Fixture Patching
- Fixtures are defined in the fixture library with channel layouts
- Traditional lighting fixtures typically on Universe 1
- Pixel bar fixtures on Universes 2+ with per-universe routing
- DMX addressing starts from specified universe and channel

### Performance Considerations
- DMX module outputs at 44Hz for smooth lighting output
- Async module architecture allows concurrent operation of DMX, audio, MIDI, and timecode
- UI runs on main thread with egui's native event loop and repaint system
- Channel-based communication between UI and console for thread-safe operation
- Audio playback in dedicated OS thread for consistent timing