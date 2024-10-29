<p align="center">
  <img width="350" height="350" src="/_docs/halo_logo.png">
</p>

⭕️ Halo is a real-time lighting console built for the console. It's lighting engine supports beat synchronized effects
using Ableton Link and SMPTE Timecode.

## Features

 * Control multiple groups of fixtures.
 * Synchronize with an Ableton Link session.

## Requirements

* Network interface for Art-Net output
* Ableton Link compatible device/software (optional)

## Usage

```bash
cargo run --release
```

## Getting Started

Halo doesn't use a programmer, editor or GUI. It is only a playback engine. You define shows ahead of time using the
show file format. At the moment this is done purely in code.

## Planned Features

- [ ] LTC/SMPTE Timecode
- [ ] Show file live reloading

## Concepts

No programmer. No editor. Halo is only a playback engine. You do the programming in code using HScript.

 * Surfaces: Ways to control the show or view output. Usually a MIDI interface or LCD.
 * Cue: TODO
 * Cue List: A collection of cues. Can be started, stopped, etc.
 * Executor: Cue Lists can have multiple executors to run tasks in parallel.
 * Fader: TODO
 * Scene: TODO
 * Fixture: A device that emits light.
 * Scene: A programmed look you have set up the lights to display.
 * Venue: Constrain movers and custom presets. (could skip this)

## Fixture Profiles

 * Float
 * Float16 (two channels)
 * RGB (color mixer)
 * Bool: Boolean value on a single DMX channel: 0-127 means false, 128-255 means true

## Features

 * Fixture Test Mode
   * Cycle through fixtures and highlight them to test patches.
 * Playback Engine
   * Save/Load Shows
   * SMPTE Input
 * Beat Sync
   * Ableton Link
 * Pixel Engine
   * Support WS??? LED lights. Enttec Octo
   * Convert videos into pixels?
   * There is no GUI pixel mapping. You need to do this yourself.
 * OSC Support
   * Start show
   * Cues
     * Go
     * Jump to Cue

## References

 * https://opensource.com/article/17/5/open-source-lighting
 * https://dev.to/davidsbond/golang-reverse-engineering-an-akai-mpd26-using-gousb-3b49
 * https://corylulu.github.io/VDocs/NodeIODMX.html?itm=174
 * https://github.com/node-dmx/dmx
 * https://github.com/qmsk/dmx
