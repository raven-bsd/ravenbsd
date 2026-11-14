// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::table::TableRow;
use crate::{Case, Outcome, Verdict};

fn row(suite: &str, program: &str, case: &str, verdict: Verdict) -> TableRow {
    TableRow {
        outcome: Outcome::new(Case::new(suite, program, case), verdict, "", "C40"),
        oracle_pass: verdict == Verdict::Pass,
    }
}

fn config(id: &str, threshold: f64, cap: Option<usize>) -> GateConfig {
    GateConfig {
        schema: 1,
        criteria: vec![Criterion {
            id: id.to_string(),
            suites: vec!["netbsd-libc".to_string()],
            unit: "bodies".to_string(),
            threshold,
            max_exclusions: cap,
            external: false,
        }],
    }
}

fn table_with(pass: usize, fail: usize) -> ConformanceTable {
    let mut rows = Vec::new();
    for index in 0..pass {
        rows.push(row("netbsd-libc", "p", &format!("a{index}"), Verdict::Pass));
    }
    for index in 0..fail {
        rows.push(row("netbsd-libc", "p", &format!("b{index}"), Verdict::Fail));
    }
    ConformanceTable {
        rows,
        stats: Vec::new(),
    }
}

#[test]
fn ninety_percent_meets_a_ninety_percent_threshold() {
    let rows = evaluate(&config("C2", 90.0, None), &table_with(90, 10));
    assert_eq!(rows[0].measured, Some(90.0));
    assert!(rows[0].met);
}

#[test]
fn eighty_nine_point_nine_nine_percent_does_not_meet_it() {
    let rows = evaluate(&config("C2", 90.0, None), &table_with(8999, 1001));
    assert!(rows[0].measured.unwrap() < 90.0);
    assert!(!rows[0].met);
}

#[test]
fn ninety_point_zero_one_percent_meets_it() {
    let rows = evaluate(&config("C2", 90.0, None), &table_with(9001, 999));
    assert!(rows[0].measured.unwrap() > 90.0);
    assert!(rows[0].met);
}

#[test]
fn the_exclusion_cap_is_inclusive() {
    let mut table = table_with(10, 0);
    for index in 0..250 {
        table.rows.push(row(
            "netbsd-libc",
            "p",
            &format!("x{index}"),
            Verdict::Excluded,
        ));
    }
    let rows = evaluate(&config("C2", 90.0, Some(250)), &table);
    assert_eq!(rows[0].measured, Some(100.0));
    assert!(rows[0].met);
}

#[test]
fn one_exclusion_over_the_cap_is_not_met() {
    let mut table = table_with(10, 0);
    for index in 0..251 {
        table.rows.push(row(
            "netbsd-libc",
            "p",
            &format!("x{index}"),
            Verdict::Excluded,
        ));
    }
    let rows = evaluate(&config("C2", 90.0, Some(250)), &table);
    assert!(!rows[0].met);
    assert!(rows[0].note.contains("exclusion cap exceeded"));
}

#[test]
fn a_suite_with_every_row_not_run_is_not_met_with_a_note() {
    let mut table = table_with(0, 0);
    table
        .rows
        .push(row("netbsd-libc", "p", "a", Verdict::NotRun));
    let rows = evaluate(&config("C2", 90.0, None), &table);
    assert_eq!(rows[0].measured, Some(0.0));
    assert!(!rows[0].met);
    assert!(rows[0].note.contains("not run"));
}

#[test]
fn a_suite_with_no_rows_is_not_vendored() {
    let rows = evaluate(
        &config("C2", 90.0, None),
        &ConformanceTable {
            rows: Vec::new(),
            stats: Vec::new(),
        },
    );
    assert_eq!(rows[0].measured, None);
    assert!(rows[0].note.contains("not vendored"));
}

#[test]
fn an_external_criterion_is_not_evaluated() {
    let mut criterion = config("C7", 80.0, None);
    criterion.criteria[0].external = true;
    let rows = evaluate(&criterion, &table_with(1, 0));
    assert_eq!(rows[0].measured, None);
    assert!(rows[0].note.contains("external"));
    assert!(!rows[0].met);
}

#[test]
fn a_programs_criterion_counts_programs() {
    let mut criterion = config("C4", 90.0, None);
    criterion.criteria[0].unit = "programs".to_string();
    let mut table = ConformanceTable {
        rows: Vec::new(),
        stats: Vec::new(),
    };
    for index in 0..9 {
        table
            .rows
            .push(row("netbsd-libc", &format!("p{index}"), "a", Verdict::Pass));
    }
    table
        .rows
        .push(row("netbsd-libc", "p9", "a", Verdict::Fail));
    let rows = evaluate(&criterion, &table);
    assert_eq!(rows[0].measured, Some(90.0));
    assert!(rows[0].met);
}

#[test]
fn requirement_skips_are_outside_the_denominator() {
    let mut table = table_with(9, 0);
    table
        .rows
        .push(row("netbsd-libc", "p", "skip", Verdict::Skip));
    let rows = evaluate(&config("C2", 90.0, None), &table);
    assert_eq!(rows[0].measured, Some(90.0));
}

#[test]
fn render_prints_the_threshold_and_denominator() {
    let rows = evaluate(&config("C2", 90.0, Some(250)), &table_with(90, 10));
    let text = render(&rows);
    assert!(text.contains("C2 NetBSD ATF libc+libm+libpthread: 90.0 % of 100 eligible"));
    assert!(text.contains("(threshold 90.0 %)"));
    assert!(text.contains("exclusions 0/250"));
    assert!(text.contains("G1: MET (1 of 1 measured rows)"));
}

#[test]
fn render_says_not_met_and_the_exit_is_three_by_contract() {
    let rows = evaluate(&config("C2", 90.0, None), &table_with(0, 10));
    let text = render(&rows);
    assert!(text.contains("NOT MET"));
    assert!(text.contains("G1: NOT MET"));
    assert!(!met(&rows));
}

#[test]
fn all_met_rows_mean_the_gate_is_met() {
    let rows = evaluate(&config("C2", 90.0, None), &table_with(91, 9));
    assert!(met(&rows));
}

#[test]
fn an_external_row_alone_does_not_meet_the_gate() {
    let mut criterion = config("C7", 80.0, None);
    criterion.criteria[0].external = true;
    assert!(!met(&evaluate(&criterion, &table_with(1, 0))));
}
