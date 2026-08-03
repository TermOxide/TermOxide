# TermOxide Workspace

Framework crates live under `crates/`.

The shared runtime loop is exposed by the framework API in `crates/termoxide/src/lib.rs`.
Runtime rendering primitives (`Container`, `Text`, `Fragment`, and `Raw`) live in
`termoxide_rendering`; common widgets such as `Button` live in the separate
`termoxide_components` crate.
This repository includes a minimal runnable example at `crates/termoxide/examples/minimal_app.rs`.
This repository also includes a standalone external app demo at `demos/external_app`.

## Run the example app

```bash
cargo run -p termoxide --example minimal_app
```

## Run the external app demo

```bash
cargo run --manifest-path demos/external_app/Cargo.toml
```

## Notes

- Modifier keys are formatted as readable text (`control`, `shift`, `alt`, `super`) rather than numeric bit values.
- This is crates.io-ready direction: your app depends on `termoxide` and calls `termoxide::run_with_app(...)`.
- No framework source edits are needed to create a new app.
