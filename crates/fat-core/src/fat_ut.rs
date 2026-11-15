// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the FAT parser: golden FAT12/16/32 images, 30 malformed vectors and bit-flip
//! robustness.

use alloc::vec::Vec;

use super::fixtures::{
    cluster_offset, data_start, fat12_image, fat16_image, fat32_image, root_dir_start,
    set_fat_entry,
};
use super::*;

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn root_entry(kind: FatKind, index: usize) -> usize {
    let base = if kind == FatKind::Fat32 {
        cluster_offset(kind, 7)
    } else {
        root_dir_start(kind)
    };
    base + index * DIR_ENTRY_SIZE
}

fn set_entry_cluster(image: &mut [u8], kind: FatKind, index: usize, cluster: u32) {
    let entry = root_entry(kind, index);
    put16(image, entry + 20, (cluster >> 16) as u16);
    put16(image, entry + 26, cluster as u16);
}

fn set_entry_size(image: &mut [u8], kind: FatKind, index: usize, size: u32) {
    let entry = root_entry(kind, index);
    put32(image, entry + 28, size);
}

#[test]
fn fat12_golden_image() {
    let image = fat12_image();
    let volume = FatVolume::parse(&image).expect("FAT12");
    assert_eq!(volume.kind(), FatKind::Fat12);
    assert_eq!(volume.cluster_count(), 61);
    assert_eq!(volume.cluster_size(), 512);
    let root = volume.root_dir().expect("root");
    let names: Vec<&str> = root.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["HELLO.TXT", "A very long file name.txt", "SUBDIR"]);
    assert!(root[0].is_file());
    assert!(root[2].is_dir());

    let hello = root[0].clone();
    assert_eq!(volume.read_file(&hello, 64).expect("hello"), b"hello");
    let long = volume
        .lookup(b"a very long file name.txt")
        .expect("lookup")
        .expect("long file");
    let data = volume.read_file(&long, 1024).expect("long read");
    assert_eq!(data.len(), 600);
    assert_eq!(data[0], b'a');
    assert_eq!(data[599], b'a' + (599 % 26) as u8);
    let inner = volume
        .lookup(b"SUBDIR/inner.txt")
        .expect("lookup inner")
        .expect("inner file");
    assert_eq!(volume.read_file(&inner, 64).expect("inner"), b"inner");
    assert!(volume.lookup(b"missing").expect("missing lookup").is_none());
    assert_eq!(volume.lookup(b"../x"), Err(FatError::InvalidPath));
}

#[test]
fn fat16_golden_image() {
    let image = fat16_image();
    let volume = FatVolume::parse(&image).expect("FAT16");
    assert_eq!(volume.kind(), FatKind::Fat16);
    assert!(volume.cluster_count() > 4_084);
    assert_eq!(volume.root_dir().expect("root").len(), 3, "root entries");
    let hello = volume.lookup(b"HELLO.TXT").expect("lookup").expect("hello");
    assert_eq!(volume.read_file(&hello, 8).expect("hello"), b"hello");
}

#[test]
fn fat32_golden_image() {
    let image = fat32_image();
    let volume = FatVolume::parse(&image).expect("FAT32");
    assert_eq!(volume.kind(), FatKind::Fat32);
    assert!(volume.cluster_count() >= 65_525);
    assert_eq!(volume.root_cluster(), 7);
    let root = volume.root_dir().expect("root");
    assert_eq!(root.len(), 3);
    let long = volume
        .lookup(b"A VERY LONG FILE NAME.TXT")
        .expect("lookup")
        .expect("long");
    assert_eq!(volume.read_file(&long, 1024).expect("read").len(), 600);
    let inner = volume
        .lookup(b"subdir/INNER.TXT")
        .expect("lookup")
        .expect("inner");
    assert_eq!(volume.read_file(&inner, 5).expect("inner"), b"inner");
}

