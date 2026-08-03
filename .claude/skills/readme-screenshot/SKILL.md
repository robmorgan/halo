---
name: readme-screenshot
description: Capture a fresh presentation-quality Halo screenshot and refresh the README image (_docs/screenshot.png). Use after big UI changes when Rob asks to update the README screenshot. Usage: /readme-screenshot
---

# Refresh the README screenshot

Capture a presentation-quality shot of the running app and replace
`_docs/screenshot.png`, the image the README embeds (`README.md` →
`<img src="_docs/screenshot.png" ...>`). Rob runs this after big UI changes,
not on every release. The shot must be **reviewed and approved by Rob in-chat
before anything in the repo changes**.

This skill shares machinery with the `screenshot` skill (window helper at
`.claude/skills/screenshot/uidrive.swift`); see that skill for deeper
UI-driving details (clicking, dragging, gotchas). `$SCRATCH` below is the
session scratchpad directory.

## Phase 1 — Build & launch (staged for presentation)

1. Working-tree guard: `git status --short _docs/screenshot.png README.md`.
   If either has uncommitted changes, stop and ask Rob before proceeding —
   never clobber unreviewed work.
2. `cargo build --release`.
3. Launch with a **real track** (never synthetic tones) on an isolated DB,
   with autoplay so the UI looks alive:

   ```bash
   HALO_DB="$SCRATCH/halo.db" HALO_AUTOPLAY=1 RUST_LOG=info target/release/halo \
     ../timestretch-rs/benchmarks/audio/public-corpus/01-Interplanetary_Criminal-Saucers.mp3 \
     > "$SCRATCH/app.log" 2>&1 &
   ```

   The positional arg auto-loads onto deck A; its `.tsanalysis.json` sidecar is
   cached next to the file, so analysis is fast. Let it play **~15–20 s** before
   capturing: waveform mid-track, playhead moving, LUFS/peak meters lit — not a
   dead idle UI.

## Phase 2 — Standardize the window & capture

1. Compile the window helper:
   `swiftc -O -o "$SCRATCH/uidrive" .claude/skills/screenshot/uidrive.swift`
2. Bring Halo frontmost and resize to the **standard 1440×810 pt** (16:9) so
   every README refresh has consistent dimensions:

   ```bash
   osascript -e 'tell application "System Events" to set frontmost of first process whose name contains "halo" to true'
   osascript -e 'tell application "System Events" to set size of front window of first process whose name contains "halo" to {1440, 810}'
   ```

3. Wake the display — `screencapture` silently produces a black/failed capture
   when the display is asleep: `caffeinate -u -t 3`
4. Capture (no shadow, exact 2× point→pixel mapping):

   ```bash
   read -r WINID X Y W H <<< "$("$SCRATCH/uidrive" winid halo)"
   screencapture -x -o -l "$WINID" "$SCRATCH/readme-shot.png"
   ```

   Expect **2880×1620 px**. If the geometry from `winid` isn't 1440×810, the
   resize didn't take — fix that before capturing, don't ship an odd-sized shot.

## Phase 3 — Review gate (Rob approves before any repo change)

1. Read `$SCRATCH/readme-shot.png` so it renders in-chat and sanity-check it:
   both decks and the mixer visible, meters lit, playhead mid-track, no debug
   overlays, no settings window or dialogs open.
2. Ask Rob to approve or request restaging (different view, track position,
   loop engaged, lighting view, etc.). Re-stage and re-capture as needed.
   **Do not overwrite `_docs/screenshot.png` until he approves.**

## Phase 4 — Refresh repo & commit

1. On approval: `cp "$SCRATCH/readme-shot.png" _docs/screenshot.png`
2. Verify the README embed still exists (`grep '_docs/screenshot.png' README.md`).
   Only touch `README.md` if the embed is missing — re-add the established
   pattern at the tail of `## About`:

   ```html
   <p align="center">
     <img src="_docs/screenshot.png" alt="Halo Screenshot" width="600">
   </p>
   ```

3. Commit the image (plus `README.md` only if touched) — commit signing hangs
   on a 1Password `op-ssh-sign` prompt, so run in **background Bash** and tell
   Rob to approve it. Subject `docs: refresh README screenshot`, a 1–2 line
   body noting what UI change prompted the refresh, ending with the
   `Co-Authored-By` trailer. Do **not** push unless Rob asks.

## Phase 5 — Cleanup & report

1. `pkill -f 'target/release/halo'`
2. `grep -ci 'underrun\|error' "$SCRATCH/app.log"` — expect 0; mention anything found.
3. Report: committed image dimensions and file size, the commit hash, and that
   the README embed is intact.
