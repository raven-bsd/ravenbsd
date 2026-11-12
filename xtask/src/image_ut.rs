// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for image assembly helpers.

use std::path::Path;

use super::*;

#[test]
fn boot_toml_uses_profile_and_override() {
    let profile = Profile::load(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
        "base",
    )
    .unwrap();
    assert_eq!(
        boot_toml(&profile, None),
        "cmdline = \"console=ttyS0 test-exit=1\"\nlog_level = \"info\"\n"
    );
    assert_eq!(
        boot_toml(&profile, Some("console=ttyS0 \"quoted\"")),
        "cmdline = \"console=ttyS0 \\\"quoted\\\"\"\nlog_level = \"info\"\n"
    );
}

#[test]
fn default_output_is_under_build_arch() {
    assert_eq!(
        default_output(Path::new("/repo"), Arch::X86_64),
        Path::new("/repo/build/x86_64/ravenbsd-x86_64.img")
    );
}

#[test]
fn relative_overrides_are_anchored_at_the_root() {
    let root = Path::new("/repo");
    assert_eq!(
        resolve_override(root, Path::new("build/fixtures/s24-spawn/initfs.img")),
        Path::new("/repo/build/fixtures/s24-spawn/initfs.img")
    );
    assert_eq!(
        resolve_override(root, Path::new("/tmp/kernel.elf")),
        Path::new("/tmp/kernel.elf")
    );
}

#[test]
fn platform_parses_generic_pi4_and_rejects_unknown() {
    assert!(matches!(Platform::parse("generic"), Ok(Platform::Generic)));
    assert!(matches!(Platform::parse("pi4"), Ok(Platform::Pi4)));
    assert!(Platform::parse("raspi4b").is_err());
}
