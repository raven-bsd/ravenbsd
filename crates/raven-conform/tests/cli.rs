// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end CLI tests.

use std::process::{Command, Output};

use raven_conform::Results;
use raven_conform::summary::SummaryRow;

const BIN: &str = env!("CARGO_BIN_EXE_raven-conform");

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("run raven-conform")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("process exited normally")
}

#[test]
fn parse_pjdfstest_prints_results_json() {
    let output = run(&["parse", "pjdfstest", &fixture("pjdfstest-sample.txt")]);
    assert_eq!(
        code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results: Results = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(results.tool, "pjdfstest");
    assert_eq!(results.results.len(), 10);
    assert_eq!(results.target, None);
}

#[test]
fn parse_ltp_prints_results_json() {
    let output = run(&["parse", "ltp", &fixture("ltp-sample.log")]);
    assert_eq!(code(&output), 0);
    let results: Results = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(results.tool, "ltp");
    assert_eq!(results.results.len(), 9);
}

#[test]
fn parse_accepts_target() {
    let output = run(&[
        "parse",
        "pjdfstest",
        &fixture("pjdfstest-sample.txt"),
        "--target",
        "testbox",
    ]);
    assert_eq!(code(&output), 0);
    let results: Results = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(results.target.as_deref(), Some("testbox"));
}

#[test]
fn compare_regression_exits_one() {
    let output = run(&[
        "compare",
        "--known",
        &fixture("KNOWN_FAILURES"),
        &fixture("results-regression.json"),
    ]);
    assert_eq!(code(&output), 1);
    assert!(String::from_utf8_lossy(&output.stdout).contains("REGRESSION"));
}

#[test]
fn compare_clean_exits_zero() {
    let output = run(&[
        "compare",
        "--known",
        &fixture("KNOWN_FAILURES"),
        &fixture("results-clean.json"),
    ]);
    assert_eq!(
        code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn compare_fixed_exits_three() {
    let output = run(&[
        "compare",
        "--known",
        &fixture("KNOWN_FAILURES"),
        &fixture("results-fixed.json"),
    ]);
    assert_eq!(code(&output), 3);
    assert!(String::from_utf8_lossy(&output.stdout).contains("remove from KNOWN_FAILURES"));
}

#[test]
fn compare_allow_new_exits_four_and_warns() {
    let output = run(&[
        "compare",
        "--known",
        &fixture("KNOWN_FAILURES"),
        &fixture("results-regression.json"),
        "--allow-new",
    ]);
    assert_eq!(code(&output), 4);
    assert!(String::from_utf8_lossy(&output.stdout).contains("NEW-KNOWN"));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("WARNING: the lead refuses additions without justification")
    );
}

#[test]
fn summary_prints_markdown() {
    let output = run(&["summary", &fixture("results-regression.json")]);
    assert_eq!(code(&output), 0);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.starts_with("| Suite | Total | Pass | Fail | Skip | Broken | Pass rate |"));
    assert!(text.contains("| tests/chmod/00.t | 3 | 0 | 2 | 1 | 0 | 0.0% |"));
}

#[test]
fn summary_json_is_machine_readable() {
    let output = run(&["summary", "--json", &fixture("results-regression.json")]);
    assert_eq!(code(&output), 0);
    let rows: Vec<SummaryRow> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].suite, "tests/chmod/00.t");
}

#[test]
fn unknown_command_is_an_error() {
    let output = run(&["unknown"]);
    assert_ne!(code(&output), 0);
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown command"));
}

#[test]
fn missing_file_is_an_error() {
    let output = run(&["parse", "pjdfstest", "definitely-missing.txt"]);
    assert_ne!(code(&output), 0);
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read"));
}

#[test]
fn help_exits_zero() {
    let output = run(&["--help"]);
    assert_eq!(code(&output), 0);
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
}
