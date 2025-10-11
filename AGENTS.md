# Rust Crates

In the crates folder where the Rust code lives:

- Crate names are prefixed with `halo-`. For example, the `core` folder's crate is named `halo-core`
- When using format! and you can inline variables into {}, always do that.
- Never use `unsafe` blocks or functions in any code

# Code Formatting

After making any changes to Rust code, always run:
```bash
cargo +nightly fmt --all
```
