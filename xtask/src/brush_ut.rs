// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn build_brush_requires_an_arch() {
    assert!(parse_build(&args(&[])).is_err());
    assert!(parse_build(&args(&["--release"])).is_err());
}

#[test]
fn build_brush_defaults_to_release() {
    let options = parse_build(&args(&["--arch", "x86_64"])).unwrap();
    assert_eq!(options.arch, Arch::X86_64);
    assert!(options.release);
    assert!(
        !parse_build(&args(&["--arch", "x86_64", "--debug"]))
            .unwrap()
            .release
    );
}

#[test]
fn build_brush_maps_every_target() {
    assert_eq!(target_for(Arch::X86_64), "x86_64-unknown-ravenbsd");
    assert_eq!(target_for(Arch::Aarch64), "aarch64-unknown-ravenbsd");
    assert_eq!(target_for(Arch::Riscv64), "riscv64gc-unknown-ravenbsd");
}

#[test]
fn build_brush_rejects_an_unknown_flag() {
    let error = parse_build(&args(&["--arch", "x86_64", "--wat"])).unwrap_err();
    assert!(error.to_string().contains("--wat"));
}

#[test]
fn only_the_build_brush_pair_is_intercepted() {
    assert!(try_run(&args(&["build", "user", "--arch", "x86_64"])).is_none());
    assert!(try_run(&args(&["qtest", "--case", "shell-brush"])).is_none());
}

#[test]
fn shell_brush_is_rewritten_to_the_payload_path() {
    let rewritten = rewrite_qtest(args(&[
        "qtest",
        "--case",
        "shell-brush",
        "--arch",
        "x86_64",
    ]));
    assert_eq!(rewritten[2], "tests/brush/shell-brush.toml");
    assert_eq!(rewritten[3], "--arch");
}

#[test]
fn other_qtest_cases_are_untouched() {
    let original = args(&["qtest", "--case", "shell-session"]);
    assert_eq!(rewrite_qtest(original.clone()), original);
}
