// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Read-only, panic-free FAT12/16/32 parser.
//!
//! The parser validates the boot sector/BPB geometry, locates the FAT and data regions, follows
//! cluster chains with loop and range checks, and decodes 8.3 and long-file-name directory
//! entries. Every offset is bounds-checked and every arithmetic step is checked; malformed input
//! returns [`FatError`] and never panics. The crate is `no_std` + `alloc` and contains no
//! `unsafe` code.
//!
//! The on-disk field layout is a fact from the FAT specification; the implementation is fresh
//! BSD-3-Clause code. FAT12, FAT16 and FAT32 are recognised by their cluster count, as the
//! specification requires. There are no write paths and no cached mutable state.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

/// Size of the boot sector in bytes.
pub const BOOT_SECTOR_SIZE: usize = 512;

/// Size of one directory entry.
pub const DIR_ENTRY_SIZE: usize = 32;

/// Attribute bit: directory.
pub const ATTR_DIRECTORY: u8 = 0x10;
/// Attribute bit: volume label.
pub const ATTR_VOLUME_ID: u8 = 0x08;
/// Attribute mask for the long-file-name entries.
pub const ATTR_LONG_NAME: u8 = 0x0f;

/// Maximum directory bytes read in one call (bounds a hostile chain).
const MAX_DIR_BYTES: usize = 4 * 1024 * 1024;

/// Maximum path components accepted by [`FatVolume::lookup`].
const MAX_PATH_DEPTH: usize = 64;

/// The FAT variant selected by the data-cluster count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FatKind {
    /// Fewer than 4,085 data clusters.
    Fat12,
    /// 4,085 to 65,524 data clusters.
    Fat16,
    /// 65,525 or more data clusters.
    Fat32,
}

/// A malformed or unsupported FAT image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FatError {
    /// The image is shorter than a boot sector.
    TooShort,
    /// The boot-sector signature is not `55 AA`.
    BadBootSignature,
    /// The jump instruction is not `EB xx 90` or `E9`.
    BadJump,
    /// The bytes-per-sector field is not one of 512, 1024, 2048, 4096.
    BadBytesPerSector(u16),
    /// Sectors per cluster is zero, not a power of two, or above 128.
    BadSectorsPerCluster(u8),
    /// The reserved-sector count is zero.
    BadReservedSectors(u16),
    /// The FAT count is outside 1..=4.
    BadFatCount(u8),
    /// The media descriptor is not a fixed-disk or removable value.
    BadMedia(u8),
    /// The total-sector or FAT-size fields are zero or inconsistent.
    BadGeometry,
    /// The image is shorter than the geometry says it must be.
    Truncated,
    /// A FAT table or the root directory does not fit in the volume.
    RegionOutOfBounds,
    /// A cluster number is zero, reserved, or beyond the data region.
    BadCluster(u32),
    /// The FAT32 root cluster is zero or outside the data region.
    BadRootCluster,
    /// A cluster chain contains a cycle.
    ChainLoop(u32),
    /// A cluster chain is longer than the volume.
    ChainTooLong,
    /// A directory entry is syntactically invalid.
    BadDirectory,
    /// A long-file-name sequence is incomplete, out of order or has a bad checksum.
    BadLfn,
    /// A file's declared size is larger than its cluster chain.
    BadLength,
    /// A file operation was requested for a directory.
    IsDirectory,
    /// A path is too deep or contains `..`.
    InvalidPath,
}

/// One decoded directory entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryEntry {
    /// The 11-byte 8.3 name, as stored on disk.
    pub short_name: [u8; 11],
    /// The display name (long name when present, otherwise the 8.3 name).
    pub name: String,
    /// Attribute byte.
    pub attributes: u8,
    /// First cluster; zero means an empty file.
    pub first_cluster: u32,
    /// Declared file size in bytes.
    pub size: u32,
}

impl DirectoryEntry {
    /// Whether the entry is a directory.
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.attributes & ATTR_DIRECTORY != 0
    }

    /// Whether the entry is a regular file (not a directory or volume label).
    #[must_use]
    pub fn is_file(&self) -> bool {
        self.attributes & (ATTR_DIRECTORY | ATTR_VOLUME_ID) == 0
    }

    /// Whether the entry is a volume label.
    #[must_use]
    pub fn is_volume_label(&self) -> bool {
        self.attributes & ATTR_VOLUME_ID != 0
    }
}

