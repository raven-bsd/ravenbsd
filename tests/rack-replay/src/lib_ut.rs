// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit and negative tests for the replay/union tooling.

use rack_coverage::Function;

use crate::replay;
use crate::trace;
use crate::union;

#[test]
fn golden_parser_reads_both_committed_formats() {
    let text = "# seed\tevents\tlost\tpipe\tretransmits\trecovery_entries/exits\tevent_digest\n\
                7\t9\t1000-2000,3000-4000\t500\t1000-2000\t2/1\t00000000000000ab\n";
    let rows = trace::parse_golden(text).expect("parse");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].seed, 7);
    assert_eq!(rows[0].events, 9);
    assert_eq!(rows[0].lost, vec![(1000, 2000), (3000, 4000)]);
    assert_eq!(rows[0].pipe, 500);
    assert_eq!(rows[0].retransmits, vec![(1000, 2000)]);
    assert_eq!(rows[0].recovery_entries, 2);
    assert_eq!(rows[0].recovery_exits, 1);
    assert_eq!(rows[0].digest, 0xab);
}

#[test]
fn golden_parser_rejects_an_inverted_range() {
    let text = "7\t9\t2000-1000\t0\t-\t0/0\t0\n";
    assert!(trace::parse_golden(text).is_err());
}

#[test]
fn the_k319_and_k324_traces_have_ten_thousand_rows() {
    let loss = trace::load_golden(&trace::loss_trace_path()).expect("loss trace");
    assert_eq!(loss.len(), 10_000);
    assert_eq!(loss[0].seed, 1);
    assert_eq!(loss[0].lost, vec![(4000, 5000)]);
    assert_eq!(loss[0].pipe, 2000);
    assert_eq!(loss[0].digest, 0xcc66_2b1d_da6b_1644);

    let recovery = trace::load_golden(&trace::recovery_trace_path()).expect("recovery trace");
    assert_eq!(recovery.len(), 10_000);
    assert_eq!(recovery[0].seed, 1);
    assert_eq!(recovery[0].lost, Vec::new());
    assert_eq!(recovery[0].pipe, 0);
    assert_eq!(recovery[0].digest, 0x7d35_84aa_f96a_9212);
}

#[test]
fn loss_replay_covers_every_golden_row() {
    let rows = trace::load_golden(&trace::loss_trace_path()).expect("loss trace");
    let report = replay::replay_loss_rows(&rows);
    assert_eq!(report.traces, 10_000);
    assert!(report.events > 0);
    assert!(report.mismatches <= report.traces);
    if report.mismatches > 0 {
        assert!(report.first_mismatch.is_some());
    }
    assert_eq!(report, replay::replay_loss_rows(&rows), "deterministic");
}

#[test]
fn golden_field_comparison_rejects_a_changed_pipe() {
    let row = trace::load_golden(&trace::loss_trace_path())
        .expect("loss trace")
        .into_iter()
        .next()
        .expect("one row");
    assert!(replay::golden_matches(
        &row.lost,
        row.pipe,
        &row.retransmits,
        row.recovery_entries,
        row.recovery_exits,
        &row,
    ));
    assert!(!replay::golden_matches(
        &row.lost,
        row.pipe.saturating_add(1),
        &row.retransmits,
        row.recovery_entries,
        row.recovery_exits,
        &row,
    ));
}

#[test]
fn a_corrupted_pipe_is_detected_as_a_golden_mismatch() {
    let mut rows = trace::load_golden(&trace::loss_trace_path()).expect("loss trace");
    rows[0].pipe = rows[0].pipe.saturating_add(1);
    let report = replay::replay_loss_rows(&rows);
    let first = report.first_mismatch.expect("corruption must be detected");
    assert_eq!(first.trace, "loss seed 1");
    assert!(first.expected.contains("pipe"));
    assert!(first.actual.contains("pipe"));
}

#[test]
fn recovery_replay_covers_every_golden_row() {
    let report = replay::replay_recovery().expect("recovery replay");
    assert_eq!(report.traces, 10_000);
    assert!(report.events > 0);
    assert!(report.mismatches <= report.traces);
    assert!(report.first_mismatch.is_some() || report.mismatches == 0);
}

#[test]
fn tlp_replay_covers_five_two_thousand_event_traces() {
    let report = replay::replay_tlp();
    assert_eq!(report.traces, 5);
    assert_eq!(report.events, 10_000);
    assert_eq!(report, replay::replay_tlp(), "deterministic");
}

#[test]
fn bbr_replay_counts_the_missing_product_module() {
    let report = replay::replay_bbr();
    assert_eq!(report.traces, 10);
    assert_eq!(report.events, 10_000);
    assert_eq!(report.mismatches, report.events);
    let first = report.first_mismatch.expect("missing module");
    assert!(first.actual.contains("absent"));
}

#[test]
fn vtable_replay_covers_five_two_thousand_event_traces() {
    let report = replay::replay_vtable();
    assert_eq!(report.traces, 5);
    assert_eq!(report.events, 10_000);
    assert_eq!(report, replay::replay_vtable(), "deterministic");
    for mismatch in report.first_mismatch.iter() {
        assert!(!mismatch.expected.is_empty());
        assert!(!mismatch.actual.is_empty());
    }
}

fn row(function: &str, status: &str) -> String {
    format!("| `{function}` | `crate::{function}` | `{function}_test` | `mutation` | {status} |")
}

fn table(rows: &[String]) -> String {
    let mut text = String::from("| Function | Rust | Test | Mutation | Status |\n");
    text.push_str("|---|---|---|---|---|\n");
    for row in rows {
        text.push_str(row);
        text.push('\n');
    }
    text
}

fn donor(names: &[&str]) -> Vec<Function> {
    names
        .iter()
        .enumerate()
        .map(|(index, name)| Function {
            line: index + 1,
            name: (*name).to_owned(),
        })
        .collect()
}

#[test]
fn union_flags_a_function_claimed_by_two_groups() {
    let donor = donor(&["alpha", "beta"]);
    let reports = vec![
        ("K269".to_owned(), table(&[row("alpha", "PASS")])),
        ("K328".to_owned(), table(&[row("alpha", "PASS")])),
    ];
    let report = union::audit(&reports, &donor);
    assert_eq!(report.missing, vec!["beta".to_owned()]);
    assert_eq!(report.cross_report_duplicates, vec!["alpha".to_owned()]);
    assert_eq!(report.row_count, 1);
    assert!(!report.is_ok());
}

#[test]
fn union_accepts_a_complete_single_owner_matrix() {
    let donor = donor(&["alpha"]);
    let reports = vec![("K269".to_owned(), table(&[row("alpha", "PASS")]))];
    let report = union::audit(&reports, &donor);
    assert!(report.is_ok(), "{}", report.render());
    assert_eq!(report.row_count, 1);
}
