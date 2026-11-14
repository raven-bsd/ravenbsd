// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::{Case, Outcome, Verdict};

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "raven-atf-table-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp");
    let _ = fs::create_dir_all(path.join("runs/oracle"));
    let _ = fs::create_dir_all(path.join("runs/target"));
    path
}

fn outcome(suite: &str, program: &str, case: &str, verdict: Verdict, reason: &str) -> Outcome {
    Outcome::new(Case::new(suite, program, case), verdict, reason, "C40")
}

#[test]
fn a_tsv_round_trip_keeps_every_field() {
    let root = temp("roundtrip");
    let path = root.join("run.tsv");
    let rows = vec![
        outcome("s", "p", "b", Verdict::Fail, "behaviour:bad"),
        outcome("s", "p", "a", Verdict::Pass, ""),
    ];
    write_outcomes(&path, &rows).expect("write");
    let text = fs::read_to_string(&path).expect("read");
    assert!(text.starts_with("# SPDX-License-Identifier: BSD-3-Clause\n"));
    assert!(text.contains("# schema 1\n"));
    let parsed = read_raw(&path).expect("parse");
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].case, "a");
    assert_eq!(parsed[0].verdict, "pass");
    assert_eq!(parsed[1].reason, "behaviour:bad");
}

#[test]
fn a_pass_row_omits_empty_trailing_fields() {
    let root = temp("trim");
    let path = root.join("run.tsv");
    let mut pass = outcome("s", "p", "a", Verdict::Pass, "");
    pass.ticket.clear();
    write_outcomes(&path, &[pass]).expect("write");
    let line = fs::read_to_string(&path)
        .expect("read")
        .lines()
        .find(|line| !line.starts_with('#'))
        .expect("row")
        .to_string();
    assert_eq!(line, "s\tp\ta\tpass");
}

#[test]
fn rows_are_sorted_by_case() {
    let root = temp("sort");
    let path = root.join("run.tsv");
    write_outcomes(
        &path,
        &[
            outcome("s", "p", "z", Verdict::Pass, ""),
            outcome("s", "p", "a", Verdict::Pass, ""),
        ],
    )
    .expect("write");
    let rows = read_raw(&path).expect("read");
    assert_eq!(rows[0].case, "a");
    assert_eq!(rows[1].case, "z");
}

#[test]
fn a_malformed_row_is_an_error() {
    let root = temp("malformed");
    let path = root.join("bad.tsv");
    fs::write(&path, "only\ttwo\n").expect("write");
    assert!(read_raw(&path).is_err());
}

#[test]
fn build_statuses_round_trip() {
    let root = temp("builds");
    let path = root.join("builds.tsv");
    let mut builds = BTreeMap::new();
    builds.insert("a".to_string(), crate::BuildStatus::Built);
    builds.insert(
        "b".to_string(),
        crate::BuildStatus::Failed("missing-header:x.h".to_string()),
    );
    write_builds(&path, &builds).expect("write");
    let read = read_builds(&path).expect("read");
    assert_eq!(read.get("a"), Some(&true));
    assert_eq!(read.get("b"), Some(&false));
}

#[test]
fn merge_takes_the_target_verdict_when_the_oracle_passes() {
    let root = temp("merge");
    let oracle = root.join("runs/oracle/s.tsv");
    let target = root.join("runs/target/s.tsv");
    write_outcomes(&oracle, &[outcome("s", "p", "c", Verdict::Pass, "")]).expect("oracle");
    write_outcomes(
        &target,
        &[outcome("s", "p", "c", Verdict::Fail, "behaviour:bad")],
    )
    .expect("target");
    let table = build_from_runs(&root.join("runs")).expect("merge");
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0].outcome.verdict, Verdict::Fail);
    assert!(table.rows[0].oracle_pass);
}

#[test]
fn merge_marks_an_oracle_failure_and_has_no_ticket() {
    let root = temp("oracle-fail");
    let oracle = root.join("runs/oracle/s.tsv");
    let target = root.join("runs/target/s.tsv");
    let mut fail = outcome("s", "p", "c", Verdict::OracleFail, "oracle-behaviour:bad");
    fail.ticket = "-".to_string();
    write_outcomes(&oracle, &[fail]).expect("oracle");
    write_outcomes(&target, &[outcome("s", "p", "c", Verdict::Pass, "")]).expect("target");
    let table = build_from_runs(&root.join("runs")).expect("merge");
    assert_eq!(table.rows[0].outcome.verdict, Verdict::OracleFail);
    assert_eq!(table.rows[0].outcome.ticket, "-");
}

#[test]
fn a_missing_target_row_is_not_run_no_target_build() {
    let root = temp("no-target");
    let oracle = root.join("runs/oracle/s.tsv");
    write_outcomes(&oracle, &[outcome("s", "p", "c", Verdict::Pass, "")]).expect("oracle");
    let table = build_from_runs(&root.join("runs")).expect("merge");
    assert_eq!(table.rows[0].outcome.verdict, Verdict::NotRun);
    assert_eq!(table.rows[0].outcome.reason, "no-target-build");
}

