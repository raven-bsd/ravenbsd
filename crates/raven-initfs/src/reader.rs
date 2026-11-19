// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Borrowing, allocation-free reader for the initfs format.

use core::cmp::Ordering;

use crate::crc::crc32;
use crate::format::{
    DEFAULT_EPOCH, ENTRY_CRC_OFFSET, ENTRY_DATA_LEN_OFFSET, ENTRY_DATA_OFF_OFFSET,
    ENTRY_MODE_OFFSET, ENTRY_MTIME_OFFSET, ENTRY_NAME_LEN_OFFSET, ENTRY_NAME_OFF_OFFSET,
    ENTRY_RESERVED_LEN, ENTRY_RESERVED_OFFSET, ENTRY_SIZE, HEADER_ENTRY_COUNT_OFFSET,
    HEADER_ENTRY_SIZE_OFFSET, HEADER_EPOCH_OFFSET, HEADER_NAMES_LEN_OFFSET, HEADER_NAMES_OFFSET,
    HEADER_RESERVED_LEN, HEADER_RESERVED_OFFSET, HEADER_SIZE, HEADER_SIZE_OFFSET,
    HEADER_TOTAL_SIZE_OFFSET, HEADER_VERSION_OFFSET, LEGACY_VERSION, MAGIC, PAGE_SIZE, VERSION,
    align_up, read_u32, read_u64, validate_path,
};

/// One file inside an [`Initfs`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Entry<'a> {
    /// Absolute path inside the archive, starting with `/`.
    pub path: &'a str,
    /// The file contents, borrowed from the image.
    pub data: &'a [u8],
    /// Unix permission bits. Informational; the kernel does not enforce them.
    pub mode: u32,
    /// Modification time in seconds since the Unix epoch.
    ///
    /// A version 2 entry that records zero inherits the image epoch; a version 1 image uses the
    /// documented default epoch, never zero.
    pub mtime: u64,
    /// Offset of [`Entry::data`] from the start of the image.
    pub data_offset: u64,
}

/// Why an initfs image was rejected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InitfsError {
    /// The image is shorter than the 64-byte header.
    TooShort,
    /// The magic does not spell `RAVENFS1`.
    BadMagic,
    /// The format version is not supported.
    BadVersion,
    /// A header field is inconsistent, or the entry table or names blob is out of bounds.
    BadHeader,
    /// Entry `u32` is malformed: bad name range, invalid path or UTF-8, bad aligned or reserved
    /// field.
    BadEntry(u32),
    /// The data of entry `u32` does not match its CRC-32.
    BadChecksum(u32),
    /// Names or data regions are not strictly increasing in entry order.
    Unsorted,
    /// The data region of entry `u32` lies outside the image.
    OutOfBounds(u32),
}

/// A parsed initfs image.
///
/// [`Initfs::parse`] validates every entry's CRC up front; [`Initfs::parse_unchecked`] checks
/// the structure only, and [`Initfs::get`] verifies each entry it returns.
#[derive(Clone, Copy, Debug)]
pub struct Initfs<'a> {
    image: &'a [u8],
    entry_count: usize,
    names_offset: usize,
    version: u32,
    image_epoch: u64,
}

