// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the K27 update planner.

use super::{
    BootMeta, FixtureHealth, Options, Platform, Slot, build_fixture, detect_layout,
    ensure_pi4_kernel, inject_health, parse, parse_boot_meta, pi4_trial_from_flat, stage_pi4_esp,
    target_slot, token_for,
};
use crate::fat::FatImageBuilder;
use crate::gpt::SECTOR_SIZE;
use crate::image;
use raven_initfs::{Initfs, InitfsBuilder};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

fn temp_path(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("xtask-update-{label}-{}", std::process::id()));
    let _ = fs::remove_file(&path);
    path
}

#[test]
fn legacy_meta_defaults_to_slot_a() {
    let meta = parse_boot_meta(b"cmdline = \"console=ttyS0\"\nlog_level = \"warn\"\n").unwrap();
    assert_eq!(meta.cmdline, "console=ttyS0");
    assert_eq!(meta.log_level, "warn");
    assert_eq!(meta.slot, Slot::A);
    assert_eq!(meta.trial, None);
    assert_eq!(meta.attempts, 0);
    assert!(!meta.has_slot_config);
}

#[test]
fn slot_meta_round_trips() {
    let meta = BootMeta {
        cmdline: String::from("console=ttyS0 test-exit=1"),
        log_level: String::from("info"),
        slot: Slot::A,
        trial: Some(Slot::B),
        trial_token: Some(0x1234_5678),
        attempts: 3,
        has_slot_config: true,
    };
    let parsed = parse_boot_meta(meta.render().as_bytes()).unwrap();
    assert_eq!(parsed, meta);
}

#[test]
fn target_is_the_other_slot() {
    let legacy = BootMeta::default();
    assert_eq!(target_slot(&legacy, None), Slot::B);

    let trial = BootMeta {
        slot: Slot::A,
        trial: Some(Slot::B),
        ..BootMeta::default()
    };
    assert_eq!(target_slot(&trial, None), Slot::A);
    assert_eq!(target_slot(&trial, Some(Slot::B)), Slot::B);
}

#[test]
fn fixture_contains_both_slots_and_a_bad_trial_kernel() {
    let kernel = temp_path("fixture-kernel");
    let output = temp_path("fixture.img");
    fs::write(&kernel, b"kernel bytes").unwrap();
    let options = Options {
        kernel: Some(kernel.clone()),
        ..Options::default()
    };
    build_fixture(&options, &output).unwrap();

    let bytes = fs::read(&output).unwrap();
    let mut esp =
        vec![0u8; usize::try_from(super::image::ESP_SECTORS * super::SECTOR_SIZE).unwrap()];
    esp.copy_from_slice(
        &bytes[usize::try_from(super::image::ESP_START_LBA * super::SECTOR_SIZE).unwrap()
            ..usize::try_from(
                (super::image::ESP_START_LBA + super::image::ESP_SECTORS) * super::SECTOR_SIZE,
            )
            .unwrap()],
    );
    let filesystem =
        fatfs::FileSystem::new(std::io::Cursor::new(esp), fatfs::FsOptions::new()).unwrap();
    let root = filesystem.root_dir();
    let mut boot_toml = String::new();
    root.open_file("raven/boot.toml")
        .unwrap()
        .read_to_string(&mut boot_toml)
        .unwrap();
    let meta = parse_boot_meta(boot_toml.as_bytes()).unwrap();
    assert_eq!(meta.slot, Slot::A);
    assert_eq!(meta.trial, Some(Slot::B));
    assert_eq!(meta.attempts, 2);

    let mut slot_a = Vec::new();
    root.open_file("raven/slot-a/kernel.elf")
        .unwrap()
        .read_to_end(&mut slot_a)
        .unwrap();
    assert_eq!(slot_a, b"kernel bytes");

    let mut slot_b = Vec::new();
    root.open_file("raven/slot-b/kernel.elf")
        .unwrap()
        .read_to_end(&mut slot_b)
        .unwrap();
    assert_eq!(slot_b, b"not an ELF64 image\n");

    let _ = fs::remove_file(output);
    let _ = fs::remove_file(kernel);
}

