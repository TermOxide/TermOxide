//! Prints a hot-reload state trace, as the app that wrote it reads it:
//! `termoxide_dump <trace> [--manifest <path to the app's Cargo.toml>]`.

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    termoxide_hot_reload::dump::main()
}
