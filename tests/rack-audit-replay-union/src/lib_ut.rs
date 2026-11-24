// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the independent replay/union audit.

use super::*;
use rack_coverage::Function;

fn synthetic_donor() -> Vec<Function> {
    vec![
        Function {
            line: 1,
            name: "alpha".to_owned(),
        },
        Function {
            line: 2,
            name: "beta".to_owned(),
        },
    ]
}

#[test]
fn register_is_219_rows_with_clean_union_shape() {
    let root = repo_root();
    let audit = audit(&root, Path::new(DEFAULT_TICKETS)).expect("audit");
    assert_eq!(audit.donor_count, 219, "pinned manifest");
    assert_eq!(audit.register_rows, 219, "docs/rack-coverage.md");
    assert_eq!(audit.pass_rows, 45, "PASS rows");
    assert!(audit.register_check.missing.is_empty());
    assert!(audit.register_check.duplicates.is_empty());
    assert!(audit.register_check.unknown.is_empty());
    assert_eq!(audit.register_check.not_pass.len(), 174);
    assert_eq!(audit.register_check.empty_rust.len(), 174);
}

#[test]
fn group_union_has_no_missing_duplicate_or_unknown() {
    let audit = audit(&repo_root(), Path::new(DEFAULT_TICKETS)).expect("audit");
    assert_eq!(audit.union.donor_count, 219);
    assert_eq!(audit.union.row_count, 219);
    assert!(audit.union.missing.is_empty());
    assert!(audit.union.duplicates.is_empty());
    assert!(audit.union.unknown.is_empty());
    assert_eq!(audit.union.empty_rust.len(), 73);
    assert_eq!(audit.union.not_pass.len(), 108);
    assert_eq!(audit.union.cross_report_duplicates.len(), 60);
}

#[test]
fn pass_rows_without_a_merged_rust_function_are_reported() {
    let audit = audit(&repo_root(), Path::new(DEFAULT_TICKETS)).expect("audit");
    eprintln!("missing Rust mappings: {:?}", audit.missing_rust);
    assert_eq!(audit.missing_rust.len(), 2);
    assert!(
        audit
            .missing_rust
            .iter()
            .any(|finding| finding.starts_with("rack_cong_signal -> Rack::cong_signal"))
    );
    assert!(
        audit
            .missing_rust
            .iter()
            .any(|finding| finding.starts_with("rack_update_prr -> Rack::rack_update_prr"))
    );
}

#[test]
fn constant_audit_records_the_three_donor_mismatches() {
    let findings = constant_findings();
    eprintln!("constant findings: {findings:?}");
    assert_eq!(findings.len(), 3);
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("TCP_FUNC_OUTPUT_CANDROP")),
        "missing CANDROP finding: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("TCP_FUNC_DEFAULT_OK")),
        "missing DEFAULT_OK finding: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("RACK_FREE_CNT_MAX")),
        "missing FREE_CNT finding: {findings:?}"
    );
}

#[test]
fn negative_missing_duplicate_and_blank_rows_are_detected() {
    let donor = synthetic_donor();
    let missing = parse_matrix(
        "| Function | Rust | Test | Mutation | Status |\n\
         |---|---|---|---|---|\n\
         | `alpha` | `fn alpha` | `alpha_test` | mutation | PASS |\n",
    );
    let report = check(&donor, &missing);
    assert_eq!(report.missing, vec!["beta".to_owned()]);

    let duplicate = parse_matrix(
        "| Function | Rust | Test | Mutation | Status |\n\
         |---|---|---|---|---|\n\
         | `alpha` | `fn alpha` | `alpha_test` | mutation | PASS |\n\
         | `alpha` | `fn alpha` | `alpha_test` | mutation | PASS |\n",
    );
    assert_eq!(
        check(&donor, &duplicate).duplicates,
        vec!["alpha".to_owned()]
    );

    let blank = parse_matrix(
        "| Function | Rust | Test | Mutation | Status |\n\
         |---|---|---|---|---|\n\
         | `alpha` | `fn alpha` | - | mutation | PASS |\n",
    );
    assert_eq!(check(&donor, &blank).empty_test, vec!["alpha".to_owned()]);
}
