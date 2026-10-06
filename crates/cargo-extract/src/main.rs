use std::{
    collections::{HashSet, VecDeque},
    env,
    process::Command,
};

use anyhow::{Context, Result};
use guppy::{
    CargoMetadata,
    graph::{DependencyDirection::Reverse, PackageGraph},
};

fn main() -> Result<()> {
    // Get the --since argument from the command line
    let since = env::args().nth(1).context(
        "Usage: cargo run -p cargo-extract -- <since_commit_or_tag>\n   or: cargo-extract <since_commit_or_tag>",
    )?;

    // Run `cargo workspaces changed --since <since>`
    let output = Command::new("cargo")
        .args(["workspaces", "changed", "--since", &since])
        .output()
        .context("failed to run cargo workspaces changed")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("cargo workspaces changed failed ({}): {}", output.status, stderr);
    }

    let changed_stdout = String::from_utf8(output.stdout)?;
    let changed: HashSet<String> = changed_stdout.split_whitespace().map(|s| s.to_string()).collect();

    if changed.is_empty() {
        return Ok(());
    }

    let metadata_output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .output()
        .context("failed to run cargo metadata")?;

    if !metadata_output.status.success() {
        let stderr = String::from_utf8_lossy(&metadata_output.stderr);
        anyhow::bail!("cargo metadata failed ({}): {}", metadata_output.status, stderr);
    }

    let metadata = CargoMetadata::parse_json(&String::from_utf8(metadata_output.stdout)?)?;

    let graph = PackageGraph::from_metadata(metadata)?;

    let mut name_to_id = std::collections::HashMap::new();
    for pkg in graph.packages() {
        name_to_id.insert(pkg.name().to_string(), pkg.id().clone());
    }

    // BFS over reverse dependencies
    let mut impacted = HashSet::new();
    let mut queue: VecDeque<_> = changed.into_iter().collect();

    while let Some(pkg_name) = queue.pop_front() {
        let Some(pkg_id) = name_to_id.get(&pkg_name) else {
            continue;
        };

        if !impacted.insert(pkg_name.clone()) {
            continue;
        }

        let reverse = graph.query_reverse(std::slice::from_ref(pkg_id))?.resolve();

        for pkg in reverse.packages(Reverse) {
            let name = pkg.name().to_string();
            if !impacted.contains(&name) {
                queue.push_back(name);
            }
        }
    }

    let mut impacted: Vec<_> = impacted.into_iter().collect();
    impacted.sort();
    println!("{}", impacted.join(" "));

    Ok(())
}
