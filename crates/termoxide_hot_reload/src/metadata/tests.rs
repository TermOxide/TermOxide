#![allow(clippy::unwrap_used)]

use super::*;

/// A package named `name`, of `targets` given as `(name, kind)`, with a lib of
/// `crate_types` when it has one, and `features` and `metadata` as JSON.
fn package(name: &str, targets: &[(&str, &str)], crate_types: &[&str], features: &str, metadata: &str) -> String {
    let targets: Vec<_> = targets
        .iter()
        .map(|(target, kind)| {
            let crate_types = if *kind == "bin" { vec!["bin"] } else { crate_types.to_vec() };
            format!(
                r#"{{"name": "{target}", "kind": {kind:?}, "crate_types": {crate_types:?}}}"#,
                kind = [kind]
            )
        })
        .collect();
    let targets = targets.join(", ");
    let manifest_path = format!("/apps/{name}/Cargo.toml");
    format!(
        r#"{{"name": "{name}", "manifest_path": "{manifest_path}", "targets": [{targets}], "features": {features}, "#,
    ) + &format!(r#""metadata": {metadata}}}"#)
}

fn app(targets: &[(&str, &str)], crate_types: &[&str], features: &str, metadata: &str) -> AppCrate {
    let metadata = format!(
        r#"{{"packages": [{}]}}"#,
        package("clock", targets, crate_types, features, metadata)
    );
    AppCrate::find(metadata.as_bytes(), Path::new("/apps/clock/Cargo.toml")).unwrap()
}

#[test]
fn the_package_is_the_one_of_the_manifest_among_the_workspace_members() {
    let metadata = format!(
        r#"{{"packages": [{}, {}]}}"#,
        package("timer", &[], &[], "{}", "null"),
        package("clock", &[], &[], "{}", "null")
    );

    assert_eq!(
        AppCrate::find(metadata.as_bytes(), Path::new("/apps/clock/Cargo.toml"))
            .unwrap()
            .name,
        "clock"
    );
    assert!(AppCrate::find(metadata.as_bytes(), Path::new("/apps/Cargo.toml")).is_err());
}

#[test]
fn an_app_persists_only_when_its_metadata_says_so() {
    assert!(!app(&[], &[], "{}", "null").persistent());
    assert!(!app(&[], &[], "{}", r#"{"other": 1}"#).persistent());
    assert!(app(&[], &[], "{}", r#"{"termoxide": {"persistent": true}}"#).persistent());
}

#[test]
fn a_runnable_lib_is_a_cdylib_and_an_rlib_with_a_guest_feature() {
    let guest = r#"{"guest": []}"#;
    let lib = [("clock", "cdylib")];

    assert!(app(&lib, &["cdylib", "rlib"], guest, "null").check_runnable().is_ok());
    let error = app(&lib, &["cdylib"], guest, "null").check_runnable().unwrap_err().to_string();
    assert!(error.ends_with("it lacks rlib"), "{error}");
    let error = app(&lib, &["lib"], guest, "null").check_runnable().unwrap_err().to_string();
    assert!(error.ends_with("it lacks cdylib and rlib"), "{error}");
    let error = app(&lib, &["cdylib", "rlib"], "{}", "null")
        .check_runnable()
        .unwrap_err()
        .to_string();
    assert!(error.contains("`guest` feature"), "{error}");
    assert!(app(&[("clock", "bin")], &[], guest, "null").check_runnable().is_err());
}

#[test]
fn the_app_is_named_after_its_only_binary_or_else_its_package() {
    assert_eq!(app(&[("clock", "lib")], &[], "{}", "null").app_name(None).unwrap(), "clock");
    assert_eq!(
        app(&[("clock", "lib"), ("tick", "bin")], &[], "{}", "null")
            .app_name(None)
            .unwrap(),
        "tick"
    );
}

#[test]
fn several_binaries_need_one_picked() {
    let app = app(&[("tick", "bin"), ("tock", "bin")], &[], "{}", "null");

    assert!(app.app_name(None).unwrap_err().to_string().ends_with("tick, tock"));
    assert_eq!(app.app_name(Some("tock")).unwrap(), "tock");
    assert!(app.app_name(Some("clock")).is_err());
}
