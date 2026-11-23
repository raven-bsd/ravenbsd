// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::path::Path;

use super::*;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn the_selected_set_is_the_current_slice() {
    assert_eq!(SELECTED.len(), 31);
    let names: Vec<&str> = SELECTED.iter().map(|(name, _)| *name).collect();
    for name in [
        "ls", "cat", "cp", "mv", "rm", "ln", "mkdir", "rmdir", "touch", "stat", "df", "du", "echo",
        "pwd", "basename", "dirname", "true", "false", "sleep", "date", "head", "tail", "wc",
        "sort", "cut", "tr", "env", "id", "chmod", "chown", "tty",
    ] {
        assert!(names.contains(&name), "{name}");
    }
}

#[test]
fn every_package_is_named_after_its_unix_name() {
    for (name, package) in SELECTED {
        assert_eq!(*package, format!("uu_{name}"));
    }
}

#[test]
fn package_lookup_works() {
    assert_eq!(package_for("ls"), Some("uu_ls"));
    assert_eq!(package_for("chown"), Some("uu_chown"));
    assert_eq!(package_for("nope"), None);
}

#[test]
fn the_arch_triples_are_the_raven_std_targets() {
    assert_eq!(arch_triple("x86_64"), Some("x86_64-unknown-ravenbsd"));
    assert_eq!(arch_triple("aarch64"), Some("aarch64-unknown-ravenbsd"));
    assert_eq!(arch_triple("riscv64"), Some("riscv64gc-unknown-ravenbsd"));
    assert_eq!(arch_triple("sparc"), None);
}

#[test]
fn the_default_options_are_a_release_x86_64_build() {
    let options = parse_options(&[]).expect("options");
    assert_eq!(options.arch, "x86_64");
    assert!(options.release);
    assert!(!options.list);
}

#[test]
fn debug_selects_the_debug_profile() {
    let options = parse_options(&args(&["--debug"])).expect("options");
    assert!(!options.release);
    assert!(!command_args(Path::new("/r"), &options).contains(&"--release".to_string()));
}

#[test]
fn release_and_list_parse() {
    let options =
        parse_options(&args(&["--arch", "aarch64", "--release", "--list"])).expect("options");
    assert_eq!(options.arch, "aarch64");
    assert!(options.release);
    assert!(options.list);
}

#[test]
fn an_unknown_arch_is_rejected() {
    assert!(parse_options(&args(&["--arch", "sparc"])).is_err());
}

#[test]
fn an_unknown_option_is_rejected() {
    assert!(parse_options(&args(&["--host"])).is_err());
}

#[test]
fn a_missing_arch_value_is_rejected() {
    assert!(parse_options(&args(&["--arch"])).is_err());
}

#[test]
fn the_command_uses_the_raven_toolchain_and_target() {
    let options = parse_options(&[]).expect("options");
    let vector = command_args(Path::new("/r"), &options);
    assert_eq!(vector[0], "+ravenbsd");
    assert_eq!(vector[1], "build");
    assert!(vector.contains(&"--locked".to_string()));
    assert!(vector.contains(&"--target".to_string()));
    assert!(vector.contains(&"x86_64-unknown-ravenbsd".to_string()));
    assert!(vector.contains(&"-p".to_string()));
    assert!(vector.contains(&"uu_ls".to_string()));
    assert!(vector.contains(&"--release".to_string()));
}

#[test]
fn the_command_has_every_selected_package_once() {
    let options = parse_options(&[]).expect("options");
    let vector = command_args(Path::new("/r"), &options);
    let packages = vector.iter().filter(|arg| arg.starts_with("uu_")).count();
    assert_eq!(packages, SELECTED.len());
}

#[test]
fn release_adds_the_release_flag() {
    let options = parse_options(&args(&["--release"])).expect("options");
    assert!(command_args(Path::new("/r"), &options).contains(&"--release".to_string()));
}

#[test]
fn the_artifact_path_is_under_the_isolated_target_dir() {
    let options = parse_options(&[]).expect("options");
    assert_eq!(
        artifact_path(Path::new("/r"), &options, "ls"),
        Path::new("/r/build/cargo/uutils-x86_64/x86_64-unknown-ravenbsd/release/ls")
    );
}

#[test]
fn the_install_path_is_the_image_staging_directory() {
    let options = parse_options(&[]).expect("options");
    assert_eq!(
        install_path(Path::new("/r"), &options, "stat"),
        Path::new("/r/build/x86_64/uutils/stat")
    );
}
