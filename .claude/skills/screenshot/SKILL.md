---
name: screenshot
description: Build and launch Halo with a real track, then capture the window with screencapture for UI inspection and spacing work. Use when Rob asks to run the app, screenshot it, or check/tune UI layout and element spacing.
---

# Launching Halo and screenshotting the window

Launches the app with a real track loaded on deck A, captures the window as a
PNG, and reads it back to inspect layout/spacing. No app-side screenshot
support exists — capture is external via macOS `screencapture`.

## Track

Always use the real music track so the waveforms look representative:

```
../timestretch-rs/benchmarks/audio/public-corpus/01-Interplanetary_Criminal-Saucers.mp3
```

`../timestretch-rs` is always present (it's a path dependency in `Cargo.toml`),
and the track's `.tsanalysis.json` sidecar is cached next to it, so load and
analysis are fast. Do NOT use synthetic test tones — flat waveforms make
spacing judgments harder.

## Procedure

All paths relative to the repo root. `$SCRATCH` = the session scratchpad.

1. **Build & launch** with an isolated DB so scripted runs never touch the real
   library at `~/Library/Application Support/Halo/halo.db`:

   ```bash
   cargo build --release
   HALO_DB="$SCRATCH/halo.db" RUST_LOG=info target/release/halo \
     ../timestretch-rs/benchmarks/audio/public-corpus/01-Interplanetary_Criminal-Saucers.mp3 \
     > "$SCRATCH/app.log" 2>&1 &
   sleep 4
   ```

   argv[1] auto-loads the track onto deck A. For a mid-playback capture add
   `HALO_AUTOPLAY=1` (also available: `HALO_PITCH=<percent>`, `HALO_LOOP=1`,
   `HALO_IMPORT=<folder>`).

2. **Compile the UI helper** from the copy kept in this skill directory
   (`uidrive.swift`, subcommands: `winid <name>` / `list` / `click <x> <y>` /
   `drag <x1> <y1> <x2> <y2> <ms> [holdBeforeMs] [holdAfterMs]`):

   ```bash
   swiftc -O -o "$SCRATCH/uidrive" .claude/skills/screenshot/uidrive.swift
   ```

3. **Find the window** (and, if you'll drive the pointer, safety-check first —
   CGEvent clicks land on whatever is topmost). Bring Halo frontmost, dump the
   layer-0 window list, and confirm the Halo window is first at the expected
   position. If another window is on top or Rob is clearly active, STOP and
   hand over to him.

   ```bash
   osascript -e 'tell application "System Events" to set frontmost of first process whose name contains "halo" to true'
   "$SCRATCH/uidrive" winid halo      # -> "windowID x y w h" (screen points)
   "$SCRATCH/uidrive" list | head -5
   ```

   Default window is 1200×700 points (min 900×550), but eframe's
   `persistence` feature restores the size/position from the last run —
   always take the actual geometry from `winid` output rather than assuming
   the default. To test a specific size, resize with AppleScript first:
   `osascript -e 'tell application "System Events" to set size of front window of (first process whose name contains "halo") to {1200, 700}'`.

4. **Capture**:

   ```bash
   screencapture -x -o -l <windowID> "$SCRATCH/shot.png"
   ```

   `-o` (no shadow) gives an exact 2× point→pixel mapping; without it the
   shadow margin breaks coordinates and adds transparent padding. Expect
   2400×1400 pixels for the default window.

5. **Inspect** by reading the PNG with the Read tool. For spacing work:
   image pixels are 2× egui points — measure in the image and divide by 2.
   Click targets for `uidrive` are screen points: window origin (from
   `winid`) + in-window point coords. Don't rely on hardcoded coordinates
   from earlier sessions — the layout is exactly what's being iterated on,
   so re-derive targets from the current screenshot each time.

   When interacting mid-playback, stage everything in ONE bash command so
   tool-call gaps don't drift the transport:

   ```bash
   "$SCRATCH/uidrive" click <x> <y> && sleep 1 && \
   screencapture -x -o -l <windowID> "$SCRATCH/shot2.png"
   ```

   **Synthetic input gotcha**: several Halo controls (hot cues, CUE, the
   shortcut keys) detect press/release *edges* by sampling held state per
   frame — instantaneous synthetic taps land between frames and vanish.
   Plain `uidrive click` and `osascript keystroke` therefore often do
   nothing. Make presses span frames instead:

   ```bash
   # press-and-hold "click": drag in place with hold times
   "$SCRATCH/uidrive" drag <x> <y> <x> <y> 30 150 150
   # held keypress
   osascript -e 'tell application "System Events" to key down "v"'; sleep 0.3
   osascript -e 'tell application "System Events" to key up "v"'
   ```

   And re-verify the Halo window is still topmost (`uidrive list`)
   **immediately before every batch of pointer input**, not just at
   launch — if focus has moved, presses land in whatever app is now on
   top. When that happens, stop driving and hand the check to Rob.

6. **Clean up** (pattern must be precise — plain `halo` is too greedy):

   ```bash
   pkill -f 'target/release/halo'
   grep -ci 'underrun\|error' "$SCRATCH/app.log"   # expect 0
   ```

For iterating on spacing: edit → `cargo build --release` → relaunch → recapture,
comparing successive PNGs in `$SCRATCH`.
