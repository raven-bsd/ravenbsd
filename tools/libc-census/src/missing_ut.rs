// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `missing` ranking.

use std::collections::BTreeMap;
use std::fs;

use super::*;
use crate::model::{LibraryUse, SnapshotSummary, UndefOutput};
use crate::testutil::TempDir;

fn symbol(name: &str, ring: u8, standard: Option<&str>) -> String {
    let mut text = format!(
        "[[symbol]]\nname = \"{name}\"\nring = {ring}\nring_computed = {ring}\nring_source = \"computed\"\n"
    );
    if let Some(standard) = standard {
        text.push_str(&format!("standard = [\"{standard}\"]\n"));
    }
    text
}

fn write_symbols(temp: &TempDir) {
    let mut text = String::from("schema = 1\n");
    text.push_str(&symbol("strlcpy", 1, None));
    text.push_str(&symbol("epoll_create", 2, None));
    text.push_str(&symbol("memchr", 0, Some("iso-c")));
    temp.write("out/symbols.toml", &text);
}

fn options(temp: &TempDir) -> MissingOptions {
    MissingOptions {
        implemented: temp.path().join("implemented.txt"),
        out: temp.path().join("out"),
        min_packages: 3,
        symbols: Some(temp.path().join("out/symbols.toml")),
    }
}

fn write_undef(temp: &TempDir) {
    let undef = UndefOutput {
        skipped: None,
        strong: BTreeMap::from([("strlcpy".to_string(), 2), ("epoll_create".to_string(), 1)]),
        weak: BTreeMap::from([("strlcpy".to_string(), 1)]),
    };
    let uses = UsesOutput {
        undef: Some(undef),
        packages: 2,
        ..UsesOutput::default()
    };
    temp.write("out/uses.json", &serde_json::to_string(&uses).unwrap());
}

#[test]
fn p0_p1_p2_are_classified() {
    let temp = TempDir::new("missing-classes");
    write_symbols(&temp);
    write_undef(&temp);
    temp.write("implemented.txt", "other\n");
    let report = run_missing(&options(&temp)).unwrap();
    assert_eq!(report.source, "undef");
    assert!(report.p0_nonempty);
    let classes: BTreeMap<&str, Class> = report
        .rows
        .iter()
        .map(|row| (row.name.as_str(), row.class))
        .collect();
    assert_eq!(classes["strlcpy"], Class::P0);
    assert_eq!(classes["epoll_create"], Class::P1);
    assert_eq!(classes["memchr"], Class::P2);
}

#[test]
fn implemented_names_are_removed() {
    let temp = TempDir::new("missing-implemented");
    write_symbols(&temp);
    write_undef(&temp);
    temp.write("implemented.txt", "strlcpy\nmemchr\n");
    let report = run_missing(&options(&temp)).unwrap();
    assert_eq!(report.rows.len(), 1);
    assert_eq!(report.rows[0].name, "epoll_create");
}

#[test]
fn the_min_packages_threshold_changes_p0_to_p1() {
    let temp = TempDir::new("missing-min");
    write_symbols(&temp);
    write_undef(&temp);
    temp.write("implemented.txt", "");
    let mut options = options(&temp);
    options.min_packages = 5;
    let report = run_missing(&options).unwrap();
    assert!(!report.p0_nonempty);
    let strlcpy = report
        .rows
        .iter()
        .find(|row| row.name == "strlcpy")
        .unwrap();
    assert_eq!(strlcpy.class, Class::P1);
}

#[test]
fn undef_wins_over_patches_and_sources() {
    let temp = TempDir::new("missing-precedence");
    write_symbols(&temp);
    write_undef(&temp);
    let ports = PortsOutput {
        snapshots: BTreeMap::from([(
            "aports".to_string(),
            SnapshotSummary {
                packages: 1,
                skipped: None,
            },
        )]),
        symbols: BTreeMap::from([("strlcpy".to_string(), 9)]),
        ..PortsOutput::default()
    };
    temp.write("out/ports.json", &serde_json::to_string(&ports).unwrap());
    let uses = UsesOutput {
        symbols: BTreeMap::from([("epoll_create".to_string(), 7)]),
        packages: 1,
        undef: Some(UndefOutput {
            skipped: None,
            strong: BTreeMap::from([("strlcpy".to_string(), 2), ("epoll_create".to_string(), 1)]),
            weak: BTreeMap::from([("strlcpy".to_string(), 1)]),
        }),
        ..UsesOutput::default()
    };
    temp.write("out/uses.json", &serde_json::to_string(&uses).unwrap());
    temp.write("implemented.txt", "");
    let report = run_missing(&options(&temp)).unwrap();
    assert_eq!(report.source, "undef");
    let strlcpy = report
        .rows
        .iter()
        .find(|row| row.name == "strlcpy")
        .unwrap();
    assert_eq!(strlcpy.packages, 3);
}

