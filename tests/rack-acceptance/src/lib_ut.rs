// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit, negative and mutation-sensitive tests for the acceptance audit.

use rack_coverage::Function;

use crate::audit;
use crate::register::{self, RegisterRow};
use crate::reports::GroupRow;
use crate::source::SourceIndex;

#[test]
fn pinned_donor_and_register_have_the_exact_inventory() {
    let donor = register::load_donor().expect("donor manifest");
    let register = register::load_register().expect("register");
    assert_eq!(donor.len(), 219);
    assert_eq!(register.len(), 219);
    assert_eq!(register[0].function, "rack_get_lt_bw");
    assert_eq!(register[218].function, "tcp_addrack");
    assert_eq!(register[218].owner, "K335");
}

#[test]
fn source_index_finds_a_known_mapping_and_test() {
    let source = SourceIndex::load().expect("source");
    assert!(source.has_function("rack_log_gpset"));
    assert!(source.has_function("rack_ack_received"));
    assert!(source.has_function("rack_proc_sack_blk"));
    assert!(source.has_function("tcp_rack_output"));
    assert!(source.has_test_named("donor_rack_log_gpset"));
    assert!(source.symbols_exist("RackRetransmit::rack_ack_received"));
    assert!(source.symbols_exist("RackSack::rack_proc_sack_blk"));
    assert!(source.symbols_exist("RackOutput::tcp_rack_output"));
    assert!(source.has_mapping_for("rack_get_lt_bw"));
    assert!(source.has_test_for("rack_get_lt_bw"));
}

#[test]
fn audit_produces_the_exact_219_row_status_and_218_target() {
    let donor = register::load_donor().expect("donor");
    let register_rows = register::load_register().expect("register");
    let source = SourceIndex::load().expect("source");
    let report = audit::run(&donor, &register_rows, &source, &[]);
    assert_eq!(report.donor_total, 219);
    assert_eq!(report.register_total, 219);
    assert_eq!(report.rows.len(), 219);
    assert_eq!(report.ticket_target, 218);
    assert_eq!(report.gaps.len(), 219 - report.accepted);
    assert_eq!(report.replay.classes.len(), 5);
    assert_eq!(
        report.rows[0].function, "rack_get_lt_bw",
        "donor order preserved"
    );
    assert_eq!(report.rows[218].function, "tcp_addrack");
}

fn fixture_row(test: &str, rust: &str, mutation: &str) -> RegisterRow {
    RegisterRow {
        number: 1,
        function: "rack_log_gpset".to_owned(),
        owner: "K329".to_owned(),
        rust: rust.to_owned(),
        test: test.to_owned(),
        mutation: mutation.to_owned(),
        status: "PASS".to_owned(),
    }
}

#[test]
fn a_blank_test_cell_makes_the_audit_not_ok() {
    let donor = vec![Function {
        line: 1,
        name: "rack_log_gpset".to_owned(),
    }];
    let register = vec![fixture_row(
        "-",
        "crate::rack_log::rack_log_gpset",
        "event suppression",
    )];
    let source = SourceIndex::load().expect("source");
    let report = audit::run(&donor, &register, &source, &[]);
    assert_eq!(report.k315.empty_test, vec!["rack_log_gpset".to_owned()]);
    assert!(!report.is_ok(), "a blank test cell must fail the gate");
}

#[test]
fn a_blank_rust_cell_makes_the_audit_not_ok() {
    let donor = vec![Function {
        line: 1,
        name: "rack_log_gpset".to_owned(),
    }];
    let register = vec![fixture_row(
        "donor_rack_log_gpset",
        "-",
        "event suppression",
    )];
    let source = SourceIndex::load().expect("source");
    let report = audit::run(&donor, &register, &source, &[]);
    assert_eq!(report.k315.empty_rust, vec!["rack_log_gpset".to_owned()]);
    assert!(!report.is_ok(), "a blank Rust cell must fail the gate");
}

#[test]
fn a_blank_mutation_cell_makes_the_audit_not_ok() {
    let donor = vec![Function {
        line: 1,
        name: "rack_log_gpset".to_owned(),
    }];
    let register = vec![fixture_row(
        "donor_rack_log_gpset",
        "crate::rack_log::rack_log_gpset",
        "-",
    )];
    let source = SourceIndex::load().expect("source");
    let report = audit::run(&donor, &register, &source, &[]);
    assert_eq!(
        report.k315.empty_mutation,
        vec!["rack_log_gpset".to_owned()]
    );
    assert!(!report.is_ok(), "a blank mutation cell must fail the gate");
}

