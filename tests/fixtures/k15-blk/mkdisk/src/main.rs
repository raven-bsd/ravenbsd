// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Build the 1 GiB sparse GPT disk used by the `virtio-blk` qtest.
//!
//! The disk is sparse: only the first MiB is written, so the file is 1 GiB long but costs one
//! MiB on disk until the test writes its pattern.  The GPT is self-consistent for that 1 MiB
//! prefix (the driver and `blockd` read at most 1 MiB for the scan), with one partition named
//! `pattern` at LBA 34..1800 where the driver's partition pattern lives.  The device data area
//! starts at LBA 2048, after the backup header.

use std::env;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::process::ExitCode;

use raven_proto_block::gpt::crc32;

const SECTOR: usize = 512;
const PREFIX_SECTORS: u64 = 2048;
const TOTAL_SECTORS: u64 = 2_097_152; // 1 GiB
const HEADER_SIZE: usize = 92;
const ENTRY_COUNT: u32 = 128;
const ENTRY_SIZE: usize = 128;
const PRIMARY_ENTRIES: u64 = 2;
const FIRST_USABLE: u64 = 34;
const LAST_USABLE: u64 = 1800;
const BACKUP_ENTRIES: u64 = 2015;
const BACKUP_HEADER: u64 = 2047;
const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
const GPT_REVISION: u32 = 0x0001_0000;
const LINUX_DATA_GUID: [u8; 16] = [
    0xaf, 0x3d, 0xc6, 0x0f, 0x83, 0x84, 0x72, 0x47, 0x8e, 0x79, 0x3d, 0x69, 0xd8, 0x47, 0x7d, 0xe4,
];
const DISK_GUID: [u8; 16] = [
    0x15, 0x15, 0x15, 0x15, 0x26, 0x26, 0x37, 0x37, 0x48, 0x48, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59,
];
const PATTERN_GUID: [u8; 16] = [
    0xbb, 0xbb, 0xbb, 0xbb, 0xcc, 0xcc, 0xdd, 0xdd, 0xee, 0xee, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

fn main() -> ExitCode {
    let path = match env::args().nth(1) {
        Some(path) => path,
        None => {
            eprintln!("usage: raven-k15-mkdisk <output>");
            return ExitCode::FAILURE;
        }
    };
    let prefix = build_prefix();
    let result = write_image(&path, &prefix);
    match result {
        Ok(()) => {
            println!(
                "{path}: {} bytes, {} MiB prefix",
                TOTAL_SECTORS * SECTOR as u64,
                PREFIX_SECTORS / 2048
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("cannot write {path}: {error}");
            ExitCode::FAILURE
        }
    }
}

fn write_image(path: &str, prefix: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?;
    file.set_len(TOTAL_SECTORS * SECTOR as u64)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(prefix)?;
    file.sync_all()
}

fn build_prefix() -> Vec<u8> {
    let mut disk = vec![0u8; PREFIX_SECTORS as usize * SECTOR];
    write_mbr(&mut disk);
    let mut primary = vec![0u8; ENTRY_COUNT as usize * ENTRY_SIZE];
    write_entry(
        &mut primary,
        0,
        PATTERN_GUID,
        FIRST_USABLE,
        LAST_USABLE,
        "pattern",
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
    let sectors = u32::try_from(TOTAL_SECTORS - 1).unwrap_or(u32::MAX);
    disk[entry + 12..entry + 16].copy_from_slice(&sectors.to_le_bytes());
    disk[510] = 0x55;
    disk[511] = 0xaa;
}

fn write_entry(
    array: &mut [u8],
    index: usize,
    unique_guid: [u8; 16],
    first: u64,
    last: u64,
    name: &str,
) {
    let offset = index * ENTRY_SIZE;
    array[offset..offset + 16].copy_from_slice(&LINUX_DATA_GUID);
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
