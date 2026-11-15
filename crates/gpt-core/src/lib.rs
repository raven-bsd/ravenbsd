// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Read-only, panic-free GUID Partition Table parser.
//!
//! The parser accepts a complete disk image (or any byte slice at least as long as the GPT
//! structures it describes) and validates the protective MBR, the primary header, the header
//! CRC, the entry-array size and CRC, every used entry's LBA range and the absence of overlaps.
//! It never writes, never allocates from untrusted lengths without a bound and returns
//! [`GptError`] for every malformed input. The library is `no_std` + `alloc` and contains no
//! `unsafe` code.
//!
//! The on-disk field offsets and constants are facts from the UEFI specification; the
//! implementation is fresh BSD-3-Clause code. The crate deliberately parses only the primary
//! structures: a valid backup header is not required for a read-only mount, and the backup
//! location is still bounds-checked.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;

use alloc::string::String;
use alloc::vec::Vec;

/// On-disk sector size of the images this parser accepts.
pub const SECTOR_SIZE: usize = 512;

/// GPT header signature, `EFI PART`.
pub const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";

/// The only supported GPT revision (1.0).
pub const GPT_REVISION: u32 = 0x0001_0000;

/// Minimum size of a GPT header.
pub const HEADER_MIN_SIZE: u32 = 92;

/// Minimum size of one GPT entry.
pub const ENTRY_MIN_SIZE: usize = 128;

/// Upper bound accepted for the entry count and table size.
const MAX_ENTRIES: u32 = 4096;
const MAX_ENTRY_SIZE: u32 = 4096;
const MAX_TABLE_BYTES: usize = 1024 * 1024;

/// A malformed or unsupported GPT image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GptError {
    /// The image is shorter than two sectors or a required structure is truncated.
    TooShort,
    /// The protective MBR signature, type or LBA fields are invalid.
    BadProtectiveMbr,
    /// The GPT header signature is not `EFI PART`.
    BadSignature,
    /// The GPT revision is not 1.0.
    UnsupportedRevision(u32),
    /// The header size is outside 92..=512.
    BadHeaderSize(u32),
    /// The header extends past the image.
    HeaderOutOfBounds,
    /// The stored header CRC does not match the header bytes.
    HeaderCrc,
    /// The header's `my_lba` is not the primary header location (LBA 1).
    WrongHeaderLba(u64),
    /// The alternate/backup LBA is outside the disk or is the primary header.
    BadAlternateLba(u64),
    /// `first_usable_lba`/`last_usable_lba` are inverted or past the disk.
    BadUsableRange,
    /// The entry size is not a power of two in 128..=4096.
    BadEntrySize(u32),
    /// The entry count is zero or exceeds the parser bound.
    BadEntryCount(u32),
    /// The entry table is larger than the parser bound or overflows.
    TableTooLarge,
    /// The entry table extends past the image.
    EntryArrayOutOfBounds,
    /// The stored entry-array CRC does not match the table bytes.
    EntryArrayCrc,
    /// A used entry has an inverted, out-of-range or overlapping LBA range.
    EntryOutOfRange {
        /// Zero-based entry index.
        index: u32,
    },
    /// Two used entries overlap.
    OverlappingEntries,
}

/// One used GPT partition entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartitionEntry {
    /// Partition type GUID, mixed-endian as stored on disk.
    pub type_guid: [u8; 16],
    /// Unique partition GUID, mixed-endian as stored on disk.
    pub unique_guid: [u8; 16],
    /// First LBA (inclusive).
    pub first_lba: u64,
    /// Last LBA (inclusive).
    pub last_lba: u64,
    /// Attribute bits.
    pub attributes: u64,
    /// UTF-16 partition name with trailing NULs removed.
    pub name: String,
}

/// A parsed GPT disk.
pub struct Gpt {
    total_sectors: u64,
    first_usable: u64,
    last_usable: u64,
    disk_guid: [u8; 16],
    revision: u32,
    header_size: u32,
    header_crc: u32,
    entry_crc: u32,
    partitions: Vec<PartitionEntry>,
}

