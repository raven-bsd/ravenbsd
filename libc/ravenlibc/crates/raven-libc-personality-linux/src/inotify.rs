// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `inotify` (`L05`, `15` T4).
//!
//! The primitive is the TO-BE-DESIGNED file-event service of `vfsd` (`PG-38`). Every function
//! validates its flags/mask first (`EINVAL` for a bad flag or a mask with no event bit) and then
//! calls [`FsWatch`], whose default methods return `ENOSYS`. The symbol ships now so configure
//! scripts and ports link and fall back at run time (`15` 4.3).

use crate::seams::{Errno, FdKind, FdTable, FsWatch};

/// `IN_CLOEXEC`.
pub const IN_CLOEXEC: i32 = raven_libc_abi::fcntl::O_CLOEXEC;
/// `IN_NONBLOCK`.
pub const IN_NONBLOCK: i32 = raven_libc_abi::fcntl::O_NONBLOCK;

/// `IN_ACCESS`.
pub const IN_ACCESS: u32 = 0x0000_0001;
/// `IN_MODIFY`.
pub const IN_MODIFY: u32 = 0x0000_0002;
/// `IN_ATTRIB`.
pub const IN_ATTRIB: u32 = 0x0000_0004;
/// `IN_CLOSE_WRITE`.
pub const IN_CLOSE_WRITE: u32 = 0x0000_0008;
/// `IN_CLOSE_NOWRITE`.
pub const IN_CLOSE_NOWRITE: u32 = 0x0000_0010;
/// `IN_OPEN`.
pub const IN_OPEN: u32 = 0x0000_0020;
/// `IN_MOVED_FROM`.
pub const IN_MOVED_FROM: u32 = 0x0000_0040;
/// `IN_MOVED_TO`.
pub const IN_MOVED_TO: u32 = 0x0000_0080;
/// `IN_CREATE`.
pub const IN_CREATE: u32 = 0x0000_0100;
/// `IN_DELETE`.
pub const IN_DELETE: u32 = 0x0000_0200;
/// `IN_DELETE_SELF`.
pub const IN_DELETE_SELF: u32 = 0x0000_0400;
/// `IN_MOVE_SELF`.
pub const IN_MOVE_SELF: u32 = 0x0000_0800;
/// Every event bit.
pub const IN_ALL_EVENTS: u32 = 0x0000_0fff;
/// `IN_ONLYDIR`.
pub const IN_ONLYDIR: u32 = 0x0100_0000;
/// `IN_DONT_FOLLOW`.
pub const IN_DONT_FOLLOW: u32 = 0x0200_0000;
/// `IN_EXCL_UNLINK`.
pub const IN_EXCL_UNLINK: u32 = 0x0400_0000;
/// `IN_MASK_CREATE`.
pub const IN_MASK_CREATE: u32 = 0x1000_0000;
/// `IN_MASK_ADD`.
pub const IN_MASK_ADD: u32 = 0x2000_0000;
/// `IN_ONESHOT`.
pub const IN_ONESHOT: u32 = 0x8000_0000;

/// The control bits a watch mask may carry.
pub const IN_CONTROL_BITS: u32 =
    IN_ONLYDIR | IN_DONT_FOLLOW | IN_EXCL_UNLINK | IN_MASK_CREATE | IN_MASK_ADD | IN_ONESHOT;

/// Whether `mask` is a valid watch mask: at least one event bit and no unknown bit.
#[must_use]
pub const fn mask_valid(mask: u32) -> bool {
    mask & !(IN_ALL_EVENTS | IN_CONTROL_BITS) == 0 && mask & IN_ALL_EVENTS != 0
}

/// `inotify_init1(flags)`.
pub fn init1<T: FdTable, W: FsWatch>(t: &mut T, watch: &mut W, flags: i32) -> Result<i32, Errno> {
    if flags & !(IN_CLOEXEC | IN_NONBLOCK) != 0 {
        return Err(Errno::EINVAL);
    }
    let handle = watch.init(flags)?;
    t.install(handle, FdKind::Inotify, flags & IN_CLOEXEC != 0)
}

/// `inotify_init()`.
pub fn init<T: FdTable, W: FsWatch>(t: &mut T, watch: &mut W) -> Result<i32, Errno> {
    init1(t, watch, 0)
}

/// `inotify_add_watch(fd, path, mask)`.
pub fn add_watch<T: FdTable, W: FsWatch>(
    t: &mut T,
    watch: &mut W,
    fd: i32,
    path: &[u8],
    mask: u32,
) -> Result<i32, Errno> {
    if !mask_valid(mask) || path.is_empty() {
        return Err(Errno::EINVAL);
    }
    if t.kind_of(fd)? != FdKind::Inotify {
        return Err(Errno::EINVAL);
    }
    watch.add_watch(fd, path, mask)
}

/// `inotify_rm_watch(fd, wd)`.
pub fn rm_watch<T: FdTable, W: FsWatch>(
    t: &mut T,
    watch: &mut W,
    fd: i32,
    wd: i32,
) -> Result<(), Errno> {
    if t.kind_of(fd)? != FdKind::Inotify {
        return Err(Errno::EINVAL);
    }
    if wd < 0 {
        return Err(Errno::EINVAL);
    }
    watch.rm_watch(fd, wd)
}

#[cfg(test)]
#[path = "inotify_ut.rs"]
mod tests;
