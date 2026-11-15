// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stored-result table and verdict tests.

use super::*;
use crate::model::CheckResult;
use crate::rows::{BlockedOn, CheckSpec, PassRule, Thresholds};
use crate::testutil::TempDir;

fn spec(id: &str, row: u8, required: bool) -> CheckSpec {
    CheckSpec {
        id: id.to_string(),
        row,
        title: format!("check {id}"),
        required,
        kind: "argv".to_string(),
        argv: vec!["cargo".to_string()],
        child_argv: Vec::new(),
        preflight: Vec::new(),
        pass: PassRule::default(),
        blocked_on: BlockedOn::default(),
        evidence: format!("build/libc-gate/{id}.txt"),
        source: "test".to_string(),
        read: None,
        file: None,
        min_seconds: None,
        cases: Vec::new(),
        expand: None,
        ticket: None,
        note: None,
    }
}

fn rows(checks: Vec<CheckSpec>) -> Rows {
    Rows {
        check: checks,
        thresholds: Thresholds::default(),
    }
}

fn result(id: &str, row: u8, status: Status) -> CheckResult {
    CheckResult {
        id: id.to_string(),
        row,
        title: format!("check {id}"),
        status,
        exit: Some(0),
        reason: "test".to_string(),
        evidence: format!("build/libc-gate/{id}.txt"),
    }
}

fn six_rows() -> Rows {
    rows(
        (1..=6u8)
            .map(|row| spec(&format!("{row}.1"), row, true))
            .collect(),
    )
}

fn six_results(status: Status) -> Vec<CheckResult> {
    (1..=6u8)
        .map(|row| result(&format!("{row}.1"), row, status))
        .collect()
}

#[test]
fn all_met_is_met() {
    let rows = six_rows();
    let results = six_results(Status::Met);
    assert_eq!(verdict_line(&results, &rows).0, Status::Met);
}

#[test]
fn a_missing_result_is_not_met() {
    let rows = rows(vec![spec("1.1", 1, true)]);
    assert_eq!(verdict_line(&[], &rows).0, Status::NotImplemented);
}

#[test]
fn a_blocked_result_is_blocked() {
    let rows = rows(vec![spec("5.1", 5, true)]);
    let results = [result("5.1", 5, Status::Blocked)];
    assert_eq!(verdict_line(&results, &rows).0, Status::Blocked);
}

#[test]
fn info_never_gates() {
    let rows = rows(vec![spec("4.4", 4, false)]);
    let results = [result("4.4", 4, Status::NotImplemented)];
    let (verdict, rows_status) = verdict_line(&results, &rows);
    assert_eq!(rows_status[3], Status::Info);
    assert_eq!(verdict, Status::Blocked);
}

#[test]
fn a_not_met_row_wins_over_blocked() {
    let rows = rows(vec![spec("1.1", 1, true), spec("2.1", 2, true)]);
    let results = [
        result("1.1", 1, Status::NotImplemented),
        result("2.1", 2, Status::Blocked),
    ];
    assert_eq!(verdict_line(&results, &rows).0, Status::NotImplemented);
}

#[test]
fn render_contains_every_spec_and_the_verdict() {
    let rows = six_rows();
    let results = six_results(Status::Met);
    let table = render(&results, &rows);
    assert!(table.contains("| 1 correctness | 1.1"));
    assert!(table.contains("| 2 portability | 2.1"));
    assert!(table.contains("Verdict: MET"));
}

#[test]
fn render_is_deterministic() {
    let rows = rows(vec![spec("1.1", 1, true)]);
    let results = [result("1.1", 1, Status::Met)];
    assert_eq!(render(&results, &rows), render(&results, &rows));
}

#[test]
fn expanded_children_aggregate_to_the_parent() {
    let rows = rows(vec![spec("1.4", 1, true)]);
    let results = [
        result("1.4.b", 1, Status::Met),
        result("1.4.a", 1, Status::Met),
    ];
    let (_, rows_status) = verdict_line(&results, &rows);
    assert_eq!(rows_status[0], Status::Met);
}

#[test]
fn an_expanded_parent_with_one_failed_child_is_not_met() {
    let rows = rows(vec![spec("1.5", 1, true)]);
    let results = [
        result("1.5.a", 1, Status::Met),
        result("1.5.b", 1, Status::NotImplemented),
    ];
    let (verdict, _) = verdict_line(&results, &rows);
    assert_eq!(verdict, Status::NotImplemented);
}

#[test]
fn load_results_reads_json_files() {
    let temp = TempDir::new("report");
    let out = temp.path().join("build/libc-gate");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        out.join("1.1.json"),
        serde_json::to_string(&result("1.1", 1, Status::Met)).unwrap(),
    )
    .unwrap();
    let results = load_results(&out).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "1.1");
}

#[test]
fn a_missing_result_directory_is_empty() {
    let temp = TempDir::new("report-empty");
    assert!(load_results(&temp.path().join("nope")).unwrap().is_empty());
}
