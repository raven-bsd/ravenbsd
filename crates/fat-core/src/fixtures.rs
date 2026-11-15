// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic FAT golden images for the host tests and the fuzz replay harness.
//!
//! Every image is fresh, first-party test data: a boot sector/BPB, one FAT, the root directory,
//! an 8.3 file, a multi-cluster file with a two-entry long name, and a subdirectory. The FAT12
//! image is the small fuzz base; FAT16 and FAT32 exercise the other element widths. This module
//! is compiled only for tests or with the `fixtures` feature.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::{BOOT_SECTOR_SIZE, FatKind};

const SECTOR: usize = 512;
const MEDIA: u8 = 0xf8;
const VOLUME_LABEL: &[u8; 11] = b"RAVEN      ";

/// A valid FAT12 image (64 sectors, one cluster per sector, static root).
#[must_use]
pub fn fat12_image() -> Vec<u8> {
    image(FatKind::Fat12)
}

/// A valid FAT16 image (5,000 sectors, static root).
#[must_use]
pub fn fat16_image() -> Vec<u8> {
    image(FatKind::Fat16)
}

/// A valid FAT32 image (66,100 sectors, cluster-chain root).
#[must_use]
pub fn fat32_image() -> Vec<u8> {
    image(FatKind::Fat32)
}

