//! `termoxide run --release`: the app's lib linked into an executable of its
//! own, built from a runner crate written for it in its target directory.

use std::{
    env,
    fs,
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use color_eyre::{Result, eyre::eyre};

use crate::{
    build::Message,
    metadata::{AppCrate, Dependency},
};

const WRITTEN: &str = "Written by `termoxide run --release`, again on every run.";

/// Writes the runner of `app` and builds it for release, returning its
/// executable.
pub(crate) fn build(app: &AppCrate) -> Result<PathBuf> {
    let dir = app.target_dir.join("termoxide/runner").join(&app.name);
    fs::create_dir_all(&dir)?;
    write_if_changed(&dir.join("Cargo.toml"), &manifest(app)?)?;
    write_if_changed(&dir.join("main.rs"), &main(app)?)?;
    // The versions the app was built with.
    if let Err(e) = fs::copy(app.workspace_root.join("Cargo.lock"), dir.join("Cargo.lock"))
        && e.kind() != io::ErrorKind::NotFound
    {
        return Err(e.into());
    }

    let output = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args([
            "build",
            "--release",
            "--message-format",
            "json-render-diagnostics",
            "--manifest-path",
        ])
        .arg(dir.join("Cargo.toml"))
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()?;
    if !output.status.success() {
        return Err(eyre!("building {} for release failed ({})", app.name, output.status));
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Message>(line).ok())
        .filter(|message| message.reason == "compiler-artifact")
        .find_map(|message| message.executable)
        .ok_or_else(|| eyre!("building {} for release made no executable", app.name))
}

/// Leaves `path` untouched when it already holds `contents`, so cargo doesn't
/// rebuild the runner.
fn write_if_changed(path: &Path, contents: &str) -> io::Result<()> {
    if fs::read_to_string(path).is_ok_and(|written| written == contents) {
        return Ok(());
    }
    fs::write(path, contents)
}

/// The runner's manifest: its binary is named after the app's package, so the
/// logs and the trace land where a dev run's do.
fn manifest(app: &AppCrate) -> Result<String> {
    let name = &app.name;
    let dir = app.dir().display().to_string();
    let hot_reload = source(app.hot_reload()?)?;
    Ok(format!(
        r#"# {WRITTEN}

[package]
name = "termoxide-runner"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[[bin]]
name = {name:?}
path = "main.rs"

[dependencies]
{name:?} = {{ path = {dir:?} }}
termoxide_hot_reload = {{ {hot_reload}, features = ["host"] }}
"#
    ))
}

fn main(app: &AppCrate) -> Result<String> {
    let lib = app.lib_name()?;
    let dir = app.dir().display().to_string();
    let (name, persistent) = (&app.name, app.persistent());
    Ok(format!(
        r#"// {WRITTEN}

fn main() -> std::process::ExitCode {{
    termoxide_hot_reload::run_release(
        (::{lib}::__termoxide_app, {lib:?}),
        termoxide_hot_reload::Package {{ manifest_dir: {dir:?}, app: Some({name:?}) }},
        {persistent},
    )
}}
"#
    ))
}

/// Where `dependency` comes from, as a manifest says it.
fn source(dependency: &Dependency) -> Result<String> {
    if let Some(path) = &dependency.path {
        return Ok(format!("path = {:?}", path.display().to_string()));
    }
    match dependency.source.as_deref().and_then(|source| source.split_once('+')) {
        Some(("git", url)) => {
            let (url, reference) = url.split_once('?').unwrap_or((url, ""));
            let reference = reference
                .split_once('=')
                .map(|(kind, name)| format!(", {kind} = {name:?}"))
                .unwrap_or_default();
            Ok(format!("git = {url:?}{reference}"))
        },
        // Only crates.io has no registry name.
        Some(_) if dependency.registry.is_none() => Ok(format!("version = {:?}", dependency.req)),
        _ => {
            Err(eyre!(
                "termoxide_hot_reload comes from {}, which `termoxide run --release` can't depend on",
                dependency
                    .source
                    .as_deref()
                    .or(dependency.registry.as_deref())
                    .unwrap_or("nowhere")
            ))
        },
    }
}

#[cfg(test)]
mod tests;