/// A parsed FAT volume.
pub struct FatVolume<'a> {
    image: &'a [u8],
    kind: FatKind,
    bytes_per_sector: usize,
    sectors_per_cluster: usize,
    root_entry_count: u32,
    total_sectors: u32,
    fat_start: usize,
    root_dir_start: usize,
    data_start: usize,
    cluster_size: usize,
    cluster_count: u32,
    root_cluster: u32,
    total_bytes: usize,
}

impl<'a> FatVolume<'a> {
    /// Parse a complete FAT image.
    ///
    /// # Errors
    /// Returns [`FatError`] for a truncated, inconsistent or unsupported volume.
    pub fn parse(image: &'a [u8]) -> Result<Self, FatError> {
        if image.len() < BOOT_SECTOR_SIZE {
            return Err(FatError::TooShort);
        }
        if image.get(510..512) != Some(&[0x55, 0xaa]) {
            return Err(FatError::BadBootSignature);
        }
        let jump = *image.first().ok_or(FatError::TooShort)?;
        if jump != 0xeb && jump != 0xe9 {
            return Err(FatError::BadJump);
        }
        if jump == 0xeb && image.get(2) != Some(&0x90) {
            return Err(FatError::BadJump);
        }

        let bytes_per_sector = u16_at(image, 11).ok_or(FatError::TooShort)?;
        if !matches!(bytes_per_sector, 512 | 1024 | 2048 | 4096) {
            return Err(FatError::BadBytesPerSector(bytes_per_sector));
        }
        let sectors_per_cluster = *image.get(13).ok_or(FatError::TooShort)?;
        if sectors_per_cluster == 0
            || !sectors_per_cluster.is_power_of_two()
            || sectors_per_cluster > 128
        {
            return Err(FatError::BadSectorsPerCluster(sectors_per_cluster));
        }
        let reserved_sectors = u32::from(u16_at(image, 14).ok_or(FatError::TooShort)?);
        if reserved_sectors == 0 {
            return Err(FatError::BadReservedSectors(0));
        }
        let fat_count = u32::from(*image.get(16).ok_or(FatError::TooShort)?);
        if !(1..=4).contains(&fat_count) {
            return Err(FatError::BadFatCount(fat_count as u8));
        }
        let root_entry_count = u32::from(u16_at(image, 17).ok_or(FatError::TooShort)?);
        let total16 = u32::from(u16_at(image, 19).ok_or(FatError::TooShort)?);
        let total32 = u32_at(image, 32).ok_or(FatError::TooShort)?;
        let total_sectors = if total16 != 0 { total16 } else { total32 };
        if total_sectors == 0 {
            return Err(FatError::BadGeometry);
        }
        let media = *image.get(21).ok_or(FatError::TooShort)?;
        if media != 0xf0 && media < 0xf8 {
            return Err(FatError::BadMedia(media));
        }
        let fat16 = u32::from(u16_at(image, 22).ok_or(FatError::TooShort)?);
        let fat32 = u32_at(image, 36).ok_or(FatError::TooShort)?;
        let fat_sectors = if fat16 != 0 { fat16 } else { fat32 };
        if fat_sectors == 0 {
            return Err(FatError::BadGeometry);
        }
        let root_cluster = u32_at(image, 44).ok_or(FatError::TooShort)?;

        let bytes_per_sector = usize::from(bytes_per_sector);
        let sectors_per_cluster = usize::from(sectors_per_cluster);
        let cluster_size = bytes_per_sector
            .checked_mul(sectors_per_cluster)
            .ok_or(FatError::BadGeometry)?;
        let fat_start = usize::try_from(reserved_sectors)
            .ok()
            .and_then(|value| value.checked_mul(bytes_per_sector))
            .ok_or(FatError::BadGeometry)?;
        let fat_bytes = usize::try_from(fat_sectors)
            .ok()
            .and_then(|value| value.checked_mul(bytes_per_sector))
            .ok_or(FatError::BadGeometry)?;
        let root_dir_sectors = usize::try_from(root_entry_count)
            .ok()
            .and_then(|value| value.checked_mul(DIR_ENTRY_SIZE))
            .ok_or(FatError::BadGeometry)?
            .div_ceil(bytes_per_sector);
        let root_dir_start = fat_start
            .checked_add(
                fat_bytes
                    .checked_mul(fat_count as usize)
                    .ok_or(FatError::BadGeometry)?,
            )
            .ok_or(FatError::BadGeometry)?;
        let data_start = root_dir_start
            .checked_add(
                root_dir_sectors
                    .checked_mul(bytes_per_sector)
                    .ok_or(FatError::BadGeometry)?,
            )
            .ok_or(FatError::BadGeometry)?;
        let total_bytes = usize::try_from(total_sectors)
            .ok()
            .and_then(|value| value.checked_mul(bytes_per_sector))
            .ok_or(FatError::BadGeometry)?;
        if total_bytes > image.len() {
            return Err(FatError::Truncated);
        }
        if data_start > total_bytes {
            return Err(FatError::RegionOutOfBounds);
        }
        let data_sectors = total_bytes
            .checked_sub(data_start)
            .ok_or(FatError::BadGeometry)?
            / bytes_per_sector;
        let cluster_count =
            u32::try_from(data_sectors / sectors_per_cluster).map_err(|_| FatError::BadGeometry)?;
        if cluster_count == 0 || cluster_count > 0x0fff_fff5 {
            return Err(FatError::BadGeometry);
        }
        let kind = if cluster_count < 4_085 {
            FatKind::Fat12
        } else if cluster_count < 65_525 {
            FatKind::Fat16
        } else {
            FatKind::Fat32
        };
        if kind == FatKind::Fat32 {
            if root_entry_count != 0
                || root_cluster < 2
                || u64::from(root_cluster) >= u64::from(cluster_count) + 2
            {
                return Err(FatError::BadRootCluster);
            }
        } else if root_entry_count == 0 {
            return Err(FatError::BadGeometry);
        }

        let required = match kind {
            FatKind::Fat12 => {
                let entries = u64::from(cluster_count) + 2;
                usize::try_from((entries * 3).div_ceil(2)).map_err(|_| FatError::BadGeometry)?
            }
            FatKind::Fat16 => usize::try_from((u64::from(cluster_count) + 2) * 2)
                .map_err(|_| FatError::BadGeometry)?,
            FatKind::Fat32 => usize::try_from((u64::from(cluster_count) + 2) * 4)
                .map_err(|_| FatError::BadGeometry)?,
        };
        if required > fat_bytes {
            return Err(FatError::BadGeometry);
        }
        let fat_end = fat_start
            .checked_add(fat_bytes)
            .ok_or(FatError::RegionOutOfBounds)?;
        if fat_end > data_start || fat_end > total_bytes {
            return Err(FatError::RegionOutOfBounds);
        }
        let root_end = root_dir_start
            .checked_add(
                root_dir_sectors
                    .checked_mul(bytes_per_sector)
                    .ok_or(FatError::BadGeometry)?,
            )
            .ok_or(FatError::RegionOutOfBounds)?;
        if root_end > data_start || root_end > total_bytes {
            return Err(FatError::RegionOutOfBounds);
        }

        Ok(Self {
            image,
            kind,
            bytes_per_sector,
            sectors_per_cluster,
            root_entry_count,
            total_sectors,
            fat_start,
            root_dir_start,
            data_start,
            cluster_size,
            cluster_count,
            root_cluster,
            total_bytes,
        })
    }

