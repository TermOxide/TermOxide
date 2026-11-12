#![allow(clippy::unwrap_used)]

use super::*;

fn dependency(source: &str) -> Dependency {
    serde_json::from_str(&format!(r#"{{"name": "termoxide_hot_reload", "req": "^0.1", {source}}}"#)).unwrap()
}

#[test]
fn the_runner_depends_on_termoxide_hot_reload_where_the_app_does() {
    let path = dependency(r#""source": null, "path": "/termoxide/crates/termoxide_hot_reload""#);
    let crates_io = dependency(r#""source": "registry+https://github.com/rust-lang/crates.io-index""#);
    let git = dependency(r#""source": "git+https://github.com/TermOxide/TermOxide?branch=main""#);
    let elsewhere =
        dependency(r#""source": "registry+https://my.registry/index", "registry": "https://my.registry/index""#);

    assert_eq!(source(&path).unwrap(), r#"path = "/termoxide/crates/termoxide_hot_reload""#);
    assert_eq!(source(&crates_io).unwrap(), r#"version = "^0.1""#);
    assert_eq!(
        source(&git).unwrap(),
        r#"git = "https://github.com/TermOxide/TermOxide", branch = "main""#
    );
    assert!(source(&elsewhere).is_err());
}

#[test]
fn the_runner_runs_the_lib_by_its_crate_name_with_the_apps_default() {
    let app: AppCrate = serde_json::from_str(
        r#"{"name": "clock-app", "manifest_path": "/apps/\"clock\"/Cargo.toml",
            "targets": [{"name": "clockwork", "kind": ["cdylib", "rlib"], "crate_types": ["cdylib", "rlib"]}],
            "features": {}, "metadata": {"termoxide": {"persistent": true}},
            "dependencies": [{"name": "termoxide_hot_reload", "req": "*", "source": null, "path": "/termoxide"}]}"#,
    )
    .unwrap();

    let manifest: toml::Table = toml::from_str(&manifest(&app).unwrap()).unwrap();
    assert_eq!(manifest["bin"][0]["name"].as_str(), Some("clock-app"));
    assert_eq!(manifest["dependencies"]["clock-app"]["path"].as_str(), Some("/apps/\"clock\""));
    let main = main(&app).unwrap();
    assert!(main.contains(r#"(::clockwork::__termoxide_app, "clockwork")"#), "{main}");
    assert!(main.contains("true,"), "{main}");
}
