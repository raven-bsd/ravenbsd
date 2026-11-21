// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for image assembly helpers.

use std::fs;
use std::path::Path;

use super::*;

#[test]
fn dotted_product_profile_name_loads() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let profile = Profile::load(root, "0.1.0-ufs").expect("0.1.0-ufs profile");
    assert_eq!(profile.name, "0.1.0-ufs");
    let boot = Profile::load(root, "0.1.0-ufs-boot").expect("0.1.0-ufs-boot profile");
    assert_eq!(boot.name, "0.1.0-ufs-boot");
    assert!(ufs_boot_profile(root, "0.1.0-ufs").is_some());
}

#[test]
fn ufs_root_is_selected_by_a_spec_and_a_boot_profile() {
    let root = std::env::temp_dir().join(format!("raven-image-ufs-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("profiles/demo")).unwrap();
    fs::write(root.join("profiles/demo/rootfs.toml"), "size = 1048576\n").unwrap();
    assert_eq!(ufs_boot_profile(&root, "demo"), None);

    fs::write(
        root.join("profiles/demo-boot.toml"),
        "name = \"demo-boot\"\n[boot]\ncmdline = \"console=ttyS0\"\nlog_level = \"info\"\n",
    )
    .unwrap();
    assert_eq!(
        ufs_boot_profile(&root, "demo"),
        Some("demo-boot".to_string())
    );

    fs::remove_file(root.join("profiles/demo/rootfs.toml")).unwrap();
    assert_eq!(ufs_boot_profile(&root, "demo"), None);
    let _ = fs::remove_dir_all(&root);
}

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
fn replace_initfs_file_replaces_one_entry() {
    let mut builder = raven_initfs::InitfsBuilder::new();
    builder.add_file("/etc/keep", b"keep", 0o644).unwrap();
    builder.add_file("/etc/replace", b"old", 0o644).unwrap();
    let image = builder.build();
    let replaced = replace_initfs_file(&image, "/etc/replace", b"new", 0o600, 0).expect("replace");
    let parsed = raven_initfs::Initfs::parse(&replaced).expect("parse");
    assert_eq!(parsed.get("/etc/keep").unwrap().data, b"keep");
    assert_eq!(parsed.get("/etc/replace").unwrap().data, b"new");
    assert_eq!(parsed.get("/etc/replace").unwrap().mode, 0o600);
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

#[test]
fn release_profiles_request_release_kernel_and_loader() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let release = Profile::load(root, "release").unwrap();
    assert!(build::image_build_release(Arch::X86_64, &release));
    let base = Profile::load(root, "base").unwrap();
    assert!(!build::image_build_release(Arch::X86_64, &base));
    let pi4 = Profile::load(root, "pi4-release").unwrap();
    assert!(build::image_build_release(Arch::Aarch64, &pi4));
}
