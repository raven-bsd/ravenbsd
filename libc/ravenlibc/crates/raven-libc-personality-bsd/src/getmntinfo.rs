// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `getmntinfo`/`getfsstat` (`X56`, `15` T4 row, `PG-37`).
//!
//! The mount list needs `MOUNT_LIST` (`PG-37`); until it exists the [`MountList`] default is
//! `ENOSYS` and `getmntinfo` returns 0 with `errno = ENOSYS` in the shell. The record is the
//! Linux `struct statfs` layout of DR-13, not the BSD shape.

use alloc::vec::Vec;

use crate::seams::{Errno, MountList};

/// `MNT_WAIT`.
pub const MNT_WAIT: i32 = 1;
/// `MNT_NOWAIT`.
pub const MNT_NOWAIT: i32 = 2;
/// `MNT_LAZY`.
pub const MNT_LAZY: i32 = 3;

/// The Linux `struct statfs` (120 bytes on LP64).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatfsLinux {
    /// Filesystem type.
    pub f_type: u64,
    /// Preferred block size.
    pub f_bsize: u64,
    /// Total blocks.
    pub f_blocks: u64,
    /// Free blocks.
    pub f_bfree: u64,
    /// Blocks available to unprivileged users.
    pub f_bavail: u64,
    /// Total file nodes.
    pub f_files: u64,
    /// Free file nodes.
    pub f_ffree: u64,
    /// Filesystem id.
    pub f_fsid: [i32; 2],
    /// Maximum file name length.
    pub f_namelen: u64,
    /// Fragment size.
    pub f_frsize: u64,
    /// Mount flags.
    pub f_flags: u64,
    /// Spare.
    pub f_spare: [u64; 4],
}

const _: () = assert!(core::mem::size_of::<StatfsLinux>() == 120);

/// `getmntinfo(mntbufp, flags)`.
pub fn getmntinfo<M: MountList>(mounts: &mut M, flags: i32) -> Result<Vec<StatfsLinux>, Errno> {
    if !matches!(flags, MNT_WAIT | MNT_NOWAIT | MNT_LAZY) {
        return Err(Errno::EINVAL);
    }
    mounts.list()
}

/// `getfsstat(buf, bufsize, flags)`; `buf == None` returns only the count.
pub fn getfsstat<M: MountList>(
    mounts: &mut M,
    buf: Option<&mut [StatfsLinux]>,
    flags: i32,
) -> Result<i32, Errno> {
    if !matches!(flags, MNT_WAIT | MNT_NOWAIT | MNT_LAZY) {
        return Err(Errno::EINVAL);
    }
    let list = mounts.list()?;
    if let Some(target) = buf {
        let count = target.len().min(list.len());
        for (slot, entry) in target.iter_mut().zip(list.iter()).take(count) {
            *slot = *entry;
        }
    }
    i32::try_from(list.len()).map_err(|_| Errno::EOVERFLOW)
}

#[cfg(test)]
#[path = "getmntinfo_ut.rs"]
mod tests;
