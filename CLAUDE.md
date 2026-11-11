# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Halo is a 2-deck DJ app for macOS with an integrated lighting console (Art-Net output driven by per-track cues
and a live programmer). `ROADMAP.md` is the authoritative description of the architecture and feature plan —
read it before designing anything non-trivial.

## Common Commands

- **Build**: `cargo build --release`
- **Run**: `cargo run --release` (optionally pass an audio file path as the only positional arg to load it on launch)
- **Check compilation**: `cargo check --workspace --all-targets`
- **Format code**: `cargo +nightly fmt --all` (nightly required — `rustfmt.toml` uses unstable options)
- **Test**: `cargo test --workspace`
- **Lint**: `cargo clippy --workspace --all-targets` (currently not enforced in CI)
- **macOS app bundle**: `cargo bundle --release` (requires `cargo-bundle`)

There are no CLI flags. Art-Net destination, audio device, and other settings are configured in-app and
persisted (see Persistence below). Logging is controlled with `RUST_LOG`.

### Toolchain

- **Stable Rust** for building/testing (`rust-toolchain.toml` and CI pin 1.95.0, egui 0.37's minimum); **nightly** only for formatting.
- The `timestretch` crate is a **path dependency on a sibling clone**: `../timestretch-rs` next to this repo
  (from `crates/halo`, `path = "../../../timestretch-rs"`). CI checks out `robmorgan/timestretch-rs` to that
  location, pinned to v0.16.0. Enable `analysis` and `io`. Builds fail without it.

## Architecture Overview

Two workspace crates:

- **`crates/halo`** — the application: UI (egui/eframe, wgpu backend), audio engine, decks, mixer DSP, track
  library, analysis workers, and the DMX engine thread.
- **`crates/halo-light`** — UI-free, audio-free lighting domain library: fixtures, cues, programmer resolution,
  DMX rendering, Art-Net transport.

### Threading model (no tokio)

Plain threads + channels/atomics; each session has one owner:

- **UI thread** — egui at ~30 fps; publishes atomic controls and reclaims retired playback storage.
- **Render worker per loaded player** — owns a `PreparedSession`/`StreamSession`, source requests,
  seek/loop/EOF transactions, output reserve, consumption receipts and shared playhead. Includes audition.
- **Audio callback thread (cpal)** — consumes completed audio, renders scrub voices, applies channel
  strips, faders, master and limiter, the headphone cue bus (pre-fader taps, CUE MIX, its own limiter), and
  runs the master clock synced decks lock to. Fixed scratch storage, no locks or allocations,
  no session reset/processing/destruction. A bounded handoff returns old storage to the UI.
- **Worker threads** (`std::thread` + `mpsc`, drained per frame by the UI) — library import, track analysis.
- **DMX engine thread** — 44 Hz tick: reads deck playheads from atomics, `resolve()` → `render()` → Art-Net send.
- **Controller input** — CoreMIDI's callback thread (via `midir`) and the Z1's HID thread only parse messages
  into an `mpsc` queue; the UI thread applies them and sends LED/meter feedback. All hidapi calls stay on the
  Z1 thread: macOS spins the caller's run loop while enumerating, which panics winit on the main thread.

### Key modules — `crates/halo/src`

- `main.rs` — eframe/wgpu entry point; positional file arg; `env_logger`.
- `app.rs` — `HaloApp`: views (Prepare/Perform), deck UI, mixer UI, rig ownership, PATCH tab, Options wiring.
- `deck.rs` — worker-owned sessions, typed controls, transport transactions and consumed playheads.
- `audio.rs` — cpal stream setup: 4 channels on a device that has them (master 1–2, headphone cue 3–4).
- `delivery.rs` — allocation-free callback mixer, scrub handoff and retirement queues.
- `output_queue.rs` / `render_worker.rs` — epoch-aware SPSC output and bounded session pump.
- `worker_priority.rs` — host-owned scheduling policy (macOS audio priority).
- `crossover.rs` — Halo-owned LR4 filters retained for the isolator EQ.
- `dsp.rs` — `IsolatorEq` (LR4 crossover, full-kill), `DjFilter` (RBJ biquad LP/HP), `Limiter`.
- `state.rs` — shared atomics (`DeckShared`, `MixerShared`), scrub state, meters, CPU load.
- `scrub.rs` — audible scrub: varispeed voice with momentum glide/settle, engine↔voice crossfade.
- `waveform/` — 3-band RGB peaks pyramid, overview strip, zoomed beat-grid view, trigger-lane strips + cue editor.
- `decoder.rs` — symphonia decode (mp3/flac/ogg/wav) to interleaved stereo `f32`.
- `worker.rs` — background import/analysis workers (own DB connections).
- `library.rs` — SQLite (rusqlite, bundled): `tracks`, `playlists`, `playlist_tracks`, `lighting_cues`, `settings`.
- `footer.rs` — the footer: help for the control under the pointer (`StatusHelp::status_help` in place of
  tooltips) and the background task well/tray (`Tasks`: Attention > Running > Done, Done expires after 4 s).
- `options.rs` — the Options modal (AUDIO / LIGHTING / MIDI), edited as a draft and applied on SAVE.
- `midi.rs` — controller ports, messages, profiles (built-in + user, MIDI learn), the action router, feedback.
- `z1.rs` — Traktor Kontrol Z1 over HID (MODE, 12-bit knobs, 7-step meters), macOS only; its built-in
  profile is in `midi.rs`.
- `dmx.rs` — `spawn_dmx_engine`, the 44 Hz Art-Net output thread (has an end-to-end UDP test).
- `programmer_ui.rs` — lighting programmer surface (fixture grid, five parameter views, effect panels).
- `show.rs` — `simulate_show()`: deterministic demo cue generator, not real authored content.
- `fader.rs` / `knob.rs` — custom egui widgets.
- `theme.rs` — the Halo design system: tokens by role name, type styles, fonts (Inter for reading
  text, Barlow Condensed + a tabular copy for labels and numbers, JetBrains Mono for DMX addresses),
  `face-*` textures and raised / lit / well / LED / panel paint helpers, and `tray` / `tray_divider`
  for rows of keys sunk in a well. Use its tokens; don't hand-mix colours.
- `key.rs` — the `Key` widget every performance-surface button is built from (lit faces, LEDs, pressed,
  the decks' `quiet` skin), `segmented` strips of keys joined into one (the LOOP row, hot cues, the view
  switcher), and `latch`, the rectangle latch that fills red when on (A.CUE, Q).

### Key modules — `crates/halo-light/src`

- `fixture.rs` — `Rig`, grid/selection types, `default_rig()`.
- `fixture_library.rs` — `FixtureProfile`/`Channel`/`ChannelType`, hardcoded profile registry, patching.
- `cues.rs` — `CueSet` (runtime, non-overlapping per lane) + `CueFile` (persisted JSON, seconds).
- `programmer.rs` — `resolve()`: **the single merge point** (programmer override > track cues > off).
- `output.rs` — pure `render()`: resolved lanes + params → per-universe `[u8; 512]` DMX frames.
- `artnet.rs` — synchronous Art-Net over `std::net::UdpSocket` (broadcast/unicast, multi-destination).

### Persistence

- **eframe persistence** (`PERSIST_KEY = "halo"`) — UI/mixer state: trims, keylock, pitch range, quantize,
  gated mode, audition volume, device/buffer, headphone cue on/off and CUE MIX, sort order.
- **Library DB `settings` table** — Art-Net config, rig patch, and `midi` (ports, profile, user profiles).
- There is no config file; `config.json` in old branches/history is from the pre-pivot console.

## Conventions

- Crate names are prefixed with `halo-` (the `halo-light` crate lives in `crates/halo-light`).
- **Never use `unsafe`** blocks or functions in any code.
- When using `format!` (and friends) and a variable can be inlined into `{}`, always inline it.
- **No tokio** — concurrency is plain threads + `mpsc` + atomics. Don't reintroduce async scaffolding.
- The audio callback must never allocate, lock, or panic.
- `programmer::resolve()` stays the single merge point for lighting state; `output::render()` stays pure.
- After changing Rust code, always run `cargo +nightly fmt --all`.

## CI

`.github/workflows/rust.yml` runs on macOS arm64 and Linux x86_64: checks out `timestretch-rs` as a sibling,
then `cargo +nightly fmt --check`, `cargo build`, `cargo test` on stable 1.95.0.
