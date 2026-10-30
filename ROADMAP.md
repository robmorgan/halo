# Halo — Roadmap

Halo is a 2-deck DJ app for macOS, written in Rust. It uses [`timestretch`](https://github.com/robmorgan/timestretch-rs) for real-time tempo/pitch control and beat analysis, egui/eframe (wgpu backend) for the UI, cpal for audio output, and symphonia for decoding.

The `timestretch-rs` desktop reference app (`timestretch-rs/desktop/`) is a proven single-deck implementation of most of the hard wiring — decoder, worker-owned sessions, device delivery, waveform rendering, gapless loops, scrub. Halo's early phases are largely about porting those patterns into a two-deck architecture, then building the DJ-specific features (mixer, EQ, sync, hot cues, library) on top.

## Ground rules

- **`timestretch` is referenced by local path** from `crates/halo`, with CI pinned to the v0.16.0 release commit:

  ```toml
  timestretch = { version = "=0.16.0", path = "../../../timestretch-rs", features = ["analysis", "io"] }
  ```

- **All audio is `f32` interleaved stereo.** The audio callback is allocation-free and lock-free; UI intent uses atomics, rendered audio uses bounded SPSC queues, and track handoffs use bounded ownership queues. Callback storage is preallocated; retired storage is reclaimed on the UI.
- **Musical key is read from file tags only** (Mixed In Key / Rekordbox style tags). No in-app key detection before 1.0.

## Architecture overview

Each loaded player (Deck A, Deck B, Prepare audition) owns one render worker:

```
UI thread (egui, ~30 fps while playing)
    │ atomic transport / tempo + bounded seek / loop commands
    ▼
Render worker × 3 (one per loaded player)
    PreparedSession → StreamSession, source demand, seek/loop/EOF,
    bounded render pump, applied-ratio timing tags, consumption receipts
    │ preallocated output queue              ▲ consumption receipts
    ▼                                       │
Audio callback (cpal)
    delivery clock + beat-phase launch coordinator → shared playheads
    A queue + scrub → EQ/filter → trim → fader ┐
    B queue + scrub → EQ/filter → trim → fader ├ crossfader → master → limiter
    Audition queue + scrub → strip → fader ───┘ (bypasses crossfader)
```

Session preparation, processing, restart and destruction stay off the callback.
Source frames remain in the device-rate domain: Halo resamples on load, and
rescales native-rate analysis for waveforms and cues. Halo's ratio-derived
delivery cursor drives both deck displays and the independent 44 Hz DMX engine. Each FIFO frame carries the applied source advance, split at
`next_applied().applied_at` boundaries; old queued tags survive tempo changes.
Only delivered frames advance the cursor. Epochs carry their anchor, rate
conversion and loop bounds; continuous wraps retain iteration counts. These are
software delivery coordinates, not exact sample origins or speaker timestamps.
Library receipts remain authoritative for delivered EOF. Coherent per-deck
snapshots include the common output-frame clock, track/anchor versions, fractional
position and loop iteration.
Seek/loop changes invalidate old output with epoch boundaries, including while
paused. A running loop preserves DSP history. EOF means delivered audio has
been acknowledged, not merely rendered or collected. Scrub landing preparation
runs on the worker without a UI round trip.

The initial output reserve is 50 ms, following the upstream desktop. The deck
latency readout includes session control boundaries and pending/unacknowledged
output; it is not physical speaker latency. Auto selects Accelerate on macOS
and Portable elsewhere. No analysis artifact is a DSP input.

Qualification: `cargo test --workspace` covers receipt/epoch races, callback
allocation and retirement, three-player transport, scrub handoff, device reloads,
cache compatibility, bass, stereo separation and pitch intent. Device reloads
retain source paths independently of library IDs and restart pending decodes at
the new rate. The opt-in scheduling checks run with
`cargo test --release -p halo qualification -- --ignored --nocapture --test-threads=1`:
the synthetic harness covers three players plus background analysis at
44.1/48/96 kHz and 128/512/2048-frame callbacks; the device harness opens the
default output with its master muted and checks playback, pause, seek and loops.

The v0.16 migration was validated on macOS arm64 and emulated Linux x86_64 with
Rust 1.90.0: 145 tests pass on each, with both scheduling checks excluded from
ordinary tests. Linux builds and the macOS release build pass; nightly formatting
and Clippy with warnings denied are clean. All nine synthetic scheduling cases
pass with zero underruns and callbacks within their device budgets (maximum
observed callback: 0.69 ms). The MOTU M4 device check at 96 kHz passes all three
buffer sizes with zero underruns. Subjective listening is a
separate release check; these automated checks verify scheduling and measured
signal properties.

---

## Phase 0 — Foundation

**Goal:** a running app shell that can decode a file and play it.

- Cargo binary crate, `timestretch` by local path.
- Dependencies: `eframe` (default-features off; `accesskit`, `default_fonts`, `wgpu` — the wgpu backend is a deliberate CPU-efficiency choice on macOS), `cpal`, `symphonia` (mp3/flac/ogg/wav/pcm/vorbis), `rfd`, `log`/`env_logger`.
- Port `desktop/src/decoder.rs` → decode any supported file to interleaved stereo `f32`.
- Single cpal output stream with a stub callback; error handling that never panics in the callback.
- App shell: window, dark CDJ-style theme, file-open dialog.

**Milestone:** open a file, hear raw (un-stretched) playback.

## Phase 1 — Dual-deck core + mixer basics

**Goal:** two independent decks mixed to one output — the audio-path skeleton every later phase builds on.

- `Deck` owns a worker-driven `StreamSession` and an output queue, shared by performance decks and Prepare audition. The worker owns source demand, seeks, loops, EOF and delivery receipts; the callback publishes the playhead.
- Mixer stage in the audio callback: per-deck gain (trim) and channel fader, constant-power crossfader, sum to output.
- Transport per deck: play/pause and CDJ-style cue (set cue while paused, press to return + hold-to-preview, release returns to cue).
- **Basic CPU meter** — callback processing time ÷ buffer duration, published via atomic. Built now, deliberately early: it's the tool that validates the DSP budget for every phase after this.

**Milestone:** load a track on each deck and mix between them by ear with volume + crossfader.

## Phase 2 — Analysis + rich waveform display

**Goal:** the full CDJ-style deck display.

- On load: downmix and `timestretch::detect_beat_grid` for an initial BPM, then background `analyze` → `PreAnalysisArtifact` cached in SQLite. Compatible `.tsa`/legacy sidecars can be imported. Analysis updates display metadata without rebuilding playback.
- Port `desktop/src/waveform/`: 3-band RGB peaks pyramid (`peaks.rs`), full-track overview strip as GPU texture (`overview.rs`), centered-playhead zoomed scrolling waveform with beat/bar/phrase marks (`zoomed.rs`, `mod.rs`), audible scrub gesture (`scrub.rs`).
- Track artwork from file tags via `lofty`, displayed in the deck header.
- Elapsed + remaining time readouts; **prominent BPM display** = detected BPM × current tempo rate, updating live.

**Milestone:** both decks show artwork, zoomed waveform with beat grid, overview strip with playhead, elapsed/remaining, and live BPM.

## Phase 3 — EQ + filter (completing the channel strip)

**Goal:** full per-deck mixer DSP. Independent of Phase 2 — can overlap with it.

- 3-band isolator EQ per deck (low / mid / high, full-kill at minimum). Halo owns the LR4 crossover in `crossover.rs`, retained from timestretch 0.15 after its public DSP helpers were removed.
- LP/HP filter per deck: mode toggle (low-pass / high-pass) + cutoff knob, gentle resonance.
- Final DSP chain order: engine → gain → EQ → filter → fader → crossfader → sum. Smooth all parameter changes (per-block ramps) to avoid zipper noise.
- Verify CPU headroom with the Phase 1 meter.

**Milestone:** EQ-kill mixing and filter sweeps on both decks, no clicks or zipper noise.

## Phase 4 — Tempo, pitch & sync

**Goal:** beatmatching — manual and one-button.

- Tempo slider per deck with ±8 / ±16 / ±50% and WIDE ranges, mapped to typed `TempoRate` controls. WIDE supports 1/32–4x with keylock or 1/4–4x with linked pitch, plus a zero-speed transport detent. It no longer selects a different processor.
- Keylock selects `Parameters::keylock` or `Parameters::linked`; switching off below 1/4x visibly raises the fader to that supported minimum.
- Pitch bend / nudge buttons (temporary rate offset while held).
- **Persistent Sync/Master:** Sync intent survives pause and automatic master promotion.
  A paused follower matches tempo without moving its exact parked coordinate.
  The matched rate uses the tracks' analyzed BPMs and the master's base tempo;
  detected beat intervals guide initial alignment without changing that BPM target.
  Play prepares audio there, then the callback opens delivery at the first matching
  individual beat phase, including between-beat positions and crossings inside a
  callback. A second Play cancels. Already prepared, aligned media starts immediately.
- **Preparation and feedback:** runtime telemetry distinguishes Preparing, Waiting,
  Locked, Nudging and Unavailable. The waiting ghost approaches the stationary
  playhead from the left using actual remaining phase distance; preparation has its
  own status. Track/transport/anchor/grid/master revisions reject stale preparation.
  Grid ownership uses bounded handoff and UI retirement, as playback storage does.
- **Established lock:** capture both delivery cursors at launch and compare their
  subsequent source advance in nominal beats, including loop iterations. Sparse
  audio, missing markers and wandering detected onsets cannot manufacture drift
  or restart a playing follower. EQ, filter and fader changes never enter this
  clock calculation. This is fixed-BPM sync, not automatic tempo-ramp following.
- **Re-lock:** explicit engagement during playback or delivery-phase error above
  1/8 beat fades out over 5 ms, seeks the nearest aligned position on the worker, primes, waits for
  matching phase, then fades in over 5 ms. Smaller drift uses at most ±1.5% correction,
  starting above 1/128 beat so rounded DSP tempos are corrected before the re-lock threshold;
  error above 1/64 beat for two delivered beats after controls reach delivery also
  forces re-lock. Held nudges suspend correction; release reacquires. Manual pitch
  movement disengages Sync. Master seeks and scrub epochs invalidate the captured
  relationship, as do track, grid and master changes.
- **Master:** the callback retains the current playing master unless explicitly
  changed. Pause, stop, EOF and unload transfer master to the other playing deck;
  faders and underruns do not. MASTER, BPM, Sync and lighting use that same choice.
  MASTER and SYNC indicators are exclusive: promotion hides the retained Sync
  intent until the deck becomes a follower again.
  The DMX snapshot carries both decks so handoff works through UI stalls.
- **Recovery and limits:** seek/hot-cue, loop edits and scrub landings establish new
  epochs and reacquire before delivery. Integer-beat loops preserve continuous
  phase; incompatible loops, missing grids and unsupported matched tempos report
  Unavailable while allowing ordinary playback and retaining intent. The first
  deck started with no playing master starts normally. Reverse and bar/phrase sync
  are not implemented. No database migration or timestretch API change is needed.

**Milestone:** press Sync and the decks lock in phase; manual beatmatching works with slider + nudge.

Persistent Sync validation (macOS arm64, 2026-10-30): 167 workspace tests pass,
including on CI's Rust 1.90.0; strict Clippy, nightly formatting and the release
build pass. The deterministic launch harness covers 60 phase/rate combinations,
including fractional parked positions and delayed workers, with a maximum error
of 0.9976 output frames; all 60 launches occur inside callback blocks. Real worker
tests cover Play, seek/hot-cue requests, loop edits and scrub recovery without UI
polling. Cached beat timings from a jittered 130 BPM master and straight 124 BPM
follower reproduce the tempo regression: the matched BPM now stays fixed while
armed and playing. A breakdown regression reproduces a false re-lock before the
fix and verifies uninterrupted delivery through wandering/missing markers, silent
audio and the end of detected beats on either deck, with master EQ/filter cuts.
It also checks zero callback allocations, unchanged launch identity, and complete
delivery receipts. Loop/conversion and master seek/scrub epoch tests protect the
captured phase reference. Callback allocation/retirement tests include waiting
launches and grid replacement. All nine three-player scheduling cases have zero
underruns and stay within their callback budgets (maximum observed in the
qualification run: 0.741 ms). The muted M4 device check at 96 kHz passes all three buffer sizes with
zero underruns. Subjective listening for launch timing, forced transitions and
nudge recovery remains a manual release check.

A rounded-tempo regression also covers two minutes of delivery with applied-rate
errors in either direction and delayed controls. It reproduced a false re-lock
after about 40.6 seconds before the correction deadband was reduced. The updated
suite has 168 passing tests; a 95-second muted M4 run at 96 kHz with the reported
118/130 BPM track pair retained one launch and had zero underruns.


## Phase 5 — Performance features: hot cues + loops

**Goal:** performance-ready decks.

- **Hot cues**, 8 per deck: Normal mode (empty slot = set, occupied = jump; delete via modifier) and **Gated mode** (plays from the cue while held, stops on release). Optional quantize snaps stored/triggered cues to the beat grid (`BeatGrid::snap_to_grid`).
- **Loops:** manual loop in/out; **4-beat quantized loop** button snapped to the grid; halve/double loop-size controls covering **1/16 beat up to 16 beats** (`snap_to_subdivision` for sub-beat sizes). Use the grid-quantized autoloop ladder and session-owned continuous wraps (no processor reset across the seam). Enabling/resizing/disabling loops is an epoch transaction; seeks outside a loop disable it first.
- Loop + hot cue state survives seeks and tempo changes; active loop drawn on both waveforms.

**Milestone:** finger-drum hot cues in both modes; set, resize (1/16→16 beats), and exit loops seamlessly.

## Phase 6 — Library + track browser

**Goal:** prepare and play a full set without a file dialog.

- SQLite library via `rusqlite`: tracks table (path, tags, duration, BPM, key, artwork ref), playlist tree (folders + playlists), analysis cache keyed by the artifact's content hash. One-time import of existing `.halo.tsanalysis.json` sidecars, after which the DB is the only analysis cache (no new sidecars written).
- Use `PreAnalysisArtifact::resample_to(rate)`: analysis runs **once at the file's native sample rate** and is rescaled to the engine/device rate on load. This keeps one analysis row per track instead of duplicates per output-device rate, and stops a device switch (48 kHz interface → 44.1 kHz headphones) from invalidating the cache.
- Import: add folders, scan, read tags with `lofty` (title / artist / album / **key** / artwork), queue background analysis for BPM + beat grid.
- Browser UI: left tree panel (playlist folders → playlists), right table with sortable columns (title, artist, BPM, key, duration, date added), search box, drag or button to load a track to a deck.

**Milestone:** import a music folder, build a playlist, sort by BPM/key, and load tracks to decks from the browser.

## Phase 7 — Polish

- CPU meter promoted from dev readout to a proper always-visible indicator (audio-callback load + process CPU).
- Audio device selection and buffer-size/latency settings.
- Soft limiter on the master output.
- Keyboard shortcuts for transport, cues, loops, and browser navigation.
- macOS app bundle + icon; persistence of UI/mixer state between sessions.

**Milestone:** a build you'd hand to another DJ.

---

## Lighting & FX

Halo drives show lighting alongside the decks. Current state (branch
`mixer-deck-ui-overhaul`): per-deck trigger lanes under the waveforms —
now the **Phase L3 preview** role lanes (Look / Energy / Accent,
`show_preview.rs` + `show_strip.rs`/`show_editor.rs`), interactive and
session-only: seeded per track, edited in Prepare, not persisted, and
not yet wired to the DMX path. The legacy three-lane layer remains
underneath: editable per-track cues persisted in the library
(`lighting_cues` table, seconds-based JSON) still feed the rig and
STORE-from-live; Prepare/Perform views with an
independent audition player and a direct-manipulation cue editor; a
console-style programmer override layer resolved per lane
(Programmer > track cues > off, `programmer::resolve()` as the single
source of truth) with STORE-from-live; and the programmer surface —
fixture grid over a simulated rig, group selects, five parameter views
(Intensity / Color / Position / Beam / Pixel FX) with beat-synced effect
panels. The fixture engine core has landed: the default rig is patched
from real library profiles (auto-addressed across 5 universes),
`output::render()` flattens resolved lanes + programmer params into
per-universe DMX frames, and a 44 Hz engine thread sends them over
Art-Net independently of the UI (playhead read live from the deck
atomics, so cues keep firing through UI stalls). The PATCH footer tab
edits the rig live (profile/universe/address/grid, conflict detection,
add/unpatch/reset) and persists it to the library DB; the settings
window picks broadcast vs unicast-to-node, also persisted. **Phase L1 is
functionally complete** pending hardware validation; per-fixture
pan/tilt limits in the patch sheet are a small leftover.

### Phase L1 — Fixture engine

**Goal:** the programmer stops being a mockup — selection and values
drive real per-fixture output.

The previous console (`../halo-old`, tokio-based) already has proven
implementations of the two hard pieces — the fixture library and Art-Net
output — and both are synchronous underneath: the tokio in halo-old is
orchestration scaffolding (`AsyncModule` / `ModuleManager`) around sync
domain code. The import strategy is to take the domain code and leave the
scaffolding; Halo's existing worker pattern (`std::thread` + `mpsc`,
drained per-frame) replaces it. **No tokio dependency.**