#[test]
fn read_limit_is_honoured() {
    let image = fat12_image();
    let volume = FatVolume::parse(&image).expect("FAT12");
    let hello = volume.lookup(b"HELLO.TXT").expect("lookup").expect("hello");
    assert_eq!(volume.read_file(&hello, 2).expect("limit"), b"he");
}

#[test]
fn malformed_vectors_are_rejected() {
    let mut vectors: Vec<(&str, Vec<u8>)> = Vec::new();
    let base = fat12_image();
    vectors.push(("short-boot", base[..511].to_vec()));

    let mut disk = base.clone();
    disk[510] = 0;
    vectors.push(("boot-signature", disk));

    let mut disk = base.clone();
    disk[0] = 0;
    vectors.push(("bad-jump", disk));

    let mut disk = base.clone();
    disk[2] = 0;
    vectors.push(("bad-jump-short", disk));

    let mut disk = base.clone();
    put16(&mut disk, 11, 0);
    vectors.push(("bytes-per-sector-zero", disk));

    let mut disk = base.clone();
    put16(&mut disk, 11, 513);
    vectors.push(("bytes-per-sector-odd", disk));

    let mut disk = base.clone();
    disk[13] = 0;
    vectors.push(("sectors-per-cluster-zero", disk));

    let mut disk = base.clone();
    disk[13] = 3;
    vectors.push(("sectors-per-cluster-not-power", disk));

    let mut disk = base.clone();
    disk[13] = 255;
    vectors.push(("sectors-per-cluster-large", disk));

    let mut disk = base.clone();
    put16(&mut disk, 14, 0);
    vectors.push(("reserved-zero", disk));

    let mut disk = base.clone();
    disk[16] = 0;
    vectors.push(("fat-count-zero", disk));

    let mut disk = base.clone();
    disk[16] = 5;
    vectors.push(("fat-count-large", disk));

    let mut disk = base.clone();
    disk[21] = 0;
    vectors.push(("media", disk));

    let mut disk = base.clone();
    put16(&mut disk, 19, 0);
    put32(&mut disk, 32, 0);
    vectors.push(("total-zero", disk));

    let mut disk = base.clone();
    put16(&mut disk, 22, 0);
    vectors.push(("fat-size-zero", disk));

    vectors.push(("truncated", base[..base.len() - 512].to_vec()));

    let mut disk = base.clone();
    put16(&mut disk, 22, 100);
    vectors.push(("fat-past-volume", disk));

    let mut disk = base.clone();
    put16(&mut disk, 17, 0);
    vectors.push(("root-entries-zero-fat12", disk));

    let mut disk = fat16_image();
    put16(&mut disk, 17, 0);
    vectors.push(("root-entries-zero-fat16", disk));

    let mut disk = fat16_image();
    put16(&mut disk, 22, 1);
    vectors.push(("fat-table-too-small", disk));

    assert!(vectors.len() >= 20, "only {} vectors", vectors.len());
    for (name, disk) in vectors {
        assert!(
            FatVolume::parse(&disk).is_err(),
            "vector `{name}` unexpectedly parsed"
        );
    }
}

#[test]
fn fat32_malformed_root_is_rejected() {
    let image = fat32_image();
    let mut disk = image.clone();
    put32(&mut disk, 44, 0);
    assert_eq!(
        FatVolume::parse(&disk).err(),
        Some(FatError::BadRootCluster)
    );

    let mut disk = image;
    put32(&mut disk, 44, 70_000);
    assert_eq!(
        FatVolume::parse(&disk).err(),
        Some(FatError::BadRootCluster)
    );
}

