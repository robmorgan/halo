# Halo: ShowControl for DJSL

I wanted to perform a live show powered by Ableton.

TUI-based lighting console.

Halo is a lighting console built for the console.

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

## TODO

 - [ ] you could make a float16 property to map a value over two DMX channels. Might help to fine-tune moving heads and create smoother looks? e.g: 180+16
 - [ ] You could convert your system to run off percentages. e.g: 0.0 - 1.0. Floats. so things are basically percentage based.

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

## Milestones

1. **Milestone 1:** Playback scenes and sequences.
2. **Milestone 2:** Surfaces for input and output. Ableton Link.
3. **Milestone 3:** Venues. Camera Fixtures (for helping record in Capture)

## Requirements

 * OLA

## Usage

Start OLAD in debug mode in another terminal window:

```bash
$ olad -l 3
```

Then start Halo:

```bash
$ ./halo
```

## References

 * https://opensource.com/article/17/5/open-source-lighting
 * https://dev.to/davidsbond/golang-reverse-engineering-an-akai-mpd26-using-gousb-3b49
 * https://corylulu.github.io/VDocs/NodeIODMX.html?itm=174
 * https://github.com/node-dmx/dmx
 * https://github.com/qmsk/dmx

## Libraries

 * https://github.com/google/gousb: For reading/writing MIDI control surfaces.
 * https://github.com/trimmer-io/go-timecode
 * https://github.com/gomidi/midi
 * https://github.com/fogleman/ease: Easing Functions
