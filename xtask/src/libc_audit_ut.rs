// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::path::{Path, PathBuf};

use super::*;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent")
        .to_path_buf()
}

#[test]
fn defaults_forward_the_script() {
    let options = parse(&[]).expect("parse");
    assert!(!options.rebuild);
    assert!(!options.self_test);
    assert_eq!(
        options.script_args(),
        vec!["scripts/libc-symbol-audit.sh".to_string()]
    );

    let spec = command(Path::new("/repo"), &options);
    assert_eq!(spec.program, "sh");
    assert_eq!(spec.args, options.script_args());
    assert_eq!(spec.cwd, Path::new("/repo"));
}

#[test]
fn every_option_is_forwarded_in_order() {
    let options = parse(&[
        "--out".into(),
        "build/audit.tsv".into(),
        "--expected".into(),
        "expected.tsv".into(),
        "--required".into(),
        "required.tsv".into(),
        "--exports".into(),
        "libc.so.1".into(),
        "--self-test".into(),
        "--rebuild".into(),
    ])
    .expect("all options");
    assert!(options.rebuild);
    assert!(options.self_test);
    assert_eq!(
        options.script_args(),
        vec![
            "scripts/libc-symbol-audit.sh",
            "--rebuild",
            "--self-test",
            "--exports",
            "libc.so.1",
            "--required",
            "required.tsv",
            "--expected",
            "expected.tsv",
            "--out",
            "build/audit.tsv",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>()
    );
}

#[test]
fn unknown_and_incomplete_options_are_rejected() {
    assert!(parse(&["--bogus".into()]).is_err());
    assert!(parse(&["--out".into()]).is_err());
    assert!(parse(&["--exports".into()]).is_err());
    assert!(parse(&["--required".into()]).is_err());
    assert!(parse(&["--expected".into()]).is_err());
}

#[test]
fn help_is_a_success_without_a_child() {
    assert_eq!(run(Path::new("."), &["--help".into()]).expect("help"), 0);
    assert_eq!(run(Path::new("."), &["-h".into()]).expect("short help"), 0);
}

#[test]
fn the_script_self_test_is_wired() {
    let code = run(&repo_root(), &["--self-test".into()]).expect("self-test");
    assert_eq!(code, 0);
}

#[test]
fn the_c74_gate_row_runs_this_command() {
    let text = include_str!("../../tests/libc/gate/rows.toml");
    let value: toml::Value = toml::from_str(text).expect("gate row toml");
    let checks = value
        .get("check")
        .and_then(toml::Value::as_array)
        .expect("check array");
    let check = checks
        .iter()
        .find(|check| check.get("id").and_then(toml::Value::as_str) == Some("1.1.audit"))
        .expect("1.1.audit check");
    assert_eq!(
        check.get("id").and_then(toml::Value::as_str),
        Some("1.1.audit")
    );
    assert_eq!(check.get("row").and_then(toml::Value::as_integer), Some(1));
    assert_eq!(
        check.get("kind").and_then(toml::Value::as_str),
        Some("argv")
    );
    assert_eq!(
        check.get("required").and_then(toml::Value::as_bool),
        Some(true)
    );
    let argv: Vec<&str> = check
        .get("argv")
        .and_then(toml::Value::as_array)
        .expect("argv")
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    assert_eq!(argv, vec!["cargo", "xtask", "libc-audit"]);
    assert_eq!(
        check.get("evidence").and_then(toml::Value::as_str),
        Some("build/libc-gate/1.1.audit.txt")
    );
}
