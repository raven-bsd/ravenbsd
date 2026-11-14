// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Command-line integration tests for `raven-atf-run`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_raven-atf-run")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn run(args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .output()
        .expect("run raven-atf-run")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn help_is_exit_zero() {
    let output = run(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("raven-atf-run suite"));
}

#[test]
fn list_prints_the_golden_cases() {
    let program = fixture("tp-mixed.sh");
    let output = run(&["list", program.to_str().expect("path")]);
    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    assert_eq!(text, "ok_case\nneeds_root\nsleeper\n");
}

#[test]
fn run_pass_is_exit_zero_and_tap_well_formed() {
    let program = fixture("tp-pass.sh");
    let output = run(&["run", program.to_str().expect("path"), "--report", "tap"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.starts_with("1..2\n"));
    assert!(text.contains("ok 1 - "));
}

#[test]
fn run_one_case_selected_by_t() {
    let program = fixture("tp-mixed.sh");
    let output = run(&[
        "run",
        program.to_str().expect("path"),
        "-t",
        "ok_case",
        "--report",
        "tap",
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains("s/p".replace("s/p", "tp-mixed.sh").as_str()));
}

#[test]
fn the_mixed_fixture_is_exit_one_by_design() {
    let program = fixture("tp-mixed.sh");
    let output = run(&[
        "run",
        program.to_str().expect("path"),
        "--timeout",
        "1",
        "--report",
        "tap",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("ok 1 - tp-mixed.sh/ok_case/ok_case"));
    assert!(text.contains("# SKIP needs-root") || text.contains("# SKIP"));
    assert!(text.contains("not ok 3 - tp-mixed.sh/sleeper/sleeper # FAIL timeout after 1s"));
}

#[test]
fn run_fail_is_exit_one() {
    let program = fixture("tp-fail.sh");
    let output = run(&["run", program.to_str().expect("path")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("strchr returned NULL"));
}

#[test]
fn an_unknown_subcommand_is_exit_two() {
    let output = run(&["frobnicate"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("usage"));
}

#[test]
fn target_mode_without_sysroot_is_exit_two() {
    let output = run(&["suite", "--suite", "netbsd-libc", "--mode", "target"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--sysroot"));
}

#[test]
fn the_qemu_spelling_is_exit_two() {
    let output = run(&["suite", "--suite", "netbsd-libc", "--target", "qemu-x86_64"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("not implemented here (C73)"));
}

#[test]
fn gen_suite_needs_a_selection() {
    let output = run(&["gen-suite"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--all"));
}

#[test]
fn check_a_matching_run_is_exit_zero() {
    let baseline = fixture("baseline-ok.tsv");
    let run_file = fixture("run-ok.tsv");
    let output = run(&[
        "check",
        "--table",
        baseline.to_str().expect("path"),
        "--run",
        run_file.to_str().expect("path"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(stdout(&output).contains("improved 0"));
}

#[test]
fn check_a_regression_is_exit_one_and_names_the_case() {
    let baseline = fixture("baseline-ok.tsv");
    let run_file = fixture("run-regressed.tsv");
    let output = run(&[
        "check",
        "--table",
        baseline.to_str().expect("path"),
        "--run",
        run_file.to_str().expect("path"),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("REGRESSION"));
    assert!(stdout(&output).contains("strchr_basic"));
}

#[test]
fn gate_met_fixture_is_exit_zero() {
    let baseline = fixture("baseline-met.tsv");
    let config = fixture("g1-fixture.toml");
    let output = run(&[
        "gate",
        "--g1",
        "--table",
        baseline.to_str().expect("path"),
        "--config",
        config.to_str().expect("path"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(stdout(&output).contains("G1: MET"));
}

#[test]
fn gate_not_met_fixture_is_exit_three() {
    let baseline = fixture("run-regressed.tsv");
    let config = fixture("g1-fixture.toml");
    let output = run(&[
        "gate",
        "--g1",
        "--table",
        baseline.to_str().expect("path"),
        "--config",
        config.to_str().expect("path"),
    ]);
    assert_eq!(output.status.code(), Some(3));
    assert!(stdout(&output).contains("NOT MET"));
}

#[test]
fn table_write_records_real_rows_and_refuses_regressions() {
    let root = std::env::temp_dir().join(format!("raven-atf-cli-table-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("runs/oracle")).expect("oracle");
    fs::create_dir_all(root.join("runs/target")).expect("target");
    let row = "s\tp\tc\tpass\n";
    fs::write(root.join("runs/oracle/s.tsv"), row).expect("oracle write");
    fs::write(root.join("runs/target/s.tsv"), row).expect("target write");
    let baseline = root.join("baseline.tsv");
    let markdown = root.join("CONFORMANCE.md");
    let output = run(&[
        "table",
        "--write",
        "--runs",
        root.join("runs").to_str().expect("runs"),
        "--baseline",
        baseline.to_str().expect("baseline"),
        "--md",
        markdown.to_str().expect("md"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        fs::read_to_string(&baseline)
            .expect("baseline")
            .contains("s\tp\tc\tpass")
    );
    assert!(
        fs::read_to_string(&markdown)
            .expect("md")
            .contains("Nothing is deleted from this table")
    );

    fs::write(
        root.join("runs/target/s.tsv"),
        "s\tp\tc\tfail\tbehaviour:bad\tC40\n",
    )
    .expect("target rewrite");
    let output = run(&[
        "table",
        "--write",
        "--runs",
        root.join("runs").to_str().expect("runs"),
        "--baseline",
        baseline.to_str().expect("baseline"),
        "--md",
        markdown.to_str().expect("md"),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("refuses"));
    let _ = fs::remove_dir_all(&root);
}
