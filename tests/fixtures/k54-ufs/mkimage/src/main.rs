// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Wrap a raw UFS image in a one-partition GPT disk.
//!
//! The output has a protective MBR, a primary header/array at LBAs 1..33, the
//! payload at LBA 34, a backup array at the end and a backup header in the last
//! sector.  Everything is derived from the payload length, so the image is
//! deterministic.

use std::env;
use std::fs;
use std::process::ExitCode;

use raven_proto_block::gpt::crc32;

const SECTOR: usize = 512;
const HEADER_SIZE: usize = 92;
const ENTRY_COUNT: u32 = 128;
const ENTRY_SIZE: usize = 128;
const PRIMARY_ENTRIES: u64 = 2;
const FIRST_USABLE: u64 = 34;
const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
const GPT_REVISION: u32 = 0x0001_0000;
const FREEBSD_UFS_GUID: [u8; 16] = [
    0xb6, 0x7c, 0x6e, 0x51, 0xcf, 0x6e, 0xd6, 0x11, 0x8f, 0xf8, 0x00, 0x02, 0x2d, 0x09, 0x71, 0x2b,
];
const DISK_GUID: [u8; 16] = [
    0x22, 0x22, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44, 0x55, 0x55, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
];
const PARTITION_GUID: [u8; 16] = [
    0x54, 0x54, 0x54, 0x54, 0x55, 0x55, 0x66, 0x66, 0x77, 0x77, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88,
];

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let (Some(payload), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: raven-k54-mkimage <ufs-image> <output>");
        return ExitCode::FAILURE;
    };
    let payload = match fs::read(&payload) {
        Ok(payload) => payload,
        Err(error) => {
            eprintln!("cannot read {payload}: {error}");
            return ExitCode::FAILURE;
        }
    };
    if payload.is_empty() || !payload.len().is_multiple_of(SECTOR) {
        eprintln!("payload must be a nonempty whole number of 512-byte sectors");
        return ExitCode::FAILURE;
    }
    let image = build(&payload);
    if let Err(error) = fs::write(&output, &image) {
        eprintln!("cannot write {output}: {error}");
        return ExitCode::FAILURE;
    }
    println!(
        "{output}: {} bytes, payload {} sectors",
        image.len(),
        payload.len() / SECTOR
    );
    ExitCode::SUCCESS
}

fn build(payload: &[u8]) -> Vec<u8> {
    let payload_sectors = (payload.len() / SECTOR) as u64;
    let total = FIRST_USABLE + payload_sectors + 33;
    let backup_entries = total - 33;
    let backup_header = total - 1;
    let last_usable = FIRST_USABLE + payload_sectors - 1;
    let mut disk = vec![0u8; total as usize * SECTOR];

    write_mbr(&mut disk, total);
    let mut array = vec![0u8; ENTRY_COUNT as usize * ENTRY_SIZE];
    write_entry(
        &mut array,
        0,
        FREEBSD_UFS_GUID,
        PARTITION_GUID,
        FIRST_USABLE,
        last_usable,
        "ufs-root",
    );
    let primary = PRIMARY_ENTRIES as usize * SECTOR;
    disk[primary..primary + array.len()].copy_from_slice(&array);
    let backup = backup_entries as usize * SECTOR;
    disk[backup..backup + array.len()].copy_from_slice(&array);
    write_header(
        &mut disk,
        1,
        backup_header,
        PRIMARY_ENTRIES,
        first_usable(),
        last_usable,
    );
    write_header(
        &mut disk,
        backup_header,
        1,
        backup_entries,
        first_usable(),
        last_usable,
    );
    let payload_start = FIRST_USABLE as usize * SECTOR;
    disk[payload_start..payload_start + payload.len()].copy_from_slice(payload);
    disk
}

const fn first_usable() -> u64 {
    FIRST_USABLE
}

fn write_mbr(disk: &mut [u8], total: u64) {
    let entry = 446;
    disk[entry + 4] = 0xee;
    disk[entry + 8..entry + 12].copy_from_slice(&1u32.to_le_bytes());
    disk[entry + 12..entry + 16].copy_from_slice(&((total - 1) as u32).to_le_bytes());
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

fn write_header(
    disk: &mut [u8],
    current: u64,
    backup: u64,
    entries_lba: u64,
    first_usable: u64,
    last_usable: u64,
) {
    let offset = current as usize * SECTOR;
    let array_offset = entries_lba as usize * SECTOR;
    let entries_crc = crc32(&disk[array_offset..array_offset + ENTRY_COUNT as usize * ENTRY_SIZE]);
    let header = &mut disk[offset..offset + SECTOR];
    header[..8].copy_from_slice(GPT_SIGNATURE);
    header[8..12].copy_from_slice(&GPT_REVISION.to_le_bytes());
    header[12..16].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
    header[24..32].copy_from_slice(&current.to_le_bytes());
    header[32..40].copy_from_slice(&backup.to_le_bytes());
    header[40..48].copy_from_slice(&first_usable.to_le_bytes());
    header[48..56].copy_from_slice(&last_usable.to_le_bytes());
    header[56..72].copy_from_slice(&DISK_GUID);
    header[72..80].copy_from_slice(&entries_lba.to_le_bytes());
    header[80..84].copy_from_slice(&ENTRY_COUNT.to_le_bytes());
    header[84..88].copy_from_slice(&(ENTRY_SIZE as u32).to_le_bytes());
    header[88..92].copy_from_slice(&entries_crc.to_le_bytes());
    let checksum = crc32(&header[..HEADER_SIZE]);
    header[16..20].copy_from_slice(&checksum.to_le_bytes());
}