impl Gpt {
    /// Parse the protective MBR, primary GPT header and entry array from `disk`.
    ///
    /// # Errors
    /// Returns [`GptError`] for any malformed, truncated, unsupported or overflowing image.
    pub fn parse(disk: &[u8]) -> Result<Self, GptError> {
        if disk.len() < 2 * SECTOR_SIZE {
            return Err(GptError::TooShort);
        }
        let total_sectors = (disk.len() / SECTOR_SIZE) as u64;
        validate_protective_mbr(disk)?;

        let header_size = u32_at(disk, SECTOR_SIZE + 12).ok_or(GptError::TooShort)?;
        if !(HEADER_MIN_SIZE..=SECTOR_SIZE as u32).contains(&header_size) {
            return Err(GptError::BadHeaderSize(header_size));
        }
        let header_end = SECTOR_SIZE
            .checked_add(header_size as usize)
            .ok_or(GptError::HeaderOutOfBounds)?;
        let header_bytes = disk
            .get(SECTOR_SIZE..header_end)
            .ok_or(GptError::HeaderOutOfBounds)?;

        if header_bytes.get(..8) != Some(GPT_SIGNATURE.as_slice()) {
            return Err(GptError::BadSignature);
        }
        let revision = u32_at(header_bytes, 8).ok_or(GptError::TooShort)?;
        if revision != GPT_REVISION {
            return Err(GptError::UnsupportedRevision(revision));
        }
        let header_crc = u32_at(header_bytes, 16).ok_or(GptError::TooShort)?;
        let mut checked = header_bytes.to_vec();
        if checked.len() < 20 {
            return Err(GptError::TooShort);
        }
        checked[16..20].fill(0);
        if crc32(&checked) != header_crc {
            return Err(GptError::HeaderCrc);
        }

        let my_lba = u64_at(header_bytes, 24).ok_or(GptError::TooShort)?;
        if my_lba != 1 {
            return Err(GptError::WrongHeaderLba(my_lba));
        }
        let alternate_lba = u64_at(header_bytes, 32).ok_or(GptError::TooShort)?;
        if alternate_lba == 0 || alternate_lba == my_lba || alternate_lba >= total_sectors {
            return Err(GptError::BadAlternateLba(alternate_lba));
        }
        let first_usable = u64_at(header_bytes, 40).ok_or(GptError::TooShort)?;
        let last_usable = u64_at(header_bytes, 48).ok_or(GptError::TooShort)?;
        if first_usable == 0 || first_usable > last_usable || last_usable >= total_sectors {
            return Err(GptError::BadUsableRange);
        }
        let mut disk_guid = [0u8; 16];
        disk_guid.copy_from_slice(header_bytes.get(56..72).ok_or(GptError::TooShort)?);
        let entries_lba = u64_at(header_bytes, 72).ok_or(GptError::TooShort)?;
        let entry_count = u32_at(header_bytes, 80).ok_or(GptError::TooShort)?;
        let entry_size = u32_at(header_bytes, 84).ok_or(GptError::TooShort)?;
        let entry_crc = u32_at(header_bytes, 88).ok_or(GptError::TooShort)?;

        if entry_count == 0 || entry_count > MAX_ENTRIES {
            return Err(GptError::BadEntryCount(entry_count));
        }
        if entry_size < ENTRY_MIN_SIZE as u32
            || entry_size > MAX_ENTRY_SIZE
            || !entry_size.is_power_of_two()
        {
            return Err(GptError::BadEntrySize(entry_size));
        }
        let table_len = (entry_count as usize)
            .checked_mul(entry_size as usize)
            .ok_or(GptError::TableTooLarge)?;
        if table_len == 0 || table_len > MAX_TABLE_BYTES {
            return Err(GptError::TableTooLarge);
        }
        let table_sectors = (table_len as u64).div_ceil(SECTOR_SIZE as u64);
        if entries_lba < 2
            || entries_lba
                .checked_add(table_sectors)
                .is_none_or(|end| end > first_usable)
        {
            return Err(GptError::EntryArrayOutOfBounds);
        }
        let table_start = usize::try_from(entries_lba)
            .ok()
            .and_then(|value| value.checked_mul(SECTOR_SIZE))
            .ok_or(GptError::EntryArrayOutOfBounds)?;
        let table_end = table_start
            .checked_add(table_len)
            .ok_or(GptError::EntryArrayOutOfBounds)?;
        let table = disk
            .get(table_start..table_end)
            .ok_or(GptError::EntryArrayOutOfBounds)?;
        if crc32(table) != entry_crc {
            return Err(GptError::EntryArrayCrc);
        }

        let partitions = parse_entries(
            table,
            entry_count,
            entry_size as usize,
            first_usable,
            last_usable,
        )?;
        Ok(Self {
            total_sectors,
            first_usable,
            last_usable,
            disk_guid,
            revision,
            header_size,
            header_crc,
            entry_crc,
            partitions,
        })
    }

    /// Number of 512-byte sectors covered by the image.
    #[must_use]
    pub fn total_sectors(&self) -> u64 {
        self.total_sectors
    }

