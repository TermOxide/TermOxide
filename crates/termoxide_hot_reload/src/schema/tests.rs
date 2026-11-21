#![allow(clippy::unwrap_used)]

use serde::{Deserialize, Serialize};
use termoxide_reactive::Signal;

use super::*;

#[derive(Serialize, Deserialize)]
struct Counter {
    count: Signal<u32>,
}

/// `Counter` with a field added: its encoding changed.
#[derive(Serialize, Deserialize)]
#[serde(rename = "Counter")]
struct CounterWithMore {
    count: Signal<u32>,
    more: Signal<u8>,
}

/// `Counter` with its field's type changed.
#[derive(Serialize, Deserialize)]
#[serde(rename = "Counter")]
struct CounterOfWords {
    count: Signal<String>,
}

#[test]
fn the_protocol_can_be_fingerprinted() {
    // Every type a message carries is traced: events, views with their styles
    // and colours, paths.
    assert_eq!(wire().unwrap(), wire().unwrap());
}

#[test]
fn a_type_keeps_its_fingerprint() {
    assert_eq!(fingerprint::<Counter>().unwrap(), fingerprint::<Counter>().unwrap());
}

#[test]
fn a_new_field_changes_the_fingerprint() {
    assert_ne!(fingerprint::<Counter>().unwrap(), fingerprint::<CounterWithMore>().unwrap());
}

#[test]
fn a_field_of_another_type_changes_the_fingerprint() {
    assert_ne!(fingerprint::<Counter>().unwrap(), fingerprint::<CounterOfWords>().unwrap());
}

#[derive(Serialize, Deserialize)]
enum Mode {
    Normal,
    Insert,
}

#[derive(Serialize, Deserialize)]
#[serde(rename = "Mode")]
enum ModeReordered {
    Insert,
    Normal,
}

#[derive(Serialize, Deserialize)]
struct Editor {
    mode: Signal<Mode>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename = "Editor")]
struct EditorReordered {
    mode: Signal<ModeReordered>,
}

#[test]
fn reordering_a_nested_enum_changes_the_fingerprint() {
    // The variants keep their names but swap indices: postcard would decode
    // one as the other.
    assert_ne!(fingerprint::<Editor>().unwrap(), fingerprint::<EditorReordered>().unwrap());
}

#[derive(Serialize, Deserialize)]
enum Pane {
    List,
    Edit(String),
    Help,
}

#[derive(Serialize, Deserialize)]
#[serde(rename = "Pane")]
enum PaneWithLaterVariantsSwapped {
    List,
    Help,
    Edit(String),
}

#[derive(Serialize, Deserialize)]
#[serde(rename = "Pane")]
enum PaneEditingANumber {
    List,
    Edit(u64),
    Help,
}

#[derive(Serialize, Deserialize)]
struct Screen<P: 'static> {
    pane: Signal<P>,
}

#[test]
fn every_variant_of_a_nested_enum_counts() {
    let screen = fingerprint::<Screen<Pane>>().unwrap();

    assert_ne!(screen, fingerprint::<Screen<PaneWithLaterVariantsSwapped>>().unwrap());
    assert_ne!(screen, fingerprint::<Screen<PaneEditingANumber>>().unwrap());
}

#[derive(Serialize, Deserialize)]
enum Focus<P> {
    Nothing,
    On(P),
}

#[test]
fn an_enum_only_a_later_variant_holds_is_traced_whole() {
    assert_ne!(
        fingerprint::<Screen<Focus<Pane>>>().unwrap(),
        fingerprint::<Screen<Focus<PaneWithLaterVariantsSwapped>>>().unwrap()
    );
}

#[derive(Serialize, Deserialize)]
struct Tree {
    label: Pane,
    children: Vec<Tree>,
    parent: Option<Box<Tree>>,
}

#[derive(Serialize, Deserialize)]
enum Chain {
    Link(Box<Chain>),
    End,
}

#[test]
fn types_that_contain_themselves_are_traced() {
    assert!(fingerprint::<Tree>().is_ok());
    assert!(fingerprint::<Chain>().is_ok());
}

/// From FNV's published test vectors.
#[test]
fn fnv1a_matches_its_reference_values() {
    assert_eq!(fnv1a(b""), FNV_OFFSET_BASIS);
    assert_eq!(fnv1a(b"a"), 0xAF63_DC4C_8601_EC8C);
    assert_eq!(fnv1a(b"foobar"), 0x8594_4171_F739_67E8);
}
