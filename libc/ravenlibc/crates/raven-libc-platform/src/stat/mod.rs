// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `struct stat` and `struct statvfs` conversion (doc 11 section 3.12).
//!
//! Three stat layers stay distinct: the 64-byte wire record of `raven-proto-fs`, the 128-byte
//! libc `struct stat` frozen in `RAVEN_1.0` (H-15) and the Linux x86-64 144-byte record that only
//! `linuxd` knows. This module converts wire to libc and never invents fields the wire lacks:
//! `st_dev`/`st_rdev` are zero, `st_blksize` is `PAGE_SIZE`, nanoseconds are zero, and
//! `STATX_BTIME` is never claimed.

use raven_abi::PAGE_SIZE;
use raven_libc_abi::structs::{Stat, Statvfs, Timespec};
use raven_proto_fs::{Stat as WireStat, StatFs};

use crate::fd::ofd::OfdKind;

/// Where a stat record came from; controls the file-type bits only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatSource {
    /// A regular file.
    File,
    /// A directory.
    Dir,
    /// A terminal.
    Tty,
    /// A socket.
    Socket,
    /// A pipe.
    Pipe,
    /// Adopted handle of unknown kind.
    Unknown,
}

impl StatSource {
    /// Map an OFD kind.
    pub const fn from_kind(kind: OfdKind) -> Self {
        match kind {
            OfdKind::File => Self::File,
            OfdKind::Dir => Self::Dir,
            OfdKind::Tty => Self::Tty,
            OfdKind::Socket => Self::Socket,
            OfdKind::Pipe => Self::Pipe,
            _ => Self::Unknown,
        }
    }
}

/// Convert the 64-byte wire record to the libc `struct stat`.
pub fn from_wire(wire: &WireStat, source: StatSource) -> Stat {
    let mut mode = wire.mode;
    if mode & raven_libc_abi::stat::S_IFMT == 0 {
        mode |= match source {
            StatSource::Dir => raven_libc_abi::stat::S_IFDIR,
            StatSource::Tty => raven_libc_abi::stat::S_IFCHR,
            StatSource::Socket => raven_libc_abi::stat::S_IFSOCK,
            StatSource::Pipe => raven_libc_abi::stat::S_IFIFO,
            StatSource::File | StatSource::Unknown => raven_libc_abi::stat::S_IFREG,
        };
    }
    Stat {
        st_dev: 0,
        st_ino: wire.ino,
        st_mode: mode,
        st_nlink: wire.nlink,
        st_uid: wire.uid,
        st_gid: wire.gid,
        st_rdev: 0,
        __pad1: 0,
        st_size: wire.size as i64,
        st_blksize: PAGE_SIZE as i32,
        __pad2: 0,
        st_blocks: wire.blocks as i64,
        st_atim: Timespec {
            tv_sec: wire.atime,
            tv_nsec: 0,
        },
        st_mtim: Timespec {
            tv_sec: wire.mtime,
            tv_nsec: 0,
        },
        st_ctim: Timespec {
            tv_sec: wire.ctime,
            tv_nsec: 0,
        },
        __unused: [0; 2],
    }
}

/// Convert `StatFs` to `struct statvfs`; unknown fields keep the documented defaults.
pub fn statvfs_from_wire(wire: &StatFs) -> Statvfs {
    Statvfs {
        f_bsize: u64::from(wire.block_size),
        f_frsize: u64::from(wire.block_size),
        f_blocks: wire.blocks,
        f_bfree: wire.free_blocks,
        // G-19 adds `bavail`; until then free blocks is the honest lower bound.
        f_bavail: wire.free_blocks,
        f_files: wire.files,
        f_ffree: wire.free_files,
        f_favail: wire.free_files,
        f_fsid: 0,
        f_flag: 0,
        f_namemax: raven_libc_abi::limits::NAME_MAX as u64,
        f_type: 0,
        __reserved: [0; 5],
    }
}

/// The `statx` mask bits the wire can honestly fill; `STATX_BTIME` is never included.
pub const STATX_BASIC_MASK: u32 = 0x0000_07ff;

/// The fixed conversion defaults, mirrored by `tests/fixtures/xlat.toml`.
pub const STAT_DEFAULTS: &[(&str, i64)] = &[
    ("st_dev", 0),
    ("st_rdev", 0),
    ("st_blksize", 4096),
    ("st_blocks_unit", 512),
    ("statx_btime", 0),
];

#[cfg(test)]
#[path = "stat_ut.rs"]
mod tests;
