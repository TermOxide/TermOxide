use anyhow::{Context, Result};
use guppy::{
    CargoMetadata,
    graph::{DependencyDirection::Reverse, PackageGraph},
};
use std::collections::{HashSet, VecDeque};
use std::env;
use std::process::Command;

fn main() -> Result<()> {
    // Get the --since argument from the command line
    let since = env::args()
        .nth(1)
        .context("Usage: cargo run <since_commit_or_tag>")?;

    // Run `cargo workspaces changed --since <since>`
    let output = Command::new("cargo")
        .args(["workspaces", "changed", "--since", &since])
        .output()
        .context("failed to run cargo workspaces changed")?;

    let changed_stdout = String::from_utf8(output.stdout)?;
    let changed: HashSet<String> = changed_stdout
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    if changed.is_empty() {
        return Ok(());
    }

    let metadata = CargoMetadata::parse_json(&String::from_utf8(
        Command::new("cargo")
            .args(["metadata", "--format-version", "1"])
            .output()?
            .stdout,
    )?)?;

    let graph = PackageGraph::from_metadata(metadata)?;

    let mut name_to_id = std::collections::HashMap::new();
    for pkg in graph.packages() {
        name_to_id.insert(pkg.name().to_string(), pkg.id().clone());
    }

    // BFS over reverse dependencies
    let mut impacted = HashSet::new();
    let mut queue: VecDeque<_> = changed.into_iter().collect();

    while let Some(pkg_name) = queue.pop_front() {
        if !impacted.insert(pkg_name.clone()) {
            continue;
        }

        let Some(pkg_id) = name_to_id.get(&pkg_name) else {
            continue;
        };

        let reverse = graph.query_reverse(&[pkg_id.clone()])?.resolve();

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
