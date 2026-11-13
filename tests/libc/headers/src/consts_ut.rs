// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for the constant/probe tables.

use super::*;

#[test]
fn spec_table_has_at_least_60_rows() {
    let rows = load_spec().expect("spec");
    assert!(rows.len() >= 60, "only {} rows", rows.len());
    assert!(rows.iter().any(|row| row.name == "PIPE_BUF"));
    assert!(
        rows.iter()
            .any(|row| row.name == "PAGesSIZE" || row.name == "PAGESIZE")
    );
}

#[test]
fn header_only_rows_all_have_reasons() {
    let rows = load_header_only().expect("header-only");
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|row| !row.reason.trim().is_empty()));
}

#[test]
fn probe_table_has_at_least_25_rows() {
    let rows = load_probes().expect("probes");
    assert!(rows.len() >= 25, "only {} rows", rows.len());
    for row in &rows {
        let mut modes: Vec<&str> = row
            .visible_in
            .iter()
            .chain(row.hidden_in.iter())
            .map(String::as_str)
            .collect();
        modes.sort_unstable();
        let mut expected: Vec<&str> = crate::matrix::FEATURE_MODES.to_vec();
        expected.sort_unstable();
        assert_eq!(
            modes, expected,
            "probe {}:{} does not partition the modes",
            row.header, row.symbol
        );
    }
}

#[test]
fn exceptions_file_is_empty_for_c37() {
    let rows = load_exceptions().expect("exceptions");
    assert!(rows.is_empty(), "C37 must not use -Wno-*: {rows:?}");
}

#[test]
fn family_prefixes_work() {
    assert!(in_family("EPERM"));
    assert!(in_family("SIGINT"));
    assert!(in_family("_SC_PAGESIZE"));
    assert!(!in_family("BASENAME"));
    assert!(in_family("PROT_READ"));
}

#[test]
fn generated_defines_and_errno_parity() {
    let defines = generated_defines().expect("generated");
    assert_eq!(defines.get("EPERM").map(String::as_str), Some("1"));
    assert_eq!(defines.get("O_LARGEFILE").map(String::as_str), Some("0"));
    let (numbers, aliases) = errno_parity().expect("errno");
    assert_eq!(numbers, linux_errno_count() + 14);
    assert_eq!(aliases, 4);
}

#[test]
fn family_scan_flags_a_mismatch() {
    let mut generated = BTreeMap::new();
    generated.insert("O_RDONLY".to_string(), "0".to_string());
    let header_only = BTreeMap::new();
    let dump = vec![
        ("O_RDONLY".to_string(), "1".to_string()),
        ("O_BOGUS".to_string(), "2".to_string()),
    ];
    let (mismatches, unlisted) = scan_families(&dump, &generated, &header_only);
    assert_eq!(mismatches.len(), 1);
    assert_eq!(unlisted, vec!["O_BOGUS".to_string()]);
}
