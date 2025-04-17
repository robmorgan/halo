<!-- LOGO -->
<h1>
<p align="center">
  <img src="https://github.com/user-attachments/assets/66b08c09-defc-464e-a2d3-c734d92da5da" alt="Logo" width="128">
  <br>Halo Roadmap 2025
</h1>
</p>

The current goal is to introduce a UI that is both powerful and easy to operate on the fly during a live performance.
We introduced a 6-week challenge to hopefully get there in time for an autumn 2025 show.

## MVP

* Play music directly from the Halo UI (no need for Ableton).
* Use a hardware mixer for effects. (e.g: DJM-800)
* Use a MIDI controller for cue and override control (e.g: Novation Launch Control XL)

## Week 1 (CW10): Core UI Components

**Deliverable:** Basic application frame with working panes.

- [ ] Create basic styling system and color theme
- [x] Implement header and footer UI
- [x] Build session pane with time display and session controls
- [x] Create cue list panel with basic cue display

## Week 2 (CW11): Fixture & Cue Functionality

**Deliverable:** Working fixture display, cue activation, and basic timeline.

- [x] Build fixture grid with selection functionality
- [ ] Implement cue activation and progress tracking
- [ ] Add basic cue functionality (play, stop, pause)
- [x] Add timeline UI with position indicator and playback controls

## Week 3 (CW12): Programmer Panel

**Deliverable:** Functional programmer that can control fixtures panel with fixture selection and effect controls.

- [x] Build parameter controls (intensity, color, position)
- [ ] Implement parameter value storage and application to fixtures
- [x] Add visual feedback for parameter changes
- [x] We need a colour mapping system to translate the RGBW values to single channel values for certain fixtures (e.g: Shehds Spots). Short-cut: would be to map the Red channel for now.
- [ ] Provide a way to hide the programmer

## Week 4 (CW13): Effects System & Overrides

**Deliverable:** Effects system with basic effects and overrides.

- [ ] Implement effects engine (waveforms, parameters)
- [x] Build effects UI in programmer panel
- [x] Add override buttons with quick-access functionality

## Week 5 (CW14): Integration & Polish

**Deliverable:** Complete, integrated application ready for testing.

- [ ] Show saving and loading
- [ ] Add patch panel functionality
- [ ] Add settings modal functionality (audio device, dmx, fixture library)
- [ ] Final UI polish and documentation
- [ ] Performance optimization and bug fixes

## Future

- [ ] Use tokio for async io + UDP
- [ ] Bring back a headless console mode with keyboard input
- [ ] 2D fixture visualization with different fixture types