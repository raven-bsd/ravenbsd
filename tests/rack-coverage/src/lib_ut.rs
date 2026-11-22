// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Positive and negative fixtures for the RACK coverage checker.

use crate::donor::{parse_functions, parse_manifest};
use crate::matrix::{check, parse_matrix};

const DONOR: &str = include_str!("../fixtures/donor.c");
const MATRIX: &str = include_str!("../fixtures/matrix.md");
const PINNED: &str = include_str!("../fixtures/rack-functions.txt");

fn donor() -> Vec<crate::donor::Function> {
    parse_functions(DONOR)
}

fn report(matrix: &str) -> crate::matrix::Report {
    check(&donor(), &parse_matrix(matrix))
}

#[test]
fn parser_extracts_only_function_definitions() {
    let functions = donor();
    let names: Vec<&str> = functions
        .iter()
        .map(|function| function.name.as_str())
        .collect();
    assert_eq!(names, ["helper", "public_fn"]);
    assert!(functions[0].line > 0);
    assert!(functions[0].line < functions[1].line);
}

#[test]
fn pinned_manifest_has_219_unique_functions() {
    let functions = parse_manifest(PINNED);
    assert_eq!(functions.len(), 219);
    let mut names: Vec<&str> = functions
        .iter()
        .map(|function| function.name.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 219, "the pinned function names are unique");
    assert!(
        functions
            .iter()
            .any(|function| function.name == "rack_ack_received")
    );
    assert!(
        functions
            .iter()
            .any(|function| function.name == "tcp_addrack")
    );
}

#[test]
fn complete_fixture_matrix_passes() {
    let report = report(MATRIX);
    assert_eq!(report.donor_count, 2);
    assert_eq!(report.row_count, 2);
    assert!(report.is_ok(), "{:?}", report.errors());
}

#[test]
fn missing_row_fails_and_prints_the_name() {
    let matrix: String = MATRIX
        .lines()
        .filter(|line| !line.contains("public_fn"))
        .collect::<Vec<_>>()
        .join("\n");
    let report = report(&matrix);
    assert_eq!(report.missing, ["public_fn"]);
    assert!(
        report
            .errors()
            .iter()
            .any(|error| error.contains("public_fn"))
    );
}

#[test]
fn duplicate_row_fails() {
    let helper = MATRIX
        .lines()
        .find(|line| line.contains("`helper`"))
        .expect("helper row");
    let matrix = MATRIX.replace("| `public_fn`", &format!("{helper}\n| `public_fn`"));
    let report = report(&matrix);
    assert_eq!(report.duplicates, ["helper"]);
}

#[test]
fn empty_test_fails() {
    let matrix = MATRIX.replace("`helper_returns_one`", "`-`");
    let report = report(&matrix);
    assert_eq!(report.empty_test, ["helper"]);
}

#[test]
fn empty_rust_fails() {
    let matrix = MATRIX.replace("`crate::helper`", "`GAP`");
    let report = report(&matrix);
    assert_eq!(report.empty_rust, ["helper"]);
}

#[test]
fn empty_mutation_fails() {
    let matrix = MATRIX.replace("condition inverted -> test fails", "TODO");
    let report = report(&matrix);
    assert_eq!(report.empty_mutation, ["helper"]);
}

#[test]
fn non_pass_status_fails() {
    let matrix = MATRIX.replace("| PASS |", "| BLOCKED |");
    let report = report(&matrix);
    assert_eq!(report.not_pass, ["helper", "public_fn"]);
}

#[test]
fn unknown_row_fails() {
    let matrix = MATRIX.replace("`helper`", "`not_in_donor`");
    let report = report(&matrix);
    assert_eq!(report.unknown, ["not_in_donor"]);
    assert_eq!(report.missing, ["helper"]);
}

#[test]
fn four_column_k269_table_reports_missing_and_not_pass() {
    let table = "\
| Donor function (rack.c) | Rust | Named tests | Mutation/negative |
|---|---|---|---|
| `helper` (L21) | `crate::helper` | `helper_returns_one` | condition inverted |
";
    let report = report(table);
    assert_eq!(report.donor_count, 2);
    assert_eq!(report.row_count, 1);
    assert_eq!(report.missing, ["public_fn"]);
    assert_eq!(report.not_pass, ["helper"]);
    assert!(report.empty_rust.is_empty());
    assert!(report.empty_test.is_empty());
    assert!(report.empty_mutation.is_empty());
}

#[test]
fn parser_skips_unrelated_tables() {
    let markdown = "\
| Requirement | Test |
|---|---|
| R1 | something |

| Function | Rust | Test | Mutation | Status |
|---|---|---|---|---|
| `helper` | `crate::helper` | `helper_returns_one` | inverted | PASS |
| `public_fn` | `crate::public_fn` | `public_fn_returns_length` | removed | PASS |
";
    let report = report(markdown);
    assert_eq!(report.row_count, 2);
    assert!(report.is_ok(), "{:?}", report.errors());
}
