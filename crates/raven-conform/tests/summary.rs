// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Summary aggregation tests.

use raven_conform::pjdfstest::parse;
use raven_conform::summary::{SummaryRow, json, markdown, summarize};
use raven_conform::{Results, Status, TestResult};

const PJDFSTEST: &str = include_str!("fixtures/pjdfstest-sample.txt");

#[test]
fn fixture_summary_has_both_suites() {
    let results = parse(PJDFSTEST, None);
    let rows = summarize(&[results]);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].suite, "tests/chmod/00.t");
    assert_eq!(rows[0].total, 6);
    assert_eq!(rows[0].pass, 3);
    assert_eq!(rows[0].fail, 2);
    assert_eq!(rows[0].skip, 1);
    assert_eq!(rows[0].broken, 0);
    assert!((rows[0].pass_rate - 50.0).abs() < f64::EPSILON);
    assert_eq!(rows[1].suite, "tests/open/00.t");
    assert_eq!(rows[1].pass_rate, 100.0);
}

#[test]
fn summary_aggregates_multiple_files() {
    let first = results(vec![
        result("alpha", "one", Status::Pass),
        result("alpha", "two", Status::Fail),
    ]);
    let second = results(vec![
        result("alpha", "three", Status::Skip),
        result("beta", "four", Status::Pass),
    ]);
    let rows = summarize(&[first, second]);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].total, 3);
    assert_eq!(rows[0].pass, 1);
    assert_eq!(rows[0].fail, 1);
    assert_eq!(rows[0].skip, 1);
    assert_eq!(rows[1].suite, "beta");
}

#[test]
fn markdown_table_is_deterministic() {
    let rows = summarize(&[results(vec![
        result("b", "x", Status::Broken),
        result("a", "y", Status::Pass),
    ])]);
    let table = markdown(&rows);
    assert!(table.starts_with("| Suite | Total | Pass | Fail | Skip | Broken | Pass rate |\n"));
    assert!(table.contains("| a | 1 | 1 | 0 | 0 | 0 | 100.0% |\n"));
    assert!(table.contains("| b | 1 | 0 | 0 | 0 | 1 | 0.0% |\n"));
    assert!(table.find("| a ").unwrap() < table.find("| b ").unwrap());
}

#[test]
fn json_summary_round_trips() {
    let rows = summarize(&[results(vec![result("suite", "x", Status::Pass)])]);
    let text = json(&rows).unwrap();
    let decoded: Vec<SummaryRow> = serde_json::from_str(&text).unwrap();
    assert_eq!(decoded, rows);
}

#[test]
fn empty_results_produce_an_empty_table() {
    let rows = summarize(&[results(Vec::new())]);
    assert!(rows.is_empty());
    assert_eq!(
        markdown(&rows),
        "| Suite | Total | Pass | Fail | Skip | Broken | Pass rate |\n\
         |---|---:|---:|---:|---:|---:|---:|\n"
    );
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