#[test]
fn the_k269_rows_are_accepted() {
    let donor = register::load_donor().expect("donor");
    let register_rows = register::load_register().expect("register");
    let source = SourceIndex::load().expect("source");
    let report = audit::run(&donor, &register_rows, &source, &[]);
    assert_eq!(report.accepted, 219);
    assert_eq!(report.duplicates, Vec::<String>::new());
    assert!(report.gaps.is_empty(), "K269 rows are no longer gaps");
}

#[test]
fn a_missing_mapping_is_an_owned_gap() {
    let donor = vec![Function {
        line: 1,
        name: "fake_fn".to_owned(),
    }];
    let register = vec![RegisterRow {
        number: 1,
        function: "fake_fn".to_owned(),
        owner: "K999".to_owned(),
        rust: "crate::fake::fake_fn".to_owned(),
        test: "fake_fn_test".to_owned(),
        mutation: "mutation".to_owned(),
        status: "PASS".to_owned(),
    }];
    let source = SourceIndex::load().expect("source");
    let report = audit::run(&donor, &register, &source, &[]);
    assert_eq!(report.accepted, 0);
    assert_eq!(report.gaps.len(), 1);
    assert_eq!(report.gaps[0].owner, "K999");
    assert!(
        report.gaps[0]
            .reasons
            .contains(&"no Rust mapping".to_owned())
    );
}

#[test]
fn register_rows_win_over_historical_group_claims() {
    let donor = vec![Function {
        line: 1,
        name: "rack_log_gpset".to_owned(),
    }];
    let register = vec![RegisterRow {
        number: 1,
        function: "rack_log_gpset".to_owned(),
        owner: "K329".to_owned(),
        rust: "crate::rack_log::rack_log_gpset".to_owned(),
        test: "donor_rack_log_gpset".to_owned(),
        mutation: "event suppression".to_owned(),
        status: "PASS".to_owned(),
    }];
    let source = SourceIndex::load().expect("source");
    let groups = vec![
        GroupRow {
            group: "K328".to_owned(),
            function: "rack_log_gpset".to_owned(),
            rust: "crate::rack_log::rack_log_gpset".to_owned(),
            test: "donor_rack_log_gpset".to_owned(),
            mutation: "mutation".to_owned(),
        },
        GroupRow {
            group: "K329".to_owned(),
            function: "rack_log_gpset".to_owned(),
            rust: "crate::rack_log::rack_log_gpset".to_owned(),
            test: "donor_rack_log_gpset".to_owned(),
            mutation: "mutation".to_owned(),
        },
    ];
    let report = audit::run(&donor, &register, &source, &groups);
    assert!(report.duplicates.is_empty(), "register owner wins");
    assert_eq!(report.accepted, 1);
}

#[test]
fn duplicate_register_rows_are_flagged() {
    let donor = vec![Function {
        line: 1,
        name: "rack_log_gpset".to_owned(),
    }];
    let row = RegisterRow {
        number: 1,
        function: "rack_log_gpset".to_owned(),
        owner: "K329".to_owned(),
        rust: "crate::rack_log::rack_log_gpset".to_owned(),
        test: "donor_rack_log_gpset".to_owned(),
        mutation: "event suppression".to_owned(),
        status: "PASS".to_owned(),
    };
    let register = vec![row.clone(), row];
    let source = SourceIndex::load().expect("source");
    let report = audit::run(&donor, &register, &source, &[]);
    assert_eq!(report.duplicates, vec!["rack_log_gpset".to_owned()]);
}

#[test]
fn a_register_row_without_a_durable_mutation_is_a_gap() {
    let donor = vec![Function {
        line: 1,
        name: "rack_log_gpset".to_owned(),
    }];
    let register = vec![RegisterRow {
        number: 1,
        function: "rack_log_gpset".to_owned(),
        owner: "K329".to_owned(),
        rust: "crate::rack_log::rack_log_gpset".to_owned(),
        test: "donor_rack_log_gpset".to_owned(),
        mutation: "-".to_owned(),
        status: "ASSIGNED K329".to_owned(),
    }];
    let source = SourceIndex::load().expect("source");
    let report = audit::run(&donor, &register, &source, &[]);
    assert_eq!(report.accepted, 0);
    assert!(report.gaps[0].reasons.contains(&"no mutation".to_owned()));
}
