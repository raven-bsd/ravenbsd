// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! File paths and deferred file operations for the Linux file batch.
//!
//! The dispatcher is a pure function table, so handlers validate arguments, resolve Linux paths
//! and queue a [`FileOp`]. `main.rs` performs the scheme `OPEN`, `READ`, `WRITE`, `SEEK`, `DUP`
//! and `CLOSE` calls and installs the resulting descriptors in [`LinuxProcess`].

use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::table::{FileKind, LinuxProcess, USER_BUFFER_MAX};

/// `AT_FDCWD`.
pub const AT_FDCWD: i32 = -100;

/// Linux access-mode mask.
pub const O_ACCMODE: i32 = 0x3;
/// `O_RDONLY`.
pub const O_RDONLY: i32 = 0;
/// `O_WRONLY`.
pub const O_WRONLY: i32 = 1;
/// `O_RDWR`.
pub const O_RDWR: i32 = 2;
/// `O_CREAT`.
pub const O_CREAT: i32 = 0x40;
/// `O_EXCL`.
pub const O_EXCL: i32 = 0x80;
/// `O_NOCTTY`.
pub const O_NOCTTY: i32 = 0x100;
/// `O_TRUNC`.
pub const O_TRUNC: i32 = 0x200;
/// `O_APPEND`.
pub const O_APPEND: i32 = 0x400;
/// `O_NONBLOCK`.
pub const O_NONBLOCK: i32 = 0x800;
/// `O_LARGEFILE`.
pub const O_LARGEFILE: i32 = 0x8000;
/// `O_DIRECTORY`.
pub const O_DIRECTORY: i32 = 0x1_0000;
/// `O_NOFOLLOW`.
pub const O_NOFOLLOW: i32 = 0x2_0000;
/// `O_CLOEXEC`.
pub const O_CLOEXEC: i32 = 0x8_0000;

/// `F_DUPFD`.
pub const F_DUPFD: i32 = 0;
/// `F_GETFD`.
pub const F_GETFD: i32 = 1;
/// `F_SETFD`.
pub const F_SETFD: i32 = 2;
/// `F_GETFL`.
pub const F_GETFL: i32 = 3;
/// `F_SETFL`.
pub const F_SETFL: i32 = 4;
/// `F_DUPFD_CLOEXEC`.
pub const F_DUPFD_CLOEXEC: i32 = 1030;
/// `FD_CLOEXEC`.
pub const FD_CLOEXEC: i32 = 1;

/// `SEEK_SET`.
pub const SEEK_SET: i32 = 0;
/// `SEEK_CUR`.
pub const SEEK_CUR: i32 = 1;
/// `SEEK_END`.
pub const SEEK_END: i32 = 2;

/// Longest Linux path accepted from the trap buffer.
pub const PATH_MAX: usize = 128;

