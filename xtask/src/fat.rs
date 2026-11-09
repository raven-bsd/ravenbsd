// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic FAT32 ESP builder on top of the `fatfs` crate.
//!
//! The image is formatted in memory with a fixed volume id and label, a fixed time provider
//! (derived from `SOURCE_DATE_EPOCH`, clamped to the FAT date range), sorted file insertion and
//! sorted directory creation. The output is a byte-for-byte function of the inputs and the epoch.

use std::collections::BTreeSet;
use std::io::{Cursor, Write};

use anyhow::{Result, bail, ensure};
use fatfs::{
    Date, DateTime, FatType, FileSystem, FormatVolumeOptions, FsOptions, Time, TimeProvider,
};

const VOLUME_ID: u32 = 0x4e41_5652;
const VOLUME_LABEL: [u8; 11] = *b"RAVENESP   ";
const DEFAULT_EPOCH: i64 = 315_532_800; // 1980-01-01T00:00:00Z

/// A deterministic FAT32 image builder.
pub(crate) struct FatImageBuilder {
    size_bytes: usize,
    dirs: BTreeSet<String>,
    files: Vec<(String, Vec<u8>)>,
    epoch: Option<i64>,
}

impl FatImageBuilder {
    /// Create a builder for a FAT32 volume of `size_bytes` (must be sector aligned).
    pub(crate) fn new(size_bytes: usize) -> Result<Self> {
        ensure!(
            size_bytes.is_multiple_of(512),
            "FAT image size is not sector aligned"
        );
        Ok(Self {
            size_bytes,
            dirs: BTreeSet::new(),
            files: Vec::new(),
            epoch: None,
        })
    }

    /// Set the timestamp source; `None` means the FAT minimum (1980-01-01).
    pub(crate) fn with_epoch(mut self, epoch: Option<i64>) -> Self {
        self.epoch = epoch;
        self
    }

    /// Add a directory. Directories are created in sorted order.
    pub(crate) fn add_dir(&mut self, path: &str) -> Result<()> {
        validate_path(path)?;
        self.dirs.insert(path.to_string());
        Ok(())
    }

    /// Add a file. Files are written in sorted path order.
    pub(crate) fn add_file(&mut self, path: &str, data: &[u8]) -> Result<()> {
        validate_path(path)?;
        if self.files.iter().any(|(existing, _)| existing == path) {
            bail!("duplicate FAT file `{path}`");
        }
        self.files.push((path.to_string(), data.to_vec()));
        Ok(())
    }

    /// Format and populate the image, returning its bytes.
    pub(crate) fn finish(self) -> Result<Vec<u8>> {
        let mut buffer = vec![0u8; self.size_bytes];
        let total_sectors = u32::try_from(self.size_bytes / 512)
            .map_err(|_| anyhow::anyhow!("FAT image is too large"))?;
        let options = FormatVolumeOptions::new()
            .total_sectors(total_sectors)
            .fat_type(FatType::Fat32)
            .volume_id(VOLUME_ID)
            .volume_label(VOLUME_LABEL);
        let provider: &'static FixedTimeProvider =
            Box::leak(Box::new(FixedTimeProvider::new(self.epoch)));
        {
            let mut cursor = Cursor::new(buffer.as_mut_slice());
            fatfs::format_volume(&mut cursor, options)?;
            cursor.set_position(0);
            let filesystem = FileSystem::new(
                cursor,
                FsOptions::new()
                    .time_provider(provider)
                    .update_accessed_date(false),
            )?;
            let root = filesystem.root_dir();
            for dir in &self.dirs {
                root.create_dir(dir)?;
            }
            let mut files = self.files;
            files.sort_by(|left, right| left.0.cmp(&right.0));
            for (path, data) in files {
                let mut file = root.create_file(&path)?;
                file.write_all(&data)?;
                file.flush()?;
            }
            drop(root);
            let _ = filesystem.stats()?;
            filesystem.unmount()?;
        }
        fix_dot_entries(&mut buffer)?;
        Ok(buffer)
    }
}

