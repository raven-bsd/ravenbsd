// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Evaluator tests with local fixture commands.

use super::*;
use crate::rows::{BlockedOn, PassRule, Thresholds};
use crate::testutil::TempDir;

fn spec(id: &str, row: u8) -> CheckSpec {
    CheckSpec {
        id: id.to_string(),
        row,
        title: format!("check {id}"),
        required: true,
        kind: "argv".to_string(),
        argv: vec!["true".to_string()],
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

fn options(repo: &Path) -> Options {
    Options {
        repo: repo.to_path_buf(),
        out: repo.join("build/libc-gate"),
        timeout: Duration::from_secs(2),
        only: None,
    }
}

fn rows(checks: Vec<CheckSpec>) -> Rows {
    Rows {
        check: checks,
        thresholds: Thresholds::default(),
    }
}

#[test]
fn true_is_met() {
    let temp = TempDir::new("eval-true");
    let rows = rows(vec![spec("1.1", 1)]);
    let results = eval(&rows, &options(temp.path())).unwrap();
    assert_eq!(results[0].status, Status::Met);
}

#[test]
fn false_is_not_met() {
    let temp = TempDir::new("eval-false");
    let mut check = spec("1.1", 1);
    check.argv = vec!["false".to_string()];
    let results = eval(&rows(vec![check]), &options(temp.path())).unwrap();
    assert_eq!(results[0].status, Status::NotImplemented);
}

#[test]
fn exit_77_is_blocked() {
    let temp = TempDir::new("eval-77");
    let mut check = spec("1.1", 1);
    check.argv = vec!["sh".to_string(), "-c".to_string(), "exit 77".to_string()];
    check.blocked_on.exit = vec![77];
    let results = eval(&rows(vec![check]), &options(temp.path())).unwrap();
    assert_eq!(results[0].status, Status::Blocked);
}

#[test]
fn a_missing_prerequisite_file_is_blocked_without_running() {
    let temp = TempDir::new("eval-missing");
    let mut check = spec("2.2", 2);
    check.argv = vec!["false".to_string()];
    check.blocked_on.missing_files = vec!["tools/libc-world/world.toml".to_string()];
    let results = eval(&rows(vec![check]), &options(temp.path())).unwrap();
    assert_eq!(results[0].status, Status::Blocked);
    assert!(results[0].reason.contains("missing"));
}

#[test]
fn an_info_check_is_info_even_when_the_reader_would_fail() {
    let temp = TempDir::new("eval-info");
    let mut check = spec("4.4", 4);
    check.kind = "info".to_string();
    check.required = false;
    let results = eval(&rows(vec![check]), &options(temp.path())).unwrap();
    assert_eq!(results[0].status, Status::Info);
}

#[test]
fn a_done_ticket_prerequisite_runs() {
    let temp = TempDir::new("eval-ticket");
    temp.write("tickets/INDEX.md", "| C1 | x | done |\n");
    let mut check = spec("1.2", 1);
    check.blocked_on.ticket = Some("C1".to_string());
    let results = eval(&rows(vec![check]), &options(temp.path())).unwrap();
    assert_eq!(results[0].status, Status::Met);
}

#[test]
fn a_blocked_ticket_prerequisite_is_blocked() {
    let temp = TempDir::new("eval-ticket-blocked");
    temp.write(
        "tickets/INDEX.md",
        "| C1 | x | done |\n| C2 | x | blocked |\n",
    );
    let mut check = spec("1.2", 1);
    check.argv = vec!["false".to_string()];
    check.blocked_on.ticket = Some("C2".to_string());
    let results = eval(&rows(vec![check]), &options(temp.path())).unwrap();
    assert_eq!(results[0].status, Status::Blocked);
    assert!(results[0].reason.contains("C2"));
}

#[test]
fn ticket_state_reads_the_index() {
    let temp = TempDir::new("eval-ticket-state");
    temp.write(
        "tickets/INDEX.md",
        "| C9 | lane | 7 | title | deps | owns | done |\n",
    );
    assert_eq!(ticket_state(temp.path(), "C9").as_deref(), Some("done"));
    assert_eq!(ticket_state(temp.path(), "C8"), None);
}

#[test]
fn an_oracle_expansion_with_no_children_is_blocked() {
    let temp = TempDir::new("eval-oracle-empty");
    let mut check = spec("1.4", 1);
    check.argv = vec!["true".to_string()];
    check.expand = Some("oracle".to_string());
    let results = eval(&rows(vec![check]), &options(temp.path())).unwrap();
    assert_eq!(results[0].status, Status::Blocked);
}

#[test]
fn a_files_reader_checks_existence() {
    let temp = TempDir::new("eval-files");
    temp.write("exists.txt", "x");
    let mut check = spec("6.4", 6);
    check.kind = "read".to_string();
    check.read = Some("files".to_string());
    check.file = Some("exists.txt".to_string());
    let results = eval(&rows(vec![check]), &options(temp.path())).unwrap();
    assert_eq!(results[0].status, Status::Met);
}

#[test]
fn a_missing_files_reader_input_is_blocked() {
    let temp = TempDir::new("eval-files-missing");
    let mut check = spec("6.4", 6);
    check.kind = "read".to_string();
    check.read = Some("files".to_string());
    check.file = Some("nope.txt".to_string());
    let results = eval(&rows(vec![check]), &options(temp.path())).unwrap();
    assert_eq!(results[0].status, Status::Blocked);
}

#[test]
fn the_evidence_file_and_json_are_written() {
    let temp = TempDir::new("eval-evidence");
    let rows = rows(vec![spec("1.1", 1)]);
    let results = eval(&rows, &options(temp.path())).unwrap();
    assert!(Path::new(&results[0].evidence).is_file());
    assert!(temp.path().join("build/libc-gate/1.1.json").is_file());
}

#[test]
fn only_runs_the_selected_prefix() {
    let temp = TempDir::new("eval-only");
    let rows = rows(vec![spec("1.1", 1), spec("2.1", 2)]);
    let mut options = options(temp.path());
    options.only = Some("2".to_string());
    let results = eval(&rows, &options).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "2.1");
}

#[test]
fn only_accepts_a_comma_separated_list() {
    let temp = TempDir::new("eval-only-list");
    let rows = rows(vec![spec("1.1", 1), spec("2.2", 2), spec("5.1", 5)]);
    let mut options = options(temp.path());
    options.only = Some("2.2,5".to_string());
    let results = eval(&rows, &options).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|result| result.id == "2.2"));
    assert!(results.iter().any(|result| result.id == "5.1"));
}

