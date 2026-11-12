// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GPT scanner tests built from a deterministic in-memory table.

use super::*;
use alloc::vec;

const TOTAL: u64 = 2048;
const PRIMARY_ENTRIES: u64 = 2;
const BACKUP_ENTRIES: u64 = 2015;
const BACKUP_HEADER: u64 = 2047;

fn put_u32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn write_entry(array: &mut [u8], index: usize, start: u64, end: u64, name: &str) {
    let offset = index * 128;
    array[offset] = 0xaf; // nonzero type GUID
    put_u64(array, offset + 32, start);
    put_u64(array, offset + 40, end);
    for (slot, unit) in array[offset + 56..offset + 128]
        .chunks_exact_mut(2)
        .zip(name.encode_utf16())
    {
        slot.copy_from_slice(&unit.to_le_bytes());
    }
}

fn refresh_header_crc(disk: &mut [u8], lba: u64) {
    let offset = lba as usize * GPT_SECTOR;
    put_u32(disk, offset + 16, 0);
    let checksum = crc32(&disk[offset..offset + HEADER_SIZE]);
    put_u32(disk, offset + 16, checksum);
}

fn write_header(disk: &mut [u8], lba: u64, entries_lba: u64) {
    let offset = lba as usize * GPT_SECTOR;
    let array_offset = entries_lba as usize * GPT_SECTOR;
    let entries_crc = crc32(&disk[array_offset..array_offset + 128 * 128]);
    let header = &mut disk[offset..offset + GPT_SECTOR];
    header[..8].copy_from_slice(GPT_SIGNATURE);
    put_u32(header, 8, GPT_REVISION);
    put_u32(header, 12, HEADER_SIZE as u32);
    put_u32(header, 24, u32::try_from(lba).expect("lba"));
    put_u32(header, 32, if lba == 1 { BACKUP_HEADER as u32 } else { 1 });
    put_u64(header, 40, 34);
    put_u64(header, 48, 2014);
    header[56..72].copy_from_slice(&[0x11; 16]);
    put_u64(header, 72, entries_lba);
    put_u32(header, 80, 128);
    put_u32(header, 84, 128);
    put_u32(header, 88, entries_crc);
    put_u32(header, 16, 0);
    let checksum = crc32(&header[..HEADER_SIZE]);
    put_u32(header, 16, checksum);
}

fn valid_image() -> Vec<u8> {
    let mut disk = vec![0u8; TOTAL as usize * GPT_SECTOR];
    disk[510] = 0x55;
    disk[511] = 0xaa;
    disk[450] = 0xee;
    put_u32(&mut disk, 454, 1);
    put_u32(&mut disk, 458, (TOTAL - 1) as u32);
    let mut primary = vec![0u8; 128 * 128];
    write_entry(&mut primary, 0, 34, 100, "first");
    write_entry(&mut primary, 1, 101, 200, "second");
    let mut backup = primary.clone();
    write_entry(&mut backup, 0, 34, 100, "first");
    write_entry(&mut backup, 1, 101, 200, "second");
    disk[PRIMARY_ENTRIES as usize * GPT_SECTOR
        ..PRIMARY_ENTRIES as usize * GPT_SECTOR + primary.len()]
        .copy_from_slice(&primary);
    disk[BACKUP_ENTRIES as usize * GPT_SECTOR..BACKUP_ENTRIES as usize * GPT_SECTOR + backup.len()]
        .copy_from_slice(&backup);
    write_header(&mut disk, 1, PRIMARY_ENTRIES);
    write_header(&mut disk, BACKUP_HEADER, BACKUP_ENTRIES);
    disk
}

#[test]
fn valid_table_scans_two_partitions() {
    let gpt = scan(&valid_image()).expect("gpt");
    assert_eq!(gpt.partitions.len(), 2);
    assert_eq!(gpt.partitions[0].index, 1);
    assert_eq!(gpt.partitions[0].start_lba, 34);
    assert_eq!(gpt.partitions[0].last_lba, 100);
    assert_eq!(gpt.partitions[0].name, "first");
    assert_eq!(gpt.partitions[1].name, "second");
    assert_eq!(gpt.partitions[1].sectors(), 100);
}

