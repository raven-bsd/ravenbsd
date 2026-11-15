// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic GPT golden images for the host tests and the fuzz replay harness.
//!
//! The layout mirrors the first-party image writer in `xtask`: a protective MBR, the primary
//! header at LBA 1 and its 128-entry table at LBA 2, the matching backup table/header at the end
//! of the disk. This module is compiled only for tests or with the `fixtures` feature.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::{GPT_REVISION, GPT_SIGNATURE, SECTOR_SIZE, crc32};

/// ESP type GUID `C12A7328-F81F-11D2-BA4B-00A0C93EC93B` in on-disk mixed-endian order.
pub const TYPE_ESP: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];

/// Linux data type GUID `0FC63DAF-8483-4772-8E79-3D69D8477DE4` in on-disk mixed-endian order.
pub const TYPE_LINUX_DATA: [u8; 16] = [
    0xaf, 0x3d, 0xc6, 0x0f, 0x83, 0x84, 0x72, 0x47, 0x8e, 0x79, 0x3d, 0x69, 0xd8, 0x47, 0x7d, 0xe4,
];

const ENTRY_COUNT: usize = 128;
const ENTRY_SIZE: usize = 128;
const ENTRY_SECTORS: usize = (ENTRY_COUNT * ENTRY_SIZE) / SECTOR_SIZE;
const FIRST_USABLE: u64 = 2 + ENTRY_SECTORS as u64;

/// A partition description for [`image`].
#[derive(Clone, Copy, Debug)]
pub struct FixturePartition {
    /// Type GUID in on-disk order.
    pub type_guid: [u8; 16],
    /// First LBA.
    pub first_lba: u64,
    /// Last LBA.
    pub last_lba: u64,
    /// UTF-8 name (encoded as UTF-16 on disk).
    pub name: &'static str,
}

/// The default valid image: 96 sectors, one ESP partition at LBA 34..=62.
#[must_use]
pub fn golden_image() -> Vec<u8> {
    image(
        96,
        &[FixturePartition {
            type_guid: TYPE_ESP,
            first_lba: FIRST_USABLE,
            last_lba: 62,
            name: "ESP",
        }],
    )
}

/// Build a valid GPT image of `total_sectors` 512-byte sectors.
///
/// # Panics
/// Panics when the geometry or the partition list cannot fit; this is a test fixture.
#[must_use]
pub fn image(total_sectors: u64, partitions: &[FixturePartition]) -> Vec<u8> {
    assert!(total_sectors >= FIRST_USABLE + ENTRY_SECTORS as u64 + 2);
    assert!(partitions.len() <= ENTRY_COUNT);
    let bytes = usize::try_from(total_sectors)
        .expect("fixture sector count")
        .checked_mul(SECTOR_SIZE)
        .expect("fixture size");
    let mut disk = vec![0u8; bytes];
    let last_usable = total_sectors - 1 - ENTRY_SECTORS as u64 - 1;

    // Protective MBR.
    let mbr = 0usize;
    disk[mbr + 446 + 4] = 0xee;
    put_u32(&mut disk, mbr + 446 + 8, 1);
    put_u32(
        &mut disk,
        mbr + 446 + 12,
        u32::try_from(total_sectors - 1).unwrap_or(u32::MAX),
    );
    disk[mbr + 510] = 0x55;
    disk[mbr + 511] = 0xaa;

    // Entry array.
    let mut entries = vec![0u8; ENTRY_COUNT * ENTRY_SIZE];
    for (index, partition) in partitions.iter().enumerate() {
        let base = index * ENTRY_SIZE;
        entries[base..base + 16].copy_from_slice(&partition.type_guid);
        let mut unique = [0u8; 16];
        unique[0] = 0x11;
        unique[1] = index as u8;
        unique[15] = 0xa5;
        entries[base + 16..base + 32].copy_from_slice(&unique);
        put_u64(&mut entries, base + 32, partition.first_lba);
        put_u64(&mut entries, base + 40, partition.last_lba);
        for (unit_index, unit) in partition.name.encode_utf16().take(35).enumerate() {
            let offset = base + 56 + unit_index * 2;
            entries[offset..offset + 2].copy_from_slice(&unit.to_le_bytes());
        }
    }
    let entry_crc = crc32(&entries);
    let primary_entries_lba = 2u64;
    let backup_entries_lba = total_sectors - 1 - ENTRY_SECTORS as u64;
    let primary_header = header(
        1,
        total_sectors - 1,
        FIRST_USABLE,
        last_usable,
        primary_entries_lba,
        entry_crc,
    );
    let backup_header = header(
        total_sectors - 1,
        1,
        FIRST_USABLE,
        last_usable,
        backup_entries_lba,
        entry_crc,
    );
    let primary_offset = SECTOR_SIZE * usize::try_from(primary_entries_lba).expect("lba");
    disk[primary_offset..primary_offset + entries.len()].copy_from_slice(&entries);
    let backup_offset = SECTOR_SIZE * usize::try_from(backup_entries_lba).expect("lba");
    disk[backup_offset..backup_offset + entries.len()].copy_from_slice(&entries);
    let primary_header_offset = SECTOR_SIZE;
    disk[primary_header_offset..primary_header_offset + 92].copy_from_slice(&primary_header);
    let backup_header_offset = SECTOR_SIZE * usize::try_from(total_sectors - 1).expect("lba");
    disk[backup_header_offset..backup_header_offset + 92].copy_from_slice(&backup_header);
    disk
}

fn header(
    my_lba: u64,
    alternate_lba: u64,
    first_usable: u64,
    last_usable: u64,
    entries_lba: u64,
    entry_crc: u32,
) -> [u8; 92] {
    let mut header = [0u8; 92];
    header[..8].copy_from_slice(GPT_SIGNATURE);
    put_u32(&mut header, 8, GPT_REVISION);
    put_u32(&mut header, 12, 92);
    put_u64(&mut header, 24, my_lba);
    put_u64(&mut header, 32, alternate_lba);
    put_u64(&mut header, 40, first_usable);
    put_u64(&mut header, 48, last_usable);
    for (index, byte) in header[56..72].iter_mut().enumerate() {
        *byte = 0x30 + index as u8;
    }
    put_u64(&mut header, 72, entries_lba);
    put_u32(&mut header, 80, ENTRY_COUNT as u32);
    put_u32(&mut header, 84, ENTRY_SIZE as u32);
    put_u32(&mut header, 88, entry_crc);
    let crc = crc32(&header);
    put_u32(&mut header, 16, crc);
    header
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

/// A stable name for diagnostics in the fuzz harness.
#[must_use]
pub fn fixture_name() -> String {
    String::from("gpt-96-sector-esp")
}
