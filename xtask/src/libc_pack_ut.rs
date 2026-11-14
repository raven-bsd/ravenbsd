// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Command-vector tests of `libc-pack` (C60 section 6 step 4). No process is spawned: the
//! tests compare the parsed `Options` and the exact `CommandSpec` the pipeline would run.

use std::path::Path;

use super::{Arch, build_spec, parse};
use crate::process::CommandSpec;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn defaults_are_x86_64_server_and_build_dir() {
    let options = parse(&[]).expect("empty vector");
    assert_eq!(options.arch, Arch::X86_64);
    assert_eq!(options.set, "server");
    assert!(!options.check);
    assert!(options.probe.is_none());
    assert!(!options.host_run);
    assert!(!options.json);
    assert_eq!(options.out, Path::new("build/x86_64"));
}

#[test]
fn every_flag_parses_in_any_order() {
    let options = parse(&args(&[
        "--json",
        "--out",
        "target/pack",
        "--host-run",
        "--probe",
        "all",
        "--check",
        "--set",
        "tiny",
        "--arch",
        "aarch64",
    ]))
    .expect("all flags");
    assert_eq!(options.arch, Arch::Aarch64);
    assert_eq!(options.set, "tiny");
    assert!(options.check);
    assert_eq!(options.probe.as_deref(), Some("all"));
    assert!(options.host_run);
    assert!(options.json);
    assert_eq!(options.out, Path::new("target/pack"));
}

#[test]
fn the_short_name_drives_the_default_out_dir() {
    for (arch, dir) in [
        (Arch::X86_64, "build/x86_64"),
        (Arch::Aarch64, "build/aarch64"),
        (Arch::Riscv64, "build/riscv64"),
    ] {
        let options = parse(&args(&["--arch", arch.short()])).expect("arch");
        assert_eq!(options.out, Path::new(dir));
        assert_eq!(options.arch, arch);
    }
}

#[test]
fn every_unknown_arch_is_rejected() {
    for value in ["sparc64", "X86_64", "riscv", ""] {
        assert!(parse(&args(&["--arch", value])).is_err(), "{value}");
    }
}

#[test]
fn every_unknown_set_is_rejected() {
    for value in ["base", "Server", ""] {
        assert!(parse(&args(&["--set", value])).is_err(), "{value}");
    }
}

#[test]
fn every_flag_without_its_value_is_rejected() {
    for flag in ["--arch", "--set", "--probe", "--out"] {
        assert!(parse(&args(&[flag])).is_err(), "{flag}");
    }
}

#[test]
fn an_unknown_flag_is_rejected() {
    assert!(parse(&args(&["--arch=x86_64"])).is_err());
    assert!(parse(&args(&["--clean"])).is_err());
    assert!(parse(&args(&["x86_64"])).is_err());
}

#[test]
fn x86_64_build_spec_has_the_exact_cargo_vector() {
    let spec = build_spec(
        Path::new("/repo"),
        Arch::X86_64,
        "libc-a-server",
        "server",
        Path::new("build/x86_64/cargo/libc-a"),
    );
    let expected = CommandSpec::new(
        "cargo",
        vec![
            "rustc",
            "-p",
            "raven-libc",
            "--crate-type",
            "staticlib",
            "--profile",
            "libc-a-server",
            "--target",
            "x86_64-unknown-none",
            "--target-dir",
            "build/x86_64/cargo/libc-a",
            "--features",
            "server",
            "--",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        "/repo",
    )
    .env(
        "RUSTFLAGS",
        "-C relocation-model=pic -C code-model=small --remap-path-prefix=/repo=/ravenbsd",
    )
    .env_remove("CARGO_ENCODED_RUSTFLAGS");
    assert_eq!(spec.program, expected.program);
    assert_eq!(spec.args, expected.args);
    assert_eq!(spec.env, expected.env);
    assert_eq!(spec.env_remove, expected.env_remove);
    assert_eq!(spec.cwd, expected.cwd);
}

#[test]
fn aarch64_build_spec_drops_the_x86_only_code_model_flag() {
    let spec = build_spec(
        Path::new("/repo"),
        Arch::Aarch64,
        "libc-a",
        "tiny",
        Path::new("out"),
    );
    assert!(
        spec.args
            .contains(&"aarch64-unknown-none-softfloat".to_string())
    );
    assert!(spec.args.contains(&"tiny".to_string()));
    let flags = spec
        .env
        .iter()
        .find(|(key, _)| key == "RUSTFLAGS")
        .map(|(_, value)| value.as_str())
        .expect("RUSTFLAGS");
    assert!(!flags.contains("code-model"));
    assert!(flags.contains("relocation-model=pic"));
    assert!(flags.contains("lto=thin"));
    assert!(flags.contains("embed-bitcode=yes"));
    assert_eq!(spec.env_remove, vec!["CARGO_ENCODED_RUSTFLAGS".to_string()]);
}

#[test]
fn riscv64_build_spec_targets_the_bare_metal_triple() {
    let spec = build_spec(
        Path::new("/repo"),
        Arch::Riscv64,
        "libc-a",
        "server",
        Path::new("out"),
    );
    assert!(
        spec.args
            .contains(&"riscv64gc-unknown-none-elf".to_string())
    );
    let flags = spec
        .env
        .iter()
        .find(|(key, _)| key == "RUSTFLAGS")
        .map(|(_, value)| value.as_str())
        .expect("RUSTFLAGS");
    assert!(flags.contains("relocation-model=pic"));
    assert!(flags.contains("lto=thin"));
    assert!(flags.contains("embed-bitcode=yes"));
    assert_eq!(spec.program, "cargo");
    assert_eq!(spec.cwd, Path::new("/repo"));
}

#[test]
fn the_probe_value_is_kept_verbatim() {
    let options = parse(&args(&["--probe", "P0"])).expect("probe");
    assert_eq!(options.probe.as_deref(), Some("P0"));
    let options = parse(&args(&["--probe", "all"])).expect("probe");
    assert_eq!(options.probe.as_deref(), Some("all"));
}
