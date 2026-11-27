// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `DIR` stream and the public FreeBSD `struct dirent` record (doc 07 3.6 and 4.3).
//!
//! `DirStream` is the musl `__dirstream` design (COPY, MIT via `musl/COPYRIGHT`): a 2048-byte
//! `getdents` buffer, the `d_off` of the last returned entry, and `buf_pos`/`buf_end`. The record
//! parser is ours (doc 07 4.3, `RD-new: malformed getdents record -> EIO`): every length is
//! bounds-checked with `checked_add`, a record shorter than the donor 32-byte minimum or past the
//! buffer is `EIO`, a `d_namlen` outside 1..=255 is `EIO`, and a name without its terminating NUL
//! inside the record is `EIO` too.

use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EIO, ENOMEM};

pub mod closedir;
pub mod dirfd;
pub mod dirstream;
pub mod fdopendir;
pub mod opendir;
pub mod posix_getdents;
pub mod readdir;
pub mod readdir_r;
pub mod rewinddir;
pub mod seekdir;
pub mod telldir;

pub use dirstream::DirStream;

/// One parsed public `struct dirent` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    /// `d_fileno`: the inode number.
    pub ino: u64,
    /// `d_off`: the iteration cookie (zero when the private stream carries none).
    pub off: i64,
    /// `d_type`: one of the `DT_*` values.
    pub typ: u8,
    /// `d_name` without its NUL.
    pub name: Vec<u8>,
    /// `d_reclen`: the record length, at least 32.
    pub reclen: u16,
}

/// A little-endian `u64` at `pos`; `EIO` when out of bounds.
fn le_u64(buf: &[u8], pos: usize) -> Result<u64, Errno> {
    let end = pos.checked_add(8).ok_or(Errno(EIO as u16))?;
    let Some(bytes) = buf.get(pos..end) else {
        return Err(Errno(EIO as u16));
    };
    let mut array = [0u8; 8];
    array.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(array))
}

/// A little-endian `i64` at `pos`; `EIO` when out of bounds.
fn le_i64(buf: &[u8], pos: usize) -> Result<i64, Errno> {
    le_u64(buf, pos).map(|v| v as i64)
}

/// A little-endian `u16` at `pos`; `EIO` when out of bounds.
fn le_u16(buf: &[u8], pos: usize) -> Result<u16, Errno> {
    let end = pos.checked_add(2).ok_or(Errno(EIO as u16))?;
    let Some(bytes) = buf.get(pos..end) else {
        return Err(Errno(EIO as u16));
    };
    let mut array = [0u8; 2];
    array.copy_from_slice(bytes);
    Ok(u16::from_le_bytes(array))
}

/// Offset of `d_name` in the public FreeBSD `struct dirent`.
const DIRENT_NAME_OFFSET: usize = 24;
/// Shortest public record: the header plus a one-byte name and its NUL, 8-aligned.
const DIRENT_MIN_LEN: u16 = 32;
/// Longest name the donor record holds.
const DIRENT_NAME_MAX: usize = 255;

/// Parse one public FreeBSD `struct dirent` record at `pos`; returns the entry and the next
/// position.
///
/// Layout (`freebsd:sys/sys/dirent.h` at 133344405): `d_fileno: u64` at 0, `d_off: i64` at 8,
/// `d_reclen: u16` at 16, `d_type: u8` at 18, `d_pad0` at 19, `d_namlen: u16` at 20, `d_pad1` at
/// 22, `d_name` at 24, NUL after `d_namlen` bytes.
///
/// # Errors
///
/// `EIO` for a truncated header, `d_reclen < 32`, a record past the buffer, a `d_namlen` outside
/// 1..=255, a name longer than its record, a missing NUL or an embedded NUL, or an allocation
/// failure (`ENOMEM`).
pub fn parse_dirent(buf: &[u8], pos: usize) -> Result<(DirEntry, usize), Errno> {
    let header_end = pos
        .checked_add(DIRENT_NAME_OFFSET)
        .ok_or(Errno(EIO as u16))?;
    let Some(header) = buf.get(pos..header_end) else {
        return Err(Errno(EIO as u16));
    };
    let ino = le_u64(header, 0)?;
    let off = le_i64(header, 8)?;
    let reclen = le_u16(header, 16)?;
    let typ = header.get(18).copied().unwrap_or(0);
    if reclen < DIRENT_MIN_LEN {
        return Err(Errno(EIO as u16));
    }
    let record_end = pos
        .checked_add(usize::from(reclen))
        .ok_or(Errno(EIO as u16))?;
    let Some(record) = buf.get(pos..record_end) else {
        return Err(Errno(EIO as u16));
    };
    let namlen = usize::from(le_u16(header, 20)?);
    if namlen == 0 || namlen > DIRENT_NAME_MAX {
        return Err(Errno(EIO as u16));
    }
    let name_end = DIRENT_NAME_OFFSET
        .checked_add(namlen)
        .ok_or(Errno(EIO as u16))?;
    let Some(name) = record.get(DIRENT_NAME_OFFSET..name_end) else {
        return Err(Errno(EIO as u16));
    };
    if name.contains(&0) || record.get(name_end) != Some(&0) {
        return Err(Errno(EIO as u16));
    }
    let mut owned = Vec::new();
    owned
        .try_reserve(namlen)
        .map_err(|_| Errno(ENOMEM as u16))?;
    owned.extend_from_slice(name);
    Ok((
        DirEntry {
            ino,
            off,
            typ,
            name: owned,
            reclen,
        },
        record_end,
    ))
}

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;

#[cfg(test)]
#[path = "ops_ut.rs"]
mod ops_tests;
