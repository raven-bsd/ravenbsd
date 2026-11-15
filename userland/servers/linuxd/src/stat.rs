// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Linux `struct stat` and `struct statx` encoding.
//!
//! The offsets come from the x86-64 layouts owned by `raven-linux-abi`; the encoders write
//! little-endian fields into zeroed buffers, so no `unsafe` pointer casts and no duplicate layout
//! definitions are needed. Only the basic metadata the synthetic tree can answer is filled:
//! type/mode, inode, link count, size, block counts, owner 0 and zero timestamps.

use core::mem::{offset_of, size_of};

use raven_linux_abi::structs::x86_64::{Stat, Statx};

use crate::procfs::{BLKSIZE, Metadata};

type AbiStat = Stat;
type AbiStatx = Statx;

/// Size of the x86-64 `struct stat`.
pub const STAT_SIZE: usize = size_of::<AbiStat>();
/// Size of the x86-64 `struct statx`.
pub const STATX_SIZE: usize = size_of::<AbiStatx>();

/// `STATX_BASIC_STATS`: the fields this server can answer.
pub const STATX_BASIC_STATS: u32 = 0x07ff;

/// Encode `metadata` as an x86-64 `struct stat`.
#[must_use]
pub fn encode_stat(metadata: &Metadata) -> [u8; STAT_SIZE] {
    let mut out = [0u8; STAT_SIZE];
    put_u64(&mut out, offset_of!(AbiStat, st_ino), metadata.inode);
    put_u64(&mut out, offset_of!(AbiStat, st_nlink), metadata.nlink);
    put_u32(&mut out, offset_of!(AbiStat, st_mode), metadata.mode);
    put_u64(&mut out, offset_of!(AbiStat, st_size), metadata.size);
    put_u64(&mut out, offset_of!(AbiStat, st_blksize), BLKSIZE);
    put_u64(&mut out, offset_of!(AbiStat, st_blocks), metadata.blocks);
    out
}

/// Encode `metadata` as an x86-64 `struct statx`.
#[must_use]
pub fn encode_statx(metadata: &Metadata) -> [u8; STATX_SIZE] {
    let mut out = [0u8; STATX_SIZE];
    put_u32(&mut out, offset_of!(AbiStatx, stx_mask), STATX_BASIC_STATS);
    put_u32(&mut out, offset_of!(AbiStatx, stx_blksize), BLKSIZE as u32);
    put_u32(
        &mut out,
        offset_of!(AbiStatx, stx_nlink),
        u32::try_from(metadata.nlink).unwrap_or(u32::MAX),
    );
    put_u16(
        &mut out,
        offset_of!(AbiStatx, stx_mode),
        u16::try_from(metadata.mode & 0xffff).unwrap_or(0),
    );
    put_u64(&mut out, offset_of!(AbiStatx, stx_ino), metadata.inode);
    put_u64(&mut out, offset_of!(AbiStatx, stx_size), metadata.size);
    put_u64(&mut out, offset_of!(AbiStatx, stx_blocks), metadata.blocks);
    out
}

fn put_u16(out: &mut [u8], offset: usize, value: u16) {
    out[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut [u8], offset: usize, value: u32) {
    out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut [u8], offset: usize, value: u64) {
    out[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
#[path = "stat_ut.rs"]
mod tests;
