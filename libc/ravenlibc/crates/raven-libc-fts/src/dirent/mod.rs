// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `DIR` stream and the Linux `dirent64` record (doc 07 3.6 and 4.3).
//!
//! `DirStream` is the musl `__dirstream` design (COPY, MIT via `musl/COPYRIGHT`): a 2048-byte
//! `getdents` buffer, the `d_off` of the last returned entry, and `buf_pos`/`buf_end`. The record
//! parser is ours (doc 07 4.3, `RD-new: malformed getdents record -> EIO`): every length is
//! bounds-checked with `checked_add`, `d_reclen` below 20 or past the buffer is `EIO`, and a name
//! without a NUL inside its record is `EIO` too.

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

/// One parsed `dirent64` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    /// `d_ino`: the inode number.
    pub ino: u64,
    /// `d_off`: the `lseek` cookie that resumes after this record (OI-2).
    pub off: i64,
    /// `d_type`: one of the `DT_*` values.
    pub typ: u8,
    /// `d_name` without its NUL.
    pub name: Vec<u8>,
    /// `d_reclen`: the record length, at least 20.
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

/// Parse one Linux `dirent64` record at `pos`; returns the entry and the next position.
///
/// Layout (`doc 07 4.3`): `d_ino: u64` at 0, `d_off: i64` at 8, `d_reclen: u16` at 16,
/// `d_type: u8` at 18, `d_name: [u8; ...]` NUL-terminated at 19.
///
/// # Errors
///
/// `EIO` for a truncated header, `d_reclen < 20`, a record past the buffer, a name without a NUL
/// inside the record, or an allocation failure (`ENOMEM`).
pub fn parse_dirent(buf: &[u8], pos: usize) -> Result<(DirEntry, usize), Errno> {
    let header_end = pos.checked_add(20).ok_or(Errno(EIO as u16))?;
    let Some(header) = buf.get(pos..header_end) else {
        return Err(Errno(EIO as u16));
    };
    let ino = le_u64(header, 0)?;
    let off = le_i64(header, 8)?;
    let reclen = le_u16(header, 16)?;
    let typ = header.get(18).copied().unwrap_or(0);
    if reclen < 20 {
        return Err(Errno(EIO as u16));
    }
    let record_end = pos
        .checked_add(usize::from(reclen))
        .ok_or(Errno(EIO as u16))?;
    let Some(record) = buf.get(pos..record_end) else {
        return Err(Errno(EIO as u16));
    };
    let Some(name_field) = record.get(19..) else {
        return Err(Errno(EIO as u16));
    };
    let Some(nul) = name_field.iter().position(|&b| b == 0) else {
        return Err(Errno(EIO as u16));
    };
    let mut name = Vec::new();
    name.try_reserve(nul).map_err(|_| Errno(ENOMEM as u16))?;
    name.extend_from_slice(name_field.get(..nul).unwrap_or(&[]));
    Ok((
        DirEntry {
            ino,
            off,
            typ,
            name,
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