    /// The detected FAT variant.
    #[must_use]
    pub fn kind(&self) -> FatKind {
        self.kind
    }

    /// Bytes per sector from the BPB.
    #[must_use]
    pub fn bytes_per_sector(&self) -> usize {
        self.bytes_per_sector
    }

    /// Sectors per cluster from the BPB.
    #[must_use]
    pub fn sectors_per_cluster(&self) -> usize {
        self.sectors_per_cluster
    }

    /// Cluster size in bytes.
    #[must_use]
    pub fn cluster_size(&self) -> usize {
        self.cluster_size
    }

    /// Number of data clusters.
    #[must_use]
    pub fn cluster_count(&self) -> u32 {
        self.cluster_count
    }

    /// Total sectors from the BPB.
    #[must_use]
    pub fn total_sectors(&self) -> u32 {
        self.total_sectors
    }

    /// First cluster of the root directory (FAT32 only).
    #[must_use]
    pub fn root_cluster(&self) -> u32 {
        self.root_cluster
    }

    /// Read the root directory.
    ///
    /// # Errors
    /// Returns [`FatError`] for a malformed or out-of-range root directory.
    pub fn root_dir(&self) -> Result<Vec<DirectoryEntry>, FatError> {
        self.read_dir(None)
    }

    /// Read a directory: the root when `first` is `None`, else the cluster chain of a subdirectory.
    ///
    /// # Errors
    /// Returns [`FatError`] for a malformed chain or directory entry.
    pub fn read_dir(&self, first: Option<u32>) -> Result<Vec<DirectoryEntry>, FatError> {
        let bytes = match first {
            None if self.kind != FatKind::Fat32 => {
                let length = usize::try_from(self.root_entry_count)
                    .ok()
                    .and_then(|value| value.checked_mul(DIR_ENTRY_SIZE))
                    .ok_or(FatError::BadGeometry)?;
                let end = self
                    .root_dir_start
                    .checked_add(length)
                    .ok_or(FatError::RegionOutOfBounds)?;
                self.image
                    .get(self.root_dir_start..end)
                    .ok_or(FatError::RegionOutOfBounds)?
                    .to_vec()
            }
            None => self.read_chain(self.root_cluster, MAX_DIR_BYTES)?,
            Some(cluster) => self.read_chain(cluster, MAX_DIR_BYTES)?,
        };
        self.decode_directory(&bytes)
    }

