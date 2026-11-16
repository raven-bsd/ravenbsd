// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `rows.toml` model tests.

use super::*;

fn rows_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("rows.toml")
}

fn spec(id: &str, row: u8) -> CheckSpec {
    CheckSpec {
        id: id.to_string(),
        row,
        title: format!("check {id}"),
        required: true,
        kind: "argv".to_string(),
        argv: vec!["cargo".to_string(), "true".to_string()],
        child_argv: Vec::new(),
        preflight: Vec::new(),
        pass: PassRule::default(),
        blocked_on: BlockedOn::default(),
        evidence: format!("build/libc-gate/{id}.txt"),
        source: "test".to_string(),
        read: None,
        file: None,
        min_seconds: None,
        cases: Vec::new(),
        expand: None,
        ticket: None,
        note: None,
    }
}

fn rows(checks: Vec<CheckSpec>) -> Rows {
    Rows {
        check: checks,
        thresholds: Thresholds::default(),
    }
}

#[test]
fn the_real_rows_file_parses() {
    let rows = Rows::load(&rows_path()).expect("rows.toml");
    assert!(rows.check.len() >= 20);
    for check in &rows.check {
        assert!((1..=6).contains(&check.row), "row {}", check.id);
    }
    assert!(rows.get("1.1").is_some());
    assert!(rows.get("2.2").is_some());
    assert!(rows.get("5.1").is_some());
    assert!(rows.get("6.4").is_some());
}

#[test]
fn the_real_thresholds_match_the_ticket() {
    let rows = Rows::load(&rows_path()).expect("rows.toml");
    assert_eq!(rows.thresholds.netbsd, 90);
    assert_eq!(rows.thresholds.freebsd, 90);
    assert_eq!(rows.thresholds.openbsd, 90);
    assert_eq!(rows.thresholds.picolibc, 90);
    assert_eq!(rows.thresholds.mlibc, 85);
    assert_eq!(rows.thresholds.exclusions, 250);
    assert_eq!(rows.thresholds.soak_seconds, 3600);
    assert_eq!(rows.thresholds.world_built, 100);
    assert_eq!(rows.thresholds.world_weighted, 197.5);
}

#[test]
fn the_real_file_has_no_shell_commands() {
    let rows = Rows::load(&rows_path()).expect("rows.toml");
    for check in &rows.check {
        assert!(!check.argv.iter().any(|arg| arg == "sh" || arg == "-c"));
    }
}

#[test]
fn the_real_file_starts_every_argv_with_cargo() {
    let rows = Rows::load(&rows_path()).expect("rows.toml");
    for check in &rows.check {
        if check.kind == "argv" {
            assert_eq!(check.argv.first().map(String::as_str), Some("cargo"));
        }
    }
}

#[test]
fn duplicate_ids_are_rejected() {
    let rows = rows(vec![spec("1.1", 1), spec("1.1", 1)]);
    assert!(rows.validate().is_err());
}

#[test]
fn a_row_outside_one_to_six_is_rejected() {
    let rows = rows(vec![spec("1.1", 7)]);
    assert!(rows.validate().is_err());
}

#[test]
fn an_unknown_kind_is_rejected() {
    let mut check = spec("1.1", 1);
    check.kind = "magic".to_string();
    assert!(rows(vec![check]).validate().is_err());
}

#[test]
fn an_argv_without_cargo_is_rejected() {
    let mut check = spec("1.1", 1);
    check.argv = vec!["true".to_string()];
    assert!(rows(vec![check]).validate().is_err());
}

#[test]
fn a_shell_command_in_a_real_row_is_rejected() {
    let mut check = spec("1.1", 1);
    check.argv = vec!["cargo".to_string(), "sh".to_string()];
    assert!(rows(vec![check]).validate().is_err());
}

#[test]
fn an_evidence_path_outside_the_gate_directory_is_rejected() {
    let mut check = spec("1.1", 1);
    check.evidence = "tmp/1.1.txt".to_string();
    assert!(rows(vec![check]).validate().is_err());
}

#[test]
fn a_read_check_needs_a_reader() {
    let mut check = spec("4.2", 4);
    check.kind = "read".to_string();
    check.read = None;
    assert!(rows(vec![check]).validate().is_err());
}

#[test]
fn empty_rows_are_rejected() {
    assert!(rows(Vec::new()).validate().is_err());
}

#[test]
fn get_returns_the_requested_id() {
    let rows = rows(vec![spec("1.1", 1), spec("6.4", 6)]);
    assert_eq!(rows.get("6.4").unwrap().row, 6);
    assert!(rows.get("9.9").is_none());
}

#[test]
fn default_thresholds_are_the_frozen_values() {
    let thresholds = Thresholds::default();
    assert_eq!(thresholds.soak_seconds, 3600);
    assert_eq!(thresholds.world_built, 100);
    assert_eq!(thresholds.mlibc, 85);
}