#[test]
fn stats_count_every_verdict_and_the_rate() {
    let rows = vec![
        TableRow {
            outcome: outcome("s", "p", "a", Verdict::Pass, ""),
            oracle_pass: true,
        },
        TableRow {
            outcome: outcome("s", "p", "b", Verdict::Fail, "behaviour:bad"),
            oracle_pass: true,
        },
        TableRow {
            outcome: outcome("s", "p", "c", Verdict::Excluded, "excluded:why"),
            oracle_pass: false,
        },
        TableRow {
            outcome: outcome("s", "p", "d", Verdict::OracleFail, "oracle-build:x"),
            oracle_pass: false,
        },
        TableRow {
            outcome: outcome("s", "p", "e", Verdict::Skip, "needs-root"),
            oracle_pass: false,
        },
        TableRow {
            outcome: outcome("s", "p", "f", Verdict::NotRun, "no-run-backend"),
            oracle_pass: false,
        },
    ];
    let stats = SuiteStats::from_rows("s", &rows, &BTreeMap::new());
    assert_eq!(stats.bodies, 6);
    assert_eq!(stats.programs, 1);
    assert_eq!(stats.target_pass, 1);
    assert_eq!(stats.fail, 1);
    assert_eq!(stats.excluded, 1);
    assert_eq!(stats.oracle_fail, 1);
    assert_eq!(stats.skip, 1);
    assert_eq!(stats.not_run, 1);
    assert_eq!(stats.eligible, 3);
    assert!((stats.rate - 33.333_333).abs() < 0.001);
}

#[test]
fn a_skip_by_the_test_itself_stays_eligible() {
    let rows = vec![
        TableRow {
            outcome: outcome("s", "p", "a", Verdict::Pass, ""),
            oracle_pass: true,
        },
        TableRow {
            outcome: outcome("s", "p", "b", Verdict::Skip, "no network"),
            oracle_pass: true,
        },
    ];
    let stats = SuiteStats::from_rows("s", &rows, &BTreeMap::new());
    assert_eq!(stats.eligible, 2);
    assert!((stats.rate - 50.0).abs() < f64::EPSILON);
}

#[test]
fn markdown_has_the_fixed_header_and_sentence() {
    let table = ConformanceTable {
        rows: vec![TableRow {
            outcome: outcome("s", "p", "c", Verdict::Fail, "behaviour:bad"),
            oracle_pass: true,
        }],
        stats: vec![SuiteStats::from_rows(
            "s",
            &[TableRow {
                outcome: outcome("s", "p", "c", Verdict::Fail, "behaviour:bad"),
                oracle_pass: true,
            }],
            &BTreeMap::new(),
        )],
    };
    let text = render_markdown(&table);
    eprintln!("{text}");
    assert!(text.contains("Nothing is deleted from this table"));
    assert!(text.contains("| Suite | bodies | programs | target-builds | oracle-pass | target-pass | fail | skip | excluded | oracle-fail | not-run | rate |"));
    assert!(text.contains("| s | 1 | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0.0 % |"));
    assert!(text.contains("#### s: rows that are not `pass`"));
}

#[test]
fn rendering_the_same_table_twice_is_byte_identical() {
    let table = ConformanceTable {
        rows: vec![TableRow {
            outcome: outcome("s", "p", "c", Verdict::Pass, ""),
            oracle_pass: true,
        }],
        stats: Vec::new(),
    };
    assert_eq!(render_markdown(&table), render_markdown(&table));
}

#[test]
fn regressions_compare_baseline_passes() {
    let baseline = vec![RawRow {
        suite: "s".to_string(),
        program: "p".to_string(),
        case: "c".to_string(),
        verdict: "pass".to_string(),
        reason: String::new(),
        ticket: String::new(),
    }];
    let table = ConformanceTable {
        rows: vec![TableRow {
            outcome: outcome("s", "p", "c", Verdict::NotRun, "no-run-backend"),
            oracle_pass: true,
        }],
        stats: Vec::new(),
    };
    let regressed = regressions(&baseline, &table).expect("regressions");
    assert_eq!(regressed.len(), 1);
}

#[test]
fn write_baseline_uses_the_outcome_order() {
    let root = temp("baseline");
    let path = root.join("baseline.tsv");
    let table = ConformanceTable {
        rows: vec![TableRow {
            outcome: outcome("s", "p", "c", Verdict::Pass, ""),
            oracle_pass: true,
        }],
        stats: Vec::new(),
    };
    write_baseline(&path, &table).expect("write");
    assert!(
        fs::read_to_string(&path)
            .expect("read")
            .contains("s\tp\tc\tpass")
    );
}

#[test]
fn default_runs_is_the_build_directory() {
    assert_eq!(
        default_runs(Path::new("/repo")),
        PathBuf::from("/repo/build/libc-conformance")
    );
}

#[test]
fn an_xfail_pass_keeps_its_note() {
    let root = temp("xfail");
    let path = root.join("run.tsv");
    write_outcomes(
        &path,
        &[outcome("s", "p", "c", Verdict::Pass, "xfail:PR 4711")],
    )
    .expect("write");
    assert_eq!(read_raw(&path).expect("read")[0].reason, "xfail:PR 4711");
}