/// Build a valid image of `kind`.
///
/// # Panics
/// Panics only when the fixed fixture geometry is inconsistent; this is test data.
#[must_use]
pub fn image(kind: FatKind) -> Vec<u8> {
    let (total_sectors, root_entries, fat_sectors, root_cluster) = match kind {
        FatKind::Fat12 => (64u32, 16u16, 1u32, 0u32),
        FatKind::Fat16 => (5_000u32, 16u16, 20u32, 0u32),
        FatKind::Fat32 => (66_100u32, 0u16, 513u32, 7u32),
    };
    let total_bytes = usize::try_from(total_sectors).expect("fixture size") * SECTOR;
    let mut image = vec![0u8; total_bytes];
    let fat_start = SECTOR;
    let fat_bytes = usize::try_from(fat_sectors).expect("fat size") * SECTOR;
    let root_dir_sectors = (usize::from(root_entries) * 32).div_ceil(SECTOR);
    let root_dir_start = fat_start + fat_bytes;
    let data_start = root_dir_start + root_dir_sectors * SECTOR;
    let cluster_count = u32::try_from((total_bytes - data_start) / SECTOR).expect("cluster count");

    // Boot sector and BPB.
    image[0] = 0xeb;
    image[1] = 0x3c;
    image[2] = 0x90;
    image[3..11].copy_from_slice(b"RAVENBSD");
    put16(&mut image, 11, 512);
    image[13] = 1;
    put16(&mut image, 14, 1);
    image[16] = 1;
    put16(&mut image, 17, root_entries);
    if total_sectors <= u32::from(u16::MAX) {
        put16(&mut image, 19, total_sectors as u16);
    } else {
        put16(&mut image, 19, 0);
        put32(&mut image, 32, total_sectors);
    }
    image[21] = MEDIA;
    if kind == FatKind::Fat32 {
        put16(&mut image, 22, 0);
        put32(&mut image, 36, fat_sectors);
        put32(&mut image, 40, 0);
        put16(&mut image, 42, 0);
        put32(&mut image, 44, root_cluster);
        put16(&mut image, 48, 0);
        put16(&mut image, 50, 0);
        put16(&mut image, 24, 32);
        put16(&mut image, 26, 64);
        image[64] = 0x80;
        image[66] = 0x29;
        put32(&mut image, 67, 0x4e41_5652);
        image[71..82].copy_from_slice(VOLUME_LABEL);
        image[82..90].copy_from_slice(b"FAT32   ");
    } else {
        put16(&mut image, 22, fat_sectors as u16);
        put16(&mut image, 24, 32);
        put16(&mut image, 26, 64);
        image[36] = 0x80;
        image[38] = 0x29;
        put32(&mut image, 39, 0x4e41_5652);
        image[43..54].copy_from_slice(VOLUME_LABEL);
        image[54..62].copy_from_slice(match kind {
            FatKind::Fat12 => b"FAT12   ",
            FatKind::Fat16 => b"FAT16   ",
            FatKind::Fat32 => unreachable!(),
        });
    }
    image[510] = 0x55;
    image[511] = 0xaa;

    // FAT: reserved entries, hello, the two-cluster long file, the directory and its child.
    set_fat(&mut image, kind, fat_start, 0, media_end(kind));
    set_fat(&mut image, kind, fat_start, 1, end_of_chain(kind));
    set_fat(&mut image, kind, fat_start, 2, end_of_chain(kind));
    set_fat(&mut image, kind, fat_start, 3, 4);
    set_fat(&mut image, kind, fat_start, 4, end_of_chain(kind));
    set_fat(&mut image, kind, fat_start, 5, end_of_chain(kind));
    set_fat(&mut image, kind, fat_start, 6, end_of_chain(kind));
    if kind == FatKind::Fat32 {
        set_fat(
            &mut image,
            kind,
            fat_start,
            root_cluster,
            end_of_chain(kind),
        );
    }

    // Root directory.
    let mut root = Vec::new();
    short_entry(&mut root, b"HELLO   TXT", 0x20, 2, 5, 0);
    long_entries(&mut root, "A very long file name.txt", b"AVERYL~1TXT");
    short_entry(&mut root, b"AVERYL~1TXT", 0x20, 3, 600, 0);
    short_entry(&mut root, b"SUBDIR     ", 0x10, 5, 0, 0);
    if kind == FatKind::Fat32 {
        write_cluster(&mut image, data_start, root_cluster, &root);
    } else {
        let end = root_dir_start + root.len();
        image[root_dir_start..end].copy_from_slice(&root);
    }

    // Subdirectory cluster.
    let mut subdir = Vec::new();
    short_entry(&mut subdir, b".          ", 0x10, 5, 0, 0);
    short_entry(
        &mut subdir,
        b"..         ",
        0x10,
        if kind == FatKind::Fat32 {
            root_cluster
        } else {
            0
        },
        0,
        0,
    );
    short_entry(&mut subdir, b"INNER   TXT", 0x20, 6, 5, 0);
    write_cluster(&mut image, data_start, 5, &subdir);

    // File data.
    write_cluster(&mut image, data_start, 2, b"hello");
    let long: Vec<u8> = (0..600).map(|index| b'a' + (index % 26) as u8).collect();
    write_cluster(&mut image, data_start, 3, &long[..512]);
    write_cluster(&mut image, data_start, 4, &long[512..]);
    write_cluster(&mut image, data_start, 6, b"inner");

    let _ = cluster_count;
    image
}

/// Absolute byte offset of the first FAT.
#[must_use]
pub fn fat_start(kind: FatKind) -> usize {
    let _ = kind;
    SECTOR
}

/// Absolute byte offset of the static root directory (FAT12/16) or the start of the data region.
#[must_use]
pub fn root_dir_start(kind: FatKind) -> usize {
    SECTOR + fat_sectors(kind) * SECTOR
}

/// Absolute byte offset of the data region.
#[must_use]
pub fn data_start(kind: FatKind) -> usize {
    let root_entries = match kind {
        FatKind::Fat12 | FatKind::Fat16 => 16,
        FatKind::Fat32 => 0,
    };
    root_dir_start(kind) + (root_entries as usize * 32).div_ceil(SECTOR) * SECTOR
}

/// Absolute byte offset of a data cluster.
#[must_use]
pub fn cluster_offset(kind: FatKind, cluster: u32) -> usize {
    data_start(kind) + (cluster as usize - 2) * SECTOR
}

/// Overwrite one FAT entry (used by the malformed-vector tests).
pub fn set_fat_entry(image: &mut [u8], kind: FatKind, cluster: u32, value: u32) {
    set_fat(image, kind, fat_start(kind), cluster, value);
}

