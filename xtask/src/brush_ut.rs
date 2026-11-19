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
fn brush_stages_the_arch_path_only() {
    assert_eq!(staged_for(Arch::X86_64), "build/x86_64/brush");
    assert_eq!(staged_for(Arch::Aarch64), "build/aarch64/brush");
    assert_eq!(staged_for(Arch::Riscv64), "build/riscv64/brush");
}

#[test]
fn build_brush_accepts_the_check_only_flag() {
    let options = parse_build(&args(&["--arch", "aarch64", "--check"])).unwrap();
    assert_eq!(options.arch, Arch::Aarch64);
    assert!(options.check);
}

#[test]
fn aarch64_and_x86_select_their_brush_linker_scripts() {
    let aarch64 = linker_for(Arch::Aarch64).unwrap();
    assert!(aarch64.ends_with("xtask/brush-aarch64.ld"), "{aarch64:?}");
    let x86 = linker_for(Arch::X86_64).unwrap();
    assert!(x86.ends_with("xtask/brush-x86_64.ld"), "{x86:?}");
    assert!(linker_for(Arch::Riscv64).is_none());
}

#[test]
fn a_missing_linker_script_fails_instead_of_using_lld_defaults() {
    let dir = std::env::temp_dir().join(format!("raven-brush-k169-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let error = selected_linker(&dir, Arch::Aarch64).unwrap_err();
    assert!(
        error.to_string().contains("missing brush linker script"),
        "{error}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
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