#[test]
fn patches_are_used_without_undef_data() {
    let temp = TempDir::new("missing-patches");
    write_symbols(&temp);
    let ports = PortsOutput {
        snapshots: BTreeMap::from([(
            "aports".to_string(),
            SnapshotSummary {
                packages: 1,
                skipped: None,
            },
        )]),
        symbols: BTreeMap::from([("strlcpy".to_string(), 4)]),
        ..PortsOutput::default()
    };
    temp.write("out/ports.json", &serde_json::to_string(&ports).unwrap());
    temp.write("implemented.txt", "");
    let report = run_missing(&options(&temp)).unwrap();
    assert_eq!(report.source, "patches");
    assert!(report.p0_nonempty);
}

#[test]
fn sources_are_used_without_undef_or_patches() {
    let temp = TempDir::new("missing-sources");
    write_symbols(&temp);
    let uses = UsesOutput {
        symbols: BTreeMap::from([("epoll_create".to_string(), 2)]),
        packages: 1,
        libraries: BTreeMap::<String, LibraryUse>::new(),
        ..UsesOutput::default()
    };
    temp.write("out/uses.json", &serde_json::to_string(&uses).unwrap());
    temp.write("implemented.txt", "");
    let report = run_missing(&options(&temp)).unwrap();
    assert_eq!(report.source, "sources");
    assert!(report.rows.iter().any(|row| row.name == "epoll_create"));
}

#[test]
fn without_package_data_only_p2_is_reported() {
    let temp = TempDir::new("missing-nodata");
    write_symbols(&temp);
    temp.write("implemented.txt", "");
    let report = run_missing(&options(&temp)).unwrap();
    assert_eq!(report.source, "none");
    assert_eq!(report.rows.len(), 1);
    assert_eq!(report.rows[0].class, Class::P2);
    let text = temp.read("out/missing.md");
    assert!(text.contains("P0/P1 need a ports snapshot"));
    assert!(text.contains("`memchr`"));
}

#[test]
fn missing_md_has_a_provenance_header() {
    let temp = TempDir::new("missing-header");
    write_symbols(&temp);
    write_undef(&temp);
    temp.write("implemented.txt", "");
    run_missing(&options(&temp)).unwrap();
    let text = temp.read("out/missing.md");
    assert!(text.starts_with("<!-- SPDX-License-Identifier: BSD-3-Clause -->"));
    assert!(text.contains("## P0"));
    assert!(text.contains("## P1"));
    assert!(text.contains("## P2"));
}

#[test]
fn read_implemented_ignores_comments_and_blanks() {
    let temp = TempDir::new("missing-read");
    temp.write(
        "impl.txt",
        "/* comment\n over lines */\nreal\n/* trailing */ other\n\n",
    );
    let set = read_implemented(&temp.path().join("impl.txt")).unwrap();
    assert!(set.contains("real"));
    assert!(set.contains("other"));
}

#[test]
fn a_ring_three_assignment_is_not_p2() {
    let temp = TempDir::new("missing-ring3");
    temp.write(
        "out/symbols.toml",
        "schema = 1\n[[symbol]]\nname = \"gets\"\nring = 3\nring_computed = 3\nring_source = \"computed\"\nstandard = [\"iso-c\"]\n",
    );
    temp.write("implemented.txt", "");
    let report = run_missing(&MissingOptions {
        implemented: temp.path().join("implemented.txt"),
        out: temp.path().join("out"),
        min_packages: 3,
        symbols: Some(temp.path().join("out/symbols.toml")),
    })
    .unwrap();
    assert!(report.rows.is_empty());
}

#[test]
fn fs_import_is_exercised() {
    let temp = TempDir::new("missing-fs");
    fs::create_dir_all(temp.path().join("out")).unwrap();
    assert!(temp.path().join("out").is_dir());
}
