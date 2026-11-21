#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn persist_is_unset_unless_the_file_sets_it() {
    assert_eq!(Config::parse("").unwrap(), Config { persist: None });
    assert_eq!(Config::parse("persist = false").unwrap(), Config { persist: Some(false) });
}

#[test]
fn a_key_it_doesnt_know_is_an_error() {
    // The manifest's name for the app's default, easy to write here instead.
    let error = Config::parse("persistent = true").unwrap_err();
    assert!(error.to_string().contains("persistent"), "{error}");
}

#[test]
fn a_directory_without_one_sets_nothing() {
    let dir = std::env::temp_dir().join(format!("termoxide-config-test-{}", std::process::id()));
    assert_eq!(Config::read(&dir).unwrap(), Config::default());
}

#[test]
fn a_file_that_doesnt_parse_is_named_in_the_error() {
    let dir = std::env::temp_dir().join(format!("termoxide-config-test-broken-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(FILE), "persist = yes").unwrap();
    let error = format!("{:?}", Config::read(&dir).unwrap_err());
    let _ = fs::remove_dir_all(&dir);
    assert!(error.contains(&dir.join(FILE).display().to_string()), "{error}");
}
