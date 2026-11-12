//! What the person running an app sets in its `config.toml`, next to its trace
//! and its logs.

use std::{fs, io, path::Path};

use color_eyre::{Result, eyre::WrapErr};
use serde::Deserialize;

pub(crate) const FILE: &str = "config.toml";

#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    /// Whether runs persist, whatever the app's default.
    pub(crate) persist: Option<bool>,
}

impl Config {
    /// Reads the config in `dir`: none there sets nothing.
    pub(crate) fn read(dir: &Path) -> Result<Self> {
        let path = dir.join(FILE);
        match fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).wrap_err_with(|| format!("reading {}", path.display())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).wrap_err_with(|| format!("reading {}", path.display())),
        }
    }

    fn parse(text: &str) -> Result<Self, toml::de::Error> { toml::from_str(text) }
}

#[cfg(test)]
mod tests;