**Step 1 — import the fixture library**
(`halo-old/crates/fixtures`, ~650 lines, serde-only deps):

- Port nearly verbatim as `halo-light`'s `fixture_library.rs`: `FixtureProfile`
  (manufacturer/model + channel layout), `FixtureLibrary` (profile
  registry — hardcoded profiles for now, disk-loaded later, as halo-old
  already noted), `Fixture` (id, profile ref, universe, start address,
  live channel values), `Channel` / `ChannelType` (including indexed
  `PixelRed(n)`/`PixelGreen(n)`/`PixelBlue(n)` for pixel bars), and
  `PanTiltLimits`.
- Merge with the existing `fixture.rs` simulation rather than replacing
  it: the new grid/selection types (`FixtureKind`, groups, grid
  position) stay as the UI layer; each grid fixture gains a patched
  `Fixture` + profile behind it. `default_rig()` becomes a default patch
  built from real profiles instead of a mockup.
- Real patching UI: assign profile, universe + start address, grid
  position; persisted in the library DB (profiles by id; channel values
  are `#[serde(skip)]` and rebuilt from the profile on load, as in
  halo-old).

**Step 2 — per-fixture output state:**

- Output computed from cues + programmer; the grid's selection becomes
  the target for applied values and effects (`EffectConfig` evaluated
  per fixture with Step/Wave distribution). `resolve()` stays the single
  merge point and now yields per-fixture channel values, flattened into
  per-universe `[u8; 512]` frames.
