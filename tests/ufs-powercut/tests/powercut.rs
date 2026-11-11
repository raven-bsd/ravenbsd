// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Runs the host-side 300-cut power-cut range and checks the TSV table.

use std::fs;

use ufs_powercut_tests::{load_golden_for_test, run, run_cut};

#[test]
fn power_cut_range_writes_table() {
    let summary = run(300).expect("power-cut run");
    assert_eq!(summary.rows.len(), 300);
    assert!(
        summary.report.is_file(),
        "missing {}",
        summary.report.display()
    );
    let text = fs::read_to_string(&summary.report).expect("read report");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 301);
    assert_eq!(lines[0], "cut\tfindings\trepaired\tclean");
    assert!(lines[1].starts_with("0\t"));
    assert!(lines[300].starts_with("299\t"));
    println!("{}", summary.summary_line());

    let base = load_golden_for_test().expect("image");
    let first = run_cut(42, &base).expect("cut 42");
    let second = run_cut(42, &base).expect("cut 42 repeat");
    assert_eq!(first, second, "cut 42 must be deterministic");
}