    /// Look up a `/`-separated path (case-insensitive, no `..`).
    ///
    /// # Errors
    /// Returns [`FatError`] for a malformed path, directory or chain.
    pub fn lookup(&self, path: &[u8]) -> Result<Option<DirectoryEntry>, FatError> {
        let components: Vec<&[u8]> = path
            .split(|byte| *byte == b'/')
            .filter(|component| !component.is_empty())
            .collect();
        if components.len() > MAX_PATH_DEPTH {
            return Err(FatError::InvalidPath);
        }
        let mut current = None;
        let mut visited = BTreeSet::new();
        for (index, component) in components.iter().enumerate() {
            if *component == b"." {
                continue;
            }
            if *component == b".." {
                return Err(FatError::InvalidPath);
            }
            let entries = self.read_dir(current)?;
            let Some(entry) = entries
                .into_iter()
                .find(|entry| entry.name.as_bytes().eq_ignore_ascii_case(component))
            else {
                return Ok(None);
            };
            if index + 1 == components.len() {
                return Ok(Some(entry));
            }
            if !entry.is_dir() {
                return Ok(None);
            }
            if !visited.insert(entry.first_cluster) {
                return Err(FatError::ChainLoop(entry.first_cluster));
            }
            current = Some(entry.first_cluster);
        }
        Ok(None)
    }

    /// Read a file's contents, at most `limit` bytes.
    ///
    /// # Errors
    /// Returns [`FatError`] for a directory, a bad chain or a size larger than the chain.
    pub fn read_file(&self, entry: &DirectoryEntry, limit: usize) -> Result<Vec<u8>, FatError> {
        if entry.is_dir() {
            return Err(FatError::IsDirectory);
        }
        let size = usize::try_from(entry.size).map_err(|_| FatError::BadLength)?;
        let want = size.min(limit);
        if want == 0 {
            return Ok(Vec::new());
        }
        if entry.first_cluster == 0 {
            return Err(FatError::BadCluster(0));
        }
        let data = self.read_chain(entry.first_cluster, want)?;
        if data.len() < want {
            return Err(FatError::BadLength);
        }
        Ok(data)
    }