- PREVIEW (blind) and HIGHLIGHT gain real semantics.

**Step 3 — import Art-Net output**
(`halo-old/crates/core/src/artnet`, ~225 lines):

- Decision resolved: **Art-Net** (imported and proven against hardware);
  sACN drops to the backlog.
- `artnet.rs` + `network_config.rs` are already sync
  (`std::net::UdpSocket` via the `artnet_protocol` crate; broadcast and
  unicast modes, multiple destinations) — port into `halo-light`. The
  `DmxModule` async wrapper does **not** come along.
- New dedicated DMX output thread (same shape as the analysis worker):
  tick at 44 Hz, snapshot the shared per-universe frames from Step 2,
  blocking UDP send to each destination. A 512-byte send is
  microseconds; a plain thread holds the frame rate fine.
- Add `artnet_protocol = "0.4"` to dependencies; persist network config
  (destinations, broadcast/unicast) with app settings and expose it in a
  settings panel.

**Milestone:** latch a look in the programmer and real fixtures respond
over Art-Net; track cues fire the rig from the active deck.

### Phase L2 — Overlapping cues

Today `CueSet` enforces **non-overlap per lane** by construction (the
editor clamps drags, `insert` truncates into free gaps, the loader drops
overlaps), and `active_at()` returns the single cue under the playhead.
Starting a new cue while an old one plays is inexpressible. Evolve in
three steps, each contained in `CueSet` + `resolve()` (the painters and
persistence shell don't change; bump `CueFile.version` as fields grow):

1. **Per-cue fade in/out** — crossfade the outgoing cue into the incoming
   one inside `resolve()`. Cues stay non-overlapping in the data; this
   covers the common "new look starts while the old is still visible"
   case with minimal machinery. (Phase L3's look lane makes same-lane
   overlap inexpressible by construction — if L3 lands first, this step
   reduces to fades at look-event boundaries.)
2. **Per-fixture cue targets** — cues carry a fixture selection; relax
   the invariant from "no time overlap per lane" to "no overlap per
   fixture", so cues that touch disjoint fixtures may overlap freely
   (wash look running while a spot chase fires over it).
3. **HTP/LTP merging** — true same-fixture overlap resolved by console
   convention: highest-takes-precedence for intensity,
   latest-takes-precedence for color/position/beam. `resolve()` remains
   the one merge point.

**Milestone:** draw two overlapping cues targeting different fixtures and
both play; same-fixture overlaps merge HTP/LTP with clean fades.

### Phase L3 — Shows

**Goal:** a playlist becomes a show. Per-track cues authored once in
Prepare carry a set-level narrative (soft intro → drop → mid-set
cooldown), cues fire real pre-programmed looks instead of bare intensity,
and there's live control to pull the energy back or skip a sequence
without editing anything.

Today three gaps block this: cues carry only intensity (color / position
/ effects live in `ProgrammerParams`, rig-wide and live-only, so a
mid-set palette change repaints every later track's cues); the only live
override *replaces* a lane with a flat level rather than attenuating the
authored shape; and cues attach to tracks globally, so a track lights
identically in every set and "skip" has no non-destructive gesture.

**Re-base the lanes from fixture taxonomy to roles.** Today's
Lighting / Pixels / FX lanes are keyed by `FixtureKind` — a hardware
axis, when authoring thinks in intent ("the drop hits: everything red,
strobe chase"). One musical moment smears across three lanes kept in
sync by hand, and an intensity bar conflates *that* something happens
with *what* happens. The show model decomposes into what / how much /
when, so the lanes become:

- **Look lane** — sparse, beat-snapped *events*: each switches the rig
  to a stored look and holds until the next event (blocks tinted by the
  look's color — the strip reads as the track's color script). A look
  contains palette, position, and effects, so one event replaces three
  coordinated bars; per-fixture-kind intensity moves *inside* looks,
  where a console would put it anyway.
- **Energy lane** — a drawn envelope (automation-style breakpoints, not
  bars): the narrative arc made directly editable, multiplying whatever
  the look outputs.
- **Accent lane** — momentary one-shots (strobe hit, blinder, pyro):
  the only items needing bar-precise start *and* end, and exactly the
  set to arm/disarm live. Today's lane semantics survive here intact.

The `CueSet` machinery (windowed queries, sorted-lane invariants,
painters, drag editor, JSON persistence) is semantics-agnostic — this
re-labels the axis rather than rebuilding the editor. Look events are
cues with implicit duration; only the energy curve is a new item type.
Duration-until-next removes same-lane overlap by construction, which
supersedes Phase L2 step 1 for the look lane; HTP/LTP (L2 step 3) still
governs accents firing over the active look.

Build order is smallest-first; every step keeps `resolve()` as the
single merge point and `render()` pure:

1. **Energy** — the authored curve, plus a live master fader that
   scales/offsets it; both multiply resolved output inside `resolve()`.
   The priority stack becomes Programmer (replace) > energy (scale) >
   track cues > off. Authored shape is preserved — builds and chases
   just sit lower. Optionally damp strobe and effect rate when energy
   drops below a threshold. The fader alone is small and immediately
   useful live; the curve lands with the lane re-base.
2. **Look lane** — a `Look` is a stored snapshot of programmer params
   (STORE-from-live already captures one), persisted by id in the
   library. `resolve()` yields (active look, energy, accents) instead
   of three levels; `render()` renders the cue's look instead of the
   global live params. Bump `CueFile.version` with a migration from
   the three intensity lanes. Crossfade between looks at event
   boundaries (Phase L2 step 1's fade machinery, applied here).
3. **Show entity** — a `Show` is a playlist plus per-entry deltas:
   non-destructive arm/disarm on look events and accents (click a bar
   hollow to skip it tonight), and an optional per-entry energy/theme
   override so the same track can sit differently in different sets.
   Track cues stay the authored default; the show stores only deltas
   (new tables alongside `playlists`).

Live UX falls out directly: tap a look event to jump or skip, one fader
pulls the arc down, disarm an accent — nothing destructive.

Open question to resolve here: lighting follows a single deck's
playhead, so during a two-deck blend the look hard-switches when the
lighting deck changes — decide whether lighting should follow the audio
crossfader once shows span transitions.

**Milestone:** run a playlist as a show — the look lane plays the
track's color script, the energy curve draws the arc, one fader pulls
the whole rig to 60% when the room isn't there, and tonight's skipped
accent never touches the authored cues.

---

## Backlog (post-1.0)

- In-app musical key detection (chromagram-based — a natural fit for the `timestretch` crate).
- Headphone cue / split output (needs a second output or multi-channel device routing).
- Session recording to disk.
- MIDI controller mapping.
- Rekordbox library import.
- sACN (E1.31) output alongside Art-Net.
- Fixture profiles loaded from disk (user-editable library) instead of the built-in registry.

## Sequencing rationale

1. **Audio-path correctness first** (Phases 0–1): everything else hangs off a solid two-deck, lock-free audio skeleton.
2. **Analysis before anything that needs the grid** (Phase 2 before 4 and 5): sync, quantized loops, and quantized cues all consume the beat grid.
3. **EQ/filter is dependency-free DSP** (Phase 3): only needs the mixer chain, so it can overlap with Phase 2.
4. **Library last** (Phase 6): file dialogs are good enough until the performance features exist; the browser then lands with analysis and tag reading already proven.
5. **CPU meter early** (Phase 1): it's the instrument for keeping every later DSP addition inside budget.
