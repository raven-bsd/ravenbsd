// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the GPT parser: golden image, 24 malformed vectors and bit-flip robustness.

use alloc::vec;
use alloc::vec::Vec;

use super::fixtures::{FixturePartition, TYPE_ESP, TYPE_LINUX_DATA, golden_image, image};
use super::*;

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn fix_header_crc(image: &mut [u8]) {
    let size = u32_at(image, SECTOR_SIZE + 12).expect("header size") as usize;
    let mut header = image[SECTOR_SIZE..SECTOR_SIZE + size].to_vec();
    header[16..20].fill(0);
    let crc = crc32(&header);
    put32(image, SECTOR_SIZE + 16, crc);
}

fn fix_entry_crc(image: &mut [u8]) {
    let entries_lba = u64_at(image, SECTOR_SIZE + 72).expect("entries lba");
    let count = u32_at(image, SECTOR_SIZE + 80).expect("entry count") as usize;
    let size = u32_at(image, SECTOR_SIZE + 84).expect("entry size") as usize;
    let start = usize::try_from(entries_lba).expect("lba") * SECTOR_SIZE;
    let end = start + count * size;
    let crc = crc32(&image[start..end]);
    put32(image, SECTOR_SIZE + 88, crc);
    fix_header_crc(image);
}

fn set_entry(image: &mut [u8], index: usize, first: u64, last: u64) {
    let start = 2 * SECTOR_SIZE + index * 128;
    put64(image, start + 32, first);
    put64(image, start + 40, last);
}

#[test]
fn golden_image_parses() {
    let gpt = Gpt::parse(&golden_image()).expect("golden GPT");
    assert_eq!(gpt.total_sectors(), 96);
    assert_eq!(gpt.first_usable_lba(), 34);
    assert_eq!(gpt.last_usable_lba(), 62);
    assert_eq!(gpt.revision(), GPT_REVISION);
    assert_eq!(gpt.header_size(), 92);
    assert_ne!(gpt.header_crc(), 0);
    assert_ne!(gpt.entry_crc(), 0);
    assert_eq!(gpt.partitions().len(), 1);
    let partition = &gpt.partitions()[0];
    assert_eq!(partition.type_guid, TYPE_ESP);
    assert_eq!(partition.first_lba, 34);
    assert_eq!(partition.last_lba, 62);
    assert_eq!(partition.name, "ESP");
}

#[test]
fn empty_table_is_valid() {
    let gpt = Gpt::parse(&image(96, &[])).expect("empty table");
    assert!(gpt.partitions().is_empty());
}

#[test]
fn two_partitions_parse_in_order() {
    let disk = image(
        96,
        &[
            FixturePartition {
                type_guid: TYPE_ESP,
                first_lba: 34,
                last_lba: 40,
                name: "ESP",
            },
            FixturePartition {
                type_guid: TYPE_LINUX_DATA,
                first_lba: 42,
                last_lba: 62,
                name: "ROOT",
            },
        ],
    );
    let gpt = Gpt::parse(&disk).expect("two partitions");
    assert_eq!(gpt.partitions().len(), 2);
    assert_eq!(gpt.partitions()[1].name, "ROOT");
}