/// `fatfs` 0.3.6 writes long-name entries in front of the special `.` and `..` entries when it
/// creates a directory, which `fsck.vfat` rejects (the special entries must be first). This
/// deterministic post-pass removes those LFN entries and puts `.`/`..` in slots 0 and 1. It also
/// works for empty directories and leaves child entries untouched.
fn fix_dot_entries(buffer: &mut [u8]) -> Result<()> {
    let bytes_per_sector =
        usize::from(read_u16(buffer, 11).ok_or_else(|| anyhow::anyhow!("short FAT boot sector"))?);
    let sectors_per_cluster = usize::from(
        *buffer
            .get(13)
            .ok_or_else(|| anyhow::anyhow!("short FAT boot sector"))?,
    );
    let reserved =
        usize::from(read_u16(buffer, 14).ok_or_else(|| anyhow::anyhow!("short FAT boot sector"))?);
    let fats = usize::from(
        *buffer
            .get(16)
            .ok_or_else(|| anyhow::anyhow!("short FAT boot sector"))?,
    );
    let fat_sectors =
        read_u32(buffer, 36).ok_or_else(|| anyhow::anyhow!("short FAT boot sector"))?;
    let root_cluster =
        read_u32(buffer, 44).ok_or_else(|| anyhow::anyhow!("short FAT boot sector"))?;
    ensure!(
        bytes_per_sector > 0 && sectors_per_cluster > 0,
        "invalid FAT geometry"
    );
    let cluster_size = bytes_per_sector
        .checked_mul(sectors_per_cluster)
        .ok_or_else(|| anyhow::anyhow!("FAT cluster size overflow"))?;
    let fat_start = reserved
        .checked_mul(bytes_per_sector)
        .ok_or_else(|| anyhow::anyhow!("FAT offset overflow"))?;
    let data_start = (reserved + fats * usize::try_from(fat_sectors)?)
        .checked_mul(bytes_per_sector)
        .ok_or_else(|| anyhow::anyhow!("FAT data offset overflow"))?;
    ensure!(data_start <= buffer.len(), "FAT data starts past the image");
    let total_clusters = (buffer.len() - data_start) / cluster_size;
    if total_clusters == 0 || root_cluster < 2 {
        return Ok(());
    }
    let geometry = FatGeometry {
        fat_start,
        data_start,
        cluster_size,
        total_clusters,
    };
    let mut visited = std::collections::BTreeSet::new();
    fix_directory(buffer, &geometry, root_cluster, 0, &mut visited)?;
    Ok(())
}

struct FatGeometry {
    fat_start: usize,
    data_start: usize,
    cluster_size: usize,
    total_clusters: usize,
}

fn fix_directory(
    buffer: &mut [u8],
    geometry: &FatGeometry,
    cluster: u32,
    parent: u32,
    visited: &mut std::collections::BTreeSet<u32>,
) -> Result<()> {
    if !visited.insert(cluster) {
        return Ok(());
    }
    let mut clusters = vec![cluster];
    let mut current = cluster;
    for _ in 0..geometry.total_clusters {
        let next = fat_next(buffer, geometry.fat_start, current)?;
        if next == 0x0fff_ffff || next == 0 {
            break;
        }
        ensure!(
            next >= 2 && usize::try_from(next)? - 2 < geometry.total_clusters,
            "bad FAT chain"
        );
        clusters.push(next);
        current = next;
    }

    let first_offset = cluster_offset(geometry.data_start, geometry.cluster_size, cluster)?;
    fix_dot_slots(buffer, first_offset, geometry.cluster_size, cluster, parent)?;

    let mut children = Vec::new();
    for cluster in &clusters {
        let offset = cluster_offset(geometry.data_start, geometry.cluster_size, *cluster)?;
        let end = offset
            .checked_add(geometry.cluster_size)
            .ok_or_else(|| anyhow::anyhow!("directory range overflow"))?;
        let directory = buffer
            .get(offset..end)
            .ok_or_else(|| anyhow::anyhow!("directory outside image"))?;
        for entry in directory.chunks_exact(32) {
            if entry[0] == 0 {
                break;
            }
            if entry[0] == 0xe5 || entry[11] == 0x0f {
                continue;
            }
            if entry[11] & 0x10 != 0 && !is_dot(entry) {
                let child = first_cluster(entry);
                if child >= 2 && usize::try_from(child)? - 2 < geometry.total_clusters {
                    children.push(child);
                }
            }
        }
    }
    for child in children {
        fix_directory(buffer, geometry, child, cluster, visited)?;
    }
    Ok(())
}

