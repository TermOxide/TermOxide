use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use color_eyre::{Result, eyre::eyre};
use serde::Deserialize;

/// The app's package, as `run_app!` captures it in the app's own crate.
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct Package {
    pub manifest_dir: &'static str,
}

/// The target the guest is built for.
pub(crate) const GUEST_TARGET: &str = "wasm32-wasip2";

/// Builds the app as a wasm component and returns the path to it.
///
/// Cargo's progress and errors go to the terminal: this runs before the host
/// takes the screen over.
pub(crate) fn guest(package: Package) -> Result<PathBuf> {
    let manifest = Path::new(package.manifest_dir).join("Cargo.toml");
    let output = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args([
            "build",
            "--lib",
            "--target",
            GUEST_TARGET,
            "--no-default-features",
            "--features",
            "guest",
        ])
        .args(["--message-format", "json-render-diagnostics", "--manifest-path"])
        .arg(&manifest)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()?;
    if !output.status.success() {
        return Err(eyre!("building the guest for {GUEST_TARGET} failed ({})", output.status));
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Message>(line).ok())
        .filter(|message| message.reason == "compiler-artifact" && message.target.kind.iter().any(|k| k == "cdylib"))
        .flat_map(|message| message.filenames)
        .find(|file| file.extension().is_some_and(|ext| ext == "wasm"))
        .ok_or_else(|| eyre!("{} built no wasm component: is its lib a `cdylib`?", manifest.display()))
}

/// The parts of a line of cargo's JSON output this needs.
#[derive(Deserialize)]
struct Message {
    reason: String,
    #[serde(default)]
    target: Target,
    #[serde(default)]
    filenames: Vec<PathBuf>,
}

#[derive(Default, Deserialize)]
struct Target {
    kind: Vec<String>,
}