#[test]
fn malformed_vectors_are_rejected() {
    let mut vectors: Vec<(&str, Vec<u8>)> = Vec::new();

    let golden = golden_image();
    vectors.push(("short-mbr", golden[..511].to_vec()));
    vectors.push(("short-header", golden[..600].to_vec()));
    vectors.push(("short-entry-table", golden[..2048].to_vec()));

    let mut disk = golden.clone();
    disk[510] = 0;
    vectors.push(("mbr-signature", disk));

    let mut disk = golden.clone();
    disk[446 + 4] = 0x83;
    vectors.push(("mbr-type", disk));

    let mut disk = golden.clone();
    put32(&mut disk, 446 + 8, 0);
    vectors.push(("mbr-start", disk));

    let mut disk = golden.clone();
    put32(&mut disk, 446 + 12, 0);
    vectors.push(("mbr-sectors", disk));

    let mut disk = golden.clone();
    disk[SECTOR_SIZE] = b'X';
    vectors.push(("header-signature", disk));

    let mut disk = golden.clone();
    put32(&mut disk, SECTOR_SIZE + 8, 0x0002_0000);
    vectors.push(("header-revision", disk));

    for size in [0u32, 91, 513] {
        let mut disk = golden.clone();
        put32(&mut disk, SECTOR_SIZE + 12, size);
        vectors.push(("header-size", disk));
    }

    let mut disk = golden.clone();
    disk[SECTOR_SIZE + 16] ^= 1;
    vectors.push(("header-crc", disk));

    let mut disk = golden.clone();
    put64(&mut disk, SECTOR_SIZE + 24, 2);
    fix_header_crc(&mut disk);
    vectors.push(("header-lba", disk));

    let mut disk = golden.clone();
    put64(&mut disk, SECTOR_SIZE + 32, 96);
    fix_header_crc(&mut disk);
    vectors.push(("alternate-lba", disk));

    let mut disk = golden.clone();
    put64(&mut disk, SECTOR_SIZE + 40, 63);
    fix_header_crc(&mut disk);
    vectors.push(("usable-range", disk));

    let mut disk = golden.clone();
    put32(&mut disk, SECTOR_SIZE + 84, 127);
    fix_header_crc(&mut disk);
    vectors.push(("entry-size", disk));

    let mut disk = golden.clone();
    put32(&mut disk, SECTOR_SIZE + 80, 0);
    fix_header_crc(&mut disk);
    vectors.push(("entry-count-zero", disk));

    let mut disk = golden.clone();
    put32(&mut disk, SECTOR_SIZE + 80, 4097);
    fix_header_crc(&mut disk);
    vectors.push(("entry-count-large", disk));

    let mut disk = golden.clone();
    put64(&mut disk, SECTOR_SIZE + 72, 200);
    fix_header_crc(&mut disk);
    vectors.push(("entries-lba", disk));

    let mut disk = golden.clone();
    put64(&mut disk, SECTOR_SIZE + 72, 34);
    fix_header_crc(&mut disk);
    vectors.push(("entries-overlap-usable", disk));

    let mut disk = golden.clone();
    put32(&mut disk, SECTOR_SIZE + 88, 0);
    fix_header_crc(&mut disk);
    vectors.push(("entry-crc", disk));

    let mut disk = golden.clone();
    disk[1024] ^= 1;
    vectors.push(("entry-byte", disk));

    let mut disk = golden.clone();
    set_entry(&mut disk, 0, 70, 60);
    fix_entry_crc(&mut disk);
    vectors.push(("entry-inverted", disk));

    let mut disk = golden.clone();
    set_entry(&mut disk, 0, 20, 40);
    fix_entry_crc(&mut disk);
    vectors.push(("entry-outside", disk));

    let mut disk = golden.clone();
    set_entry(&mut disk, 0, 34, 50);
    set_entry(&mut disk, 1, 40, 62);
    disk[2 * SECTOR_SIZE + 128 + 4] = 0xee;
    fix_entry_crc(&mut disk);
    vectors.push(("entry-overlap", disk));

    assert!(vectors.len() >= 20, "only {} vectors", vectors.len());
    for (name, disk) in vectors {
        assert!(
            Gpt::parse(&disk).is_err(),
            "vector `{name}` unexpectedly parsed"
        );
    }
}

#[test]
fn bit_flips_never_panic() {
    let golden = golden_image();
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    for _ in 0..512 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let mut disk = golden.clone();
        let at = (state as usize) % disk.len();
        disk[at] ^= ((state >> 32) as u8) | 1;
        let _ = Gpt::parse(&disk);
    }
}

#[test]
fn invalid_utf16_name_does_not_panic() {
    let mut disk = golden_image();
    let name = 2 * SECTOR_SIZE + 56;
    disk[name] = 0x00;
    disk[name + 1] = 0xd8;
    fix_entry_crc(&mut disk);
    let gpt = Gpt::parse(&disk).expect("invalid UTF-16 is replaced, not rejected");
    assert!(gpt.partitions()[0].name.contains('\u{fffd}'));
}

#[test]
fn crc_helper_matches_known_value() {
    assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
}

#[test]
fn blank_disk_is_rejected_before_any_table_walk() {
    let disk = vec![0u8; 96 * SECTOR_SIZE];
    assert_eq!(Gpt::parse(&disk).err(), Some(GptError::BadProtectiveMbr));
}
