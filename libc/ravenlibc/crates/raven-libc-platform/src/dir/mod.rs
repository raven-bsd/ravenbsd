// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Directory helpers that are pure libc (the `opendir` family is C46).

use raven_abi::errno::Errno;

pub mod getcwd;

pub use getcwd::getcwd;

/// Offset of `d_name` in the public FreeBSD `struct dirent`.
pub const DIRENT_NAME_OFFSET: usize = 24;
/// Length of the shortest public record (a one-byte name).
pub const DIRENT_MIN_LEN: usize = 32;
/// Length of the longest public record (a 255-byte name).
pub const DIRENT_MAX_LEN: usize = 280;

/// The FreeBSD `_GENERIC_DIRLEN`: header, name, NUL, rounded up to eight bytes.
pub const fn dirent_reclen(namlen: usize) -> usize {
    (DIRENT_NAME_OFFSET + namlen + 7) & !7
}

/// Largest private K60 record the server can return (16-byte header, 255-byte name, 8-align).
const K60_MAX_RECORD: usize = 272;
/// Smallest caller buffer that can receive one maximum-length record through the read window.
///
/// A K60 record expands to a public record by at most 5/3 (272 -> 280 for a full name), so
/// `getdents` reads 3/5 of the caller buffer and the buffer must hold one maximum K60 record.
pub const DIRENT_MIN_BUFFER: usize = (K60_MAX_RECORD * 5).div_ceil(3);

/// Maximum name length in one public record.
pub const DIRENT_NAME_MAX: usize = 255;

/// Encode packed K60 entries into public FreeBSD `struct dirent` records.
///
/// The private record is `raven_proto_fs::DIRENT_HEADER_LEN` bytes plus the name padded to eight;
/// the public record is the donor 24-byte header plus the name and its NUL, padded to eight. The
/// private stream carries no seek cookie, so `d_off` is zero and the open-file-description cursor
/// remains the iteration state (`seekdir(0)` is the rewind).
///
/// # Errors
///
/// `EPROTO` for a malformed private record, `ENAMETOOLONG` for a name the public record cannot
/// hold, and `EIO` when the encoded record would not fit the caller buffer.
pub fn encode_dirents(raw: &[u8], out: &mut [u8]) -> Result<usize, Errno> {
    let mut written = 0usize;
    for item in raven_proto_fs::DirEntryIter::new(raw) {
        let entry = item.map_err(|_| Errno::EPROTO)?;
        let name = entry.name;
        if name.len() > DIRENT_NAME_MAX {
            return Err(Errno::ENAMETOOLONG);
        }
        let record_len = dirent_reclen(name.len());
        let Some(end) = written.checked_add(record_len) else {
            return Err(Errno::EIO);
        };
        if end > out.len() {
            return Err(Errno::EIO);
        }
        let record = out.get_mut(written..end).ok_or(Errno::EIO)?;
        record.fill(0);
        record
            .get_mut(0..8)
            .ok_or(Errno::EIO)?
            .copy_from_slice(&entry.header.ino.to_le_bytes());
        record
            .get_mut(16..18)
            .ok_or(Errno::EIO)?
            .copy_from_slice(&(record_len as u16).to_le_bytes());
        record[18] = entry.header.kind;
        record
            .get_mut(20..22)
            .ok_or(Errno::EIO)?
            .copy_from_slice(&(name.len() as u16).to_le_bytes());
        record
            .get_mut(DIRENT_NAME_OFFSET..DIRENT_NAME_OFFSET + name.len())
            .ok_or(Errno::EIO)?
            .copy_from_slice(name);
        written = end;
    }
    Ok(written)
}

#[cfg(test)]
#[path = "dir_ut.rs"]
mod tests;
