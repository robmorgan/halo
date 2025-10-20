<!-- LOGO -->
<h1>
<p align="center">
  <img src="https://github.com/user-attachments/assets/66b08c09-defc-464e-a2d3-c734d92da5da" alt="Logo" width="128">
  <br>Halo
</h1>
  <p align="center">
    <img src="https://github.com/user-attachments/assets/9c5dae6a-0f76-417e-bd9e-6472253865ba" alt="Halo Screenshot" width="600">
    <br>
    <strong>Lighting console bringing advanced features to solo performances.</strong>
  </p>
</p>

## About

⭕️ Halo is a real-time lighting console, designed to bring modern, immersive experiences into the hands of solo
performers. Traditional consoles are typically deployed at front of house (FOH) and require a dedicated lighting
designer. On the other hand, software designed for solo performers is often limited in features and is difficult to
operate during a live show. Halo bridges this gap through a combination of pre-defined cues, beat-synchronized
effects, and live improvisation through MIDI overrides. This enables performers to elevate their shows with immersive
lighting that responds to their performance.

> [!WARNING]
> This project is still in heavy development and unsuitable for production use (even though I'm using it for shows).


## Features

* **Intuitive UI.** Featuring a Dashboard, Programmer, Cue Editor, Patch Panel, Show Manager, and Settings panels.
* **Show File System.** Save and load complete shows with cues, effects, and fixture assignments.
* **Programmer.** Control lighting fixtures using a professional programmer interface with real-time feedback.
* **Cue System.** Create, save, and recall lighting scenes using cue lists with timecode synchronization.
* **Effect Engine.** Beat synchronized effects engine with sine, sawtooth and square patterns plus customizable parameters.
* **Pixel Engine.** Dedicated pixel engine for displaying various effects and colors on pixel bar fixtures.
* **Multi-destination Art-Net.** Route different fixture types (traditional lighting, pixels) to separate Art-Net nodes.
* **Audio Playback.** Integrated audio file playback synchronized with cues and Ableton Link.
* **SMPTE Timecode.** Both internal and external timecode synchronization for precise show timing and automation.
* **MIDI Integration.** Control your show with MIDI devices (currently supports Akai MPK49) with override system.
* **Configuration System.** Persistent settings with UI-based configuration panel for audio, MIDI, and DMX preferences.
* **Fixture Library.** Built-in support for various lighting fixtures with extensible fixture definitions.
* **Async Module Architecture.** Separate modules for DMX (44Hz output), Audio, MIDI, and Timecode running concurrently.

## Requirements

* **macOS** (required for Core Audio and MIDI dependencies)
* **Rust toolchain** (cargo, rustc) - MSRV: 1.76.0
* **Network interface for Art-Net output** (e.g., [Enttec ODE MK2](https://support.enttec.com/support/solutions/articles/101000438016-ode-mk2-70405-70406-))
* **Optional:** MIDI controller (e.g., Akai MPK49, Novation Launch Control XL)
* **Optional:** Ableton Link compatible device/software for beat synchronization

## Installation

```bash
git clone https://github.com/robmorgan/halo.git
cd halo
cargo build --release
```

## Usage

### Basic Usage

Start with Art-Net broadcast mode:

```bash
cargo run --release -- --source-ip <SOURCE_IP>
```

Load a show file:

```bash
cargo run --release -- --source-ip <SOURCE_IP> --show-file shows/myshow.json
```

### Multi-Destination Setup

Route lighting and pixel fixtures to separate Art-Net nodes:

```bash
cargo run --release -- --source-ip 192.168.1.100 \
  --lighting-dest-ip 192.168.1.200 \
  --pixel-dest-ip 192.168.1.201 \
  --enable-midi
```

See [docs/multi-destination-artnet.md](docs/multi-destination-artnet.md) for detailed multi-destination configuration.

### Command Line Options

```bash
USAGE:
    halo [OPTIONS]

OPTIONS:
    --source-ip <IP>                Art-Net source IP address (required)
    --dest-ip <IP>                  Single destination IP (legacy, optional)
    --lighting-dest-ip <IP>         Lighting fixtures destination IP
    --pixel-dest-ip <IP>            Pixel fixtures destination IP
    --lighting-universe <NUM>       Universe for lighting fixtures (default: 1)
    --pixel-start-universe <NUM>    Starting universe for pixel fixtures (default: 2)
    --artnet-port <PORT>            Art-Net port (default: 6454)
    --broadcast                     Force broadcast mode
    -m, --enable-midi               Enable MIDI support
    --show-file <PATH>              Path to show JSON file
```

## Documentation

For detailed documentation, see the [docs/](docs/) directory:

* [Architecture Overview](docs/architecture.md) - System design and component structure
* [CLI Reference](docs/cli-reference.md) - Complete command-line interface documentation
* [Multi-Destination Art-Net](docs/multi-destination-artnet.md) - Setting up multiple Art-Net destinations
* [Troubleshooting Guide](docs/troubleshooting.md) - Common issues and solutions

## License

Halo is licensed under the Fair Core License, Version 1.0, ALv2.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
