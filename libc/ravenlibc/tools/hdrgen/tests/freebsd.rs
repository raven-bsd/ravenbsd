// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fail-closed integration checks for the FreeBSD profile CLI.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn resources() -> PathBuf {
    let mut directory = Some(repo_root());
    while let Some(current) = directory {
        let candidate = current.join("ravenbsd-resources");
        if candidate.join("donors.lock").is_file() {
            return candidate;
        }
        directory = current.parent().map(Path::to_path_buf);
    }
    panic!("cannot find ravenbsd-resources");
}

fn run(extra: &[&str]) -> Output {
    let root = repo_root();
    let res = resources();
    let mut args = vec![
        "verify",
        "--profile",
        "freebsd",
        "--root",
        root.to_str().unwrap(),
        "--resources",
        res.to_str().unwrap(),
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
    let output = run(&[
        "--target",
        "x86_64",
        "--freebsd",
        "/nonexistent/s721-freebsd",
    ]);
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
    let empty = run(&["--target", "x86_64", "--families", ""]);
    assert_eq!(empty.status.code(), Some(2), "{}", stderr(&empty));
    assert!(
        stderr(&empty).contains("at least one family"),
        "{}",
        stderr(&empty)
    );
    let typo = run(&["--target", "x86_64", "--families", "errnoo"]);
    assert_eq!(typo.status.code(), Some(2), "{}", stderr(&typo));
    assert!(
        stderr(&typo).contains("unknown family"),
        "{}",
        stderr(&typo)
    );
}

#[test]
fn a_nonexistent_abi_root_is_not_a_pass() {
    let output = run(&["--target", "x86_64", "--abi", "/nonexistent/s721-abi"]);
    assert_ne!(output.status.code(), Some(0), "{}", stderr(&output));
}

#[test]
fn riscv64_is_not_a_freebsd_target() {
    let output = run(&["--target", "riscv64"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("unknown FreeBSD --target"),
        "{}",
        stderr(&output)
    );
}
