// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the rule and invariant evaluation.

use super::*;
use crate::report::Row;

fn row(probe: &str, s: u64, bss: u64) -> Row {
    Row::measured(
        probe,
        Arch::X86_64,
        Linkage::Static,
        Set::Tiny,
        Lang::Rust,
        s,
        bss,
        1000,
        10,
        0,
        0,
        0,
        0,
    )
}

fn rule(probe: &str, invariant: &str) -> Rule {
    Rule {
        probe: probe.to_string(),
        invariant: invariant.to_string(),
        linkage: None,
        set: None,
        lang: None,
        forbid_members: Vec::new(),
        forbid_symbols: Vec::new(),
        require_members: Vec::new(),
        max_text_over_p0: None,
        max_init_array: None,
    }
}

#[test]
fn forbid_members_fails_on_a_match() {
    let mut rule = rule("P2", "R-13");
    rule.forbid_members = vec!["regex*".to_string()];
    let members = vec!["raven_libc_regex-1.cgu.2.rcgu.o".to_string()];
    let context = RuleContext {
        s: 100,
        bss: 0,
        init_array: 0,
        members: &members,
        symbols: &[],
        p0_s: None,
    };
    let result = evaluate(&rule, &row("P2", 100, 0), &context);
    assert_eq!(result.status, "fail");
    assert!(result.detail.contains("regex*"));
}

#[test]
fn forbid_members_passes_when_absent() {
    let mut rule = rule("P2", "R-13");
    rule.forbid_members = vec!["regex*".to_string()];
    let members = vec!["raven_libc_stdio-1.cgu.2.rcgu.o".to_string()];
    let context = RuleContext {
        s: 100,
        bss: 0,
        init_array: 0,
        members: &members,
        symbols: &[],
        p0_s: None,
    };
    assert_eq!(evaluate(&rule, &row("P2", 100, 0), &context).status, "pass");
}

#[test]
fn require_members_fails_when_absent() {
    let mut rule = rule("P3", "R-13");
    rule.require_members = vec!["fmt_float*".to_string()];
    let members = vec!["raven_libc_fmt-1.cgu.2.rcgu.o".to_string()];
    let context = RuleContext {
        s: 100,
        bss: 0,
        init_array: 0,
        members: &members,
        symbols: &[],
        p0_s: None,
    };
    assert_eq!(evaluate(&rule, &row("P3", 100, 0), &context).status, "fail");
}

#[test]
fn forbid_symbols_uses_the_complete_symbol_list() {
    let mut rule = rule("P0", "R-23");
    rule.forbid_symbols = vec!["__*df3".to_string()];
    let symbols = vec!["__adddf3".to_string(), "main".to_string()];
    let context = RuleContext {
        s: 100,
        bss: 0,
        init_array: 0,
        members: &[],
        symbols: &symbols,
        p0_s: None,
    };
    let result = evaluate(&rule, &row("P0", 100, 0), &context);
    assert_eq!(result.status, "fail");
    assert!(result.detail.contains("__adddf3"));
}

#[test]
fn init_array_rule_fails_on_three_bytes() {
    let mut rule = rule("P0", "R-9");
    rule.max_init_array = Some(0);
    let context = RuleContext {
        s: 100,
        bss: 0,
        init_array: 3,
        members: &[],
        symbols: &[],
        p0_s: None,
    };
    assert_eq!(evaluate(&rule, &row("P0", 100, 0), &context).status, "fail");
}

#[test]
fn max_text_over_p0_passes_and_fails() {
    let mut rule = rule("P6", "R-16");
    rule.max_text_over_p0 = Some(512);
    let pass_context = RuleContext {
        s: 1500,
        bss: 0,
        init_array: 0,
        members: &[],
        symbols: &[],
        p0_s: Some(1200),
    };
    assert_eq!(
        evaluate(&rule, &row("P6", 1500, 0), &pass_context).status,
        "pass"
    );
    let fail_context = RuleContext {
        s: 2000,
        bss: 0,
        init_array: 0,
        members: &[],
        symbols: &[],
        p0_s: Some(1200),
    };
    assert_eq!(
        evaluate(&rule, &row("P6", 2000, 0), &fail_context).status,
        "fail"
    );
}

#[test]
fn a_rule_for_an_unbuilt_probe_is_skipped() {
    let rule = rule("P4", "R-9");
    let skipped = Row::skipped(
        "P4",
        Arch::X86_64,
        Linkage::Static,
        Set::Tiny,
        Lang::Rust,
        "undefined symbol: pthread_create",
    );
    let context = RuleContext {
        s: 0,
        bss: 0,
        init_array: 0,
        members: &[],
        symbols: &[],
        p0_s: None,
    };
    let result = evaluate(&rule, &skipped, &context);
    assert_eq!(result.status, "skipped");
    assert_ne!(result.status, "pass");
}

#[test]
fn filters_restrict_a_rule() {
    let mut rule = rule("P0", "R-9");
    rule.linkage = Some("static".to_string());
    rule.set = Some("tiny".to_string());
    assert!(applies(&rule, Linkage::Static, Set::Tiny, Lang::Rust));
    assert!(!applies(&rule, Linkage::Dynamic, Set::Tiny, Lang::Rust));
    assert!(!applies(&rule, Linkage::Static, Set::Server, Lang::Rust));
}