fn fix_dot_slots(
    buffer: &mut [u8],
    offset: usize,
    cluster_size: usize,
    self_cluster: u32,
    parent: u32,
) -> Result<()> {
    let directory = buffer
        .get(offset..offset.saturating_add(cluster_size))
        .ok_or_else(|| anyhow::anyhow!("directory outside image"))?;
    let mut entries: Vec<[u8; 32]> = directory
        .chunks_exact(32)
        .map(|entry| {
            let mut copy = [0u8; 32];
            copy.copy_from_slice(entry);
            copy
        })
        .collect();
    let mut dot = None;
    let mut dotdot = None;
    for (index, entry) in entries.iter().enumerate() {
        if entry[0] == b'.' && entry[1] == b' ' {
            dot = Some(index);
        } else if entry[0] == b'.' && entry[1] == b'.' {
            dotdot = Some(index);
        }
    }
    let (Some(dot), Some(dotdot)) = (dot, dotdot) else {
        return Ok(());
    };
    let mut dot_entry = entries[dot];
    let mut dotdot_entry = entries[dotdot];
    let dot_cluster = if parent == 0 { 0 } else { self_cluster };
    let dotdot_cluster = if parent == 0 || parent == 2 {
        0
    } else {
        parent
    };
    write_cluster(&mut dot_entry, dot_cluster);
    write_cluster(&mut dotdot_entry, dotdot_cluster);
    let mut lfn_for_dot = None;
    let mut lfn_for_dotdot = None;
    if dot > 0 && entries[dot - 1][11] == 0x0f {
        lfn_for_dot = Some(dot - 1);
    }
    if dotdot > 0 && entries[dotdot - 1][11] == 0x0f {
        lfn_for_dotdot = Some(dotdot - 1);
    }
    let mut rebuilt = Vec::with_capacity(entries.len());
    rebuilt.push(dot_entry);
    rebuilt.push(dotdot_entry);
    for (index, entry) in entries.iter().enumerate() {
        if index == dot
            || index == dotdot
            || Some(index) == lfn_for_dot
            || Some(index) == lfn_for_dotdot
            || entry[0] == 0
        {
            continue;
        }
        rebuilt.push(*entry);
    }
    entries.fill([0u8; 32]);
    for (index, entry) in rebuilt.into_iter().enumerate() {
        if let Some(slot) = entries.get_mut(index) {
            *slot = entry;
        }
    }
    if let Some(slots) = buffer.get_mut(offset..offset.saturating_add(cluster_size)) {
        for (index, entry) in entries.iter().enumerate() {
            let start = index * 32;
            if let Some(slot) = slots.get_mut(start..start + 32) {
                slot.copy_from_slice(entry);
            }
        }
    }
    Ok(())
}

fn write_cluster(entry: &mut [u8], cluster: u32) {
    let low = (cluster & 0xffff) as u16;
    let high = (cluster >> 16) as u16;
    entry[26..28].copy_from_slice(&low.to_le_bytes());
    entry[20..22].copy_from_slice(&high.to_le_bytes());
}

fn cluster_offset(data_start: usize, cluster_size: usize, cluster: u32) -> Result<usize> {
    let index = usize::try_from(cluster).map_err(|_| anyhow::anyhow!("cluster number overflow"))?;
    data_start
        .checked_add(
            index
                .checked_sub(2)
                .ok_or_else(|| anyhow::anyhow!("invalid cluster {cluster}"))?
                .checked_mul(cluster_size)
                .ok_or_else(|| anyhow::anyhow!("cluster offset overflow"))?,
        )
        .ok_or_else(|| anyhow::anyhow!("cluster offset overflow"))
}

fn fat_next(buffer: &[u8], fat_start: usize, cluster: u32) -> Result<u32> {
    let index = usize::try_from(cluster).map_err(|_| anyhow::anyhow!("cluster number overflow"))?;
    let offset = fat_start
        .checked_add(
            index
                .checked_mul(4)
                .ok_or_else(|| anyhow::anyhow!("FAT offset overflow"))?,
        )
        .ok_or_else(|| anyhow::anyhow!("FAT offset overflow"))?;
    read_u32(buffer, offset).ok_or_else(|| anyhow::anyhow!("FAT entry outside image"))
}

fn first_cluster(entry: &[u8]) -> u32 {
    let low = u16::from_le_bytes([entry[26], entry[27]]);
    let high = u16::from_le_bytes([entry[20], entry[21]]);
    u32::from(low) | (u32::from(high) << 16)
}