#[test]
fn fixture_root_staging_writes_the_health_record() {
    let directory = std::env::temp_dir().join(format!("xtask-update-root-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    let staging = directory.join("root");
    fs::create_dir_all(staging.join("etc/services.d")).unwrap();
    fs::write(
        staging.join("etc/services.d/probe.toml"),
        "name = \"probe\"\nexec = \"/sbin/probe\"\n",
    )
    .unwrap();
    let kernel = directory.join("kernel.elf");
    fs::write(&kernel, b"kernel bytes").unwrap();
    let output = directory.join("fixture.img");
    let options = Options {
        fixture: Some(output.clone()),
        health: Some(FixtureHealth::Trial),
        kernel: Some(kernel),
        root_staging: Some(staging.clone()),
        root_size: 1024 * 1024,
        ..Options::default()
    };
    build_fixture(&options, &output).unwrap();

    let mut esp = super::read_esp(&output).unwrap();
    let boot = super::fat_read(&mut esp, "raven/boot.toml")
        .unwrap()
        .unwrap();
    let meta = parse_boot_meta(&boot).unwrap();
    let token = meta.trial_token.unwrap();
    assert_eq!(meta.trial, Some(Slot::B));
    assert_eq!(
        super::fat_read(&mut esp, "raven/slot-b/kernel.elf")
            .unwrap()
            .unwrap(),
        b"kernel bytes"
    );
    let health = fs::read_to_string(staging.join("etc/boot-health")).unwrap();
    assert!(health.contains("slot = \"b\"\n"));
    assert!(health.contains(&format!("token = \"{token:08x}\"\n")));
    assert!(health.contains("lba = \"34\"\n"));

    let bytes = fs::read(&output).unwrap();
    let marker = &bytes[super::MARKER_LBA as usize * SECTOR_SIZE as usize..][..512];
    assert_eq!(marker[11], super::HEALTH_STATE_TRIAL);
    let ufs_start = (image::ESP_START_LBA + image::ESP_SECTORS) * SECTOR_SIZE;
    let ufs = &bytes[ufs_start as usize..(ufs_start + 1024 * 1024) as usize];
    assert!(ufs.iter().any(|byte| *byte != 0));

    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn health_record_is_injected_into_the_trial_initfs() {
    let mut builder = InitfsBuilder::new();
    builder.add_file("/init", b"init", 0o755).unwrap();
    let original = builder.build();
    let out = inject_health(&original, Slot::B, 0xdead_beef).unwrap();
    let parsed = Initfs::parse(&out).unwrap();
    assert_eq!(parsed.get("/init").unwrap().data, b"init");
    let health = parsed.get("/etc/boot-health").unwrap();
    let text = std::str::from_utf8(health.data).unwrap();
    assert!(text.contains("slot = \"b\"\n"));
    assert!(text.contains("token = \"deadbeef\"\n"));
    assert!(text.contains("lba = \"34\"\n"));
}

#[test]
fn token_changes_with_the_payload() {
    assert_eq!(token_for(b"a", b"b"), token_for(b"a", b"b"));
    assert_ne!(token_for(b"a", b"b"), token_for(b"a", b"c"));
}

#[test]
fn pi4_platform_requires_aarch64() {
    let error = parse(&["--platform".to_string(), "pi4".to_string()]).unwrap_err();
    assert!(error.to_string().contains("requires --arch aarch64"));
}

#[test]
fn pi4_defaults_to_the_pi4_profile() {
    let options = parse(&[
        "--platform".to_string(),
        "pi4".to_string(),
        "--arch".to_string(),
        "aarch64".to_string(),
    ])
    .unwrap()
    .unwrap();
    assert_eq!(options.profile, "pi4");
}

#[test]
fn layout_detection_reads_the_boot_partition() {
    let size = usize::try_from(image::ESP_SECTORS * SECTOR_SIZE).unwrap();
    let mut builder = FatImageBuilder::new(size).unwrap();
    builder.add_dir("raven").unwrap();
    builder.add_file("raven/kernel.elf", b"elf").unwrap();
    let mut esp = builder.finish().unwrap();
    assert_eq!(detect_layout(&mut esp).unwrap(), Platform::Generic);

    let mut builder = FatImageBuilder::new(size).unwrap();
    builder.add_file("kernel8.img", b"flat").unwrap();
    let mut esp = builder.finish().unwrap();
    assert_eq!(detect_layout(&mut esp).unwrap(), Platform::Pi4);
}

#[test]
fn pi4_kernel_entry_is_checked() {
    let elf = aarch64_elf();
    ensure_pi4_kernel(&elf).unwrap();
    let mut uefi = elf;
    uefi[24..32].copy_from_slice(&0xffff_ffff_8020_0000u64.to_le_bytes());
    assert!(ensure_pi4_kernel(&uefi).is_err());
}

#[test]
fn pi4_update_stages_slots_and_metadata() {
    let mut init_builder = InitfsBuilder::new();
    init_builder.add_file("/init", b"init", 0o755).unwrap();
    let initfs = init_builder.build();
    let legacy = b"legacy flat image".to_vec();
    let flat = [legacy.as_slice(), initfs.as_slice()].concat();
    let (trial, token) = pi4_trial_from_flat(flat.clone(), &initfs, Slot::B).unwrap();
    assert!(trial.starts_with(&legacy));
    let injected = Initfs::parse(&trial[legacy.len()..]).unwrap();
    let health = injected.get("/etc/boot-health").unwrap();
    let text = std::str::from_utf8(health.data).unwrap();
    assert!(text.contains(&format!("token = \"{token:08x}\"\n")));

    let mut esp = base_pi4_esp();
    let meta = BootMeta {
        cmdline: String::from("console=ttyAMA0 loglevel=info"),
        log_level: String::from("info"),
        slot: Slot::A,
        trial: Some(Slot::B),
        trial_token: Some(token),
        attempts: 3,
        has_slot_config: true,
    };
    let staged = stage_pi4_esp(&mut esp, &meta, &flat, &trial, &trial).unwrap();

    let mut staged = staged;
    assert_eq!(
        super::fat_read(&mut staged, "config.txt").unwrap().unwrap(),
        b"arm_64bit=1\nkernel=kernel8.img\n"
    );
    assert_eq!(
        super::fat_read(&mut staged, "start4.elf").unwrap().unwrap(),
        b"firmware"
    );
    assert_eq!(
        super::fat_read(&mut staged, "overlays/disable-bt.dtbo")
            .unwrap()
            .unwrap(),
        b"overlay"
    );
    let boot = super::fat_read(&mut staged, "raven/boot.toml")
        .unwrap()
        .unwrap();
    assert_eq!(parse_boot_meta(&boot).unwrap(), meta);
    assert_eq!(
        super::fat_read(&mut staged, "raven/slot-a/kernel8.img")
            .unwrap()
            .unwrap(),
        flat
    );
    assert_eq!(
        super::fat_read(&mut staged, "raven/slot-b/kernel8.img")
            .unwrap()
            .unwrap(),
        trial
    );
    assert_eq!(
        super::fat_read(&mut staged, "kernel8.img")
            .unwrap()
            .unwrap(),
        trial
    );
}

fn base_pi4_esp() -> Vec<u8> {
    let size = usize::try_from(image::ESP_SECTORS * SECTOR_SIZE).unwrap();
    let mut builder = FatImageBuilder::new(size).unwrap();
    builder
        .add_file("config.txt", b"arm_64bit=1\nkernel=kernel8.img\n")
        .unwrap();
    builder
        .add_file("kernel8.img", b"legacy flat image")
        .unwrap();
    builder.add_file("start4.elf", b"firmware").unwrap();
    builder.add_dir("overlays").unwrap();
    builder
        .add_file("overlays/disable-bt.dtbo", b"overlay")
        .unwrap();
    builder.add_dir("raven").unwrap();
    builder
        .add_file(
            "raven/boot.toml",
            b"cmdline = \"console=ttyAMA0\"\nlog_level = \"info\"\n",
        )
        .unwrap();
    builder.finish().unwrap()
}

/// Minimal aarch64 ELF whose entry point is the Pi 4 firmware load address.
fn aarch64_elf() -> Vec<u8> {
    let mut elf = vec![0u8; 64];
    elf[..4].copy_from_slice(b"\x7fELF");
    elf[4] = 2;
    elf[5] = 1;
    elf[18..20].copy_from_slice(&183u16.to_le_bytes());
    elf[24..32].copy_from_slice(&0x80000u64.to_le_bytes());
    elf
}