#[test]
fn a_timeout_is_blocked() {
    let temp = TempDir::new("eval-timeout");
    let mut check = spec("5.2", 5);
    check.argv = vec!["sh".to_string(), "-c".to_string(), "sleep 2".to_string()];
    let mut options = options(temp.path());
    options.timeout = Duration::from_secs(1);
    let results = eval(&rows(vec![check]), &options).unwrap();
    assert_eq!(results[0].status, Status::Blocked);
}

#[test]
fn status_from_exit_honours_pass_and_blocked() {
    let spec = spec("1.1", 1);
    let met = Process {
        exit: Some(0),
        ..Process::default()
    };
    assert_eq!(status_from_exit(&spec, &met), Status::Met);
    let failed = Process {
        exit: Some(1),
        ..Process::default()
    };
    assert_eq!(status_from_exit(&spec, &failed), Status::NotImplemented);
    let blocked = Process {
        exit: Some(77),
        ..Process::default()
    };
    let mut spec = spec;
    spec.blocked_on.exit = vec![77];
    assert_eq!(status_from_exit(&spec, &blocked), Status::Blocked);
}

#[test]
fn run_process_captures_output() {
    let temp = TempDir::new("eval-process");
    let process = run_process(
        temp.path(),
        &["sh".to_string(), "-c".to_string(), "echo hello".to_string()],
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(process.exit, Some(0));
    assert!(process.stdout.contains("hello"));
}