#[test]
fn directory_and_chain_defects_are_rejected() {
    let image = fat12_image();

    let mut disk = image.clone();
    put32(&mut disk, root_entry(FatKind::Fat12, 0) + 28, 1_000);
    let broken = FatVolume::parse(&disk).expect("FAT12");
    let hello = broken.lookup(b"HELLO.TXT").expect("lookup").expect("hello");
    assert_eq!(broken.read_file(&hello, 2_000), Err(FatError::BadLength));

    let mut disk = image.clone();
    set_fat_entry(&mut disk, FatKind::Fat12, 3, 3);
    let broken = FatVolume::parse(&disk).expect("FAT12");
    let long = broken
        .lookup(b"A very long file name.txt")
        .expect("lookup")
        .expect("long");
    assert_eq!(broken.read_file(&long, 2_000), Err(FatError::ChainLoop(3)));

    let mut disk = image.clone();
    set_fat_entry(&mut disk, FatKind::Fat12, 3, 0);
    let broken = FatVolume::parse(&disk).expect("FAT12");
    let long = broken
        .lookup(b"A very long file name.txt")
        .expect("lookup")
        .expect("long");
    assert!(matches!(
        broken.read_file(&long, 2_000),
        Err(FatError::BadCluster(_))
    ));

    let mut disk = image.clone();
    set_fat_entry(&mut disk, FatKind::Fat12, 3, 0x7ff);
    let broken = FatVolume::parse(&disk).expect("FAT12");
    let long = broken
        .lookup(b"A very long file name.txt")
        .expect("lookup")
        .expect("long");
    assert!(matches!(
        broken.read_file(&long, 2_000),
        Err(FatError::BadCluster(_))
    ));

    let mut disk = image.clone();
    set_entry_size(&mut disk, FatKind::Fat12, 0, 5);
    set_entry_cluster(&mut disk, FatKind::Fat12, 0, 0);
    let broken = FatVolume::parse(&disk).expect("FAT12");
    let hello = broken.lookup(b"HELLO.TXT").expect("lookup").expect("hello");
    assert_eq!(broken.read_file(&hello, 8), Err(FatError::BadCluster(0)));

    // A dangling LFN sequence: the short entry is deleted, the next entry is not named by it.
    let mut disk = image.clone();
    disk[root_entry(FatKind::Fat12, 3)] = 0;
    let broken = FatVolume::parse(&disk).expect("FAT12");
    assert_eq!(broken.root_dir(), Err(FatError::BadLfn));

    let mut disk = image.clone();
    disk[root_entry(FatKind::Fat12, 1) + 13] ^= 1;
    let broken = FatVolume::parse(&disk).expect("FAT12");
    assert_eq!(broken.root_dir(), Err(FatError::BadLfn));

    let mut disk = image.clone();
    set_entry_cluster(&mut disk, FatKind::Fat12, 0, 0xffff);
    let broken = FatVolume::parse(&disk).expect("FAT12");
    assert!(matches!(broken.root_dir(), Err(FatError::BadCluster(_))));

    let mut disk = image.clone();
    set_entry_cluster(&mut disk, FatKind::Fat12, 4, 0);
    let broken = FatVolume::parse(&disk).expect("FAT12");
    assert!(matches!(broken.root_dir(), Err(FatError::BadCluster(0))));

    let mut disk = image;
    for offset in 0..11 {
        disk[root_entry(FatKind::Fat12, 0) + offset] = b' ';
    }
    let broken = FatVolume::parse(&disk).expect("FAT12");
    assert_eq!(broken.root_dir(), Err(FatError::BadDirectory));
}

#[test]
fn bit_flips_never_panic() {
    let base = fat12_image();
    let mut state = 0xd1b5_4a32_d192_ed03u64;
    for _ in 0..256 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let mut disk = base.clone();
        let at = (state as usize) % disk.len();
        disk[at] ^= ((state >> 32) as u8) | 1;
        if let Ok(volume) = FatVolume::parse(&disk) {
            let _ = volume.root_dir();
            let _ = volume.lookup(b"HELLO.TXT");
            let _ = volume.lookup(b"A very long file name.txt");
        }
    }
}

#[test]
fn fixture_geometry_is_consistent() {
    assert_eq!(data_start(FatKind::Fat12), 1_536);
    assert_eq!(cluster_offset(FatKind::Fat12, 2), 1_536);
    assert_eq!(
        cluster_offset(FatKind::Fat32, 2),
        data_start(FatKind::Fat32)
    );
}
