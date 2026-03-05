use anyhow::{Result, anyhow};
use clap::Parser;
use guppy::{CargoMetadata, graph::PackageGraph};
use std::{fs, process::Command};
use toml_edit::{Document, Item};

#[derive(Parser)]
struct Args {
    crate_name: String,
    bump: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let metadata = CargoMetadata::parse_json(&String::from_utf8(
        Command::new("cargo")
            .args(["metadata", "--format-version", "1"])
            .output()?
            .stdout,
    )?)?;
    let graph = PackageGraph::from_metadata(metadata)?;

    let pkg = graph
        .packages()
        .find(|p| p.name() == args.crate_name)
        .ok_or_else(|| anyhow!("Crate '{}' not found in workspace", args.crate_name))?;

    let cargo_toml_path = pkg.manifest_path();

    let content = fs::read_to_string(cargo_toml_path)?;
    let mut doc = content.parse::<Document>()?;

    let version_item = doc["package"]["version"]
        .as_str()
        .ok_or_else(|| anyhow!("No version field in Cargo.toml"))?;
    let mut parts: Vec<u64> = version_item
        .split('.')
        .map(|p| p.parse())
        .collect::<Result<_, _>>()?;

    match args.bump.as_str() {
        "major" => {
            parts[0] += 1;
            parts[1] = 0;
            parts[2] = 0;
        }
        "minor" => {
            parts[1] += 1;
            parts[2] = 0;
        }
        "patch" => {
            parts[2] += 1;
        }
        _ => {
            eprintln!("Invalid bump type");
            std::process::exit(1);
        }
    }

    let new_version = format!("{}.{}.{}", parts[0], parts[1], parts[2]);
    doc["package"]["version"] = Item::Value(new_version.clone().into());
    fs::write(cargo_toml_path, doc.to_string())?;
    println!(
        "Crate '{}' version updated to {}",
        args.crate_name, new_version
    );

    Ok(())
}