#[test]
fn short_image_is_rejected() {
    assert_eq!(scan(&[0u8; 512]), Err(GptError::TooShort));
}

#[test]
fn missing_protective_mbr_is_rejected() {
    let mut disk = valid_image();
    disk[450] = 0;
    assert_eq!(scan(&disk), Err(GptError::NoProtectiveMbr));
}

#[test]
fn corrupted_primary_header_crc_is_rejected() {
    let mut disk = valid_image();
    disk[512 + 24] ^= 0x40;
    assert_eq!(scan(&disk), Err(GptError::BadHeaderCrc));
}

#[test]
fn corrupted_primary_entries_crc_is_rejected() {
    let mut disk = valid_image();
    disk[PRIMARY_ENTRIES as usize * GPT_SECTOR + 1] ^= 0x40;
    assert_eq!(scan(&disk), Err(GptError::BadEntryCrc));
}

#[test]
fn corrupted_backup_header_is_rejected() {
    let mut disk = valid_image();
    disk[BACKUP_HEADER as usize * GPT_SECTOR + 56] ^= 0x40;
    refresh_header_crc(&mut disk, BACKUP_HEADER);
    assert_eq!(scan(&disk), Err(GptError::BackupMismatch));
}

#[test]
fn backup_array_disagreement_is_rejected() {
    let mut disk = valid_image();
    disk[BACKUP_ENTRIES as usize * GPT_SECTOR + 1] ^= 0x40;
    assert_eq!(scan(&disk), Err(GptError::BadEntryCrc));
}

#[test]
fn overlapping_entries_are_rejected() {
    let mut disk = valid_image();
    for base in [PRIMARY_ENTRIES, BACKUP_ENTRIES] {
        let array = base as usize * GPT_SECTOR + 128;
        put_u64(&mut disk, array + 32, 90);
        put_u64(&mut disk, array + 40, 110);
    }
    write_header(&mut disk, 1, PRIMARY_ENTRIES);
    write_header(&mut disk, BACKUP_HEADER, BACKUP_ENTRIES);
    assert_eq!(scan(&disk), Err(GptError::BadEntry));
}

#[test]
fn entry_outside_usable_range_is_rejected() {
    let mut disk = valid_image();
    for base in [PRIMARY_ENTRIES, BACKUP_ENTRIES] {
        let array = base as usize * GPT_SECTOR;
        put_u64(&mut disk, array + 32, 33);
    }
    write_header(&mut disk, 1, PRIMARY_ENTRIES);
    write_header(&mut disk, BACKUP_HEADER, BACKUP_ENTRIES);
    assert_eq!(scan(&disk), Err(GptError::BadEntry));
}

#[test]
fn empty_entry_is_skipped() {
    let mut disk = valid_image();
    disk[BACKUP_ENTRIES as usize * GPT_SECTOR + 128] = 0;
    // Both arrays must be changed before the backup is rechecksummed.
    disk[PRIMARY_ENTRIES as usize * GPT_SECTOR + 128] = 0;
    write_header(&mut disk, 1, PRIMARY_ENTRIES);
    write_header(&mut disk, BACKUP_HEADER, BACKUP_ENTRIES);
    let gpt = scan(&disk).expect("gpt");
    assert_eq!(gpt.partitions.len(), 1);
}

#[test]
fn invalid_utf16_name_is_rejected() {
    let mut disk = valid_image();
    for base in [PRIMARY_ENTRIES, BACKUP_ENTRIES] {
        let array = base as usize * GPT_SECTOR + 56;
        disk[array] = 0x00;
        disk[array + 1] = 0xd8;
    }
    write_header(&mut disk, 1, PRIMARY_ENTRIES);
    write_header(&mut disk, BACKUP_HEADER, BACKUP_ENTRIES);
    assert_eq!(scan(&disk), Err(GptError::BadName));
}

#[test]
fn bad_backup_lba_is_rejected() {
    let mut disk = valid_image();
    put_u32(&mut disk, 512 + 32, 1);
    refresh_header_crc(&mut disk, 1);
    assert_eq!(scan(&disk), Err(GptError::BadHeader));
}
