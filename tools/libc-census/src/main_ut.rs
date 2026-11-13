// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the CLI helpers.

use super::*;
use libc_census::model::Cell;

#[test]
fn value_reports_a_missing_argument() {
    let args = vec!["--out".to_string()];
    let error = value(&args, 1, "--out").unwrap_err().to_string();
    assert!(error.contains("--out needs a value"));
}

#[test]
fn path_value_advances_the_index() {
    let args = vec!["--out".to_string(), "build/out".to_string()];
    let mut index = 0;
    let path = path_value(&args, &mut index, "--out").unwrap();
    assert_eq!(path, PathBuf::from("build/out"));
    assert_eq!(index, 1);
}

#[test]
fn default_jobs_is_bounded() {
    let jobs = default_jobs();
    assert!((1..=8).contains(&jobs));
}

#[test]
fn default_resources_names_the_sibling_checkout() {
    let path = default_resources();
    assert!(path.ends_with("ravenbsd-resources"));
}

#[test]
fn computed_rings_uses_standards_and_families() {
    let row = CellRow {
        name: "strlcpy".to_string(),
        kind: Kind::Func,
        cells: BTreeMap::from([("glibc".to_string(), Cell { d: 1, h: 1 })]),
        n6: 1,
        libcs: vec!["glibc".to_string()],
        families: vec!["linux".to_string()],
    };
    let standards = StandardsOutput {
        entries: BTreeMap::from([(
            "strlcpy".to_string(),
            StandardEntry {
                standard: vec!["posix-2024".to_string()],
                header: Some("string.h".to_string()),
                kind: "func".to_string(),
            },
        )]),
    };
    let rings = computed_rings(&[row], &standards);
    assert_eq!(rings["strlcpy"], 0);
}
