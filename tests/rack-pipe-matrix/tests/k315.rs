// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K315 union row for the K333 pipe/lost group.
//!
//! The K315 checker validates one donor-function matrix at a time. This file holds the K333
//! contribution to the 219-function union and proves it is unique, known and clean. The two
//! persist functions assigned to K333 by `docs/rack-coverage.md` are absent from the merged
//! module and are recorded as a finding for the group owner.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use rack_coverage::{Function, Row, check, parse_manifest, parse_matrix};

/// The nine K333 donor functions implemented by the merged `rack_pipe.rs`.
const K333_IMPLEMENTED: [&str; 9] = [
    "do_rack_compute_pipe",
    "rack_compute_pipe",
    "rack_mark_lost",
    "rack_mark_nolonger_lost",
    "rack_check_recovery_mode",
    "rack_post_recovery",
    "rack_exit_recovery",
    "rack_calc_thresh_rack",
    "rack_calc_thresh_tlp",
];

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relative)
}

fn manifest() -> Vec<Function> {
    let text = fs::read_to_string(workspace_path(
        "tests/rack-coverage/fixtures/rack-functions.txt",
    ))
    .expect("K315 manifest");
    parse_manifest(&text)
}

fn docs_text() -> String {
    fs::read_to_string(workspace_path("docs/rack-pipe-matrix.md"))
        .expect("docs/rack-pipe-matrix.md")
}

fn rows() -> Vec<Row> {
    parse_matrix(&docs_text())
}

#[test]
fn k315_group_row_is_unique_known_and_clean() {
    let donor = manifest();
    let rows = rows();
    assert_eq!(donor.len(), 219, "the pinned K315 donor manifest");
    assert_eq!(rows.len(), 9, "the K333 contribution");

    let report = check(&donor, &rows);
    assert_eq!(report.row_count, 9);
    assert_eq!(report.missing.len(), 210, "the other K328-K336/K269 groups");
    assert!(report.duplicates.is_empty(), "{:?}", report.duplicates);
    assert!(report.unknown.is_empty(), "{:?}", report.unknown);
    assert!(report.empty_rust.is_empty(), "{:?}", report.empty_rust);
    assert!(report.empty_test.is_empty(), "{:?}", report.empty_test);
    assert!(
        report.empty_mutation.is_empty(),
        "{:?}",
        report.empty_mutation
    );
    assert!(report.not_pass.is_empty(), "{:?}", report.not_pass);
    assert!(!report.is_ok(), "a per-group matrix is not the full union");

    for row in &rows {
        assert!(
            !report.missing.contains(&row.function),
            "{} must not be missing",
            row.function
        );
    }
}

#[test]
fn k315_group_row_covers_every_implemented_function() {
    let rows = rows();
    let names: BTreeSet<&str> = rows.iter().map(|row| row.function.as_str()).collect();
    let expected: BTreeSet<&str> = K333_IMPLEMENTED.into_iter().collect();
    assert_eq!(names, expected);
}

#[test]
fn k315_assigns_persist_functions_to_k333_but_the_module_lacks_them() {
    let docs =
        fs::read_to_string(workspace_path("docs/rack-coverage.md")).expect("docs/rack-coverage.md");
    let assigned: Vec<String> = docs
        .lines()
        .filter(|line| line.contains("ASSIGNED K333"))
        .filter_map(|line| {
            let start = line.find('`')?;
            let end = line[start + 1..].find('`')?;
            Some(line[start + 1..start + 1 + end].to_owned())
        })
        .collect();
    assert_eq!(assigned.len(), 11, "the canonical K333 assignment");

    let rows = rows();
    let implemented: BTreeSet<&str> = rows.iter().map(|row| row.function.as_str()).collect();
    let missing: Vec<&str> = assigned
        .iter()
        .filter(|function| !implemented.contains(function.as_str()))
        .map(String::as_str)
        .collect();
    assert_eq!(
        missing,
        ["rack_enter_persist", "rack_exit_persist"],
        "K345 finding: the two persist donor functions are not in rack_pipe.rs"
    );

    let donor = manifest();
    for function in &missing {
        assert!(
            donor.iter().any(|entry| entry.name == *function),
            "{function} is a pinned donor function"
        );
        assert!(
            !implemented.contains(*function),
            "{function} must not appear in the implemented K315 row"
        );
    }
}

#[test]
fn k315_checker_rejects_a_dropped_row_or_an_empty_check() {
    let donor = manifest();

    let mut dropped = rows();
    let removed = dropped.pop().expect("row");
    let report = check(&donor, &dropped);
    assert!(
        report.missing.contains(&removed.function),
        "a dropped row is reported missing"
    );

    let mut empty = rows();
    empty[0].mutation = "-".to_owned();
    let report = check(&donor, &empty);
    assert!(
        report.empty_mutation.contains(&empty[0].function),
        "an empty mutation check is rejected"
    );

    let mut failed = rows();
    failed[0].status = "FAIL".to_owned();
    let report = check(&donor, &failed);
    assert!(
        report.not_pass.contains(&failed[0].function),
        "a non-PASS row is rejected"
    );
}