    fn decode_directory(&self, bytes: &[u8]) -> Result<Vec<DirectoryEntry>, FatError> {
        let mut entries = Vec::new();
        let mut lfn_parts: Vec<LfnPart> = Vec::new();
        let mut lfn_checksum: Option<u8> = None;
        for chunk in bytes.chunks_exact(DIR_ENTRY_SIZE) {
            let first = chunk[0];
            if first == 0 {
                break;
            }
            if first == 0xe5 {
                lfn_parts.clear();
                lfn_checksum = None;
                continue;
            }
            let attributes = chunk[11];
            if attributes == ATTR_LONG_NAME {
                let ordinal = chunk[0];
                if ordinal == 0 {
                    return Err(FatError::BadLfn);
                }
                let index = ordinal & 0x1f;
                if !(1..=20).contains(&index) || lfn_parts.len() >= 20 {
                    return Err(FatError::BadLfn);
                }
                let checksum = chunk[13];
                if lfn_checksum.is_some_and(|value| value != checksum) {
                    return Err(FatError::BadLfn);
                }
                lfn_checksum = Some(checksum);
                let chars = lfn_chars(chunk);
                lfn_parts.push(LfnPart {
                    index,
                    last: ordinal & 0x40 != 0,
                    chars,
                });
                continue;
            }
            if attributes & ATTR_VOLUME_ID != 0 {
                lfn_parts.clear();
                lfn_checksum = None;
                continue;
            }
            if first == b'.' {
                lfn_parts.clear();
                lfn_checksum = None;
                continue;
            }
            let mut short_name = [0u8; 11];
            short_name.copy_from_slice(&chunk[..11]);
            if short_name[0] == 0x05 {
                short_name[0] = 0xe5;
            }
            let name = if lfn_parts.is_empty() {
                short_to_string(&short_name, chunk[12]).ok_or(FatError::BadDirectory)?
            } else {
                lfn_parts.sort_by_key(|part| part.index);
                let mut expected = 1u8;
                let mut last_seen = false;
                let mut units = Vec::new();
                for part in &lfn_parts {
                    if part.index != expected {
                        return Err(FatError::BadLfn);
                    }
                    expected = expected.saturating_add(1);
                    last_seen |= part.last;
                    units.extend_from_slice(&part.chars);
                }
                if !last_seen
                    || lfn_checksum != Some(lfn_checksum_for(&short_name))
                    || units.is_empty()
                {
                    return Err(FatError::BadLfn);
                }
                char::decode_utf16(units)
                    .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
                    .collect()
            };
            lfn_parts.clear();
            lfn_checksum = None;

            let high = if self.kind == FatKind::Fat32 {
                u32::from(u16_at(chunk, 20).ok_or(FatError::BadDirectory)?)
            } else {
                0
            };
            let low = u32::from(u16_at(chunk, 26).ok_or(FatError::BadDirectory)?);
            let first_cluster = (high << 16) | low;
            let first_cluster = if self.kind == FatKind::Fat32 {
                first_cluster & 0x0fff_ffff
            } else {
                first_cluster
            };
            if first_cluster != 0
                && (first_cluster < 2
                    || u64::from(first_cluster) >= u64::from(self.cluster_count) + 2)
            {
                return Err(FatError::BadCluster(first_cluster));
            }
            if attributes & ATTR_DIRECTORY != 0 && first_cluster < 2 {
                return Err(FatError::BadCluster(first_cluster));
            }
            let size = u32_at(chunk, 28).ok_or(FatError::BadDirectory)?;
            entries.push(DirectoryEntry {
                short_name,
                name,
                attributes,
                first_cluster,
                size,
            });
        }
        if !lfn_parts.is_empty() {
            return Err(FatError::BadLfn);
        }
        Ok(entries)
    }

    fn read_chain(&self, first: u32, limit: usize) -> Result<Vec<u8>, FatError> {
        if first == 0 {
            return Ok(Vec::new());
        }
        let clusters = self.chain(first)?;
        let mut out = Vec::new();
        for cluster in clusters {
            if out.len() >= limit {
                break;
            }
            let offset = self.cluster_offset(cluster)?;
            let take = self.cluster_size.min(limit - out.len());
            let end = offset
                .checked_add(take)
                .ok_or(FatError::RegionOutOfBounds)?;
            let data = self
                .image
                .get(offset..end)
                .ok_or(FatError::RegionOutOfBounds)?;
            out.extend_from_slice(data);
        }
        Ok(out)
    }

    fn chain(&self, first: u32) -> Result<Vec<u32>, FatError> {
        let mut visited = BTreeSet::new();
        let mut clusters = Vec::new();
        let mut current = first;
        loop {
            if current < 2 || u64::from(current) >= u64::from(self.cluster_count) + 2 {
                return Err(FatError::BadCluster(current));
            }
            if !visited.insert(current) {
                return Err(FatError::ChainLoop(current));
            }
            clusters.push(current);
            if clusters.len() > self.cluster_count as usize {
                return Err(FatError::ChainTooLong);
            }
            match self.next_cluster(current)? {
                Some(next) => current = next,
                None => break,
            }
        }
        Ok(clusters)
    }

    fn next_cluster(&self, cluster: u32) -> Result<Option<u32>, FatError> {
        let value = self.fat_entry(cluster)?;
        let end = match self.kind {
            FatKind::Fat12 => 0x0ff8,
            FatKind::Fat16 => 0xfff8,
            FatKind::Fat32 => 0x0fff_fff8,
        };
        if value >= end {
            return Ok(None);
        }
        if value == 0 || value == 1 || u64::from(value) >= u64::from(self.cluster_count) + 2 {
            return Err(FatError::BadCluster(value));
        }
        Ok(Some(value))
    }

