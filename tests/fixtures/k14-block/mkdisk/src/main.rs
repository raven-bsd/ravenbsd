// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Build the deterministic 1 MiB GPT image used by the `blockd` qtest.
//!
//! The image has two Linux-data partitions named `first` (34..100) and
//! `second` (101..200), a primary header at LBA 1, a backup header at LBA
//! 2047, and 128-entry arrays in the standard locations.  It is generated
//! rather than committed so every byte is reviewable.

use std::env;
use std::fs;
use std::process::ExitCode;

use raven_proto_block::gpt::crc32;

const SECTOR: usize = 512;
const TOTAL_SECTORS: u64 = 2048;
const HEADER_SIZE: usize = 92;
const ENTRY_COUNT: u32 = 128;
const ENTRY_SIZE: usize = 128;
const PRIMARY_ENTRIES: u64 = 2;
const FIRST_USABLE: u64 = 34;
const LAST_USABLE: u64 = 2014;
const BACKUP_ENTRIES: u64 = 2015;
const BACKUP_HEADER: u64 = 2047;
const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
const GPT_REVISION: u32 = 0x0001_0000;
const LINUX_DATA_GUID: [u8; 16] = [
    0xaf, 0x3d, 0xc6, 0x0f, 0x83, 0x84, 0x72, 0x47, 0x8e, 0x79, 0x3d, 0x69, 0xd8, 0x47, 0x7d, 0xe4,
];
const DISK_GUID: [u8; 16] = [
    0x11, 0x11, 0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55,
];
const FIRST_GUID: [u8; 16] = [
    0xaa, 0xaa, 0xaa, 0xaa, 0xbb, 0xbb, 0xcc, 0xcc, 0xdd, 0xdd, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee,
];
const SECOND_GUID: [u8; 16] = [
    0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x33, 0x33, 0x33, 0x33,
];

fn main() -> ExitCode {
    let path = match env::args().nth(1) {
        Some(path) => path,
        None => {
            eprintln!("usage: raven-k14-mkdisk <output>");
            return ExitCode::FAILURE;
        }
    };
    let image = build();
    if let Err(error) = fs::write(&path, &image) {
        eprintln!("cannot write {path}: {error}");
        return ExitCode::FAILURE;
    }
    println!("{path}: {} bytes", image.len());
    ExitCode::SUCCESS
}

fn build() -> Vec<u8> {
    let mut disk = vec![0u8; TOTAL_SECTORS as usize * SECTOR];
    write_mbr(&mut disk);
    let mut primary = vec![0u8; ENTRY_COUNT as usize * ENTRY_SIZE];
    write_entry(
        &mut primary,
        0,
        LINUX_DATA_GUID,
        FIRST_GUID,
        34,
        100,
        "first",
    );
    write_entry(
        &mut primary,
        1,
        LINUX_DATA_GUID,
        SECOND_GUID,
        101,
        200,
        "second",
    );
    let array = primary.len();
    disk[PRIMARY_ENTRIES as usize * SECTOR..PRIMARY_ENTRIES as usize * SECTOR + array]
        .copy_from_slice(&primary);
    disk[BACKUP_ENTRIES as usize * SECTOR..BACKUP_ENTRIES as usize * SECTOR + array]
        .copy_from_slice(&primary);
    write_header(&mut disk, 1, BACKUP_HEADER, PRIMARY_ENTRIES);
    write_header(&mut disk, BACKUP_HEADER, 1, BACKUP_ENTRIES);
    disk
}

fn write_mbr(disk: &mut [u8]) {
    let entry = 446;
    disk[entry + 4] = 0xee;
    disk[entry + 8..entry + 12].copy_from_slice(&1u32.to_le_bytes());
    disk[entry + 12..entry + 16].copy_from_slice(&((TOTAL_SECTORS - 1) as u32).to_le_bytes());
    disk[510] = 0x55;
    disk[511] = 0xaa;
}

fn write_entry(
    array: &mut [u8],
    index: usize,
    type_guid: [u8; 16],
    unique_guid: [u8; 16],
    first: u64,
    last: u64,
    name: &str,
) {
    let offset = index * ENTRY_SIZE;
    array[offset..offset + 16].copy_from_slice(&type_guid);
    array[offset + 16..offset + 32].copy_from_slice(&unique_guid);
    array[offset + 32..offset + 40].copy_from_slice(&first.to_le_bytes());
    array[offset + 40..offset + 48].copy_from_slice(&last.to_le_bytes());
    for (slot, unit) in array[offset + 56..offset + 128]
        .chunks_exact_mut(2)
        .zip(name.encode_utf16())
    {
        slot.copy_from_slice(&unit.to_le_bytes());
    }
}

fn write_header(disk: &mut [u8], current: u64, backup: u64, entries_lba: u64) {
    let offset = current as usize * SECTOR;
    let array_offset = entries_lba as usize * SECTOR;
    let entries_crc = crc32(&disk[array_offset..array_offset + ENTRY_COUNT as usize * ENTRY_SIZE]);
    let header = &mut disk[offset..offset + SECTOR];
    header[..8].copy_from_slice(GPT_SIGNATURE);
    header[8..12].copy_from_slice(&GPT_REVISION.to_le_bytes());
    header[12..16].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
    header[24..32].copy_from_slice(&current.to_le_bytes());
    header[32..40].copy_from_slice(&backup.to_le_bytes());
    header[40..48].copy_from_slice(&FIRST_USABLE.to_le_bytes());
    header[48..56].copy_from_slice(&LAST_USABLE.to_le_bytes());
    header[56..72].copy_from_slice(&DISK_GUID);
    header[72..80].copy_from_slice(&entries_lba.to_le_bytes());
    header[80..84].copy_from_slice(&ENTRY_COUNT.to_le_bytes());
    header[84..88].copy_from_slice(&(ENTRY_SIZE as u32).to_le_bytes());
    header[88..92].copy_from_slice(&entries_crc.to_le_bytes());
    let checksum = crc32(&header[..HEADER_SIZE]);
    header[16..20].copy_from_slice(&checksum.to_le_bytes());
}
