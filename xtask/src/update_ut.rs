// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the K27 update planner.

use super::{BootMeta, Options, Slot, build_fixture, parse_boot_meta, target_slot};
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
