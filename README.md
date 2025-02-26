<!-- LOGO -->
<h1>
<p align="center">
  <img src="https://github.com/user-attachments/assets/66b08c09-defc-464e-a2d3-c734d92da5da" alt="Logo" width="128">
  <br>Halo
</h1>
  <p align="center">
    Lighting console bringing advanced features to solo performances.
  </p>
</p>

## About

⭕️ Halo is a real-time lighting console designed to bring modern, immersive experiences into the hands of solo
performers. Advanced lighting consoles are typically deployed at front of house (FOH) and require an experienced
lighting designer. Halo intends to push the boundaries and make advanced features accessible to solo performers during
a live show. It does this by using a combination of pre-defined cues, and beat-synchronized effects and allows for live
improvisation through the concept of overrides.

> [!WARNING]
> This project is still in heavy development and unsuitable for production use (even though I'm using it for shows).

The current version of Halo operates purely in the terminal and requires shows to be defined inline in code, but our
next goal is to build a UI that is both powerful and easy to operate on the fly during a live performance. Eventually,
we will expand the lighting engine to handle SMPTE timecode so performances can be precisely synchronized.

## Features

* **DMX Control.** Control lighting fixtures using the Art-Net protocol.
* **Ableton Link.** Create dynamic lighting effects that synchronize with your music.
* **MIDI Integration.** Control your show with MIDI devices (currently supports an Akai MPK49)
* **Fixture Library.** Built-in support for various lighting fixtures. Will support other libraries over time.
* **Cue System.** Create, save, and recall lighting scenes using cue lists and cues.
* **Effect Engine.** Various effect patterns (sine, sawtooth, square) with customizable parameters
* **Art-Net Output.** Output DMX over Art-Net to control lighting fixtures.

## Requirements

As of now, you will need the following:

* Rust toolchain (cargo, rustc)
* Network interface for Art-Net output
* Optional: MIDI controller
* Optional: Ableton Link compatible device/software

## Installation

```bash
git clone https://github.com/robmorgan/halo.git
cd halo
cargo build --release
```

## Usage

Basic usage (Art-Net Broadcast):

```bash
cargo run --release -- --source-ip <SOURCE_IP>
```

With Unicast Art-Net settings and MIDI enabled:

```bash
cargo run --release -- --source-ip 192.168.1.100 --dest-ip 192.168.1.200 --enable-midi
```

Command line options:

```bash
USAGE:
    halo [OPTIONS]

OPTIONS:
    --source-ip <IP>             Art-Net Source IP address
    --dest-ip <IP>               Art-Net Destination IP address (optional)
    --artnet-port <PORT>         Art-Net port (default: 6454)
    --broadcast                  Force broadcast mode even if destination IP is provided
    -m, --enable-midi            Enable MIDI support
```

## Roadmap & Planned Features

Here is the current roadmap (to be expanded upon later):

|  #  | Step                                                      | Status |
| :-: | --------------------------------------------------------- | :----: |
|  1  | Terminal-based lighting engine                            |   ✅   |
|  2  | Functional UI with basic show features                    |   ⌛️   |
|  3  | Timecode engine                                           |   ❌   |
|  4  | Pixel Engine                                              |   ❌   |
|  5  | Richer UI features -- RDM, config, pixel effects          |   ❌   |
|  5  | OSC / Web Support?                                        |   ❌   |
|  N  | Fancy features (to be expanded upon later)                |   ❌   |

## License

Halo is licensed under the Fair Core License, Version 1.0, ALv2.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
