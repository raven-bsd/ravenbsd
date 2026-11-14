// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for freeze, check and the known-violation mechanism.

use super::*;
use crate::report::{InvariantResult, Toolchain};

fn measured(probe: &str, s: u64, bss: u64, file: u64) -> Row {
    Row::measured(
        probe,
        Arch::X86_64,
        Linkage::Static,
        Set::Tiny,
        Lang::Rust,
        s,
        bss,
        file,
        10,
        0,
        0,
        0,
        0,
    )
}

fn results_with(rows: Vec<Row>) -> Results {
    let mut results = Results::new(Toolchain {
        rustc: "rustc 1.97.1".to_string(),
        lld: "LLD 22.1.6".to_string(),
    });
    results.rows = rows;
    results
}

fn invariant_status(results: &Results, name: &str) -> String {
    results
        .invariants
        .iter()
        .find(|invariant| invariant.invariant == name)
        .map(|invariant| invariant.status.clone())
        .unwrap_or_default()
}

#[test]
fn freeze_arithmetic_matches_the_ticket_table() {
    for (measured, expected) in [
        (0, 0),
        (1, 2),
        (33, 34),
        (100, 103),
        (1787, 1841),
        (18169, 18715),
    ] {
        let row = BudgetRow::from_row(&measured_row_with_s(measured), 3);
        assert_eq!(row.budget_s, expected, "measured {measured}");
    }
}

fn measured_row_with_s(s: u64) -> Row {
    measured("P2", s, 0, 0)
}

#[test]
fn freeze_copies_measured_values_and_adds_headroom() {
    let results = results_with(vec![measured("P2", 1000, 20, 5000)]);
    let budgets = Budgets::freeze(&results);
    let row = budgets.budget.first().expect("budget");
    assert_eq!(row.measured_s, 1000);
    assert_eq!(row.budget_s, 1030);
    assert_eq!(row.measured_b, 20);
    assert_eq!(row.budget_b, 21);
    assert_eq!(row.budget_f, 5150);
    assert_eq!(budgets.status, "enforced");
    assert_eq!(budgets.headroom_percent, 3);
}

#[test]
fn freeze_skips_skipped_rows() {
    let mut results = results_with(vec![measured("P0", 10, 0, 10)]);
    results.rows.push(Row::skipped(
        "P4",
        Arch::X86_64,
        Linkage::Static,
        Set::Tiny,
        Lang::Rust,
        "undefined symbol: pthread_create",
    ));
    let budgets = Budgets::freeze(&results);
    assert_eq!(budgets.budget.len(), 1);
    assert_eq!(budgets.budget.first().expect("row").probe, "P0");
}

#[test]
fn freeze_then_check_passes() {
    let results = results_with(vec![measured("P2", 1000, 20, 5000)]);
    let budgets = Budgets::freeze(&results);
    let report = check(&results, &budgets, false);
    assert!(report.passed());
    assert_eq!(report.count(CheckStatus::Pass), 1);
}

#[test]
fn one_byte_over_fails_with_the_probe_name() {
    let results = results_with(vec![measured("P2", 1000, 0, 0)]);
    let budgets = Budgets::freeze(&results);
    let mut over = results.clone();
    over.rows[0].s = 1031;
    let report = check(&over, &budgets, false);
    assert!(!report.passed());
    let failure = report
        .rows
        .iter()
        .find(|row| row.status == CheckStatus::Fail)
        .expect("failure");
    assert!(failure.key.contains("P2"));
    assert!(failure.message.contains("S 1031 > 1030"));
}

#[test]
fn a_missing_budget_fails_without_allow_missing() {
    let results = results_with(vec![measured("P2", 1000, 0, 0)]);
    let empty = Budgets::freeze(&results_with(Vec::new()));
    let report = check(&results, &empty, false);
    assert!(!report.passed());
    assert!(report.rows.iter().any(|row| row.message == "no budget"));
    let allowed = check(&results, &empty, true);
    assert!(allowed.passed());
    assert_eq!(allowed.count(CheckStatus::Skipped), 1);
}

