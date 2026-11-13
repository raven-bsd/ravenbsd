// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::testutil::TempDir;

fn fixture_resources() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vendor/resources")
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("destination");
    for entry in fs::read_dir(from).expect("source") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy fixture");
        }
    }
}

/// The committed fixture plus one unclassified `.c` written at test time (it has no SPDX header,
/// so it must not be a tracked file).
fn test_resources(temp: &TempDir) -> PathBuf {
    let destination = temp.path().join("resources");
    copy_tree(&fixture_resources(), &destination);
    let unknown = destination.join("fixture/tests/libc/string/t_unknown.c");
    fs::write(
        &unknown,
        "/* No licence text in this invented fixture file. */\n\
         int fixture_unknown(void) { return 5; }\n",
    )
    .expect("unknown fixture");
    destination
}

fn options<'a>(to: &'a Path, resources: &'a Path, data: bool, dry_run: bool) -> VendorOptions<'a> {
    VendorOptions {
        donor: "fixture",
        from: "tests/libc/string",
        to,
        data,
        dry_run,
        date: Some("2026-11-13"),
        resources,
        no_pin_check: true,
    }
}

#[test]
fn classes_are_recognised_and_reject_phrases_refuse() {
    assert_eq!(
        classify("Redistribution and use ... Neither the name ...").unwrap(),
        Class::Bsd("BSD-3-Clause")
    );
    assert_eq!(
        classify("Redistribution and use, two conditions").unwrap(),
        Class::Bsd("BSD-2-Clause")
    );
    assert_eq!(classify("Permission to use, copy").unwrap(), Class::Isc);
    assert_eq!(classify("Public domain").unwrap(), Class::Pd);
    assert_eq!(
        classify("SPDX-License-Identifier: MIT").unwrap(),
        Class::Spdx("MIT".to_string())
    );
    assert!(classify("All advertising materials mentioning features").is_err());
    assert!(classify("no licence here").is_err());
    assert!(classify("SPDX-License-Identifier: GPL-2.0").is_err());
}

#[test]
fn a_vendor_run_copies_accepted_files_byte_identically() {
    let temp = TempDir::new("vendor-run");
    let resources = test_resources(&temp);
    let to = temp.path().join("vendor");
    run(&options(&to, &resources, false, false)).expect("vendor");
    let source = resources.join("fixture/tests/libc/string/t_bsd.c");
    let copied = to.join("t_bsd.c");
    assert_eq!(fs::read(&source).unwrap(), fs::read(&copied).unwrap());
    assert!(to.join("t_pd.c").is_file());
    assert!(to.join("t_isc.c").is_file());
    assert!(!to.join("t_reject.c").exists());
    assert!(!to.join("t_unknown.c").exists());
    assert!(!to.join("table.txt").exists());
    let review = fs::read_to_string(to.join("REVIEW.txt")).expect("review");
    assert!(review.contains("t_reject.c"));
    assert!(review.contains("t_unknown.c"));
    assert!(!review.contains("t_bsd.c"));
}

#[test]
fn provenance_is_valid_toml_with_one_import_per_copy() {
    let temp = TempDir::new("vendor-prov");
    let resources = test_resources(&temp);
    let to = temp.path().join("vendor");
    run(&options(&to, &resources, false, false)).expect("vendor");
    let text = fs::read_to_string(to.join("provenance.toml")).expect("provenance");
    let value: toml::Value = toml::from_str(&text).expect("toml");
    let imports = value
        .get("import")
        .and_then(toml::Value::as_array)
        .expect("import array");
    assert_eq!(imports.len(), 3);
    let first = &imports[0];
    let path = first
        .get("path")
        .and_then(toml::Value::as_str)
        .expect("path");
    assert!(path.ends_with("/t_bsd.c"), "{path}");
    assert_eq!(
        first.get("upstream").and_then(toml::Value::as_str),
        Some("fixture")
    );
    assert_eq!(
        first.get("upstream_path").and_then(toml::Value::as_str),
        Some("tests/libc/string/t_bsd.c")
    );
    assert_eq!(
        first.get("upstream_rev").and_then(toml::Value::as_str),
        Some("0123456789abcdef0123456789abcdef01234567")
    );
    assert_eq!(
        first.get("license").and_then(toml::Value::as_str),
        Some("BSD-3-Clause")
    );
    assert_eq!(first.get("tier").and_then(toml::Value::as_str), Some("A"));
    assert_eq!(
        first.get("copied").and_then(toml::Value::as_str),
        Some("2026-11-13")
    );
    assert_eq!(
        first.get("modified").and_then(toml::Value::as_bool),
        Some(false)
    );
}

#[test]
fn data_files_are_copied_only_with_data_and_a_sibling_source() {
    let temp = TempDir::new("vendor-data");
    let resources = test_resources(&temp);
    let to = temp.path().join("vendor");
    run(&options(&to, &resources, true, false)).expect("vendor");
    assert!(to.join("table.txt").is_file());
    let value: toml::Value =
        toml::from_str(&fs::read_to_string(to.join("provenance.toml")).unwrap()).unwrap();
    assert_eq!(value["import"].as_array().unwrap().len(), 4);
}

#[test]
fn a_dry_run_writes_nothing() {
    let temp = TempDir::new("vendor-dry");
    let resources = test_resources(&temp);
    let to = temp.path().join("vendor");
    run(&options(&to, &resources, false, true)).expect("dry run");
    assert!(!to.exists());
}

#[test]
fn a_bad_date_or_destination_is_an_error() {
    let temp = TempDir::new("vendor-bad");
    let resources = test_resources(&temp);
    let to = temp.path().join("vendor");
    let mut bad_date = options(&to, &resources, false, true);
    bad_date.date = Some("13-11-2026");
    assert!(run(&bad_date).is_err());
    let mut bad_to = options(Path::new("../elsewhere"), &resources, false, true);
    bad_to.date = Some("2026-11-13");
    assert!(run(&bad_to).is_err());
    let mut missing_date = options(&to, &resources, false, false);
    missing_date.date = None;
    assert!(run(&missing_date).is_err());
}
