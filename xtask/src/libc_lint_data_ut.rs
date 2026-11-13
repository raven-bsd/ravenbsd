// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::path::PathBuf;

use super::*;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn real_data_files_parse() {
    let (crates, features, allow, tree) = load(&repo()).expect("data");
    assert!(!crates.rows.is_empty());
    assert!(!features.rows.is_empty());
    assert!(allow.rows.is_empty());
    assert!(!tree.entries.is_empty());
}

#[test]
fn crates_toml_has_37_rows_and_class_counts() {
    let (crates, _, _, _) = load(&repo()).expect("data");
    assert_eq!(crates.rows.len(), 37);
    assert_eq!(
        crates.rows.iter().filter(|row| row.unsafe_ == "P").count(),
        26
    );
    assert_eq!(
        crates.rows.iter().filter(|row| row.unsafe_ == "M").count(),
        8
    );
    assert_eq!(
        crates.rows.iter().filter(|row| row.unsafe_ == "U").count(),
        3
    );
}

#[test]
fn features_toml_has_the_20_names() {
    let (_, features, _, _) = load(&repo()).expect("data");
    assert_eq!(features.rows.len(), 20);
    assert!(features.rows.iter().any(|row| row.name == "float-printf"));
}

#[test]
fn every_dependency_exists() {
    let (crates, _, _, _) = load(&repo()).expect("data");
    for row in &crates.rows {
        for dep in &row.deps {
            assert!(
                crates.rows.iter().any(|candidate| &candidate.name == dep),
                "{dep}"
            );
        }
    }
}

#[test]
fn tree_lists_six_required_entries() {
    let (_, _, _, tree) = load(&repo()).expect("data");
    assert_eq!(
        tree.entries.iter().filter(|entry| entry.required).count(),
        6
    );
}

#[test]
fn forbidden_cargo_toml_is_declared() {
    let (_, _, _, tree) = load(&repo()).expect("data");
    assert_eq!(tree.forbidden.files, vec!["Cargo.toml"]);
}

#[test]
fn provenance_root_parses() {
    let imports = load_provenance(&repo()).expect("provenance");
    assert!(imports.len() < 10_000);
}