    /// First usable LBA from the header.
    #[must_use]
    pub fn first_usable_lba(&self) -> u64 {
        self.first_usable
    }

    /// Last usable LBA from the header.
    #[must_use]
    pub fn last_usable_lba(&self) -> u64 {
        self.last_usable
    }

    /// Disk GUID from the header.
    #[must_use]
    pub fn disk_guid(&self) -> [u8; 16] {
        self.disk_guid
    }

    /// Header revision (always 1.0 after a successful parse).
    #[must_use]
    pub fn revision(&self) -> u32 {
        self.revision
    }

    /// Header size in bytes.
    #[must_use]
    pub fn header_size(&self) -> u32 {
        self.header_size
    }

    /// Header CRC as stored on disk.
    #[must_use]
    pub fn header_crc(&self) -> u32 {
        self.header_crc
    }

    /// Entry-array CRC as stored on disk.
    #[must_use]
    pub fn entry_crc(&self) -> u32 {
        self.entry_crc
    }

    /// Used partition entries in table order.
    #[must_use]
    pub fn partitions(&self) -> &[PartitionEntry] {
        &self.partitions
    }
}

fn validate_protective_mbr(disk: &[u8]) -> Result<(), GptError> {
    let sector = disk.get(..SECTOR_SIZE).ok_or(GptError::TooShort)?;
    if sector.get(510..512) != Some(&[0x55, 0xaa]) {
        return Err(GptError::BadProtectiveMbr);
    }
    let entry = sector.get(446..462).ok_or(GptError::BadProtectiveMbr)?;
    if entry[4] != 0xee {
        return Err(GptError::BadProtectiveMbr);
    }
    let start = u32_at(entry, 8).ok_or(GptError::BadProtectiveMbr)?;
    let sectors = u32_at(entry, 12).ok_or(GptError::BadProtectiveMbr)?;
    if start != 1 || sectors == 0 {
        return Err(GptError::BadProtectiveMbr);
    }
    Ok(())
}

fn parse_entries(
    table: &[u8],
    entry_count: u32,
    entry_size: usize,
    first_usable: u64,
    last_usable: u64,
) -> Result<Vec<PartitionEntry>, GptError> {
    let mut partitions = Vec::new();
    let mut ranges = Vec::new();
    for index in 0..entry_count {
        let start = (index as usize)
            .checked_mul(entry_size)
            .ok_or(GptError::TableTooLarge)?;
        let end = start
            .checked_add(entry_size)
            .ok_or(GptError::TableTooLarge)?;
        let entry = table
            .get(start..end)
            .ok_or(GptError::EntryArrayOutOfBounds)?;
        if entry.get(..16) == Some(&[0u8; 16]) {
            continue;
        }
        let first_lba = u64_at(entry, 32).ok_or(GptError::EntryOutOfRange { index })?;
        let last_lba = u64_at(entry, 40).ok_or(GptError::EntryOutOfRange { index })?;
        if first_lba > last_lba || first_lba < first_usable || last_lba > last_usable {
            return Err(GptError::EntryOutOfRange { index });
        }
        ranges.push((first_lba, last_lba));

        let mut type_guid = [0u8; 16];
        type_guid.copy_from_slice(entry.get(..16).ok_or(GptError::EntryOutOfRange { index })?);
        let mut unique_guid = [0u8; 16];
        unique_guid.copy_from_slice(
            entry
                .get(16..32)
                .ok_or(GptError::EntryOutOfRange { index })?,
        );
        let attributes = u64_at(entry, 48).ok_or(GptError::EntryOutOfRange { index })?;
        let name_bytes = entry.get(56..entry.len()).unwrap_or(&[]);
        partitions.push(PartitionEntry {
            type_guid,
            unique_guid,
            first_lba,
            last_lba,
            attributes,
            name: utf16_name(name_bytes),
        });
    }
    ranges.sort_unstable();
    for pair in ranges.windows(2) {
        if pair[0].1 >= pair[1].0 {
            return Err(GptError::OverlappingEntries);
        }
    }
    Ok(partitions)
}

fn utf16_name(bytes: &[u8]) -> String {
    let mut units = Vec::new();
    for chunk in bytes.chunks_exact(2) {
        units.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    while units.last() == Some(&0) {
        units.pop();
    }
    char::decode_utf16(units)
        .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn u64_at(bytes: &[u8], offset: usize) -> Option<u64> {
    let slice = bytes.get(offset..offset.checked_add(8)?)?;
    let mut value = [0u8; 8];
    value.copy_from_slice(slice);
    Some(u64::from_le_bytes(value))
}

/// IEEE CRC-32, as used by the GPT header and entry array.
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
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