    fn fat_entry(&self, cluster: u32) -> Result<u32, FatError> {
        if cluster < 2 || u64::from(cluster) >= u64::from(self.cluster_count) + 2 {
            return Err(FatError::BadCluster(cluster));
        }
        let index = usize::try_from(cluster).map_err(|_| FatError::BadCluster(cluster))?;
        match self.kind {
            FatKind::Fat12 => {
                let offset = index
                    .checked_add(index / 2)
                    .and_then(|value| value.checked_add(self.fat_start))
                    .ok_or(FatError::RegionOutOfBounds)?;
                let low = *self.image.get(offset).ok_or(FatError::RegionOutOfBounds)?;
                let high = *self
                    .image
                    .get(offset + 1)
                    .ok_or(FatError::RegionOutOfBounds)?;
                let raw = u16::from(low) | (u16::from(high) << 8);
                Ok(if index & 1 == 0 {
                    u32::from(raw) & 0x0fff
                } else {
                    u32::from(raw) >> 4
                })
            }
            FatKind::Fat16 => {
                let offset = index
                    .checked_mul(2)
                    .and_then(|value| value.checked_add(self.fat_start))
                    .ok_or(FatError::RegionOutOfBounds)?;
                let slice = self
                    .image
                    .get(offset..offset + 2)
                    .ok_or(FatError::RegionOutOfBounds)?;
                Ok(u32::from(u16::from_le_bytes([slice[0], slice[1]])))
            }
            FatKind::Fat32 => {
                let offset = index
                    .checked_mul(4)
                    .and_then(|value| value.checked_add(self.fat_start))
                    .ok_or(FatError::RegionOutOfBounds)?;
                let slice = self
                    .image
                    .get(offset..offset + 4)
                    .ok_or(FatError::RegionOutOfBounds)?;
                Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]) & 0x0fff_ffff)
            }
        }
    }

    fn cluster_offset(&self, cluster: u32) -> Result<usize, FatError> {
        if cluster < 2 || u64::from(cluster) >= u64::from(self.cluster_count) + 2 {
            return Err(FatError::BadCluster(cluster));
        }
        let index = usize::try_from(cluster - 2).map_err(|_| FatError::BadCluster(cluster))?;
        let offset = index
            .checked_mul(self.cluster_size)
            .and_then(|value| value.checked_add(self.data_start))
            .ok_or(FatError::RegionOutOfBounds)?;
        let end = offset
            .checked_add(self.cluster_size)
            .ok_or(FatError::RegionOutOfBounds)?;
        if end > self.total_bytes || end > self.image.len() {
            return Err(FatError::RegionOutOfBounds);
        }
        Ok(offset)
    }
}

struct LfnPart {
    index: u8,
    last: bool,
    chars: Vec<u16>,
}

fn lfn_chars(entry: &[u8]) -> Vec<u16> {
    let offsets = [1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
    let mut chars = Vec::with_capacity(13);
    for offset in offsets {
        let Some(unit) = u16_at(entry, offset) else {
            break;
        };
        if unit == 0 || unit == 0xffff {
            break;
        }
        chars.push(unit);
    }
    chars
}

fn lfn_checksum_for(name: &[u8; 11]) -> u8 {
    let mut sum = 0u8;
    for byte in name {
        sum = sum.rotate_right(1).wrapping_add(*byte);
    }
    sum
}

fn short_to_string(name: &[u8; 11], case_flags: u8) -> Option<String> {
    let mut base_end = 8;
    while base_end > 0 && name[base_end - 1] == b' ' {
        base_end -= 1;
    }
    let mut ext_end = 11;
    while ext_end > 8 && name[ext_end - 1] == b' ' {
        ext_end -= 1;
    }
    if base_end == 0 {
        return None;
    }
    let lower_base = case_flags & 0x08 != 0;
    let lower_ext = case_flags & 0x10 != 0;
    let mut out = String::new();
    for byte in &name[..base_end] {
        out.push(ascii_case(*byte, lower_base));
    }
    if ext_end > 8 {
        out.push('.');
        for byte in &name[8..ext_end] {
            out.push(ascii_case(*byte, lower_ext));
        }
    }
    Some(out)
}

fn ascii_case(byte: u8, lower: bool) -> char {
    let byte = if lower && byte.is_ascii_uppercase() {
        byte + 32
    } else {
        byte
    };
    char::from(byte)
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
#[path = "fat_ut.rs"]
mod tests;
