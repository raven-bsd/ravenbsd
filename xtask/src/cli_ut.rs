// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the command-line parser.

use super::*;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

#[test]
fn no_arguments_prints_help() {
    assert_eq!(parse_args(&[]).unwrap(), Command::Help);
}

#[test]
fn help_flags_print_help() {
    assert_eq!(parse_args(&args(&["--help"])).unwrap(), Command::Help);
    assert_eq!(parse_args(&args(&["-h"])).unwrap(), Command::Help);
    assert_eq!(parse_args(&args(&["help"])).unwrap(), Command::Help);
}

#[test]
fn help_lists_every_make_target() {
    for target in [
        "help",
        "setup",
        "doctor",
        "check",
        "guard",
        "kernel",
        "loader",
        "dev",
        "release",
        "image",
        "run",
        "test",
        "world",
        "provenance",
        "toolchain",
        "clean",
        "distclean",
    ] {
        assert!(HELP.contains(target), "help text is missing `{target}`");
    }
}

#[test]
fn unknown_command_is_an_error() {
    assert!(parse_args(&args(&["nonsense"])).is_err());
}

#[test]
fn build_user_accepts_multiple_packages() {
    let command = parse_args(&args(&[
        "build",
        "user",
        "--arch",
        "x86_64",
        "--release",
        "-p",
        "raven-init",
        "-p",
        "raven-echo",
    ]))
    .unwrap();
    assert_eq!(
        command,
        Command::Build {
            component: Component::User,
            arch: Arch::X86_64,
            release: true,
            packages: vec!["raven-init".into(), "raven-echo".into()],
        }
    );
}

#[test]
fn build_requires_arch_and_component() {
    assert!(parse_args(&args(&["build", "kernel"])).is_err());
    assert!(parse_args(&args(&["build", "--arch", "x86_64"])).is_err());
}

#[test]
fn build_rejects_unknown_architecture() {
    assert!(parse_args(&args(&["build", "kernel", "--arch", "sparc64"])).is_err());
}

#[test]
fn check_fast_flag_is_parsed() {
    assert_eq!(
        parse_args(&args(&["check"])).unwrap(),
        Command::Check { fast: false }
    );
    assert_eq!(
        parse_args(&args(&["check", "--fast"])).unwrap(),
        Command::Check { fast: true }
    );
    assert!(parse_args(&args(&["check", "--slow"])).is_err());
}

#[test]
fn image_options_are_parsed() {
    let command = parse_args(&args(&[
        "image",
        "--arch",
        "aarch64",
        "--profile",
        "tiny",
        "--cmdline",
        "console=ttyS0",
        "--output",
        "build/out.img",
    ]))
    .unwrap();
    assert_eq!(
        command,
        Command::Image {
            arch: Arch::Aarch64,
            platform: image::Platform::Generic,
            profile: "tiny".into(),
            cmdline: Some("console=ttyS0".into()),
            initfs: None,
            kernel: None,
            output: Some("build/out.img".into()),
        }
    );
}

#[test]
fn image_platform_is_parsed() {
    let command = parse_args(&args(&["image", "--arch", "aarch64", "--platform", "pi4"])).unwrap();
    assert_eq!(
        command,
        Command::Image {
            arch: Arch::Aarch64,
            platform: image::Platform::Pi4,
            profile: "base".into(),
            cmdline: None,
            initfs: None,
            kernel: None,
            output: None,
        }
    );
    assert!(parse_args(&args(&["image", "--arch", "aarch64", "--platform", "x"])).is_err());
}

#[test]
fn image_initfs_and_kernel_overrides_are_parsed() {
    let command = parse_args(&args(&[
        "image",
        "--arch",
        "x86_64",
        "--initfs",
        "build/fixtures/s24-spawn/initfs.img",
        "--kernel",
        "build/fixtures/testkernel/kernel.elf",
    ]))
    .unwrap();
    assert_eq!(
        command,
        Command::Image {
            arch: Arch::X86_64,
            platform: image::Platform::Generic,
            profile: "base".into(),
            cmdline: None,
            initfs: Some("build/fixtures/s24-spawn/initfs.img".into()),
            kernel: Some("build/fixtures/testkernel/kernel.elf".into()),
            output: None,
        }
    );
}

#[test]
fn run_keeps_extra_arguments_after_separator() {
    let command = parse_args(&args(&[
        "run",
        "--arch",
        "x86_64",
        "--no-build",
        "--",
        "-serial",
        "stdio",
    ]))
    .unwrap();
    assert_eq!(
        command,
        Command::Run {
            arch: Arch::X86_64,
            profile: "base".into(),
            no_build: true,
            extra: vec!["-serial".into(), "stdio".into()],
        }
    );
}

#[test]
fn qtest_arguments_are_forwarded_verbatim() {
    let command = parse_args(&args(&["qtest", "--all", "--arch", "aarch64"])).unwrap();
    assert_eq!(
        command,
        Command::Qtest {
            args: vec!["--all".into(), "--arch".into(), "aarch64".into()],
        }
    );
}

#[test]
fn guard_rejects_extra_arguments() {
    assert!(parse_args(&args(&["guard", "now"])).is_err());
}

#[test]
fn initfs_defaults_to_base_profile() {
    let command = parse_args(&args(&["initfs", "--arch", "riscv64"])).unwrap();
    assert_eq!(
        command,
        Command::Initfs {
            arch: Arch::Riscv64,
            profile: "base".into(),
        }
    );
}

#[test]
fn parses_libc_lint() {
    let command = parse_args(&args(&["libc-lint", "layout", "--json"])).unwrap();
    assert_eq!(
        command,
        Command::Libc {
            name: "libc-lint".into(),
            args: vec!["layout".into(), "--json".into()],
        }
    );
}

#[test]
fn parses_libc_oracle() {
    let command = parse_args(&args(&["libc-oracle"])).unwrap();
    assert_eq!(
        command,
        Command::Libc {
            name: "libc-oracle".into(),
            args: Vec::new(),
        }
    );
}

#[test]
fn help_lists_libc_commands() {
    for name in [
        "libc-lint",
        "libc-oracle",
        "libc-headers",
        "libc-pack",
        "libc-size",
        "libc-world",
        "libc-census",
    ] {
        assert!(HELP.contains(name), "help is missing {name}");
    }
}
