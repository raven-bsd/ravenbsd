// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the Pi 4 image assembly helpers.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use super::*;

fn temporary_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "xtask-pi-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn phdr(bytes: &mut [u8], at: usize, offset: u64, paddr: u64, filesz: u64, memsz: u64) {
    write_u32(bytes, at, 1); // PT_LOAD
    write_u32(bytes, at + 4, 5); // R|X
    write_u64(bytes, at + 8, offset);
    write_u64(bytes, at + 16, paddr);
    write_u64(bytes, at + 24, paddr);
    write_u64(bytes, at + 32, filesz);
    write_u64(bytes, at + 40, memsz);
    write_u64(bytes, at + 48, 0x1000);
}

fn minimal_elf() -> Vec<u8> {
    let mut elf = vec![0u8; 0x3000];
    elf[..4].copy_from_slice(b"\x7fELF");
    elf[4] = 2; // 64-bit
    elf[5] = 1; // little-endian
    elf[6] = 1; // version
    write_u16(&mut elf, 16, 2); // ET_EXEC
    write_u16(&mut elf, 18, 183); // EM_AARCH64
    write_u32(&mut elf, 20, 1);
    write_u64(&mut elf, 24, PI_LOAD);
    write_u64(&mut elf, 32, 64); // phoff
    write_u32(&mut elf, 48, 0);
    write_u16(&mut elf, 52, 64);
    write_u16(&mut elf, 54, 56);
    write_u16(&mut elf, 56, 2);
    phdr(&mut elf, 64, 0x1000, PI_LOAD, 4, 0x1000);
    phdr(&mut elf, 120, 0x2000, PI_LOAD + 0x1000, 4, 0x1000);
    elf[0x1000..0x1004].copy_from_slice(&[1, 2, 3, 4]);
    elf[0x2000..0x2004].copy_from_slice(&[5, 6, 7, 8]);
    elf
}

fn fake_firmware(dir: &Path) {
    for name in FIRMWARE_FILES {
        fs::write(dir.join(name), format!("firmware:{name}")).expect("firmware file");
    }
}

#[test]
fn config_txt_is_the_contract_exact_text() {
    assert_eq!(
        CONFIG_TXT,
        "arm_64bit=1\nkernel=kernel8.img\nenable_uart=1\nuart_2ndstage=1\n"
    );
}

#[test]
fn required_firmware_and_dtb_are_in_the_list() {
    for name in [
        "bootcode.bin",
        "start4.elf",
        "fixup4.dat",
        "bcm2711-rpi-4-b.dtb",
    ] {
        assert!(FIRMWARE_FILES.contains(&name), "missing {name}");
    }
}

#[test]
fn default_output_is_the_pi4_image() {
    assert_eq!(
        resolve_output(Path::new("/repo"), None),
        Path::new("/repo/build/aarch64/ravenbsd-pi4.img")
    );
    assert_eq!(
        resolve_output(Path::new("/repo"), Some(Path::new("/tmp/pi.img"))),
        Path::new("/tmp/pi.img")
    );
}

#[test]
fn flatten_places_segments_at_the_pi_load_address() {
    let flat = flatten_elf(&minimal_elf()).expect("flatten");
    assert_eq!(flat.len(), 0x2000);
    assert_eq!(&flat[..4], &[1, 2, 3, 4]);
    assert_eq!(&flat[0x1000..0x1004], &[5, 6, 7, 8]);
}

#[test]
fn kernel8_appends_the_initfs() {
    let flat = kernel8_image(&minimal_elf(), &[9, 9, 9]).expect("kernel8");
    assert_eq!(flat.len(), 0x2000 + 3);
    assert_eq!(&flat[0x2000..], &[9, 9, 9]);
}

#[test]
fn flatten_rejects_a_non_aarch64_elf() {
    let mut elf = minimal_elf();
    write_u16(&mut elf, 18, 62);
    assert!(flatten_elf(&elf).is_err());
}

#[test]
fn flatten_rejects_a_segment_below_the_load_address() {
    let mut elf = minimal_elf();
    phdr(&mut elf, 64, 0x1000, PI_LOAD - 0x1000, 4, 0x1000);
    assert!(flatten_elf(&elf).is_err());
}

#[test]
fn flatten_rejects_overlapping_segments() {
    let mut elf = minimal_elf();
    phdr(&mut elf, 120, 0x2000, PI_LOAD, 4, 0x1000);
    assert!(flatten_elf(&elf).is_err());
}

#[test]
fn boot_partition_contains_the_required_files() {
    let dir = temporary_dir("fat");
    let firmware = dir.join("firmware");
    fs::create_dir_all(&firmware).expect("firmware dir");
    fake_firmware(&firmware);
    let kernel8 = kernel8_image(&minimal_elf(), &[1, 2, 3]).expect("kernel8");
    let esp = build_esp(&kernel8, &firmware).expect("esp");
    let fs = fatfs::FileSystem::new(Cursor::new(esp.clone()), fatfs::FsOptions::new())
        .expect("fat filesystem");
    let root = fs.root_dir();
    let mut names: Vec<String> = root
        .iter()
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    names.sort();
    for name in [
        "LICENCE.broadcom",
        "bcm2711-rpi-4-b.dtb",
        "bootcode.bin",
        "config.txt",
        "fixup4.dat",
        "kernel8.img",
        "start4.elf",
    ] {
        assert!(
            names.contains(&name.to_string()),
            "missing {name} in {names:?}"
        );
    }
    let again = build_esp(&kernel8, &firmware).expect("esp again");
    assert_eq!(esp, again, "the FAT boot partition is not deterministic");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_firmware_is_a_clear_error() {
    let dir = temporary_dir("missing");
    let firmware = dir.join("firmware");
    fs::create_dir_all(&firmware).expect("firmware dir");
    fake_firmware(&firmware);
    fs::remove_file(firmware.join("start4.elf")).expect("remove");
    let error = build_esp(&[1, 2, 3], &firmware).expect_err("missing firmware");
    assert!(error.to_string().contains("start4.elf"), "{error}");
    let _ = fs::remove_dir_all(&dir);
}
