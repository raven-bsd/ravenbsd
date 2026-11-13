// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integration tests for `verify` (L2, H1-H3, the musl measurement, identity, headers).

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

fn verify(extra: &[&str]) -> Output {
    let root = repo_root();
    let res = resources();
    let mut args = vec![
        "verify",
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

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn assert_ok_or_skip(output: &Output) {
    if output.status.code() == Some(77) {
        eprintln!("SKIP: {}", String::from_utf8_lossy(&output.stderr));
        return;
    }
    assert!(
        output.status.success(),
        "verify failed:\n{}{}",
        stdout(output),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn verify_all_succeeds() {
    let output = verify(&["--target", "all"]);
    assert_ok_or_skip(&output);
}

#[test]
fn verify_prints_the_musl_probe_table() {
    let output = verify(&["--target", "all"]);
    if output.status.code() == Some(77) {
        return;
    }
    let text = stdout(&output);
    assert!(text.contains("musl probe (size/align)"), "{text}");
    assert!(text.contains("jmp_buf"), "{text}");
    assert!(text.contains("200/8"), "{text}");
    assert!(text.contains("392/8"), "{text}");
    assert!(text.contains("144/8"), "{text}");
    assert!(text.contains("differs"), "{text}");
}

#[test]
fn verify_reports_the_h3_difference_set() {
    let output = verify(&["--target", "all"]);
    if output.status.code() == Some(77) {
        return;
    }
    assert!(
        stdout(&output).contains("H3: aarch64 fcntl difference set exactly"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn verify_reports_the_compiler_identity_and_headers() {
    let output = verify(&["--target", "all"]);
    if output.status.code() == Some(77) {
        return;
    }
    let text = stdout(&output);
    assert!(text.contains("compiler identity"), "{text}");
    assert!(
        text.contains("C headers: 14 headers x 3 target(s)"),
        "{text}"
    );
}

#[test]
fn verify_single_target_succeeds() {
    for target in ["x86_64", "aarch64", "riscv64"] {
        let output = verify(&["--target", target]);
        assert_ok_or_skip(&output);
    }
}

#[test]
fn verify_rejects_an_unknown_target() {
    let output = verify(&["--target", "mips"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown --target"));
}

#[test]
fn a_missing_tool_is_a_skip_unless_required() {
    let output = Command::new(env!("CARGO_BIN_EXE_raven-libc-hdrgen"))
        .args([
            "verify",
            "--root",
            repo_root().to_str().unwrap(),
            "--resources",
            resources().to_str().unwrap(),
            "--clang",
            "definitely-not-a-clang",
        ])
        .output()
        .expect("run");
    if std::env::var("RL_ABI_REQUIRED").is_ok_and(|value| value == "1") {
        assert!(!output.status.success());
    } else {
        assert_eq!(output.status.code(), Some(77));
        assert!(String::from_utf8_lossy(&output.stderr).contains("SKIP"));
    }
}
