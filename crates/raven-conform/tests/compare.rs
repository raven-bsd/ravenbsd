// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! KNOWN_FAILURES comparison tests.

use raven_conform::compare;
use raven_conform::known::KnownFailures;
use raven_conform::result::{Results, Status, TestResult};

const KNOWN: &str = include_str!("fixtures/KNOWN_FAILURES");
const REGRESSION: &str = include_str!("fixtures/results-regression.json");
const FIXED: &str = include_str!("fixtures/results-fixed.json");
const CLEAN: &str = include_str!("fixtures/results-clean.json");

#[test]
fn clean_known_failures_exit_zero() {
    let results: Results = serde_json::from_str(CLEAN).unwrap();
    let known = KnownFailures::parse(KNOWN).unwrap();
    let comparison = compare(&results, &known, false);
    assert!(comparison.regressions.is_empty());
    assert!(comparison.fixed.is_empty());
    assert_eq!(comparison.expected, 2);
    assert_eq!(comparison.exit_code(false), 0);
}

#[test]
fn new_failure_is_a_regression() {
    let results: Results = serde_json::from_str(REGRESSION).unwrap();
    let known = KnownFailures::parse(KNOWN).unwrap();
    let comparison = compare(&results, &known, false);
    assert_eq!(comparison.regressions.len(), 1);
    assert_eq!(comparison.regressions[0].name, "chmod: expected EACCES");
    assert_eq!(comparison.exit_code(false), 1);
}

#[test]
fn passing_known_failure_is_fixed() {
    let results: Results = serde_json::from_str(FIXED).unwrap();
    let known = KnownFailures::parse(KNOWN).unwrap();
    let comparison = compare(&results, &known, false);
    assert_eq!(comparison.fixed.len(), 1);
    assert_eq!(comparison.exit_code(false), 3);
}

#[test]
fn allow_new_reports_new_known_and_exits_four() {
    let results: Results = serde_json::from_str(REGRESSION).unwrap();
    let known = KnownFailures::parse(KNOWN).unwrap();
    let comparison = compare(&results, &known, true);
    assert!(comparison.regressions.is_empty());
    assert_eq!(comparison.new_known.len(), 1);
    assert_eq!(comparison.exit_code(true), 4);
}

#[test]
fn known_skips_are_expected() {
    let results = results(vec![result("suite", "skip", Status::Skip)]);
    let known = KnownFailures::parse("suite:skip\n").unwrap();
    let comparison = compare(&results, &known, false);
    assert_eq!(comparison.expected, 1);
    assert_eq!(comparison.exit_code(false), 0);
}

#[test]
fn unlisted_passes_are_fine() {
    let results = results(vec![result("suite", "pass", Status::Pass)]);
    let known = KnownFailures::parse("").unwrap();
    let comparison = compare(&results, &known, false);
    assert_eq!(comparison.exit_code(false), 0);
}

#[test]
fn regression_takes_priority_over_fixed() {
    let results = results(vec![
        result("suite", "new", Status::Fail),
        result("suite", "old", Status::Pass),
    ]);
    let known = KnownFailures::parse("suite:old\n").unwrap();
    let comparison = compare(&results, &known, false);
    assert_eq!(comparison.regressions.len(), 1);
    assert_eq!(comparison.fixed.len(), 1);
    assert_eq!(comparison.exit_code(false), 1);
}

#[test]
fn absent_known_entry_is_not_a_failure() {
    let results = results(vec![result("suite", "other", Status::Pass)]);
    let known = KnownFailures::parse("suite:missing\n").unwrap();
    let comparison = compare(&results, &known, false);
    assert_eq!(comparison.exit_code(false), 0);
}

#[test]
fn malformed_known_file_is_an_error() {
    assert!(KnownFailures::parse("missing-colon\n").is_err());
    assert!(KnownFailures::parse("suite:\n").is_err());
    assert!(KnownFailures::parse(":name\n").is_err());
}

#[test]
fn duplicate_suffix_and_comments_are_preserved() {
    let known = KnownFailures::parse("# header\nsuite:name#2  # reason\n").unwrap();
    assert_eq!(known.len(), 1);
    assert!(known.contains("suite", "name#2"));
}

fn result(suite: &str, name: &str, status: Status) -> TestResult {
    TestResult {
        suite: suite.to_string(),
        name: name.to_string(),
        status,
        duration_ms: None,
        message: None,
    }
}

fn results(results: Vec<TestResult>) -> Results {
    Results {
        tool: String::from("x"),
        tool_version: None,
        target: None,
        unparsed: 0,
        results,
    }
}