#[test]
fn a_budget_row_without_a_result_is_reported() {
    let results = results_with(vec![measured("P2", 1000, 0, 0)]);
    let budgets = Budgets::freeze(&results);
    let empty = results_with(Vec::new());
    let report = check(&empty, &budgets, false);
    assert!(report.passed());
    assert_eq!(report.count(CheckStatus::BudgetOnly), 1);
}

#[test]
fn a_failed_invariant_with_a_known_row_is_known_and_passes() {
    let mut results = results_with(vec![measured("P0", 2000, 6384, 100)]);
    results.invariants.push(InvariantResult {
        probe: "P0".to_string(),
        arch: Arch::X86_64,
        linkage: Linkage::Static,
        set: Set::Tiny,
        lang: Lang::Rust,
        invariant: "INV-B-P0".to_string(),
        measured: 6384,
        limit: 512,
        status: "fail".to_string(),
        detail: "B(P0)".to_string(),
    });
    let budgets = Budgets::freeze(&results);
    assert_eq!(budgets.known_violation.len(), 1);
    let report = check(&results, &budgets, false);
    assert!(report.passed());
    assert_eq!(report.count(CheckStatus::Known), 1);
}

#[test]
fn a_failed_invariant_without_a_known_row_fails() {
    let mut results = results_with(vec![measured("P0", 2000, 6384, 100)]);
    results.invariants.push(InvariantResult {
        probe: "P0".to_string(),
        arch: Arch::X86_64,
        linkage: Linkage::Static,
        set: Set::Tiny,
        lang: Lang::Rust,
        invariant: "INV-B-P0".to_string(),
        measured: 6384,
        limit: 512,
        status: "fail".to_string(),
        detail: "B(P0)".to_string(),
    });
    let budgets = Budgets::freeze(&results_with(Vec::new()));
    let report = check(&results, &budgets, false);
    assert!(!report.passed());
}

#[test]
fn a_failed_rule_is_known_when_its_invariant_is_frozen() {
    let mut results = results_with(vec![measured("P2", 1000, 0, 0)]);
    results.rules.push(crate::report::RuleResult {
        probe: "P2".to_string(),
        arch: Arch::X86_64,
        linkage: Linkage::Static,
        set: Set::Tiny,
        lang: Lang::Rust,
        rule: "P2-R-13".to_string(),
        invariant: "R-13".to_string(),
        status: "fail".to_string(),
        detail: "regex member pulled".to_string(),
    });
    let budgets = Budgets::freeze(&results);
    assert!(
        budgets
            .known_violation
            .iter()
            .any(|known| known.invariant == "R-13")
    );
    let report = check(&results, &budgets, false);
    assert!(report.passed());
    assert!(
        report
            .rows
            .iter()
            .any(|row| row.status == CheckStatus::Known)
    );
}

#[test]
fn a_failed_rule_without_a_known_row_fails() {
    let mut results = results_with(vec![measured("P2", 1000, 0, 0)]);
    results.rules.push(crate::report::RuleResult {
        probe: "P2".to_string(),
        arch: Arch::X86_64,
        linkage: Linkage::Static,
        set: Set::Tiny,
        lang: Lang::Rust,
        rule: "P2-R-13".to_string(),
        invariant: "R-13".to_string(),
        status: "fail".to_string(),
        detail: "regex member pulled".to_string(),
    });
    let budgets = Budgets::freeze(&results_with(Vec::new()));
    let report = check(&results, &budgets, false);
    assert!(!report.passed());
}

