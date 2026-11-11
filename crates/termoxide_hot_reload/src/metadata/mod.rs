//! An app's crate, as `cargo metadata` describes it.

use std::{
    collections::BTreeMap,
    env,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use color_eyre::{Result, eyre::eyre};
use serde::Deserialize;

/// The package whose manifest is in a crate folder.
#[derive(Debug, Deserialize)]
pub(crate) struct AppCrate {
    pub(crate) name: String,
    manifest_path: PathBuf,
    targets: Vec<Target>,
    features: BTreeMap<String, Vec<String>>,
    metadata: Option<Metadata>,
}

#[derive(Debug, Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
    crate_types: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    termoxide: Option<Termoxide>,
}

/// `[package.metadata.termoxide]`.
#[derive(Debug, Deserialize)]
struct Termoxide {
    #[serde(default)]
    persistent: bool,
}

#[derive(Deserialize)]
struct Workspace {
    packages: Vec<AppCrate>,
}

impl AppCrate {
    /// Reads the package of `dir/Cargo.toml`.
    pub(crate) fn read(dir: &Path) -> Result<Self> {
        let manifest = dir.join("Cargo.toml");
        let output = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["metadata", "--no-deps", "--format-version", "1", "--manifest-path"])
            .arg(&manifest)
            .stdin(Stdio::null())
            .stderr(Stdio::inherit())
            .output()?;
        if !output.status.success() {
            return Err(eyre!("cargo metadata failed on {} ({})", manifest.display(), output.status));
        }
        Self::find(&output.stdout, &fs::canonicalize(&manifest)?)
    }

    /// The package of `manifest` among those of the workspace `metadata`
    /// lists, the manifest's own and its fellow members.
    fn find(metadata: &[u8], manifest: &Path) -> Result<Self> {
        let workspace: Workspace = serde_json::from_slice(metadata)?;
        workspace
            .packages
            .into_iter()
            .find(|package| {
                fs::canonicalize(&package.manifest_path).unwrap_or_else(|_| package.manifest_path.clone()) == manifest
            })
            .ok_or_else(|| eyre!("{} has no package, only a workspace", manifest.display()))
    }

    pub(crate) fn dir(&self) -> &Path { self.manifest_path.parent().unwrap_or(Path::new(".")) }

    /// Whether the app keeps its state across runs unless told otherwise.
    pub(crate) fn persistent(&self) -> bool {
        self.metadata
            .as_ref()
            .and_then(|metadata| metadata.termoxide.as_ref())
            .is_some_and(|termoxide| termoxide.persistent)
    }

    /// Checks that `termoxide run` can build the lib as the app's component,
    /// and natively.
    pub(crate) fn check_runnable(&self) -> Result<()> {
        let lib = self
            .targets
            .iter()
            .find(|target| target.kind.iter().any(|kind| kind.ends_with("lib")))
            .ok_or_else(|| eyre!("{} has no lib to run", self.name))?;
        let missing: Vec<_> = ["cdylib", "rlib"]
            .into_iter()
            .filter(|kind| !lib.crate_types.iter().any(|crate_type| crate_type == kind))
            .collect();
        if !missing.is_empty() {
            return Err(eyre!(
                "the lib of {} needs `crate-type = [\"cdylib\", \"rlib\"]`, it lacks {}",
                self.name,
                missing.join(" and ")
            ));
        }
        if !self.features.contains_key("guest") {
            return Err(eyre!(
                "{} needs a `guest` feature enabling `termoxide_hot_reload/guest`",
                self.name
            ));
        }
        Ok(())
    }

    /// The name of the app's executable, which names its directory: `bin`, else
    /// its only binary, else the package's, which `termoxide run` runs it as.
    pub(crate) fn app_name(&self, bin: Option<&str>) -> Result<String> {
        let bins: Vec<_> = self
            .targets
            .iter()
            .filter(|target| target.kind.iter().any(|kind| kind == "bin"))
            .map(|target| target.name.as_str())
            .collect();
        match (bin, bins.as_slice()) {
            (Some(bin), _) if bins.contains(&bin) => Ok(bin.to_owned()),
            (Some(bin), _) => Err(eyre!("{} has no binary {bin}", self.name)),
            (None, []) => Ok(self.name.clone()),
            (None, [bin]) => Ok((*bin).to_owned()),
            (None, bins) => {
                Err(eyre!(
                    "{} has several binaries, pick one with --bin: {}",
                    self.name,
                    bins.join(", ")
                ))
            },
        }
    }
}

#[cfg(test)]
mod tests;