impl<'a> Initfs<'a> {
    /// Parse and fully validate an initfs image.
    ///
    /// All arithmetic is checked and no input can make this function panic.
    pub fn parse(image: &'a [u8]) -> Result<Initfs<'a>, InitfsError> {
        Self::parse_inner(image, true)
    }

    /// Parse the structure without computing any entry CRC.
    ///
    /// The header, entry table, names, paths and data ranges are validated, so every accessor
    /// stays total. Use this only when the image was validated elsewhere (the kernel validates
    /// the boot image before `/init`); [`Initfs::get`] still verifies each entry it returns.
    ///
    /// All arithmetic is checked and no input can make this function panic.
    pub fn parse_unchecked(image: &'a [u8]) -> Result<Initfs<'a>, InitfsError> {
        Self::parse_inner(image, false)
    }

    fn parse_inner(image: &'a [u8], validate_crc: bool) -> Result<Initfs<'a>, InitfsError> {
        if image.len() < HEADER_SIZE {
            return Err(InitfsError::TooShort);
        }
        if image.get(..MAGIC.len()) != Some(&MAGIC[..]) {
            return Err(InitfsError::BadMagic);
        }

        let version = read_u32(image, HEADER_VERSION_OFFSET).ok_or(InitfsError::BadHeader)?;
        if version != LEGACY_VERSION && version != VERSION {
            return Err(InitfsError::BadVersion);
        }

        let header_size = read_u32(image, HEADER_SIZE_OFFSET).ok_or(InitfsError::BadHeader)?;
        if header_size as usize != HEADER_SIZE {
            return Err(InitfsError::BadHeader);
        }

        let entry_count_u32 =
            read_u32(image, HEADER_ENTRY_COUNT_OFFSET).ok_or(InitfsError::BadHeader)?;
        let entry_size = read_u32(image, HEADER_ENTRY_SIZE_OFFSET).ok_or(InitfsError::BadHeader)?;
        if entry_size as usize != ENTRY_SIZE {
            return Err(InitfsError::BadHeader);
        }

        let names_offset_u64 =
            read_u64(image, HEADER_NAMES_OFFSET).ok_or(InitfsError::BadHeader)?;
        let names_len_u64 =
            read_u64(image, HEADER_NAMES_LEN_OFFSET).ok_or(InitfsError::BadHeader)?;
        let total_size_u64 =
            read_u64(image, HEADER_TOTAL_SIZE_OFFSET).ok_or(InitfsError::BadHeader)?;

        let image_epoch = if version == VERSION {
            let epoch = read_u64(image, HEADER_EPOCH_OFFSET).ok_or(InitfsError::BadHeader)?;
            let reserved = image
                .get(HEADER_RESERVED_OFFSET..HEADER_RESERVED_OFFSET + HEADER_RESERVED_LEN)
                .ok_or(InitfsError::BadHeader)?;
            if reserved.iter().any(|&byte| byte != 0) {
                return Err(InitfsError::BadHeader);
            }
            epoch
        } else {
            // Version 1 has no epoch field: the whole 16-byte tail is reserved and must be zero.
            let legacy = image
                .get(HEADER_EPOCH_OFFSET..HEADER_EPOCH_OFFSET + 16)
                .ok_or(InitfsError::BadHeader)?;
            if legacy.iter().any(|&byte| byte != 0) {
                return Err(InitfsError::BadHeader);
            }
            DEFAULT_EPOCH
        };

        let image_len = u64::try_from(image.len()).map_err(|_| InitfsError::BadHeader)?;
        if total_size_u64 != image_len || total_size_u64 % PAGE_SIZE as u64 != 0 {
            return Err(InitfsError::BadHeader);
        }

        let entry_count = usize::try_from(entry_count_u32).map_err(|_| InitfsError::BadHeader)?;
        let names_offset = usize::try_from(names_offset_u64).map_err(|_| InitfsError::BadHeader)?;
        let names_len = usize::try_from(names_len_u64).map_err(|_| InitfsError::BadHeader)?;

        let table_len = entry_count
            .checked_mul(ENTRY_SIZE)
            .ok_or(InitfsError::BadHeader)?;
        let table_end = HEADER_SIZE
            .checked_add(table_len)
            .ok_or(InitfsError::BadHeader)?;
        if table_end > image.len() {
            return Err(InitfsError::BadHeader);
        }
        if names_offset < table_end {
            return Err(InitfsError::BadHeader);
        }

        let names_end = names_offset
            .checked_add(names_len)
            .ok_or(InitfsError::BadHeader)?;
        if names_end > image.len() {
            return Err(InitfsError::BadHeader);
        }
        let data_start = align_up(names_end, PAGE_SIZE).ok_or(InitfsError::BadHeader)?;
        if data_start > image.len() {
            return Err(InitfsError::BadHeader);
        }

        let mut previous_name: Option<&[u8]> = None;
        let mut previous_data_end = data_start as u64;

        for index in 0..entry_count {
            let entry_index = u32::try_from(index).map_err(|_| InitfsError::BadHeader)?;
            let base = HEADER_SIZE
                .checked_add(
                    index
                        .checked_mul(ENTRY_SIZE)
                        .ok_or(InitfsError::BadHeader)?,
                )
                .ok_or(InitfsError::BadHeader)?;

            let name_off = read_u32(image, base + ENTRY_NAME_OFF_OFFSET)
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let name_len = read_u32(image, base + ENTRY_NAME_LEN_OFFSET)
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let data_off = read_u64(image, base + ENTRY_DATA_OFF_OFFSET)
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let data_len = read_u64(image, base + ENTRY_DATA_LEN_OFFSET)
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let crc = read_u32(image, base + ENTRY_CRC_OFFSET)
                .ok_or(InitfsError::BadEntry(entry_index))?;

            if version == VERSION {
                let reserved = image
                    .get(
                        base + ENTRY_RESERVED_OFFSET
                            ..base + ENTRY_RESERVED_OFFSET + ENTRY_RESERVED_LEN,
                    )
                    .ok_or(InitfsError::BadEntry(entry_index))?;
                if reserved.iter().any(|&byte| byte != 0) {
                    return Err(InitfsError::BadEntry(entry_index));
                }
                let _mtime = read_u64(image, base + ENTRY_MTIME_OFFSET)
                    .ok_or(InitfsError::BadEntry(entry_index))?;
            } else {
                // Version 1 has no mtime field: the whole 16-byte tail is reserved.
                let legacy = image
                    .get(base + ENTRY_MTIME_OFFSET..base + ENTRY_MTIME_OFFSET + 16)
                    .ok_or(InitfsError::BadEntry(entry_index))?;
                if legacy.iter().any(|&byte| byte != 0) {
                    return Err(InitfsError::BadEntry(entry_index));
                }
            }

            let name_off = u64::from(name_off);
            let name_len = u64::from(name_len);
            let name_end = name_off
                .checked_add(name_len)
                .ok_or(InitfsError::BadEntry(entry_index))?;
            if name_end > names_len as u64 {
                return Err(InitfsError::BadEntry(entry_index));
            }

            let name_start = names_offset
                .checked_add(
                    usize::try_from(name_off).map_err(|_| InitfsError::BadEntry(entry_index))?,
                )
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let name_end = name_start
                .checked_add(
                    usize::try_from(name_len).map_err(|_| InitfsError::BadEntry(entry_index))?,
                )
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let name_bytes = image
                .get(name_start..name_end)
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let path =
                core::str::from_utf8(name_bytes).map_err(|_| InitfsError::BadEntry(entry_index))?;
            validate_path(path).map_err(|_| InitfsError::BadEntry(entry_index))?;

            if let Some(previous) = previous_name
                && previous >= name_bytes
            {
                return Err(InitfsError::Unsorted);
            }
            previous_name = Some(name_bytes);

            if data_off % PAGE_SIZE as u64 != 0 {
                return Err(InitfsError::BadEntry(entry_index));
            }
            let data_end = data_off
                .checked_add(data_len)
                .ok_or(InitfsError::OutOfBounds(entry_index))?;
            if data_end > total_size_u64 {
                return Err(InitfsError::OutOfBounds(entry_index));
            }
            if data_off < data_start as u64 {
                return Err(InitfsError::BadEntry(entry_index));
            }
            if data_off < previous_data_end {
                return Err(InitfsError::Unsorted);
            }
            previous_data_end = data_end;

            let data_begin =
                usize::try_from(data_off).map_err(|_| InitfsError::OutOfBounds(entry_index))?;
            let data_end =
                usize::try_from(data_end).map_err(|_| InitfsError::OutOfBounds(entry_index))?;
            let data = image
                .get(data_begin..data_end)
                .ok_or(InitfsError::OutOfBounds(entry_index))?;
            if validate_crc && crc32(data) != crc {
                return Err(InitfsError::BadChecksum(entry_index));
            }
        }

        Ok(Initfs {
            image,
            entry_count,
            names_offset,
            version,
            image_epoch,
        })
    }

    /// Look up an exact path and verify the entry's CRC-32.
    ///
    /// Returns `None` when the path is absent or the entry fails its checksum. Callers that only
    /// need metadata can use [`Initfs::entries`]; this is the data-returning lookup.
    pub fn get(&self, path: &str) -> Option<Entry<'a>> {
        let index = self.find_index(path)?;
        let entry = self.entry_at(index)?;
        let base = HEADER_SIZE.checked_add(index.checked_mul(ENTRY_SIZE)?)?;
        let crc = read_u32(self.image, base.checked_add(ENTRY_CRC_OFFSET)?)?;
        (crc32(entry.data) == crc).then_some(entry)
    }

    /// Index of an exact path, or `None`.
    fn find_index(&self, path: &str) -> Option<usize> {
        let needle = path.as_bytes();
        let mut low = 0usize;
        let mut high = self.entry_count;
        while low < high {
            let mid = low + (high - low) / 2;
            let entry = self.entry_at(mid)?;
            match entry.path.as_bytes().cmp(needle) {
                Ordering::Less => low = mid.checked_add(1)?,
                Ordering::Greater => high = mid,
                Ordering::Equal => return Some(mid),
            }
        }
        None
    }

    /// Iterate over all entries in name order.
    ///
    /// The CRCs are not checked here; use [`Initfs::get`] to obtain a verified data entry.
    pub fn entries(&self) -> impl Iterator<Item = Entry<'a>> + '_ {
        (0..self.entry_count).filter_map(|index| self.entry_at(index))
    }

    /// Number of entries in the archive.
    pub fn len(&self) -> usize {
        self.entry_count
    }

    /// The image build epoch recorded in a version 2 header, or the default epoch for version 1.
    pub fn image_epoch(&self) -> u64 {
        self.image_epoch
    }

    /// True when the archive contains no files.
    pub fn is_empty(&self) -> bool {
        self.entry_count == 0
    }

    /// Read one entry by index without verifying its CRC, or `None` if out of range.
    fn entry_at(&self, index: usize) -> Option<Entry<'a>> {
        if index >= self.entry_count {
            return None;
        }
        let base = HEADER_SIZE.checked_add(index.checked_mul(ENTRY_SIZE)?)?;
        let name_off = read_u32(self.image, base.checked_add(ENTRY_NAME_OFF_OFFSET)?)?;
        let name_len = read_u32(self.image, base.checked_add(ENTRY_NAME_LEN_OFFSET)?)?;
        let data_off = read_u64(self.image, base.checked_add(ENTRY_DATA_OFF_OFFSET)?)?;
        let data_len = read_u64(self.image, base.checked_add(ENTRY_DATA_LEN_OFFSET)?)?;
        let mode = read_u32(self.image, base.checked_add(ENTRY_MODE_OFFSET)?)?;
        let raw_mtime = if self.version == VERSION {
            read_u64(self.image, base.checked_add(ENTRY_MTIME_OFFSET)?)?
        } else {
            0
        };
        let mtime = if raw_mtime == 0 {
            self.image_epoch
        } else {
            raw_mtime
        };

        let name_start = self
            .names_offset
            .checked_add(usize::try_from(name_off).ok()?)?;
        let name_end = name_start.checked_add(usize::try_from(name_len).ok()?)?;
        let path = core::str::from_utf8(self.image.get(name_start..name_end)?).ok()?;

        let data_start = usize::try_from(data_off).ok()?;
        let data_end = usize::try_from(data_off.checked_add(data_len)?).ok()?;
        let data = self.image.get(data_start..data_end)?;

        Some(Entry {
            path,
            data,
            mode,
            mtime,
            data_offset: data_off,
        })
    }
}

impl core::fmt::Display for InitfsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InitfsError::TooShort => f.write_str("image shorter than the 64-byte header"),
            InitfsError::BadMagic => f.write_str("bad magic (expected RAVENFS1)"),
            InitfsError::BadVersion => f.write_str("unsupported initfs version"),
            InitfsError::BadHeader => f.write_str("inconsistent initfs header or table"),
            InitfsError::BadEntry(index) => write!(f, "malformed initfs entry {index}"),
            InitfsError::BadChecksum(index) => write!(f, "CRC-32 mismatch in initfs entry {index}"),
            InitfsError::Unsorted => f.write_str("initfs entries are not sorted"),
            InitfsError::OutOfBounds(index) => {
                write!(f, "initfs entry {index} data region is out of bounds")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for InitfsError {}

#[cfg(test)]
#[path = "reader_ut.rs"]
mod tests;
