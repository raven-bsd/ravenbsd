// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Wrap the K72 UFS root in a GPT disk, or extract the partition back for `ufs-fsck`.
//!
//! The disk is a standard GPT (backup header at the last LBA) whose whole image stays inside the
//! K15 blockd 1 MiB scan prefix, so the same disk works before and after K73's prefix change.
//! `mkdisk <ufs> <disk>` builds it; `extract <disk> <ufs>` copies the payload out for the host
//! fsck oracle.

use std::env;
use std::fs;
use std::process::ExitCode;

use raven_proto_block::gpt::crc32;

const SECTOR: usize = 512;
const HEADER_SIZE: usize = 92;
const ENTRY_COUNT: u32 = 128;
const ENTRY_SIZE: usize = 128;
const FIRST_USABLE: u64 = 34;
const BACKUP_SECTORS: u64 = 33;
const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
const GPT_REVISION: u32 = 0x0001_0000;
/// FreeBSD UFS partition type GUID `516E7CB6-6ECF-11D6-8FF8-00022D09712B`.
const TYPE_FREEBSD_UFS: [u8; 16] = [
    0xb6, 0x7c, 0x6e, 0x51, 0xcf, 0x6e, 0xd6, 0x11, 0x8f, 0xf8, 0x00, 0x02, 0x2d, 0x09, 0x71, 0x2b,
];
const UNIQUE_GUID: [u8; 16] = [
    0x72, 0x72, 0x72, 0x72, 0x82, 0x82, 0x92, 0x92, 0xa2, 0xa2, 0xb2, 0xb2, 0xb2, 0xb2, 0xb2, 0xb2,
];
const DISK_GUID: [u8; 16] = [
    0x27, 0x27, 0x27, 0x27, 0x37, 0x37, 0x47, 0x47, 0x57, 0x57, 0x67, 0x67, 0x67, 0x67, 0x67, 0x67,
];
/// The K15 blockd scans at most this many sectors of the disk.
const SCAN_PREFIX_SECTORS: u64 = 2048;

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let result = match arguments.as_slice() {
        [mode, source, destination] if mode == "mkdisk" => build(source, destination),
        [mode, source, destination] if mode == "extract" => extract(source, destination),
        _ => {
            eprintln!("usage: raven-k72-mkdisk mkdisk <ufs-image> <disk>");
            eprintln!("       raven-k72-mkdisk extract <disk> <ufs-image>");
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("raven-k72-mkdisk: {error}");
            ExitCode::FAILURE
        }
    }
}

fn build(source: &str, destination: &str) -> Result<(), String> {
    let payload = fs::read(source).map_err(|error| format!("read {source}: {error}"))?;
    if payload.is_empty() || !payload.len().is_multiple_of(SECTOR) {
        return Err(format!(
            "{source} is not a whole number of 512-byte sectors"
        ));
    }
    let payload_sectors = payload.len() as u64 / SECTOR as u64;
    let total = FIRST_USABLE + payload_sectors + BACKUP_SECTORS;
    if total > SCAN_PREFIX_SECTORS {
        return Err(format!(
            "disk is {total} sectors, outside the {SCAN_PREFIX_SECTORS}-sector blockd prefix"
        ));
    }
    let bytes = usize::try_from(total).map_err(|_| "disk size")? * SECTOR;
    let mut disk = vec![0u8; bytes];
    write_mbr(&mut disk, total);
    let entries = entry_array(payload_sectors);
    let array_len = entries.len();
    disk[2 * SECTOR..2 * SECTOR + array_len].copy_from_slice(&entries);
    let backup_entries =
        usize::try_from(total - 1 - (BACKUP_SECTORS - 1)).map_err(|_| "backup entries")?;
    disk[backup_entries * SECTOR..backup_entries * SECTOR + array_len].copy_from_slice(&entries);
    write_header(
        &mut disk,
        1,
        total - 1,
        FIRST_USABLE,
        FIRST_USABLE + payload_sectors - 1,
        2,
        &entries,
    );
    write_header(
        &mut disk,
        total - 1,
        1,
        FIRST_USABLE,
        FIRST_USABLE + payload_sectors - 1,
        backup_entries as u64,
        &entries,
    );
    let payload_offset = usize::try_from(FIRST_USABLE).map_err(|_| "payload offset")? * SECTOR;
    disk[payload_offset..payload_offset + payload.len()].copy_from_slice(&payload);
    fs::write(destination, &disk).map_err(|error| format!("write {destination}: {error}"))?;
    println!(
        "{destination}: {total} sectors, ufs-root 34..{}, payload {payload_sectors} sectors",
        FIRST_USABLE + payload_sectors - 1
    );
    Ok(())
}

fn extract(source: &str, destination: &str) -> Result<(), String> {
    let disk = fs::read(source).map_err(|error| format!("read {source}: {error}"))?;
    if disk.len() < 2 * SECTOR || !disk.len().is_multiple_of(SECTOR) {
        return Err(format!("{source} is not a whole number of sectors"));
    }
    let total = disk.len() as u64 / SECTOR as u64;
    if total <= FIRST_USABLE + BACKUP_SECTORS {
        return Err(format!("{source} is too short"));
    }
    let payload_sectors = total - FIRST_USABLE - BACKUP_SECTORS;
    let start = usize::try_from(FIRST_USABLE).map_err(|_| "payload start")? * SECTOR;
    let end = start + usize::try_from(payload_sectors).map_err(|_| "payload end")? * SECTOR;
    let payload = disk.get(start..end).ok_or("payload range")?;
    fs::write(destination, payload).map_err(|error| format!("write {destination}: {error}"))?;
    println!("{destination}: {payload_sectors} sectors from {source}");
    Ok(())
}

fn write_mbr(disk: &mut [u8], total: u64) {
    let entry = 446;
    disk[entry + 4] = 0xee;
    disk[entry + 8..entry + 12].copy_from_slice(&1u32.to_le_bytes());
    let sectors = u32::try_from(total - 1).unwrap_or(u32::MAX);
    disk[entry + 12..entry + 16].copy_from_slice(&sectors.to_le_bytes());
    disk[510] = 0x55;
    disk[511] = 0xaa;
}

fn entry_array(payload_sectors: u64) -> Vec<u8> {
    let mut array = vec![0u8; ENTRY_COUNT as usize * ENTRY_SIZE];
    array[0..16].copy_from_slice(&TYPE_FREEBSD_UFS);
    array[16..32].copy_from_slice(&UNIQUE_GUID);
    array[32..40].copy_from_slice(&FIRST_USABLE.to_le_bytes());
    let last = FIRST_USABLE + payload_sectors - 1;
    array[40..48].copy_from_slice(&last.to_le_bytes());
    for (slot, unit) in array[56..128]
        .chunks_exact_mut(2)
        .zip("ufs-root".encode_utf16())
    {
        slot.copy_from_slice(&unit.to_le_bytes());
    }
    array
}

fn write_header(
    disk: &mut [u8],
    current: u64,
    backup: u64,
    first_usable: u64,
    last_usable: u64,
    entries_lba: u64,
    entries: &[u8],
) {
    let offset = current as usize * SECTOR;
    let entries_crc = crc32(entries);
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
