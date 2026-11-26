// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit and negative tests for the register repair/checker.

use rack_acceptance::register::RegisterRow;
use rack_coverage::Function;

use crate::check;
use crate::propose;
use crate::source::SourceIndex;

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

fn row(function: &str) -> RegisterRow {
    RegisterRow {
        number: 1,
        function: function.to_owned(),
        owner: "K334".to_owned(),
        rust: "rack_alloc".to_owned(),
        test: "rack_alloc_returns_and_reuses_a_map".to_owned(),
        mutation: "cache size changed -> test fails".to_owned(),
        status: "PASS".to_owned(),
    }
}

#[test]
fn proposed_register_covers_all_219_rows_without_group_reports() {
    let rows = propose::propose_from_repo(None).expect("propose");
    let donor = rack_acceptance::register::load_donor().expect("donor");
    let source = SourceIndex::load().expect("source");
    let report = check::check(&donor, &rows, &source);
    assert_eq!(rows.len(), 219);
    assert!(!report.is_ok(), "the K269 gap must remain visible");
    assert_eq!(
        report.not_pass,
        vec!["rack_cong_signal".to_owned(), "rack_update_prr".to_owned()]
    );
    assert!(report.missing.is_empty());
    assert!(report.duplicates.is_empty());
    assert!(report.empty_rust.is_empty());
    assert!(report.empty_test.is_empty());
    assert!(report.empty_mutation.is_empty());
    assert!(report.missing_symbol.is_empty());
    assert!(report.missing_test.is_empty());
}

#[test]
fn a_missing_donor_row_is_rejected() {
    let donor = donor(&["rack_alloc", "rack_free"]);
    let rows = vec![row("rack_alloc")];
    let source = SourceIndex::load().expect("source");
    let report = check::check(&donor, &rows, &source);
    assert_eq!(report.missing, vec!["rack_free".to_owned()]);
    assert!(!report.is_ok());
}

#[test]
fn a_duplicate_row_is_rejected() {
    let donor = donor(&["rack_alloc"]);
    let rows = vec![row("rack_alloc"), row("rack_alloc")];
    let source = SourceIndex::load().expect("source");
    let report = check::check(&donor, &rows, &source);
    assert_eq!(report.duplicates, vec!["rack_alloc".to_owned()]);
    assert!(!report.is_ok());
}

#[test]
fn an_empty_mutation_is_rejected() {
    let donor = donor(&["rack_alloc"]);
    let mut value = row("rack_alloc");
    value.mutation = "-".to_owned();
    let source = SourceIndex::load().expect("source");
    let report = check::check(&donor, &[value], &source);
    assert_eq!(report.empty_mutation, vec!["rack_alloc".to_owned()]);
    assert!(!report.is_ok());
}

#[test]
fn a_bogus_symbol_is_rejected() {
    let donor = donor(&["rack_alloc"]);
    let mut value = row("rack_alloc");
    value.rust = "does_not_exist".to_owned();
    let source = SourceIndex::load().expect("source");
    let report = check::check(&donor, &[value], &source);
    assert_eq!(report.missing_symbol, vec!["rack_alloc".to_owned()]);
    assert!(!report.is_ok());
}

#[test]
fn a_multi_owner_row_is_rejected() {
    let donor = donor(&["rack_alloc"]);
    let mut value = row("rack_alloc");
    value.owner = "K334/K335".to_owned();
    let source = SourceIndex::load().expect("source");
    let report = check::check(&donor, &[value], &source);
    assert_eq!(report.bad_owner, vec!["rack_alloc".to_owned()]);
    assert!(!report.is_ok());
}