fn fat_sectors(kind: FatKind) -> usize {
    match kind {
        FatKind::Fat12 => 1,
        FatKind::Fat16 => 20,
        FatKind::Fat32 => 513,
    }
}

fn media_end(kind: FatKind) -> u32 {
    match kind {
        FatKind::Fat12 => 0x0ff8,
        FatKind::Fat16 => 0xfff8,
        FatKind::Fat32 => 0x0fff_fff8,
    }
}

fn end_of_chain(kind: FatKind) -> u32 {
    match kind {
        FatKind::Fat12 => 0x0fff,
        FatKind::Fat16 => 0xffff,
        FatKind::Fat32 => 0x0fff_ffff,
    }
}

fn set_fat(image: &mut [u8], kind: FatKind, fat_start: usize, cluster: u32, value: u32) {
    let index = usize::try_from(cluster).expect("cluster");
    match kind {
        FatKind::Fat12 => {
            let offset = fat_start + index + index / 2;
            if index & 1 == 0 {
                image[offset] = (value & 0xff) as u8;
                image[offset + 1] = (image[offset + 1] & 0xf0) | ((value >> 8) & 0x0f) as u8;
            } else {
                image[offset] = (image[offset] & 0x0f) | ((value << 4) & 0xf0) as u8;
                image[offset + 1] = ((value >> 4) & 0xff) as u8;
            }
        }
        FatKind::Fat16 => {
            let offset = fat_start + index * 2;
            put16(image, offset, value as u16);
        }
        FatKind::Fat32 => {
            let offset = fat_start + index * 4;
            put32(image, offset, value);
        }
    }
}

fn write_cluster(image: &mut [u8], data_start: usize, cluster: u32, data: &[u8]) {
    let offset = data_start + usize::try_from(cluster - 2).expect("cluster") * SECTOR;
    let length = data.len().min(SECTOR);
    image[offset..offset + length].copy_from_slice(&data[..length]);
}

fn short_entry(
    out: &mut Vec<u8>,
    name: &[u8; 11],
    attributes: u8,
    first_cluster: u32,
    size: u32,
    case_flags: u8,
) {
    let mut entry = [0u8; 32];
    entry[..11].copy_from_slice(name);
    entry[11] = attributes;
    entry[12] = case_flags;
    put16(&mut entry, 20, (first_cluster >> 16) as u16);
    put16(&mut entry, 26, first_cluster as u16);
    put32(&mut entry, 28, size);
    out.extend_from_slice(&entry);
}

fn long_entries(out: &mut Vec<u8>, name: &str, short: &[u8; 11]) {
    let units: Vec<u16> = name.encode_utf16().collect();
    let chunks = units.len().div_ceil(13);
    let checksum = lfn_checksum(short);
    for chunk in (0..chunks).rev() {
        let ordinal = (chunk + 1) as u8 | if chunk + 1 == chunks { 0x40 } else { 0 };
        let mut entry = [0xffu8; 32];
        entry[0] = ordinal;
        entry[11] = 0x0f;
        entry[12] = 0;
        entry[13] = checksum;
        entry[26] = 0;
        entry[27] = 0;
        let offsets = [1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
        for (slot, offset) in offsets.iter().enumerate() {
            let unit = units.get(chunk * 13 + slot).copied().unwrap_or(0xffff);
            put16(&mut entry, *offset, unit);
        }
        out.extend_from_slice(&entry);
    }
}

fn lfn_checksum(name: &[u8; 11]) -> u8 {
    let mut sum = 0u8;
    for byte in name {
        sum = sum.rotate_right(1).wrapping_add(*byte);
    }
    sum
}

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// A stable name for diagnostics in the fuzz harness.
#[must_use]
pub fn fixture_name() -> String {
    String::from("fat12-64-sector")
}

// Keep the boot-sector constant referenced so a future geometry change cannot silently diverge.
const _: usize = BOOT_SECTOR_SIZE;