#[test]
fn boot_growth_fails_the_gate() {
    let mut results = results_with(vec![measured("P0", 10, 0, 10)]);
    results.boot.push(BootRow {
        arch: Arch::X86_64,
        syscalls_before_main: 1,
        start_instructions: 190,
        init_array: 0,
        preinit_array: 0,
        ctors: 0,
        early_malloc: Vec::new(),
        forbidden_refs: Vec::new(),
        rodata: Vec::new(),
        notes: Vec::new(),
        status: "ok".to_string(),
    });
    let budgets = Budgets::freeze(&results);
    let budget = budgets.boot.first().expect("boot");
    assert_eq!(budget.budget_syscalls, 1);
    assert_eq!(budget.budget_instructions, 196);
    let mut grown = results.clone();
    grown.boot[0].syscalls_before_main = 2;
    let report = check(&grown, &budgets, false);
    assert!(!report.passed());
    let mut grown = results.clone();
    grown.boot[0].start_instructions = 197;
    assert!(!check(&grown, &budgets, false).passed());
}

#[test]
fn libc_so_absent_rows_are_skipped() {
    let mut results = results_with(vec![measured("P0", 10, 0, 10)]);
    results.libc_so.push(LibcSoRow {
        arch: Arch::X86_64,
        path: "build/x86_64/libc.so.1".to_string(),
        s: 0,
        bss: 0,
        file: 0,
        needed: 0,
        jump_slot: 0,
        glob_dat: 0,
        relative: 0,
        data_rel_ro: 0,
        status: "skipped: libc.so.1 absent (C62 not landed)".to_string(),
    });
    let budgets = Budgets::freeze(&results);
    assert_eq!(
        budgets.libc_so.first().expect("so").status,
        "skipped: libc.so.1 absent (C62 not landed)"
    );
    let report = check(&results, &budgets, false);
    assert!(report.passed());
    assert!(
        report
            .rows
            .iter()
            .any(|row| row.key.starts_with("libc.so.1") && row.status == CheckStatus::Skipped)
    );
}

#[test]
fn toml_has_the_generated_header_and_round_trips() {
    let results = results_with(vec![measured("P2", 100, 0, 500)]);
    let budgets = Budgets::freeze(&results);
    let text = budgets.to_toml().expect("toml");
    assert!(text.starts_with("# SPDX-License-Identifier: BSD-3-Clause\n"));
    assert!(
        text.contains("# GENERATED by libc-size freeze from a measurement; reviewer re-freezes")
    );
    assert!(text.contains("headroom_percent = 3"));
    assert!(text.contains("metric = \"S = Berkeley text+data"));
    let parsed: Budgets = toml::from_str(&text).expect("parse");
    assert_eq!(parsed, budgets);
}

#[test]
fn freeze_is_deterministic() {
    let mut results = results_with(vec![measured("P2", 100, 0, 500), measured("P0", 10, 0, 50)]);
    results.finalize();
    let a = Budgets::freeze(&results);
    let b = Budgets::freeze(&results);
    assert_eq!(a.to_toml().expect("a"), b.to_toml().expect("b"));
}

#[test]
fn budget_rows_are_sorted() {
    let mut results = results_with(vec![measured("P2", 100, 0, 500), measured("P0", 10, 0, 50)]);
    results.finalize();
    let budgets = Budgets::freeze(&results);
    let probes: Vec<&str> = budgets
        .budget
        .iter()
        .map(|row| row.probe.as_str())
        .collect();
    assert_eq!(probes, vec!["P0", "P2"]);
}

#[test]
fn known_violation_reports_measured_and_limit() {
    let mut results = results_with(vec![measured("P0", 2000, 6384, 100)]);
    results.invariants.push(InvariantResult {
        probe: "P0".to_string(),
        arch: Arch::X86_64,
        linkage: Linkage::Static,
        set: Set::Tiny,
        lang: Lang::Rust,
        invariant: "INV-B-P0".to_string(),
        measured: 6384,
        limit: 512,
        status: "fail".to_string(),
        detail: "B(P0) = 6384; PROPOSED 512".to_string(),
    });
    let budgets = Budgets::freeze(&results);
    let known = budgets.known_violation.first().expect("known");
    assert_eq!(known.measured, 6384);
    assert_eq!(known.limit, 512);
    assert_eq!(known.reason, "first measurement");
    assert_eq!(invariant_status(&results, "INV-B-P0"), "fail");
}
