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
use toml::{Table, Value};

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
/// logs and the trace land where a dev run's do. The profiles and the patches
/// of the app's workspace come along, which only a workspace's root sets.
fn manifest(app: &AppCrate) -> Result<String> {
    let mut hot_reload = source(app.hot_reload()?)?;
    hot_reload.insert("features".into(), vec!["host"].into());
    let mut manifest = table([
        (
            "package",
            table([
                ("name", "termoxide-runner".into()),
                ("version", "0.0.0".into()),
                ("edition", "2024".into()),
                ("publish", false.into()),
            ])
            .into(),
        ),
        ("workspace", Table::new().into()),
        (
            "dependencies",
            table([
                (app.name.as_str(), table([("path", utf8(app.dir())?.into())]).into()),
                ("termoxide_hot_reload", hot_reload.into()),
            ])
            .into(),
        ),
    ]);
    let bin = table([("name", app.name.as_str().into()), ("path", "main.rs".into())]);
    manifest.insert("bin".into(), vec![Value::Table(bin)].into());

    let workspace = app.workspace_root.join("Cargo.toml");
    let mut workspace: Table = match fs::read_to_string(&workspace) {
        Ok(text) => toml::from_str(&text).map_err(|e| eyre!("reading {}: {e}", workspace.display()))?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Table::new(),
        Err(e) => return Err(e.into()),
    };
    if let Some(profile) = workspace.remove("profile") {
        manifest.insert("profile".into(), profile);
    }
    if let Some(Value::Table(mut patch)) = workspace.remove("patch") {
        from_root(&mut patch, &app.workspace_root);
        manifest.insert("patch".into(), patch.into());
    }
    Ok(format!("# {WRITTEN}\n\n{}", toml::to_string(&manifest)?))
}

fn table<'a>(entries: impl IntoIterator<Item = (&'a str, Value)>) -> Table {
    entries.into_iter().map(|(key, value)| (key.to_owned(), value)).collect()
}

/// Makes the paths `patch` gives its crates absolute: a manifest writes them
/// from its own folder, `root`, which isn't the runner's.
fn from_root(patch: &mut Table, root: &Path) {
    let crates = patch
        .iter_mut()
        .filter_map(|(_, source)| source.as_table_mut())
        .flat_map(|source| source.iter_mut())
        .filter_map(|(_, patched)| patched.as_table_mut());
    for patched in crates {
        if let Some(Value::String(path)) = patched.get_mut("path") {
            *path = root.join(path.as_str()).to_string_lossy().into_owned();
        }
    }
}

/// `path` as text, which a manifest and a string literal both need.
fn utf8(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| eyre!("{} isn't UTF-8, which `termoxide run --release` needs", path.display()))
}

fn main(app: &AppCrate) -> Result<String> {
    let lib = app.lib_name()?;
    let dir = utf8(app.dir())?;
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
fn source(dependency: &Dependency) -> Result<Table> {
    if let Some(path) = &dependency.path {
        return Ok(table([("path", utf8(path)?.into())]));
    }
    match dependency.source.as_deref().and_then(|source| source.split_once('+')) {
        Some(("git", url)) => {
            let (url, reference) = url.split_once('?').unwrap_or((url, ""));
            let mut source = table([("git", url.into())]);
            if let Some((kind, name)) = reference.split_once('=') {
                source.insert(kind.to_owned(), name.into());
            }
            Ok(source)
        },
        // Only crates.io has no registry name.
        Some(_) if dependency.registry.is_none() => Ok(table([("version", dependency.req.as_str().into())])),
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
