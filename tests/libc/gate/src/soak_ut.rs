// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Soak table tests.

use super::*;

const HEADER: &str = "\
| target | seconds | executions | exec/s | corpus files after | new coverage edges | crashes | regression files |
|---|---|---|---|---|---|---|---|
";

fn table(rows: &str) -> String {
    format!("{HEADER}{rows}")
}

fn target(name: &str) -> String {
    name.to_string()
}

#[test]
fn parses_seconds_and_crashes() {
    let rows = parse(&table(
        "| alpha | 3600 | 10,000 | 2 | 1 | 2 | 0 | 0 |\n\
         | beta | 3599 | 10,000 | 2 | 1 | 2 | 0 | 0 |\n\
         | gamma | — (crash) | — | — | — | — | 1 | 1 |\n",
    ));
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].target, "alpha");
    assert_eq!(rows[0].seconds, Some(3600));
    assert_eq!(rows[1].seconds, Some(3599));
    assert_eq!(rows[2].seconds, None);
    assert_eq!(rows[2].crashes, 1);
}

#[test]
fn exactly_3600_seconds_passes() {
    let rows = parse(&table("| alpha | 3600 | 10,000 | 2 | 1 | 2 | 0 | 0 |\n"));
    let report = evaluate(&rows, &[target("alpha")], &[], 3600);
    assert_eq!(report.status, Status::Met);
}

#[test]
fn seconds_3599_fail() {
    let rows = parse(&table("| beta | 3599 | 10,000 | 2 | 1 | 2 | 0 | 0 |\n"));
    let report = evaluate(&rows, &[target("beta")], &[], 3600);
    assert_eq!(report.status, Status::NotImplemented);
    assert!(report.reason.contains("3599"));
}

#[test]
fn one_crash_fails() {
    let rows = parse(&table(
        "| alpha | 3600 | 10,000 | 2 | 1 | 2 | 0 | 0 |\n\
         | gamma | — (crash) | — | — | — | — | 1 | 1 |\n",
    ));
    let report = evaluate(&rows, &[target("alpha"), target("gamma")], &[], 3600);
    assert_eq!(report.status, Status::NotImplemented);
    assert!(report.reason.contains("gamma"));
}

#[test]
fn deferred_rows_do_not_fail() {
    let rows = parse(&table(
        "| alpha | 3600 | 10,000 | 2 | 1 | 2 | 0 | 0 |\n\
         | gamma | — (crash) | — | — | — | — | 1 | 1 |\n",
    ));
    let report = evaluate(&rows, &[target("alpha")], &[target("gamma")], 3600);
    assert_eq!(report.status, Status::Met);
}

#[test]
fn a_missing_target_fails() {
    let rows = parse(&table("| alpha | 3600 | 10,000 | 2 | 1 | 2 | 0 | 0 |\n"));
    let report = evaluate(&rows, &[target("delta")], &[], 3600);
    assert_eq!(report.status, Status::NotImplemented);
    assert!(report.reason.contains("delta"));
}

#[test]
fn zero_executions_fails() {
    let rows = parse(&table("| alpha | 3600 | 0 | 0 | 1 | 2 | 0 | 0 |\n"));
    let report = evaluate(&rows, &[target("alpha")], &[], 3600);
    assert_eq!(report.status, Status::NotImplemented);
}

#[test]
fn no_rows_is_blocked() {
    let report = evaluate(&[], &[], &[], 3600);
    assert_eq!(report.status, Status::Blocked);
}

#[test]
fn deferred_targets_are_parsed() {
    let text = "\
## DEFERRED targets

| target | reason |
|---|---|
| `iconv_stream` | `DEFERRED: C48 not done` |
";
    assert_eq!(deferred_targets(text), vec!["iconv_stream"]);
}

#[test]
fn apply_deferred_marks_rows() {
    let mut rows = parse(&table(
        "| alpha | 3600 | 10,000 | 2 | 1 | 2 | 0 | 0 |\n\
         | gamma | — (crash) | — | — | — | — | 1 | 1 |\n",
    ));
    apply_deferred(&mut rows, &[target("gamma")]);
    assert!(
        rows.iter()
            .find(|row| row.target == "gamma")
            .unwrap()
            .deferred
    );
}
