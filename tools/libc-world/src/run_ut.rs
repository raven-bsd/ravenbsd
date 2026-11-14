// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::testutil::{TempDir, require};

fn shell(temp: &TempDir, script: &str) -> RunSpec {
    let mut spec = RunSpec::new(
        "sh",
        vec!["-c".to_string(), script.to_string()],
        temp.path(),
        temp.join("log/step.log"),
        5,
    );
    spec.env = vec![(
        "PATH".to_string(),
        std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string()),
    )];
    spec
}

#[test]
fn a_true_command_exits_zero() {
    if !require(&["sh"]) {
        return;
    }
    let temp = TempDir::new("run-true");
    let output = run(&shell(&temp, "true")).expect("run");
    assert_eq!(output.exit, Some(0));
    assert!(!output.timed_out);
}

#[test]
fn a_false_command_exits_one() {
    if !require(&["sh"]) {
        return;
    }
    let temp = TempDir::new("run-false");
    let output = run(&shell(&temp, "false")).expect("run");
    assert_eq!(output.exit, Some(1));
}

#[test]
fn both_streams_land_in_the_log() {
    if !require(&["sh"]) {
        return;
    }
    let temp = TempDir::new("run-log");
    let output = run(&shell(&temp, "echo out; echo err >&2")).expect("run");
    assert!(output.text.contains("out\n"));
    assert!(output.text.contains("err\n"));
}

#[test]
fn the_environment_is_explicit() {
    if !require(&["sh"]) {
        return;
    }
    let temp = TempDir::new("run-env");
    let mut spec = shell(&temp, "printf '%s' \"${FOO:-absent}\"");
    spec.env.push(("FOO".to_string(), "bar".to_string()));
    assert_eq!(run(&spec).expect("run").text, "bar");
}

#[test]
fn an_unlisted_variable_is_not_inherited() {
    if !require(&["sh"]) {
        return;
    }
    let temp = TempDir::new("run-clean");
    let spec = shell(&temp, "printf '%s' \"${HOME:-none}\"");
    assert_eq!(run(&spec).expect("run").text, "none");
}

#[test]
fn the_working_directory_is_respected() {
    if !require(&["sh"]) {
        return;
    }
    let temp = TempDir::new("run-cwd");
    let spec = shell(&temp, "pwd");
    let output = run(&spec).expect("run");
    let actual = std::fs::canonicalize(output.text.trim()).expect("canonical");
    let expected = std::fs::canonicalize(temp.path()).expect("canonical");
    assert_eq!(actual, expected);
}

#[test]
fn a_timeout_is_reported_and_the_process_is_killed() {
    if !require(&["sh", "sleep"]) {
        return;
    }
    let temp = TempDir::new("run-timeout");
    let mut spec = shell(&temp, "sleep 30");
    spec.timeout_secs = 1;
    let started = Instant::now();
    let output = run(&spec).expect("run");
    assert!(output.timed_out);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_missing_program_is_an_error() {
    let temp = TempDir::new("run-missing");
    let spec = RunSpec::new(
        "definitely-not-a-program-xyz",
        Vec::new(),
        temp.path(),
        temp.join("log"),
        1,
    );
    assert!(run(&spec).is_err());
}

#[test]
fn with_env_replaces_a_duplicate() {
    let spec = RunSpec::new("true", Vec::new(), "/tmp", "/tmp/x.log", 1)
        .with_env("PATH", "/bin")
        .with_env("PATH", "/usr/bin");
    assert_eq!(spec.env.iter().filter(|(key, _)| key == "PATH").count(), 1);
    assert!(
        spec.env
            .iter()
            .any(|(key, value)| key == "PATH" && value == "/usr/bin")
    );
}

#[test]
fn the_log_parent_is_created() {
    if !require(&["sh"]) {
        return;
    }
    let temp = TempDir::new("run-parent");
    let mut spec = shell(&temp, "true");
    spec.log = temp.join("deep/nested/step.log");
    run(&spec).expect("run");
    assert!(temp.join("deep/nested/step.log").is_file());
}
