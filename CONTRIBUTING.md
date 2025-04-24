# Contributing to Halo

> We love pull requests! This guide shows the fastest path from **idea** to **merged code**. Skip straight to the *Quick‑Start* if you just want to get going, or dive into the details below.

---

## ⏩ Quick‑Start

### 1. Fork, clone & branch
```bash
git clone https://github.com/<your-username>/halo.git
cd halo
git checkout -b feat/<topic>
```

### 2. Compile fast (uses stable toolchain from rust-toolchain.toml)
```bash
cargo check --workspace --all-targets
```

### 3. Format (uses nightly toolchain)
```bash
rustup toolchain install nightly # skip if installed already
cargo +nightly fmt --all
```

### 4. Lint (uses stable toolchain)
```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

### 5. Test (uses stable toolchain)
```bash
cargo test --workspace
```

### 6. Commit (Conventional + DCO)
```bash
git commit -s -m "feat(core): add new fetcher"
```

### 7. Push & open a Merge Request against `main`
```bash
git push origin feat/<topic>
# then open a merge request on GitHub
```

-----

## Project Layout

| Crate               | Role                                                     |
| ------------------- | -------------------------------------------------------- |
| **`halo-core`**     | Core lighting engine.                                    |
| **`halo-fixtures`** | Fixture library and management.                          |
| **`halo`**          | CLI and main entrypoint                                  |
| **`halo-ui`**       | UI Components and Interface.                             |

All crates live in one Cargo **workspace**, so `cargo <cmd>` from the repo root affects everything.

-----

## Dev Environment

  * **Platform**: Development and execution require **macOS**.
  * **Rust (Build/Test)**: **Stable** toolchain, MSRV pinned in `rust-toolchain.toml` (currently *1.76.0*). Install via [rustup.rs][rustup.rs]. This is used by default for `cargo build`, `cargo check`, `cargo test`, etc.
  * **Rust (Format)**: **Nightly** toolchain is required *only* for formatting (`cargo fmt`) due to unstable options used in our `rustfmt.toml` configuration.
      * Install via: `rustup toolchain install nightly`
  * **Rust Components**: `rustfmt`, `clippy` – install via `rustup component add rustfmt clippy`. Make sure these components are available for *both* your default stable toolchain and the nightly toolchain.
  * **macOS System Tools**: Xcode Command Line Tools (provides C compiler, git, etc.). Install with `xcode-select --install`. You may also need `pkg-config` and `cmake` (e.g., install via [Homebrew][Homebrew]: `brew install pkg-config cmake`).

-----

## Coding Style

  * **Format** ‑ We use custom formatting rules (`rustfmt.toml`) which include unstable options (like `group_imports`, `imports_granularity`, `wrap_comments`, etc.). Applying these requires using the **nightly** toolchain. Format your code *before committing* using:
    ```bash
    cargo +nightly fmt --all
    ```
      * Ensure the nightly toolchain is installed (`rustup toolchain install nightly`).
      * CI runs `cargo +nightly fmt --all --check`, so PRs with incorrect formatting will fail.
  * **Lint** ‑ `cargo clippy … -D warnings`; annotate false positives with `#[allow()]` + comment. (This uses the default stable toolchain).
  * **API** ‑ follow the [Rust API Guidelines][Rust API Guidelines]; document every public item; avoid `unwrap()`.
  * **Dependencies** ‑ discuss new crates in the PR; future policy will use `cargo deny`.

-----

## Testing

  * Unit tests in modules, integration tests in `tests/`.
  * Aim to cover new code; bug‑fix PRs **must** include a failing test that passes after the fix.
  * `cargo test --workspace` must pass (uses the default stable toolchain).

-----

## Git & Commits

  * **Fork** the repo on GitHub and add your remote if you haven’t already.
  * **Branches**: use feature branches like `feat/…`, `fix/…`, `docs/…`, `test/…`.
  * **Conventional Commits** preferred (`feat(core): add bottle caching`).
  * **DCO**: add `-s` flag (`git commit -s …`).
  * Keep commits atomic; squash fix‑ups before marking the PR ready.

-----

## Merge‑Request Flow

1.  Sync with `main`; rebase preferred.
2.  Ensure your code is formatted correctly with `cargo +nightly fmt --all`.
3.  Ensure CI is green (build, fmt check, clippy, tests on macOS using appropriate toolchains).
4.  Fill out the PR template; explain *why* + *how*.
5.  Respond to review comments promptly – we’re friendly, promise!
6.  Maintainers will *Squash & Merge* (unless history is already clean).

-----

## Reporting Issues

  * **Bug** – include repro steps, expected vs. actual, macOS version & architecture (Intel/ARM).
  * **Feature** – explain use‑case, alternatives, and willingness to implement.
  * **Security** – email maintainers privately; do **not** file a public issue.

-----

## License & DCO

By submitting code you agree to the FCL-1.0-ALv2 license and certify the
[Developer Certificate of Origin](https://developercertificate.org/).

-----

## Code of Conduct

We follow the [Contributor Covenant](https://www.contributor-covenant.org/version/2/1/code_of_conduct/); be kind and
inclusive. Report misconduct privately to the core team.

-----

Thanks for contributing to ⭕️ Halo! ✨

* [rustup.rs]: https://rustup.rs/
* [Rust API Guidelines]: https://rust-lang.github.io/api-guidelines/
* [Developer Certificate of Origin]: https://developercertificate.org/
* [Contributor Covenant]: https://www.contributor-covenant.org/version/2/1/code_of_conduct/
