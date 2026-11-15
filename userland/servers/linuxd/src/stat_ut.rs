// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `struct stat`/`struct statx` encoders.

use super::*;
use raven_linux_abi::flags::stat::{S_IFDIR, S_IFREG};

fn metadata() -> Metadata {
    Metadata::new(S_IFREG | 0o444, 1234, 0x1122_3344_5566_7788, 1)
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    let mut word = [0u8; 4];
    word.copy_from_slice(&bytes[offset..offset + 4]);
    u32::from_le_bytes(word)
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    let mut word = [0u8; 8];
    word.copy_from_slice(&bytes[offset..offset + 8]);
    u64::from_le_bytes(word)
}

#[test]
fn the_layouts_match_the_x86_64_abi() {
    assert_eq!(STAT_SIZE, 144);
    assert_eq!(STATX_SIZE, 256);
    assert_eq!(STATX_BASIC_STATS, 0x7ff);
}

#[test]
fn stat_fills_the_basic_fields() {
    let meta = metadata();
    let bytes = encode_stat(&meta);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStat, st_ino)), meta.inode);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStat, st_nlink)), meta.nlink);
    assert_eq!(u32_at(&bytes, offset_of!(AbiStat, st_mode)), meta.mode);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStat, st_size)), meta.size);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStat, st_blksize)), BLKSIZE);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStat, st_blocks)), meta.blocks);
    assert_eq!(meta.blocks, 3);
    assert_eq!(u32_at(&bytes, offset_of!(AbiStat, st_uid)), 0);
    assert_eq!(u32_at(&bytes, offset_of!(AbiStat, st_gid)), 0);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStat, st_atime)), 0);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStat, st_mtime)), 0);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStat, st_ctime)), 0);
}

#[test]
fn stat_keeps_directories_and_their_link_count() {
    let meta = Metadata::new(S_IFDIR | 0o555, 4096, 7, 2);
    let bytes = encode_stat(&meta);
    assert_eq!(
        u32_at(&bytes, offset_of!(AbiStat, st_mode)),
        S_IFDIR | 0o555
    );
    assert_eq!(u64_at(&bytes, offset_of!(AbiStat, st_nlink)), 2);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStat, st_blocks)), 8);
}

#[test]
fn statx_fills_the_basic_mask_and_fields() {
    let meta = metadata();
    let bytes = encode_statx(&meta);
    assert_eq!(
        u32_at(&bytes, offset_of!(AbiStatx, stx_mask)),
        STATX_BASIC_STATS
    );
    assert_eq!(
        u32_at(&bytes, offset_of!(AbiStatx, stx_blksize)),
        BLKSIZE as u32
    );
    assert_eq!(
        u32_at(&bytes, offset_of!(AbiStatx, stx_nlink)),
        meta.nlink as u32
    );
    assert_eq!(u32_at(&bytes, offset_of!(AbiStatx, stx_mode)), meta.mode);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStatx, stx_ino)), meta.inode);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStatx, stx_size)), meta.size);
    assert_eq!(
        u64_at(&bytes, offset_of!(AbiStatx, stx_blocks)),
        meta.blocks
    );
    assert_eq!(u32_at(&bytes, offset_of!(AbiStatx, stx_uid)), 0);
    assert_eq!(u32_at(&bytes, offset_of!(AbiStatx, stx_gid)), 0);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStatx, stx_attributes_mask)), 0);
}

#[test]
fn statx_masks_the_16_bit_mode() {
    let meta = Metadata::new(S_IFREG | 0o444, 0, 1, 1);
    let bytes = encode_statx(&meta);
    assert_eq!(
        u32_at(&bytes, offset_of!(AbiStatx, stx_mode)) & 0xffff,
        meta.mode & 0xffff
    );
}
