---
name: release
description: Cut a Halo release — CHANGELOG fold, version bump, CI checks, signed tag, GitHub release with optional Halo.app. Use when Rob asks to release/publish/ship a new version of the app. Usage: /release [X.Y.Z]
---

# Release Halo

Cut a release of the Halo macOS app end to end. Halo is **not** published to
crates.io (`publish = false` on both workspace crates). A release is consistent
only when **these places agree**: both crate `version`s, `CHANGELOG.md` section,
annotated git tag `vX.Y.Z`, and GitHub release. Past tags (v0.0.1–v0.0.6) were
cut without GitHub releases and with Cargo versions drifting from the tag — so
always audit first, repair second, release third.

## Phase 1 — Preflight & state audit (read-only)

1. Get on a current main: `git checkout main && git pull --ff-only`. If the working
   tree has uncommitted WIP (e.g. ROADMAP rewrites), stash it with a named stash
   (`git stash push -m "wip-during-release" <files>`) and **restore it at the very
   end**. Never commit WIP into the release; never use `--allow-dirty`.
2. Confirm the sibling path dependency exists: `../timestretch-rs` must be present
   (builds fail without it). Do not bump or publish timestretch as part of a Halo
   release.
3. Audit and report the current state before changing anything:
   - Last tag: `git tag | sort -V | tail -3` and `git log -1 --oneline <tag>`
   - GitHub releases: `gh release list --limit 3`
   - `crates/halo/Cargo.toml` and `crates/halo-light/Cargo.toml` `version` fields
     (they must match each other and should match the latest tag once a release is
     done — today they may still say `0.1.0` while tags are `v0.0.x`)
   - Whether `CHANGELOG.md` exists and has an `## Unreleased` section
4. If a previous version is half-released (tag exists but no GitHub release, etc.),
   tell Rob and repair that first — e.g. backfill a GitHub release from the
   existing tag with `gh release create <tag> --verify-tag` and notes written per
   Phase 6. Do not move or re-point an already-pushed tag.
5. If everything already agrees and there is nothing unreleased, report "nothing to
   do" and stop.

## Phase 2 — Version choice

If Rob gave a version argument, use it. Otherwise propose one from the `Unreleased`
section (or from `git log --oneline vPREV..HEAD` if there is no CHANGELOG yet) —
while the app is 0.x, breaking / user-visible behaviour changes mean a **minor**
bump, otherwise patch — and confirm the choice with AskUserQuestion before
touching files.

Keep `halo` and `halo-light` on the **same** version; they ship as one app.

## Phase 3 — File updates (release files only)

1. `CHANGELOG.md`: if missing, create it (Keep a Changelog style) with a leading
   `# Changelog` and fold the span into `## X.Y.Z`. If it exists, rename
   `## Unreleased` → `## X.Y.Z`. Then cross-check the section against
   `git log --oneline vPREV..HEAD` for user-facing changes that were never
   changelogged, and add them. Keep entries app-focused; repo tooling / CI goes
   under a `### Internal` heading.
2. `crates/halo/Cargo.toml` and `crates/halo-light/Cargo.toml`: bump `version` to
   `X.Y.Z` in both. Then `cargo check` (or `cargo build`) to refresh `Cargo.lock`
   if needed.
3. Do **not** invent a crates.io install snippet in `README.md`. Only touch the
   README if a version number is already mentioned there and would go stale.

## Phase 4 — CI checks (before committing; run as one background command)

Match what CI runs (see `.github/workflows/rust.yml`), plus clippy because the
`.claude/settings.json` pre-commit hook will block the commit on warnings:

```
cargo +nightly fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

(`cargo build` is covered by the test compile.) Clippy is not yet enforced in CI
(commented out), but the local hook expects a clean tree — fix warnings rather
than skipping. Note CI uses stable **1.90.0**; run checks on that toolchain if
the default rustup toolchain differs.

## Phase 5 — Commit, tag, push

Commit and tag signing hangs on a 1Password `op-ssh-sign` approval prompt: run these
in **background Bash** and tell Rob to approve the prompt.

1. Stage only the release files that changed (typically
   `crates/halo/Cargo.toml crates/halo-light/Cargo.toml Cargo.lock CHANGELOG.md`,
   plus `README.md` only if touched).
2. Commit in house style — subject `Release vX.Y.Z`, body of 3–6 lines summarising
   the span since the previous version ("Since X.Y this release lands ..."), ending
   with the `Co-Authored-By` trailer.
3. `git tag -a vX.Y.Z -m "vX.Y.Z"`, then `git push origin main vX.Y.Z`.

## Phase 6 — GitHub release

Write notes to a scratchpad file:

- `## What's Changed` header, then a one-sentence framing of the release (Halo is
  a macOS app — no crates.io link)
- Themed `###` sections pulled from the CHANGELOG; **Breaking Changes first** when
  present; pull detail from `gh pr view <n>` for merged PRs in the span when useful
- Optional build notes: macOS 12+, needs `timestretch-rs` sibling checkout to build
  from source
- Footer: `**Full Changelog**: https://github.com/robmorgan/halo/compare/vPREV...vX.Y.Z`

Then create the release:

```
gh release create vX.Y.Z --title "vX.Y.Z" --notes-file <file> --verify-tag --latest
```

### Optional — attach Halo.app

If Rob wants a downloadable binary (ask if unclear):

1. `cargo install cargo-bundle` if needed, then `cargo bundle --release` from the
   repo root (bundle metadata lives on the `halo` package).
2. Zip the resulting `.app` (find under `target/release/bundle/osx/`) into
   `Halo-vX.Y.Z-macos-arm64.zip` (or the host arch), then:
   `gh release upload vX.Y.Z <zip> --clobber`
3. Note in the release body that the zip is an unsigned local build unless
   notarization is added later.

## Phase 7 — Report

Give Rob: the GitHub release URL, the tag, confirmation that both crate versions
match the tag, whether an app zip was attached, and confirmation that stashed WIP
was restored. Pop any Phase 1 stash last and confirm with `git status --short`.
