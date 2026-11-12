// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounds-checked GPT scanner.
//!
//! It validates the protective MBR, the primary and backup headers, both entry
//! arrays, the header and array CRCs, usable ranges and non-overlap.  A corrupt
//! table is an error; the caller must not guess partitions.

use alloc::string::String;
use alloc::vec::Vec;

/// GPT sector size.
pub const GPT_SECTOR: usize = 512;
const HEADER_SIZE: usize = 92;
const ENTRY_MIN_SIZE: usize = 128;
const MAX_ENTRIES: usize = 256;
const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
const GPT_REVISION: u32 = 0x0001_0000;

/// Why a GPT image was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GptError {
    /// Image is shorter than the protective MBR, header and entry array.
    TooShort,
    /// The protective MBR signature or partition type is missing.
    NoProtectiveMbr,
    /// A header signature, revision, size or geometry is invalid.
    BadHeader,
    /// A header CRC does not match.
    BadHeaderCrc,
    /// An entry-array CRC does not match.
    BadEntryCrc,
    /// An entry has an invalid range or overlaps another.
    BadEntry,
    /// A UTF-16 partition name is invalid.
    BadName,
    /// The primary and backup headers disagree.
    BackupMismatch,
}

/// One partition from a validated GPT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Partition {
    /// One-based index.
    pub index: u32,
    /// First sector.
    pub start_lba: u64,
    /// Last sector, inclusive.
    pub last_lba: u64,
    /// UTF-8 partition name; empty when the entry has no name.
    pub name: String,
}

impl Partition {
    /// Sector count.
    pub const fn sectors(&self) -> u64 {
        self.last_lba
            .saturating_sub(self.start_lba)
            .saturating_add(1)
    }
}

/// A validated GPT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gpt {
    /// Disk GUID bytes.
    pub disk_guid: [u8; 16],
    /// Partitions in entry order.
    pub partitions: Vec<Partition>,
}

struct Header {
    current_lba: u64,
    backup_lba: u64,
    first_usable: u64,
    last_usable: u64,
    disk_guid: [u8; 16],
    entries_lba: u64,
    entry_count: u32,
    entry_size: u32,
    entries_crc: u32,
}

/// Scan a GPT image.
pub fn scan(disk: &[u8]) -> Result<Gpt, GptError> {
    let sectors = disk.len() / GPT_SECTOR;
    if disk.len() < 34 * GPT_SECTOR || sectors < 34 {
        return Err(GptError::TooShort);
    }
    let mbr = disk.get(..GPT_SECTOR).ok_or(GptError::TooShort)?;
    if mbr[510] != 0x55 || mbr[511] != 0xaa || mbr[450] != 0xee {
        return Err(GptError::NoProtectiveMbr);
    }
    if u32::from_le_bytes(mbr[454..458].try_into().map_err(|_| GptError::TooShort)?) != 1 {
        return Err(GptError::NoProtectiveMbr);
    }

    let primary = parse_header(disk, 1, sectors)?;
    if primary.current_lba != 1 || primary.backup_lba == 1 || primary.backup_lba >= sectors as u64 {
        return Err(GptError::BadHeader);
    }
    let backup = parse_header(disk, primary.backup_lba, sectors)?;
    if backup.current_lba != primary.backup_lba
        || backup.backup_lba != 1
        || backup.disk_guid != primary.disk_guid
        || backup.first_usable != primary.first_usable
        || backup.last_usable != primary.last_usable
        || backup.entry_count != primary.entry_count
        || backup.entry_size != primary.entry_size
        || backup.entries_crc != primary.entries_crc
    {
        return Err(GptError::BackupMismatch);
    }
    if primary.first_usable > primary.last_usable
        || primary.last_usable >= sectors as u64
        || primary.entry_count == 0
        || primary.entry_count as usize > MAX_ENTRIES
        || primary.entry_size as usize != ENTRY_MIN_SIZE
    {
        return Err(GptError::BadHeader);
    }

    let entries = entry_array(disk, &primary)?;
    let backup_entries = entry_array(disk, &backup)?;
    if entries != backup_entries {
        return Err(GptError::BackupMismatch);
    }

    let mut partitions: Vec<Partition> = Vec::new();
    for index in 0..primary.entry_count as usize {
        let offset = index
            .checked_mul(primary.entry_size as usize)
            .ok_or(GptError::BadEntry)?;
        let entry = entries
            .get(offset..offset + ENTRY_MIN_SIZE)
            .ok_or(GptError::BadEntry)?;
        let type_guid = entry.get(..16).ok_or(GptError::BadEntry)?;
        if type_guid.iter().all(|byte| *byte == 0) {
            continue;
        }
        let first = u64::from_le_bytes(entry[32..40].try_into().map_err(|_| GptError::BadEntry)?);
        let last = u64::from_le_bytes(entry[40..48].try_into().map_err(|_| GptError::BadEntry)?);
        if first < primary.first_usable || last > primary.last_usable || first > last {
            return Err(GptError::BadEntry);
        }
        for previous in &partitions {
            if first <= previous.last_lba && previous.start_lba <= last {
                return Err(GptError::BadEntry);
            }
        }
        let name = decode_name(&entry[56..128])?;
        partitions.push(Partition {
            index: u32::try_from(index + 1).map_err(|_| GptError::BadEntry)?,
            start_lba: first,
            last_lba: last,
            name,
        });
    }
    Ok(Gpt {
        disk_guid: primary.disk_guid,
        partitions,
    })
}

