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

#[test]
fn fat_with_cmdline_preserves_files_and_adds_the_cmdline() {
    let mut builder = FatImageBuilder::new(64 * 1024 * 1024).unwrap();
    builder
        .add_file("config.txt", b"arm_64bit=1\nkernel=kernel8.img\n")
        .unwrap();
    let fat = builder.finish().unwrap();
    let fat = fat_with_cmdline(&fat, "console=ttyAMA0 loglevel=info", Some(0)).unwrap();

    let mut buffer = fat.clone();
    let filesystem = fatfs::FileSystem::new(
        std::io::Cursor::new(buffer.as_mut_slice()),
        fatfs::FsOptions::new(),
    )
    .unwrap();
    let root = filesystem.root_dir();
    let mut config = String::new();
    root.open_file("config.txt")
        .unwrap()
        .read_to_string(&mut config)
        .unwrap();
    assert_eq!(config, "arm_64bit=1\nkernel=kernel8.img\n");
    let mut cmdline = String::new();
    root.open_file("cmdline.txt")
        .unwrap()
        .read_to_string(&mut cmdline)
        .unwrap();
    assert_eq!(cmdline, "console=ttyAMA0 loglevel=info\n");
}
