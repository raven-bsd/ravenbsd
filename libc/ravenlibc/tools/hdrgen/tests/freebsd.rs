// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fail-closed integration checks for the FreeBSD profile CLI.

mod support;

use std::path::Path;
use std::process::{Command, Output};

use support::{repo_root, require_resources};

fn run(extra: &[&str], resources: &Path) -> Output {
    let root = repo_root();
    let mut args = vec![
        "verify",
        "--profile",
        "freebsd",
        "--root",
        root.to_str().unwrap(),
        "--resources",
        resources.to_str().unwrap(),
    ];
    args.extend(extra.iter().copied());
    Command::new(env!("CARGO_BIN_EXE_raven-libc-hdrgen"))
        .args(args)
        .output()
        .expect("run raven-libc-hdrgen")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_missing_donor_is_not_a_pass() {
    let Some(resources) = require_resources("a_missing_donor_is_not_a_pass") else {
        return;
    };
    let output = run(
        &[
            "--target",
            "x86_64",
            "--freebsd",
            "/nonexistent/s721-freebsd",
        ],
        &resources,
    );
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stderr(&output);
    assert!(text.contains("oracle integrity"), "{text}");
    assert!(
        text.contains("neither sys/sys nor a prepared include root"),
        "{text}"
    );
}

#[test]
fn an_empty_or_misspelled_family_is_a_usage_error() {
    let Some(resources) = require_resources("an_empty_or_misspelled_family_is_a_usage_error")
    else {
        return;
    };
    let empty = run(&["--target", "x86_64", "--families", ""], &resources);
    assert_eq!(empty.status.code(), Some(2), "{}", stderr(&empty));
    assert!(
        stderr(&empty).contains("at least one family"),
        "{}",
        stderr(&empty)
    );
    let typo = run(&["--target", "x86_64", "--families", "errnoo"], &resources);
    assert_eq!(typo.status.code(), Some(2), "{}", stderr(&typo));
    assert!(
        stderr(&typo).contains("unknown family"),
        "{}",
        stderr(&typo)
    );
}

#[test]
fn a_nonexistent_abi_root_is_not_a_pass() {
    let Some(resources) = require_resources("a_nonexistent_abi_root_is_not_a_pass") else {
        return;
    };
    let output = run(
        &["--target", "x86_64", "--abi", "/nonexistent/s721-abi"],
        &resources,
    );
    assert_ne!(output.status.code(), Some(0), "{}", stderr(&output));
}

#[test]
fn riscv64_is_not_a_freebsd_target() {
    let Some(resources) = require_resources("riscv64_is_not_a_freebsd_target") else {
        return;
    };
    let output = run(&["--target", "riscv64"], &resources);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("unknown FreeBSD --target"),
        "{}",
        stderr(&output)
    );
}