fn parse_header(disk: &[u8], lba: u64, sectors: usize) -> Result<Header, GptError> {
    let offset = usize::try_from(lba)
        .map_err(|_| GptError::BadHeader)?
        .checked_mul(GPT_SECTOR)
        .ok_or(GptError::BadHeader)?;
    let sector = disk
        .get(offset..offset + GPT_SECTOR)
        .ok_or(GptError::BadHeader)?;
    if sector.get(..8) != Some(GPT_SIGNATURE) {
        return Err(GptError::BadHeader);
    }
    let revision = u32::from_le_bytes(sector[8..12].try_into().map_err(|_| GptError::BadHeader)?);
    if revision != GPT_REVISION {
        return Err(GptError::BadHeader);
    }
    let size = u32::from_le_bytes(sector[12..16].try_into().map_err(|_| GptError::BadHeader)?);
    if !(HEADER_SIZE as u32..=GPT_SECTOR as u32).contains(&size) {
        return Err(GptError::BadHeader);
    }
    let size = size as usize;
    let expected = u32::from_le_bytes(sector[16..20].try_into().map_err(|_| GptError::BadHeader)?);
    let mut header = sector[..size].to_vec();
    header[16..20].fill(0);
    if crc32(&header) != expected {
        return Err(GptError::BadHeaderCrc);
    }
    let current_lba =
        u64::from_le_bytes(sector[24..32].try_into().map_err(|_| GptError::BadHeader)?);
    let backup_lba =
        u64::from_le_bytes(sector[32..40].try_into().map_err(|_| GptError::BadHeader)?);
    let first_usable =
        u64::from_le_bytes(sector[40..48].try_into().map_err(|_| GptError::BadHeader)?);
    let last_usable =
        u64::from_le_bytes(sector[48..56].try_into().map_err(|_| GptError::BadHeader)?);
    let mut disk_guid = [0u8; 16];
    disk_guid.copy_from_slice(&sector[56..72]);
    let entries_lba =
        u64::from_le_bytes(sector[72..80].try_into().map_err(|_| GptError::BadHeader)?);
    let entry_count =
        u32::from_le_bytes(sector[80..84].try_into().map_err(|_| GptError::BadHeader)?);
    let entry_size =
        u32::from_le_bytes(sector[84..88].try_into().map_err(|_| GptError::BadHeader)?);
    let entries_crc =
        u32::from_le_bytes(sector[88..92].try_into().map_err(|_| GptError::BadHeader)?);
    if current_lba != lba
        || backup_lba >= sectors as u64
        || entries_lba < 2
        || entries_lba >= sectors as u64
        || entry_count == 0
        || entry_count as usize > MAX_ENTRIES
        || entry_size as usize != ENTRY_MIN_SIZE
    {
        return Err(GptError::BadHeader);
    }
    let array_len = (entry_count as usize)
        .checked_mul(entry_size as usize)
        .ok_or(GptError::BadHeader)?;
    let array_offset = usize::try_from(entries_lba)
        .map_err(|_| GptError::BadHeader)?
        .checked_mul(GPT_SECTOR)
        .ok_or(GptError::BadHeader)?;
    if array_offset
        .checked_add(array_len)
        .is_none_or(|end| end > disk.len())
    {
        return Err(GptError::BadHeader);
    }
    Ok(Header {
        current_lba,
        backup_lba,
        first_usable,
        last_usable,
        disk_guid,
        entries_lba,
        entry_count,
        entry_size,
        entries_crc,
    })
}

fn entry_array<'a>(disk: &'a [u8], header: &Header) -> Result<&'a [u8], GptError> {
    let length = (header.entry_count as usize)
        .checked_mul(header.entry_size as usize)
        .ok_or(GptError::BadHeader)?;
    let offset = usize::try_from(header.entries_lba)
        .map_err(|_| GptError::BadHeader)?
        .checked_mul(GPT_SECTOR)
        .ok_or(GptError::BadHeader)?;
    let end = offset.checked_add(length).ok_or(GptError::BadHeader)?;
    let entries = disk.get(offset..end).ok_or(GptError::BadHeader)?;
    if crc32(entries) != header.entries_crc {
        return Err(GptError::BadEntryCrc);
    }
    Ok(entries)
}

fn decode_name(bytes: &[u8]) -> Result<String, GptError> {
    let mut units = Vec::new();
    for pair in bytes.chunks_exact(2).take(36) {
        let unit = u16::from_le_bytes([pair[0], pair[1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    String::from_utf16(&units).map_err(|_| GptError::BadName)
}

/// Standard reflected CRC-32 (the polynomial used by GPT).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
#[path = "gpt_ut.rs"]
mod tests;