fn is_dot(entry: &[u8]) -> bool {
    (entry[0] == b'.' && entry[1] == b' ') || (entry[0] == b'.' && entry[1] == b'.')
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn validate_path(path: &str) -> Result<()> {
    ensure!(!path.is_empty(), "empty FAT path");
    ensure!(
        !path.starts_with('/'),
        "FAT path must be relative: `{path}`"
    );
    for component in path.split('/') {
        ensure!(
            !component.is_empty() && component != "." && component != "..",
            "invalid FAT path `{path}`"
        );
    }
    Ok(())
}

#[derive(Debug)]
struct FixedTimeProvider {
    date: Date,
    time: Time,
}

impl FixedTimeProvider {
    fn new(epoch: Option<i64>) -> Self {
        let (year, month, day, hour, minute, second) = fat_datetime(epoch.unwrap_or(DEFAULT_EPOCH));
        Self {
            date: Date { year, month, day },
            time: Time {
                hour,
                min: minute,
                sec: second,
                millis: 0,
            },
        }
    }
}

impl TimeProvider for FixedTimeProvider {
    fn get_current_date(&self) -> Date {
        self.date
    }

    fn get_current_date_time(&self) -> DateTime {
        DateTime {
            date: self.date,
            time: self.time,
        }
    }
}

/// Convert a Unix timestamp to the FAT date/time range.
fn fat_datetime(epoch: i64) -> (u16, u16, u16, u16, u16, u16) {
    let epoch = epoch.max(DEFAULT_EPOCH);
    let days = epoch.div_euclid(86_400);
    let seconds = epoch.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    if year > 2107 || (year == 2107 && (month > 12 || (month == 12 && day > 31))) {
        return (2107, 12, 31, 23, 59, 58);
    }
    let hour = u16::try_from(seconds / 3_600).unwrap_or(0);
    let minute = u16::try_from((seconds % 3_600) / 60).unwrap_or(0);
    let second = u16::try_from(seconds % 60).unwrap_or(0) & !1;
    (year as u16, month as u16, day as u16, hour, minute, second)
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: usize = 64 * 1024 * 1024;

    fn sample() -> Vec<u8> {
        let mut builder = FatImageBuilder::new(SIZE).unwrap();
        builder.add_dir("EFI").unwrap();
        builder.add_dir("EFI/BOOT").unwrap();
        builder.add_dir("raven").unwrap();
        builder.add_file("EFI/BOOT/BOOTX64.EFI", b"loader").unwrap();
        builder.add_file("raven/kernel.elf", b"kernel").unwrap();
        builder.add_file("raven/initfs.img", b"initfs").unwrap();
        builder.with_epoch(Some(1_700_000_000)).finish().unwrap()
    }

    #[test]
    fn reopened_image_lists_and_reads_files() {
        let bytes = sample();
        let filesystem = FileSystem::new(Cursor::new(bytes), FsOptions::new()).unwrap();
        let root = filesystem.root_dir();
        assert!(root.open_dir("EFI").is_ok());
        assert!(root.open_dir("EFI/BOOT").is_ok());
        assert!(root.open_dir("raven").is_ok());
        let mut file = root.open_file("raven/kernel.elf").unwrap();
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut data).unwrap();
        assert_eq!(data, b"kernel");
    }

    #[test]
    fn output_is_byte_identical_across_runs() {
        assert_eq!(sample(), sample());
    }

    #[test]
    fn volume_label_and_id_are_fixed() {
        let bytes = sample();
        assert_eq!(&bytes[71..82], &VOLUME_LABEL);
        assert_eq!(
            u32::from_le_bytes(bytes[67..71].try_into().unwrap()),
            VOLUME_ID
        );
    }

    #[test]
    fn epoch_is_clamped_to_the_fat_range() {
        assert_eq!(fat_datetime(0), (1980, 1, 1, 0, 0, 0));
        assert_eq!(fat_datetime(i64::MAX), (2107, 12, 31, 23, 59, 58));
    }

    #[test]
    fn invalid_paths_are_rejected() {
        let mut builder = FatImageBuilder::new(SIZE).unwrap();
        assert!(builder.add_file("/absolute", b"x").is_err());
        assert!(builder.add_file("a/../b", b"x").is_err());
        assert!(builder.add_file("", b"x").is_err());
    }
}
