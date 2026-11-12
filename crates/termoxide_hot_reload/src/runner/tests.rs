#![allow(clippy::unwrap_used)]

use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

fn dependency(source: &str) -> Dependency {
    serde_json::from_str(&format!(r#"{{"name": "termoxide_hot_reload", "req": "^0.1", {source}}}"#)).unwrap()
}

/// An app's crate in the folder `dir`, with its lib renamed.
fn app_in(dir: &str) -> AppCrate {
    let manifest_path = serde_json::to_string(&format!("{dir}/Cargo.toml")).unwrap();
    serde_json::from_str(&format!(
        r#"{{"name": "clock-app", "manifest_path": {manifest_path},
            "targets": [{{"name": "clockwork", "kind": ["cdylib", "rlib"], "crate_types": ["cdylib", "rlib"]}}],
            "features": {{}}, "metadata": {{"termoxide": {{"persistent": true}}}},
            "dependencies": [{{"name": "termoxide_hot_reload", "req": "*", "source": null, "path": "/termoxide"}}]}}"#
    ))
    .unwrap()
}

#[test]
fn the_runner_depends_on_termoxide_hot_reload_where_the_app_does() {
    let path = dependency(r#""source": null, "path": "/termoxide/crates/termoxide_hot_reload""#);
    let crates_io = dependency(r#""source": "registry+https://github.com/rust-lang/crates.io-index""#);
    let git = dependency(r#""source": "git+https://github.com/TermOxide/TermOxide?branch=feat/hot-reload""#);
    let elsewhere =
        dependency(r#""source": "registry+https://my.registry/index", "registry": "https://my.registry/index""#);

    assert_eq!(
        source(&path).unwrap(),
        table([("path", "/termoxide/crates/termoxide_hot_reload".into())])
    );
    assert_eq!(source(&crates_io).unwrap(), table([("version", "^0.1".into())]));
    assert_eq!(
        source(&git).unwrap(),
        table([
            ("git", "https://github.com/TermOxide/TermOxide".into()),
            ("branch", "feat/hot-reload".into())
        ])
    );
    assert!(source(&elsewhere).is_err());
}

#[test]
fn the_runner_runs_the_lib_by_its_crate_name_with_the_apps_default() {
    let app = app_in("/apps/\"clock\"");

    let manifest: Table = toml::from_str(&manifest(&app).unwrap()).unwrap();
    assert_eq!(manifest["bin"][0]["name"].as_str(), Some("clock-app"));
    assert_eq!(manifest["dependencies"]["clock-app"]["path"].as_str(), Some("/apps/\"clock\""));
    assert_eq!(
        manifest["dependencies"]["termoxide_hot_reload"]["features"][0].as_str(),
        Some("host")
    );
    let main = main(&app).unwrap();
    assert!(main.contains(r#"(::clockwork::__termoxide_app, "clockwork")"#), "{main}");
    assert!(main.contains("true,"), "{main}");
}

#[test]
fn a_folder_name_rust_would_escape_is_still_a_manifest() {
    // An accent written as a letter and a combining mark, as macOS names files.
    let app = app_in("/apps/cafe\u{301}");

    let manifest: Table = toml::from_str(&manifest(&app).unwrap()).unwrap();

    assert_eq!(
        manifest["dependencies"]["clock-app"]["path"].as_str(),
        Some("/apps/cafe\u{301}")
    );
}

#[test]
fn the_runner_takes_the_profiles_and_patches_of_the_apps_workspace() {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let root = env::temp_dir().join(format!(
        "termoxide-runner-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        r#"
            [workspace]
            members = ["clock"]

            [profile.release]
            lto = true

            [patch.crates-io]
            ratatui = { path = "vendor/ratatui" }
            serde = { git = "https://github.com/serde-rs/serde" }
        "#,
    )
    .unwrap();
    let mut app = app_in(&root.join("clock").to_string_lossy());
    app.workspace_root.clone_from(&root);

    let manifest: Table = toml::from_str(&manifest(&app).unwrap()).unwrap();
    let _ = fs::remove_dir_all(&root);

    assert_eq!(manifest["profile"]["release"]["lto"].as_bool(), Some(true));
    let patch = &manifest["patch"]["crates-io"];
    assert_eq!(
        patch["ratatui"]["path"].as_str(),
        Some(&*root.join("vendor/ratatui").to_string_lossy())
    );
    assert_eq!(patch["serde"]["git"].as_str(), Some("https://github.com/serde-rs/serde"));
    assert!(manifest["workspace"].as_table().unwrap().is_empty());
}