/// One file operation the trap loop must perform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileOp {
    /// Open `scheme` and install it as `path` with the Linux `flags`.
    Open {
        /// Scheme path passed to the kernel `OPEN` (`file:/...` or `ufs:/...`).
        scheme: Vec<u8>,
        /// Linux-visible path stored in the descriptor.
        path: Vec<u8>,
        /// Linux `open` flags.
        flags: i32,
    },
    /// Read from a remote descriptor into the user address.
    Read {
        /// Linux descriptor.
        fd: i32,
        /// User destination.
        address: u64,
        /// Requested byte count (bounded by the trap loop).
        len: usize,
    },
    /// Write buffered user bytes to a remote descriptor.
    Write {
        /// Linux descriptor.
        fd: i32,
        /// Already copied-in bytes.
        data: Vec<u8>,
    },
    /// Positional read from a remote descriptor.
    Pread {
        /// Linux descriptor.
        fd: i32,
        /// User destination.
        address: u64,
        /// Requested byte count (bounded by the trap loop).
        len: usize,
        /// File offset.
        offset: u64,
    },
    /// Positional write to a remote descriptor.
    Pwrite {
        /// Linux descriptor.
        fd: i32,
        /// Already copied-in bytes.
        data: Vec<u8>,
        /// File offset.
        offset: u64,
    },
    /// Move the remote descriptor's cursor.
    Seek {
        /// Linux descriptor.
        fd: i32,
        /// Signed offset.
        offset: i64,
        /// `SEEK_*`.
        whence: i32,
    },
    /// Close a descriptor, releasing a remote scheme handle if present.
    Close {
        /// Linux descriptor.
        fd: i32,
    },
    /// `dup3`: duplicate `oldfd` onto `newfd`.
    Dup3 {
        /// Source descriptor.
        oldfd: i32,
        /// Destination descriptor.
        newfd: i32,
        /// Set `FD_CLOEXEC` on the duplicate.
        cloexec: bool,
    },
    /// `fcntl(F_DUPFD)` / `F_DUPFD_CLOEXEC`: duplicate at the lowest free descriptor.
    FcntlDup {
        /// Source descriptor.
        fd: i32,
        /// Lowest descriptor the caller accepts.
        min: i32,
        /// Set `FD_CLOEXEC` on the duplicate.
        cloexec: bool,
    },
}

/// Whether a path already carries a scheme (a `:` before the first `/`).
#[must_use]
pub fn has_scheme(path: &[u8]) -> bool {
    match path.iter().position(|byte| *byte == b':') {
        Some(index) => !path[..index].contains(&b'/') && index > 0,
        None => false,
    }
}

/// Whether a Linux path belongs to the synthetic `/proc`/`/sys` surface.
#[must_use]
pub fn is_synthetic(path: &[u8]) -> bool {
    path == b"/proc" || path.starts_with(b"/proc/") || path == b"/sys" || path.starts_with(b"/sys/")
}

/// Resolve `path` against the process cwd or a directory descriptor.
///
/// Scheme paths pass through unchanged. Relative paths are joined to the cwd for `AT_FDCWD`, or
/// to the opened directory's path. `.` and `..` are normalised without leaving `/`.
pub fn resolve_path(process: &LinuxProcess, dirfd: i32, path: &[u8]) -> Result<Vec<u8>, Errno> {
    if path.is_empty() {
        return Err(Errno::ENOENT);
    }
    if has_scheme(path) {
        return Ok(path.to_vec());
    }
    let base: &[u8] = if path.starts_with(b"/") {
        b""
    } else if dirfd == AT_FDCWD {
        process.cwd()
    } else {
        let file = process.file(dirfd).ok_or(Errno::EBADF)?;
        if !matches!(file.kind, FileKind::Directory(_) | FileKind::Remote(_)) {
            return Err(Errno::ENOTDIR);
        }
        &file.path
    };
    normalize_join(base, path)
}

/// Turn a Linux path into the scheme path the kernel `OPEN` expects.
#[must_use]
pub fn scheme_path(path: &[u8]) -> Vec<u8> {
    if has_scheme(path) {
        return path.to_vec();
    }
    let mut scheme = b"file:".to_vec();
    scheme.extend_from_slice(path);
    scheme
}

/// Bound a Linux read/write count to the server's copy budget.
#[must_use]
pub fn bounded_len(len: usize) -> usize {
    len.min(USER_BUFFER_MAX)
}

fn normalize_join(base: &[u8], path: &[u8]) -> Result<Vec<u8>, Errno> {
    let mut parts: Vec<&[u8]> = Vec::new();
    for component in base
        .split(|byte| *byte == b'/')
        .chain(path.split(|byte| *byte == b'/'))
    {
        match component {
            b"" | b"." => {}
            b".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    let mut out = Vec::new();
    out.push(b'/');
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            out.push(b'/');
        }
        out.extend_from_slice(part);
    }
    if out.len() > PATH_MAX {
        return Err(Errno::ENAMETOOLONG);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "files_ut.rs"]
mod tests;