#[test]
fn row_invariants_catch_the_two_bss_ceilings() {
    let rows = vec![row("P0", 2000, 6384), row("P2", 60000, 6936)];
    let invariants = evaluate_row_invariants(&rows);
    let b0 = invariants
        .iter()
        .find(|invariant| invariant.invariant == "INV-B-P0")
        .expect("B0");
    assert_eq!(b0.status, "fail");
    assert_eq!(b0.measured, 6384);
    let b2 = invariants
        .iter()
        .find(|invariant| invariant.invariant == "INV-B-P2")
        .expect("B2");
    assert_eq!(b2.status, "fail");
}

#[test]
fn difference_invariants_use_the_same_cell_only() {
    let mut p0 = row("P0", 2000, 0);
    p0.arch = Arch::X86_64;
    let mut p2 = row("P2", 8000, 0);
    p2.arch = Arch::Aarch64;
    let invariants = evaluate_row_invariants(&[p0, p2]);
    assert!(
        !invariants
            .iter()
            .any(|invariant| invariant.invariant == "INV-S-P2-P0")
    );

    let mut p0 = row("P0", 2000, 0);
    p0.arch = Arch::X86_64;
    let mut p2 = row("P2", 7000, 0);
    p2.arch = Arch::X86_64;
    let invariants = evaluate_row_invariants(&[p0, p2]);
    let diff = invariants
        .iter()
        .find(|invariant| invariant.invariant == "INV-S-P2-P0")
        .expect("diff");
    assert_eq!(diff.measured, 5000);
    assert_eq!(diff.status, "pass");
}

#[test]
fn difference_invariant_fails_above_5120() {
    let rows = vec![row("P0", 2000, 0), row("P2", 8000, 0)];
    let invariants = evaluate_row_invariants(&rows);
    let diff = invariants
        .iter()
        .find(|invariant| invariant.invariant == "INV-S-P2-P0")
        .expect("diff");
    assert_eq!(diff.status, "fail");
}

#[test]
fn p3_p2_invariant_uses_10240() {
    let rows = vec![row("P2", 2000, 0), row("P3", 13000, 0)];
    let invariants = evaluate_row_invariants(&rows);
    let diff = invariants
        .iter()
        .find(|invariant| invariant.invariant == "INV-S-P3-P2")
        .expect("diff");
    assert_eq!(diff.measured, 11000);
    assert_eq!(diff.status, "fail");
}

#[test]
fn dynamic_needed_must_be_one() {
    let mut row = row("P1", 2000, 0);
    row.linkage = Linkage::Dynamic;
    row.needed = 0;
    let invariants = evaluate_row_invariants(&[row]);
    let needed = invariants
        .iter()
        .find(|invariant| invariant.invariant == "INV-NEEDED")
        .expect("needed");
    assert_eq!(needed.status, "fail");
}

#[test]
fn p2_member_invariant_counts_every_plugin_member() {
    let rows = vec![row("P2", 100, 0)];
    let mut members = BTreeMap::new();
    members.insert(
        cell_key(Arch::X86_64, Linkage::Static, Set::Tiny, Lang::Rust),
        vec![
            "raven_libc_regex-1.cgu.o".to_string(),
            "raven_libc_stdio-1.cgu.o".to_string(),
        ],
    );
    let invariants = evaluate_member_invariants(&rows, &members);
    let pull = invariants
        .iter()
        .find(|invariant| invariant.invariant == "R-13")
        .expect("R-13");
    assert_eq!(pull.measured, 1);
    assert_eq!(pull.status, "fail");
}

#[test]
fn a_missing_member_list_is_skipped_not_passed() {
    let rows = vec![row("P2", 100, 0)];
    let invariants = evaluate_member_invariants(&rows, &BTreeMap::new());
    let pull = invariants
        .iter()
        .find(|invariant| invariant.invariant == "R-13")
        .expect("R-13");
    assert_eq!(pull.status, "skipped");
}

#[test]
fn libc_so_invariants_skip_when_the_row_is_skipped() {
    let rows = vec![LibcSoRow {
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
    }];
    assert!(evaluate_libc_so_invariants(&rows).is_empty());
}

#[test]
fn libc_so_invariants_fail_on_a_big_so() {
    let rows = vec![LibcSoRow {
        arch: Arch::X86_64,
        path: "build/x86_64/libc.so.1".to_string(),
        s: 1_400_000,
        bss: 0,
        file: 1,
        needed: 1,
        jump_slot: 41,
        glob_dat: 0,
        relative: 0,
        data_rel_ro: 0,
        status: "ok".to_string(),
    }];
    let invariants = evaluate_libc_so_invariants(&rows);
    assert_eq!(
        invariants
            .iter()
            .find(|invariant| invariant.invariant == "INV-LIBC-SO-S")
            .expect("S")
            .status,
        "fail"
    );
    assert_eq!(
        invariants
            .iter()
            .find(|invariant| invariant.invariant == "R-20-JUMP_SLOT")
            .expect("jump")
            .status,
        "fail"
    );
}

#[test]
fn rules_load_from_toml() {
    let dir = crate::testutil::TempDir::new("rules-load").expect("temp");
    let path = dir
        .write(
            "rules.toml",
            b"schema = 1\n[[rule]]\nprobe = \"P0\"\ninvariant = \"R-9\"\nforbid_members = [\"malloc*\"]\nmax_init_array = 0\n",
        )
        .expect("write");
    let file = load(&path).expect("load");
    assert_eq!(file.rules.len(), 1);
    assert_eq!(file.rules.first().expect("rule").probe, "P0");
}
