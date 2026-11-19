// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn parses_the_explicit_test_paths() {
    let options = parse(&args(&[
        "--dry-run",
        "--image",
        "build/a.img",
        "--device",
        "/dev/sdb",
    ]))
    .unwrap();
    assert!(options.dry_run);
    assert_eq!(options.image.unwrap(), PathBuf::from("build/a.img"));
    assert_eq!(options.device.unwrap(), PathBuf::from("/dev/sdb"));
}

#[test]
fn yes_requires_an_image_and_device() {
    assert!(parse(&args(&["--yes"])).is_err());
    let options = parse(&args(&[
        "--yes", "--image", "a.img", "--device", "/dev/sdb",
    ]))
    .unwrap();
    assert!(options.yes);
}

#[test]
fn helper_needs_both_paths() {
    assert!(parse(&args(&["--helper-write", "--image", "a.img"])).is_err());
}

#[test]
fn test_fixture_overrides_are_parsed() {
    let options = parse(&args(&[
        "--list",
        "--sys-root",
        "tests/flash/sys",
        "--mounts",
        "tests/flash/mounts",
        "--root-device",
        "sda",
    ]))
    .unwrap();
    assert_eq!(options.sys_root, PathBuf::from("tests/flash/sys"));
    assert_eq!(options.mounts, PathBuf::from("tests/flash/mounts"));
    assert_eq!(options.root_device.as_deref(), Some("sda"));
}

#[test]
fn unknown_options_are_rejected() {
    let error = parse(&args(&["--wat"])).unwrap_err();
    assert!(error.to_string().contains("--wat"));
}
