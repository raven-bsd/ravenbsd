// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Audit assertions and negative fixtures.

use std::collections::BTreeSet;
use std::path::PathBuf;

use rack_coverage::{Function, check, parse_matrix};

use crate::audit::{Audit, EXPECTED, FindingKind, Module, audit, audit_sources, function_names};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sources() -> (String, String, String, String) {
    let root = root();
    let read = |name: &str| {
        std::fs::read_to_string(root.join("crates/raven-tcpip/src").join(name)).unwrap_or_default()
    };
    (
        read(Module::Output.source()),
        read(Module::Retransmit.source()),
        read(Module::Output.tests()),
        read(Module::Retransmit.tests()),
    )
}

fn names(audit: &Audit, kind: FindingKind) -> BTreeSet<String> {
    audit
        .of_kind(kind)
        .into_iter()
        .map(|finding| finding.name.clone())
        .collect()
}

#[test]
fn expected_group_has_34_unique_functions() {
    assert_eq!(EXPECTED.len(), 34);
    assert_eq!(function_names().len(), 34);
    let mut unique = BTreeSet::new();
    for expected in EXPECTED {
        assert!(
            unique.insert(expected.function),
            "duplicate {}",
            expected.function
        );
    }
    assert_eq!(
        EXPECTED
            .iter()
            .filter(|expected| expected.module == Module::Output)
            .count(),
        21
    );
    assert_eq!(
        EXPECTED
            .iter()
            .filter(|expected| expected.module == Module::Retransmit)
            .count(),
        13
    );
}

#[test]
fn every_function_has_a_rust_mapping() {
    let audit = audit(&root());
    assert_eq!(audit.mapped, 34, "{:?}", audit.findings);
    assert!(names(&audit, FindingKind::MissingMapping).is_empty());
}

#[test]
fn every_function_has_a_named_test() {
    let audit = audit(&root());
    assert_eq!(audit.tested, 34, "{:?}", audit.findings);
    assert!(names(&audit, FindingKind::MissingTest).is_empty());
}

#[test]
fn every_function_has_a_mutation_or_negative_check() {
    let audit = audit(&root());
    assert_eq!(audit.mutated, 34, "{:?}", audit.findings);
    assert!(names(&audit, FindingKind::EmptyMutation).is_empty());
}

#[test]
fn audit_reports_the_two_wrong_tcp_state_constants() {
    let audit = audit(&root());
    let wrong = audit.of_kind(FindingKind::WrongConstant);
    let names: BTreeSet<&str> = wrong.iter().map(|finding| finding.name.as_str()).collect();
    assert_eq!(names, BTreeSet::from(["TCPS_CLOSING", "TCPS_LAST_ACK"]));
    let closing = wrong
        .iter()
        .find(|finding| finding.name == "TCPS_CLOSING")
        .expect("closing");
    assert!(closing.detail.contains("9") && closing.detail.contains("8"));
    let last = wrong
        .iter()
        .find(|finding| finding.name == "TCPS_LAST_ACK")
        .expect("last ack");
    assert!(last.detail.contains("10") && last.detail.contains("9"));
}

#[test]
fn audit_reports_duplicate_and_conflicting_constants() {
    let audit = audit(&root());
    let duplicates = names(&audit, FindingKind::DuplicateConstant);
    assert_eq!(
        duplicates,
        BTreeSet::from([
            "TCPS_CLOSING".to_owned(),
            "TCPS_FIN_WAIT_1".to_owned(),
            "TCPS_FIN_WAIT_2".to_owned(),
            "TCPS_LAST_ACK".to_owned(),
        ])
    );
    let conflicts = names(&audit, FindingKind::Conflict);
    assert_eq!(
        conflicts,
        BTreeSet::from(["TCPS_CLOSING".to_owned(), "TCPS_LAST_ACK".to_owned()])
    );
}

#[test]
fn negative_fixture_missing_mapping_is_reported() {
    let (output, retransmit, output_tests, retransmit_tests) = sources();
    let mutated = output.replacen("fn rack_output(", "fn rack_output_missing(", 1);
    let audit = audit_sources(&mutated, &retransmit, &output_tests, &retransmit_tests);
    assert!(names(&audit, FindingKind::MissingMapping).contains("rack_output"));
}

#[test]
fn negative_fixture_wrong_constant_is_reported() {
    let (output, retransmit, output_tests, retransmit_tests) = sources();
    let mutated = output.replace(
        "const TLP_MAX_PROBES: u8 = 2;",
        "const TLP_MAX_PROBES: u8 = 3;",
    );
    let audit = audit_sources(&mutated, &retransmit, &output_tests, &retransmit_tests);
    assert!(names(&audit, FindingKind::WrongConstant).contains("TLP_MAX_PROBES"));
}

fn matrix() -> String {
    let mut matrix =
        String::from("| Function | Rust | Test | Mutation | Status |\n|---|---|---|---|---|\n");
    for expected in EXPECTED {
        let module = match expected.module {
            Module::Output => "rack_output",
            Module::Retransmit => "rack_retransmit",
        };
        matrix.push_str(&format!(
            "| `{}` | `{}::{}` | `{}` | {} | PASS |\n",
            expected.function, module, expected.function, expected.test, expected.mutation
        ));
    }
    matrix
}

fn donor() -> Vec<Function> {
    EXPECTED
        .iter()
        .map(|expected| Function {
            line: 0,
            name: expected.function.to_owned(),
        })
        .collect()
}

#[test]
fn k315_group_matrix_is_complete_and_rejects_a_deleted_row() {
    let matrix = matrix();
    let rows = parse_matrix(&matrix);
    let report = check(&donor(), &rows);
    assert!(report.is_ok(), "{:?}", report.errors());
    assert_eq!(report.donor_count, 34);
    assert_eq!(report.row_count, 34);

    let missing: String = matrix
        .lines()
        .filter(|line| !line.contains("| `rack_fast_rsm_output` |"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        check(&donor(), &parse_matrix(&missing)).missing,
        ["rack_fast_rsm_output"]
    );
}
