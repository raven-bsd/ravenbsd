// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::table::RawRow;

fn row(program: &str, case: &str, verdict: &str) -> RawRow {
    RawRow {
        suite: "s".to_string(),
        program: program.to_string(),
        case: case.to_string(),
        verdict: verdict.to_string(),
        reason: String::new(),
        ticket: "C40".to_string(),
    }
}

#[test]
fn an_identical_run_passes() {
    let baseline = vec![row("p", "a", "pass"), row("p", "b", "fail")];
    let report = check(&baseline, &baseline).expect("check");
    assert!(!report.failed());
    assert!(report.improvements.is_empty());
}

#[test]
fn a_pass_that_becomes_fail_is_a_regression() {
    let baseline = vec![row("p", "a", "pass")];
    let run = vec![row("p", "a", "fail")];
    let report = check(&baseline, &run).expect("check");
    assert!(report.failed());
    assert_eq!(report.regressions.len(), 1);
    assert_eq!(report.regressions[0].1, "fail");
}

#[test]
fn a_missing_row_is_vanished() {
    let baseline = vec![row("p", "a", "pass"), row("p", "b", "pass")];
    let run = vec![row("p", "a", "pass")];
    let report = check(&baseline, &run).expect("check");
    assert!(report.failed());
    assert_eq!(report.vanished.len(), 1);
    assert_eq!(report.vanished[0].name, "b");
}

#[test]
fn a_removed_marker_acknowledges_a_removal() {
    let baseline = vec![row("p", "a", "pass"), row("p", "b", "pass")];
    let mut removed = row("p", "b", "removed");
    removed.reason = "merged into a".to_string();
    let run = vec![row("p", "a", "pass"), removed];
    let report = check(&baseline, &run).expect("check");
    assert!(!report.failed());
    assert!(report.vanished.is_empty());
}

#[test]
fn a_new_row_is_unlisted() {
    let baseline = vec![row("p", "a", "pass")];
    let run = vec![row("p", "a", "pass"), row("p", "b", "pass")];
    let report = check(&baseline, &run).expect("check");
    assert!(report.failed());
    assert_eq!(report.unlisted.len(), 1);
    assert_eq!(report.unlisted[0].name, "b");
}

#[test]
fn an_improvement_passes_and_is_reported() {
    let baseline = vec![row("p", "a", "fail")];
    let run = vec![row("p", "a", "pass")];
    let report = check(&baseline, &run).expect("check");
    assert!(!report.failed());
    assert_eq!(report.improvements.len(), 1);
}

#[test]
fn removing_a_baseline_failure_is_not_a_regression() {
    let baseline = vec![row("p", "a", "fail")];
    let run = Vec::new();
    let report = check(&baseline, &run).expect("check");
    assert!(report.failed());
    assert_eq!(report.vanished.len(), 1);
}

#[test]
fn an_unknown_verdict_in_a_run_is_an_error() {
    let mut bad = row("p", "a", "pass");
    bad.verdict = "hopeful".to_string();
    assert!(run_verdicts(&[bad]).is_err());
}

#[test]
fn a_removed_row_is_not_a_verdict() {
    assert!(
        run_verdicts(&[row("p", "a", "removed")])
            .expect("verdicts")
            .is_empty()
    );
}

#[test]
fn baseline_removed_rows_are_ignored() {
    let baseline = vec![row("p", "a", "removed")];
    let report = check(&baseline, &[]).expect("check");
    assert!(!report.failed());
}

#[test]
fn the_report_is_sorted() {
    let baseline = vec![row("p", "z", "pass"), row("p", "a", "pass")];
    let report = check(&baseline, &[]).expect("check");
    assert_eq!(report.vanished[0].name, "a");
    assert_eq!(report.vanished[1].name, "z");
}
